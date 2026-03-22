use std::sync::Arc;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::errors::SphereError;
use crate::pipeline::PipelineContext;
use crate::proto::intellisphere::v1::{self as pb, CompletionRequest, Message, StopReason};
use crate::tools::ToolInterceptor;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: i32,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub system: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub stop_reason: String,
    pub usage: UsageResponse,
}

#[derive(Debug, Serialize)]
pub struct UsageResponse {
    pub input_tokens: i32,
    pub output_tokens: i32,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_max_tokens() -> i32 {
    1024
}

/// Build tool definitions from registry for system prompt injection.
fn build_tool_defs(state: &AppState) -> Vec<pb::ToolDefinition> {
    state
        .tool_registry
        .all()
        .map(|t| pb::ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema_json: t.input_schema_json.clone(),
        })
        .collect()
}

fn map_stop_reason(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::ToolUse => "tool_use",
        StopReason::MaxTokens => "max_tokens",
        StopReason::StopSequence => "stop_sequence",
        _ => "unknown",
    }
}

/// POST /v1/chat — unary completion with multi-turn tool loop
pub async fn chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, SphereError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut context = PipelineContext::new(request_id.clone(), String::new());

    // Convert and run inbound pipeline on each user message
    let mut proto_messages: Vec<Message> = Vec::new();
    for msg in &req.messages {
        let mut proto_msg = Message {
            role: msg.role.clone(),
            content: msg.content.clone(),
            tool_results: vec![],
            tool_calls: vec![],
        };
        state.pipeline.run_inbound(&mut proto_msg, &mut context)?;
        proto_messages.push(proto_msg);
    }

    let tool_defs = build_tool_defs(&state);
    let system = req.system.unwrap_or_default();
    let max_iterations = state.config.tools.max_tool_iterations;
    let hard_cap = 25usize;
    let effective_max = max_iterations.min(hard_cap);

    let mut total_input_tokens: i32 = 0;
    let mut total_output_tokens: i32 = 0;

    // Multi-turn tool loop
    for iteration in 0..effective_max {
        let completion_req = CompletionRequest {
            provider: "anthropic".to_string(),
            model: req.model.clone(),
            system: system.clone(),
            messages: proto_messages.clone(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            tools: tool_defs.clone(),
            stop_sequences: vec![],
            metadata: Some(pb::RequestMetadata {
                request_id: request_id.clone(),
                session_id: String::new(),
                timestamp: None,
            }),
        };

        let response = state
            .core_client
            .complete(completion_req)
            .await
            .map_err(|e| SphereError::CoreError(e.message().to_string()))?;

        if let Some(usage) = &response.usage {
            total_input_tokens += usage.input_tokens;
            total_output_tokens += usage.output_tokens;
        }

        // Check if the model wants to use tools
        if response.stop_reason() == StopReason::ToolUse && !response.tool_calls.is_empty() {
            tracing::info!(
                iteration = iteration,
                tool_count = response.tool_calls.len(),
                "Tool calls requested by LLM"
            );

            // Add assistant message with tool calls to conversation
            proto_messages.push(Message {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_results: vec![],
                tool_calls: response.tool_calls.clone(),
            });

            // Execute tools via interceptor
            let tool_results = ToolInterceptor::process(
                &response.tool_calls,
                &state.tool_registry,
                &state.tool_executor,
                context.identity.as_ref(),
            )
            .await;

            // Add tool results as a user message
            proto_messages.push(Message {
                role: "user".to_string(),
                content: String::new(),
                tool_results,
                tool_calls: vec![],
            });

            // Continue the loop — send results back to Core
            continue;
        }

        // Model finished (end_turn, max_tokens, stop_sequence) — return response
        let stop_reason = map_stop_reason(response.stop_reason()).to_string();
        let mut response_msg = Message {
            role: "assistant".to_string(),
            content: response.content.clone(),
            tool_results: vec![],
            tool_calls: vec![],
        };
        state
            .pipeline
            .run_outbound(&mut response_msg, &mut context)?;

        return Ok(Json(ChatResponse {
            content: response_msg.content,
            model: response.model,
            stop_reason,
            usage: UsageResponse {
                input_tokens: total_input_tokens,
                output_tokens: total_output_tokens,
            },
        }));
    }

    // Max iterations reached — return last response with warning
    tracing::warn!(
        max_iterations = effective_max,
        "Tool loop reached maximum iterations"
    );

    Ok(Json(ChatResponse {
        content: format!(
            "[IntelliSphere: Tool loop reached maximum of {} iterations]",
            effective_max
        ),
        model: req.model.clone(),
        stop_reason: "max_iterations".to_string(),
        usage: UsageResponse {
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
        },
    }))
}

/// POST /v1/chat/stream — SSE streaming completion
pub async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, SphereError>>>, SphereError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut context = PipelineContext::new(request_id.clone(), String::new());

    let mut proto_messages: Vec<Message> = Vec::new();
    for msg in &req.messages {
        let mut proto_msg = Message {
            role: msg.role.clone(),
            content: msg.content.clone(),
            tool_results: vec![],
            tool_calls: vec![],
        };
        state.pipeline.run_inbound(&mut proto_msg, &mut context)?;
        proto_messages.push(proto_msg);
    }

    let tool_defs = build_tool_defs(&state);
    let system = req.system.unwrap_or_default();

    let completion_req = CompletionRequest {
        provider: "anthropic".to_string(),
        model: req.model,
        system,
        messages: proto_messages,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        tools: tool_defs,
        stop_sequences: vec![],
        metadata: Some(pb::RequestMetadata {
            request_id,
            session_id: String::new(),
            timestamp: None,
        }),
    };

    let stream = state
        .core_client
        .complete_stream(completion_req)
        .await
        .map_err(|e| SphereError::CoreError(e.message().to_string()))?;

    let sse_stream = stream.map(|chunk_result| {
        chunk_result
            .map(|chunk| {
                let data = serde_json::json!({
                    "delta": chunk.delta,
                    "chunk_type": chunk.chunk_type,
                });
                Event::default().data(data.to_string())
            })
            .map_err(|e| SphereError::CoreError(e.message().to_string()))
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}
