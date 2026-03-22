use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub sphere: ComponentHealth,
    pub core: ComponentHealth,
}

#[derive(Serialize)]
pub struct ComponentHealth {
    pub healthy: bool,
    pub message: String,
}

pub async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let core_health = match state.core_client.health().await {
        Ok(resp) => ComponentHealth {
            healthy: resp.healthy,
            message: resp.message,
        },
        Err(e) => ComponentHealth {
            healthy: false,
            message: format!("Core unreachable: {}", e.message()),
        },
    };

    let overall = if core_health.healthy {
        "healthy"
    } else {
        "degraded"
    };

    Json(HealthResponse {
        status: overall.to_string(),
        sphere: ComponentHealth {
            healthy: true,
            message: "OK".to_string(),
        },
        core: core_health,
    })
}
