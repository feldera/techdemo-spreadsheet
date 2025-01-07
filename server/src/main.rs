use axum::{routing::get, routing::post, Router};
use dashmap::DashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use reqwest::Client;
use tokio::sync::broadcast::Sender;
use tower_http::cors::CorsLayer;
use crate::spreadsheet::SpreadSheetView;
use crate::stats::XlsError;

mod feldera;
mod spreadsheet;
mod stats;
#[derive(Clone)]
struct AppState {
    stats_subscription: Sender<Result<String, XlsError>>,
    xls_subscription: Sender<Result<String, XlsError>>,
    spreadsheet_view: Arc<SpreadSheetView>,
    api_limits: Arc<DashSet<String>>,
    http_client: Client,
}

#[tokio::main]
async fn main() {
    let _r = env_logger::try_init();

    let http_client = Client::new();
    let stats_subscription = feldera::subscribe_change_stream(http_client.clone(), "spreadsheet_statistics", 128);
    let xls_subscription = feldera::subscribe_change_stream(http_client.clone(), "spreadsheet_view", 4096);
    let api_limits = feldera::api_limit_table(http_client.clone());
    let spreadsheet_view = Arc::new(SpreadSheetView::new(http_client.clone(), xls_subscription.subscribe()).await);

    let state = AppState {
        stats_subscription,
        xls_subscription,
        spreadsheet_view,
        api_limits,
        http_client,
    };

    let app = Router::new()
        .route("/", get(|| async { "xls app!" }))
        .route("/api/stats", get(stats::stats))
        .route("/api/spreadsheet", get(spreadsheet::ws_handler))
        .route("/api/spreadsheet", post(spreadsheet::post_handler))
        .with_state(state)
        .layer(CorsLayer::permissive());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
