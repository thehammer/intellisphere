mod chat;
mod health;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::satellite::ws;
use crate::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/chat", post(chat::chat))
        .route("/v1/chat/stream", post(chat::chat_stream))
        .route("/health", get(health::health))
        .route("/v1/satellite/session", post(ws::create_session))
        .route("/satellite", get(ws::ws_upgrade))
}
