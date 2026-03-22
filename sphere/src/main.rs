#![allow(dead_code)]

mod api;
mod audit;
mod auth;
mod config;
mod core_client;
mod errors;
mod ingress;
mod pipeline;
mod policy;
mod proto;
mod rate_limit;
mod satellite;
mod tools;

use std::collections::HashSet;
use std::net::SocketAddr;

use anyhow::Context;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::SphereConfig;
use crate::core_client::CoreClient;
use crate::ingress::IngressGate;
use crate::pipeline::PipelineChain;
use crate::rate_limit::RateLimiter;
use crate::satellite::ws::SatelliteState;
use crate::tools::{ToolExecutor, ToolRegistration, ToolRegistry, ToolZone};

pub struct AppState {
    pub config: SphereConfig,
    pub core_client: CoreClient,
    pub pipeline: PipelineChain,
    pub ingress_gate: IngressGate,
    pub rate_limiter: RateLimiter,
    pub tool_registry: ToolRegistry,
    pub tool_executor: ToolExecutor,
    pub satellite_state: Option<SatelliteState>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "intellisphere_sphere=info,tower_http=info".into()),
        )
        .init();

    let config = SphereConfig::load().context("Failed to load configuration")?;
    tracing::info!(listen_addr = %config.listen_addr, "Starting IntelliSphere Sphere");

    let core_client = CoreClient::connect(&config.core_grpc_url)
        .await
        .context("Failed to connect to Core")?;

    let pipeline = PipelineChain::from_config(&config.pipeline);

    // Build tool registry with built-in echo tool
    let mut tool_registry = ToolRegistry::new();
    tool_registry
        .register(ToolRegistration {
            name: "intellisphere_echo".to_string(),
            description: "Echo a message back. Useful for testing the tool call loop.".to_string(),
            input_schema_json: r#"{
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to echo back"
                    }
                },
                "required": ["message"]
            }"#
            .to_string(),
            zone: ToolZone::Sphere,
            required_scopes: vec![],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        })
        .expect("Failed to register built-in echo tool");

    let tool_executor = ToolExecutor::new(HashSet::new());

    let ingress_gate = IngressGate::from_config(&config.auth);
    let rate_limiter = RateLimiter::from_config(&config.rate_limit);

    // Initialize satellite state with a server secret.
    // In production this should come from config / env var.
    let satellite_secret = std::env::var("SPHERE_SATELLITE_SECRET")
        .unwrap_or_else(|_| "dev-satellite-secret".to_string());
    let satellite_state = SatelliteState::new(satellite_secret.into_bytes());

    let state = std::sync::Arc::new(AppState {
        config: config.clone(),
        core_client,
        pipeline,
        ingress_gate,
        rate_limiter,
        tool_registry,
        tool_executor,
        satellite_state: Some(satellite_state),
    });

    let app = Router::new()
        .merge(api::routes())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = config.listen_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "Sphere listening");

    axum::serve(listener, app).await?;

    Ok(())
}
