use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

use crate::proto::intellisphere::v1::{ToolCall, ToolResult};
use crate::tools::registry::{ToolRegistration, ToolZone};
use crate::tools::scoped_client::ScopedHttpClient;

/// Executes tool calls against registered handlers.
#[derive(Clone)]
pub struct ToolExecutor {
    scoped_client: ScopedHttpClient,
}

impl ToolExecutor {
    pub fn new(allowed_domains: HashSet<String>) -> Self {
        let scoped_client = ScopedHttpClient::new(
            allowed_domains,
            Duration::from_secs(5),
            1024 * 1024, // 1MB request
            1024 * 1024, // 1MB response
        );
        Self { scoped_client }
    }

    /// Execute a tool call and return the result.
    pub async fn execute(
        &self,
        tool_call: &ToolCall,
        registration: &ToolRegistration,
    ) -> ToolResult {
        match registration.zone {
            ToolZone::Sphere => self.execute_sphere_tool(tool_call, registration).await,
            ToolZone::Satellite => ToolResult {
                call_id: tool_call.call_id.clone(),
                result_json: serde_json::json!({
                    "error": "SATELLITE_UNAVAILABLE",
                    "message": "Tool requires browser connection. No Satellite session active."
                })
                .to_string(),
                is_error: true,
            },
            ToolZone::Wasm => ToolResult {
                call_id: tool_call.call_id.clone(),
                result_json: serde_json::json!({
                    "error": "NOT_IMPLEMENTED",
                    "message": "WASM tool execution is not yet supported."
                })
                .to_string(),
                is_error: true,
            },
        }
    }

    async fn execute_sphere_tool(
        &self,
        tool_call: &ToolCall,
        registration: &ToolRegistration,
    ) -> ToolResult {
        // Built-in tools are handled by name
        if tool_call.name == "intellisphere_echo" {
            return self.execute_echo(tool_call);
        }

        // HTTP handler tools
        match &registration.handler_url {
            Some(url) => {
                let timeout = Duration::from_millis(registration.timeout_ms);
                match tokio::time::timeout(
                    timeout,
                    self.scoped_client.post(url, &tool_call.arguments_json),
                )
                .await
                {
                    Ok(Ok(response)) => ToolResult {
                        call_id: tool_call.call_id.clone(),
                        result_json: response,
                        is_error: false,
                    },
                    Ok(Err(e)) => ToolResult {
                        call_id: tool_call.call_id.clone(),
                        result_json: serde_json::json!({
                            "error": "EXECUTION_ERROR",
                            "message": format!("Tool handler request failed: {}", e)
                        })
                        .to_string(),
                        is_error: true,
                    },
                    Err(_) => ToolResult {
                        call_id: tool_call.call_id.clone(),
                        result_json: serde_json::json!({
                            "error": "EXECUTION_TIMEOUT",
                            "message": "Tool handler exceeded timeout"
                        })
                        .to_string(),
                        is_error: true,
                    },
                }
            }
            None => ToolResult {
                call_id: tool_call.call_id.clone(),
                result_json: serde_json::json!({
                    "error": "EXECUTION_ERROR",
                    "message": "No handler URL configured for this tool"
                })
                .to_string(),
                is_error: true,
            },
        }
    }

    /// Built-in echo tool for testing the tool call loop.
    fn execute_echo(&self, tool_call: &ToolCall) -> ToolResult {
        let args: Value = serde_json::from_str(&tool_call.arguments_json).unwrap_or(Value::Null);
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("(no message)");

        let result = serde_json::json!({
            "echo": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        ToolResult {
            call_id: tool_call.call_id.clone(),
            result_json: result.to_string(),
            is_error: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_executor() -> ToolExecutor {
        ToolExecutor::new(HashSet::new())
    }

    #[test]
    fn test_echo_tool() {
        let executor = test_executor();
        let tool_call = ToolCall {
            call_id: "tc_1".to_string(),
            name: "intellisphere_echo".to_string(),
            arguments_json: r#"{"message":"hello"}"#.to_string(),
        };

        let result = executor.execute_echo(&tool_call);
        assert!(!result.is_error);
        assert_eq!(result.call_id, "tc_1");

        let parsed: Value = serde_json::from_str(&result.result_json).unwrap();
        assert_eq!(parsed["echo"], "hello");
        assert!(parsed["timestamp"].is_string());
    }

    #[test]
    fn test_echo_tool_missing_message() {
        let executor = test_executor();
        let tool_call = ToolCall {
            call_id: "tc_2".to_string(),
            name: "intellisphere_echo".to_string(),
            arguments_json: r#"{}"#.to_string(),
        };

        let result = executor.execute_echo(&tool_call);
        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.result_json).unwrap();
        assert_eq!(parsed["echo"], "(no message)");
    }
}
