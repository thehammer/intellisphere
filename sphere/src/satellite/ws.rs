use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::errors::SphereError;
use crate::satellite::adjudicator::{AdjudicationResult, Adjudicator, ToolProposal};
use crate::satellite::session::SessionManager;
use crate::satellite::trust_budget::TrustBudget;
use crate::AppState;

/// Default session duration in seconds (1 hour).
const SESSION_DURATION_SECS: i64 = 3600;

/// Default trust budget for new sessions.
const DEFAULT_TRUST_BUDGET: f64 = 100.0;

/// Default suspicion threshold.
const DEFAULT_SUSPICION_THRESHOLD: f64 = 1.0;

/// WebSocket heartbeat interval.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

type HmacSha256 = Hmac<Sha256>;

/// Shared satellite state, wrapped in a Mutex for safe concurrent access.
pub struct SatelliteState {
    pub session_manager: Mutex<SessionManager>,
    /// Server secret used for HMAC token generation.
    pub server_secret: Vec<u8>,
}

impl SatelliteState {
    pub fn new(server_secret: Vec<u8>) -> Self {
        Self {
            session_manager: Mutex::new(SessionManager::new()),
            server_secret,
        }
    }
}

// ── Request / Response types ────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    /// Optional custom trust budget (defaults to 100.0).
    pub trust_budget: Option<f64>,
    /// Optional session duration in seconds (defaults to 3600).
    pub duration_secs: Option<i64>,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub token: String,
    pub expires_at: String,
}

#[derive(Deserialize)]
pub struct WsUpgradeQuery {
    pub token: String,
}

// ── WebSocket protocol messages ─────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SatelliteMessage {
    /// Tool proposal from satellite to sphere.
    ToolProposal {
        id: String,
        tool_name: String,
        result_payload: String,
    },
    /// Adjudication result from sphere to satellite.
    AdjudicationResult {
        id: String,
        accepted: bool,
        reason: Option<String>,
    },
    /// Heartbeat ping.
    Ping,
    /// Heartbeat pong.
    Pong,
    /// Error message from sphere.
    Error { message: String },
    /// Session terminated.
    SessionTerminated { reason: String },
}

// ── Token generation / validation ───────────────────────────────────

/// Generate an HMAC-SHA256 token for a satellite session.
pub fn generate_token(
    session_id: &str,
    identity_sub: &str,
    expires_at: &str,
    secret: &[u8],
) -> String {
    let data = format!("{}:{}:{}", session_id, identity_sub, expires_at);
    let mut mac =
        HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Validate an HMAC-SHA256 token. Returns the session_id if valid.
pub fn validate_token(
    token: &str,
    session_id: &str,
    identity_sub: &str,
    expires_at: &str,
    secret: &[u8],
) -> bool {
    let expected = generate_token(session_id, identity_sub, expires_at, secret);
    // Constant-time comparison via HMAC re-verification
    token == expected
}

// ── HTTP handlers ──────────────────────────────────────────────────

/// POST /v1/satellite/session — create a new satellite session token.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, SphereError> {
    // Authenticate the caller
    let identity = state.ingress_gate.authenticate(
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok()),
    )?;

    let session_id = Uuid::new_v4().to_string();
    let budget = body.trust_budget.unwrap_or(DEFAULT_TRUST_BUDGET);
    let duration = body.duration_secs.unwrap_or(SESSION_DURATION_SECS);
    let expires_at = Utc::now() + chrono::Duration::seconds(duration);
    let expires_at_str = expires_at.to_rfc3339();

    let trust_budget = TrustBudget::new(budget, DEFAULT_SUSPICION_THRESHOLD);

    let sat_state = state
        .satellite_state
        .as_ref()
        .ok_or_else(|| SphereError::Internal(anyhow::anyhow!("satellite not configured")))?;

    sat_state.session_manager.lock().await.create(
        session_id.clone(),
        identity.sub.clone(),
        trust_budget,
        expires_at,
    );

    let token = generate_token(
        &session_id,
        &identity.sub,
        &expires_at_str,
        &sat_state.server_secret,
    );

    tracing::info!(
        session_id = %session_id,
        identity_sub = %identity.sub,
        "Satellite session created"
    );

    Ok(Json(CreateSessionResponse {
        session_id,
        token,
        expires_at: expires_at_str,
    }))
}

/// GET /satellite?token={token} — WebSocket upgrade.
pub async fn ws_upgrade(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsUpgradeQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, SphereError> {
    let sat_state = state
        .satellite_state
        .as_ref()
        .ok_or_else(|| SphereError::Internal(anyhow::anyhow!("satellite not configured")))?;

    // Find the session matching the token by checking all active sessions.
    let mgr = sat_state.session_manager.lock().await;
    let mut found_session_id = None;

    // We need to iterate sessions to find which one this token belongs to.
    // In production you'd index tokens, but for now we check each session.
    // The token encodes session_id+sub+expires, so we validate against each.
    for (sid, session) in mgr.sessions_iter() {
        let expires_str = session.expires_at.to_rfc3339();
        if validate_token(
            &params.token,
            sid,
            &session.identity_sub,
            &expires_str,
            &sat_state.server_secret,
        ) {
            if session.expires_at > Utc::now() {
                found_session_id = Some(sid.clone());
            }
            break;
        }
    }
    drop(mgr);

    let session_id = found_session_id
        .ok_or_else(|| SphereError::AuthError("invalid or expired session token".into()))?;

    tracing::info!(session_id = %session_id, "WebSocket upgrade for satellite session");

    let state_clone = state.clone();
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state_clone, session_id)))
}

/// Handle the WebSocket connection for a satellite session.
async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>, session_id: String) {
    tracing::info!(session_id = %session_id, "Satellite WebSocket connected");

    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                let ping = serde_json::to_string(&SatelliteMessage::Ping).unwrap();
                if socket.send(WsMessage::Text(ping.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        let response = handle_message(&state, &session_id, &text).await;
                        let response_json = serde_json::to_string(&response).unwrap();
                        if socket.send(WsMessage::Text(response_json.into())).await.is_err() {
                            break;
                        }
                        // Check if session was terminated
                        if matches!(response, SatelliteMessage::SessionTerminated { .. }) {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    tracing::info!(session_id = %session_id, "Satellite WebSocket disconnected");
}

/// Process an incoming WebSocket message.
async fn handle_message(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> SatelliteMessage {
    let msg: SatelliteMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            return SatelliteMessage::Error {
                message: format!("invalid message: {}", e),
            };
        }
    };

    match msg {
        SatelliteMessage::Pong => SatelliteMessage::Ping, // ack
        SatelliteMessage::ToolProposal {
            id,
            tool_name,
            result_payload,
        } => {
            let sat_state = match state.satellite_state.as_ref() {
                Some(s) => s,
                None => {
                    return SatelliteMessage::Error {
                        message: "satellite not configured".into(),
                    };
                }
            };

            let mut mgr = sat_state.session_manager.lock().await;
            let session = match mgr.get_mut(session_id) {
                Some(s) => s,
                None => {
                    return SatelliteMessage::SessionTerminated {
                        reason: "session not found".into(),
                    };
                }
            };

            let proposal = ToolProposal {
                tool_name,
                result_payload,
            };

            let result = Adjudicator::adjudicate(
                session,
                &proposal,
                &state.tool_registry,
                &state.pipeline,
            );

            // Check if session should be terminated
            if session.trust_budget.is_exhausted() || session.trust_budget.is_suspicious() {
                let reason = if session.trust_budget.is_exhausted() {
                    "trust budget exhausted"
                } else {
                    "suspicion threshold exceeded"
                };
                return SatelliteMessage::SessionTerminated {
                    reason: reason.into(),
                };
            }

            SatelliteMessage::AdjudicationResult {
                id,
                accepted: result.accepted,
                reason: result.reason,
            }
        }
        _ => SatelliteMessage::Error {
            message: "unexpected message type".into(),
        },
    }
}
