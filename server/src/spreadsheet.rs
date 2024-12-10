use std::net::SocketAddr;
use std::ops::{ControlFlow, Range};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{connect_info::ConnectInfo, Json, State},
    response::IntoResponse,
};
use axum::http::HeaderMap;
use chrono::Utc;
use futures::{sink::SinkExt, stream::StreamExt};
use log::{debug, error, trace, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast::Receiver, mpsc, watch};

use crate::feldera::{adhoc_query, insert};
use crate::stats::XlsError;
use crate::AppState;

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
struct Cell {
    id: i64,
    background: i32,
    raw_value: String,
    computed_value: String,
}

#[derive(serde::Deserialize, Debug, Copy, Clone)]
struct Region {
    from: i64,
    to: i64,
}

impl Default for Region {
    fn default() -> Self {
        Region { from: 0, to: 2500 }
    }
}

/// The handler for the HTTP request (this gets called when the HTTP request lands at the start
/// of websocket negotiation). After this completes, the actual switching from HTTP to
/// websocket protocol will occur.
/// This is the last point where we can extract TCP/IP metadata such as IP address of the client
/// as well as things from HTTP headers such as user-agent of the browser etc.
pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    debug!("{addr} connected.");
    ws.on_upgrade(move |socket| handle_socket(state.xls_subscription.subscribe(), socket, addr))
}

/// Actual websocket state-machine (one will be spawned per connection)
async fn handle_socket(
    mut xls_changes: Receiver<Result<String, XlsError>>,
    socket: WebSocket,
    who: SocketAddr,
) {
    let (mut sender, mut receiver) = socket.split();
    let (region_tx, mut region_rx) = watch::channel(Region::default());
    let (change_sender, mut change_receiver) = mpsc::channel::<String>(128);

    // spawn a task that forwards messages from the mpsc to the sink
    tokio::spawn(async move {
        while let Some(message) = change_receiver.recv().await {
            match sender.send(Message::Text(message.trim().to_string())).await {
                Ok(_) => {
                    trace!("{message} sent to {who}");
                }
                Err(e) => {
                    warn!("Error sending change to client: {e}");
                }
            }
        }
    });

    // Spawn a task that will push spreadsheet view changes to the client
    let change_fwder = change_sender.clone();
    let mut change_task = tokio::spawn(async move {
        let mut cnt = 0;
        loop {
            cnt += 1;
            match xls_changes.recv().await {
                Ok(Ok(change)) => match serde_json::from_str::<Cell>(&change) {
                    Ok(cell) => {
                        let region = { *region_rx.borrow_and_update() };
                        if cell.id >= region.from && cell.id < region.to {
                            match change_fwder.send(change).await {
                                Ok(_) => {}
                                Err(e) => {
                                    warn!("Error sending change to sender task: {e}");
                                    return cnt;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error parsing change: {e} (change {change})");
                    }
                },
                Ok(Err(e)) => {
                    warn!("Error receiving change: {e}");
                    return cnt;
                }
                Err(e) => {
                    warn!("Error receiving change: {e}");
                    return cnt;
                }
            }
        }
    });

    // This second task will receive messages from the client and push snapshots
    let change_fwder = change_sender.clone();
    let mut recv_task = tokio::spawn(async move {
        let mut cnt = 0;
        while let Some(Ok(msg)) = receiver.next().await {
            cnt += 1;
            match process_message(msg, who) {
                ControlFlow::Continue(Some(region)) => {
                    match adhoc_query(
                        format!(
                            "SELECT * FROM spreadsheet_view WHERE id >= {} and id < {}",
                            region.from, region.to
                        )
                        .as_str(),
                    )
                    .await
                    {
                        Ok(snapshot) => {
                            region_tx.send_replace(region);
                            for line in snapshot.split('\n') {
                                match change_fwder.send(line.to_string()).await {
                                    Ok(_) => {}
                                    Err(e) => {
                                        warn!("Error sending change to sender task: {e}");
                                        return cnt;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Error querying spreadsheet_view: {e}");
                            return cnt;
                        }
                    }
                }
                ControlFlow::Continue(None) => {}
                ControlFlow::Break(_) => {
                    break;
                }
            }
        }
        cnt
    });

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        rv_a = &mut change_task => {
            match rv_a {
                Ok(a) => debug!("{a} messages sent to {who}"),
                Err(a) => warn!("Error sending messages {a:?}")
            }
            recv_task.abort();
        },
        rv_b = &mut recv_task => {
            match rv_b {
                Ok(b) => debug!("Received {b} messages from {who}"),
                Err(b) => warn!("Error receiving messages {b:?}")
            }
            change_task.abort();
        }
    }

    trace!("Websocket context {who} destroyed");
}

/// helper to print contents of messages to stdout. Has special treatment for Close.
fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), Option<Region>> {
    match msg {
        Message::Text(t) => match serde_json::from_str::<Region>(&t) {
            Ok(region) => {
                debug!("{who} sent range: {region:?}");
                ControlFlow::Continue(Some(region))
            }
            Err(e) => {
                warn!("{who} sent invalid region JSON: {t:?} {e}");
                ControlFlow::Continue(None)
            }
        },
        Message::Close(c) => {
            debug!("{who} closed connection: {c:?}");
            ControlFlow::Break(())
        }
        _ => ControlFlow::Continue(None),
    }
}

// Insert/Update a cell

// Data structure to represent incoming JSON payload
#[derive(Deserialize, Debug)]
pub(crate) struct UpdateRequest {
    id: i64,
    raw_value: String,
    background: i32,
}

impl UpdateRequest {
    const ID_RANGE: Range<i64> = 0i64..1_040_000_000i64;
}

// Data structure to represent outgoing JSON payload
#[derive(Serialize, Debug)]
struct UpdatePayload {
    id: i64,
    raw_value: String,
    background: i32,
    ip: String,
    ts: String,
}

pub(crate) async fn post_handler(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(update_request): Json<UpdateRequest>,
) -> impl IntoResponse {
    // Load balancer puts the client IP in the HTTP header
    const CLIENT_IP_HEADER: &str = "Fly-Client-IP";
    let client_ip = headers.get(CLIENT_IP_HEADER).map(|ip| {
        String::from_utf8_lossy(ip.as_bytes()).chars().take(45).collect::<String>()
    }).unwrap_or(addr.ip().to_string().chars().take(45).collect::<String>());

    if state.api_limits.contains(&client_ip) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "API limit exceeded"})),
        );
    }
    if !UpdateRequest::ID_RANGE.contains(&update_request.id) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid cell ID"})),
        );
    }
    let raw_value = update_request.raw_value.chars().take(64).collect::<String>();

    let payload = UpdatePayload {
        id: update_request.id,
        raw_value,
        background: update_request.background,
        ip: client_ip,
        ts: Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
    };

    insert(&state.client,"spreadsheet_data", payload).await
}
