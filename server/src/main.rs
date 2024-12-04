use axum::{routing::get, routing::post, Router};
use dashmap::DashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tower_http::cors::CorsLayer;

use crate::stats::XlsError;

mod feldera;
mod spreadsheet;
mod stats;
#[derive(Clone)]
struct AppState {
    stats_subscription: Sender<Result<String, XlsError>>,
    xls_subscription: Sender<Result<String, XlsError>>,
    api_limits: Arc<DashSet<String>>,
}

#[tokio::main]
async fn main() {
    let _r = env_logger::try_init();

    let stats_subscription = feldera::subscribe_change_stream("spreadsheet_statistics", 128);
    let xls_subscription = feldera::subscribe_change_stream("spreadsheet_view", 4096);
    let api_limits = feldera::api_limit_table();

    let state = AppState {
        stats_subscription,
        xls_subscription,
        api_limits,
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
