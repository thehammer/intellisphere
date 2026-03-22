use crate::auth::Identity;
use crate::proto::intellisphere::v1::{ToolCall, ToolResult};
use crate::tools::executor::ToolExecutor;
use crate::tools::registry::{ToolRegistration, ToolRegistry};

/// Maximum allowed size for tool results (1MB).
const MAX_RESULT_SIZE: usize = 1024 * 1024;

/// Intercepts tool calls from Core, validates them against the registry,
/// checks authorization, and dispatches execution with security hardening.
pub struct ToolInterceptor;

impl ToolInterceptor {
    /// Process a list of tool calls from the LLM response.
    /// Returns tool results to be sent back to Core.
    pub async fn process(
        tool_calls: &[ToolCall],
        registry: &ToolRegistry,
        executor: &ToolExecutor,
        identity: Option<&Identity>,
    ) -> Vec<ToolResult> {
        let mut results = Vec::with_capacity(tool_calls.len());

        for tc in tool_calls {
            let result = match registry.get(&tc.name) {
                Some(registration) => {
                    // Step 1: Authorization check
                    if let Err(e) = check_authorization(registration, identity) {
                        tracing::warn!(
                            tool = %tc.name,
                            call_id = %tc.call_id,
                            error = %e,
                            "Tool authorization denied"
                        );
                        error_result(&tc.call_id, "AUTHORIZATION_DENIED", &e)
                    }
                    // Step 2: Validate params against JSON schema
                    else if let Err(e) =
                        validate_params(&tc.arguments_json, &registration.input_schema_json)
                    {
                        tracing::warn!(
                            tool = %tc.name,
                            call_id = %tc.call_id,
                            error = %e,
                            "Tool parameter validation failed"
                        );
                        error_result(
                            &tc.call_id,
                            "VALIDATION_FAILED",
                            &format!("Parameter validation failed: {}", e),
                        )
                    }
                    // Step 3: Execute with panic containment
                    else {
                        tracing::info!(
                            tool = %tc.name,
                            call_id = %tc.call_id,
                            "Executing tool"
                        );
                        execute_with_containment(tc, registration, executor).await
                    }
                }
                None => {
                    tracing::warn!(
                        tool = %tc.name,
                        call_id = %tc.call_id,
                        "Tool not found in registry"
                    );
                    error_result(
                        &tc.call_id,
                        "TOOL_NOT_FOUND",
                        &format!("Tool '{}' is not registered", tc.name),
                    )
                }
            };

            results.push(result);
        }

        results
    }
}

/// Check that the identity has required scopes and roles for the tool.
fn check_authorization(
    registration: &ToolRegistration,
    identity: Option<&Identity>,
) -> Result<(), String> {
    // Phase 1 compatibility: if no scopes/roles required, always allow
    if registration.required_scopes.is_empty() && registration.required_roles.is_empty() {
        return Ok(());
    }

    let identity = identity.ok_or("Authentication required for this tool")?;

    // Check scopes
    for scope in &registration.required_scopes {
        if !identity.scopes.contains(scope) {
            return Err(format!("Missing required scope: {}", scope));
        }
    }

    // Check roles
    for role in &registration.required_roles {
        if !identity.roles.contains(role) {
            return Err(format!("Missing required role: {}", role));
        }
    }

    Ok(())
}

/// Execute a tool call with panic containment and result size enforcement.
async fn execute_with_containment(
    tool_call: &ToolCall,
    registration: &ToolRegistration,
    executor: &ToolExecutor,
) -> ToolResult {
    // Use tokio::task::spawn to catch panics
    let tc = tool_call.clone();
    let reg = registration.clone();
    let exec = executor.clone();

    let handle = tokio::task::spawn(async move { exec.execute(&tc, &reg).await });

    match handle.await {
        Ok(mut result) => {
            // Enforce result size limit
            if result.result_json.len() > MAX_RESULT_SIZE {
                tracing::warn!(
                    call_id = %tool_call.call_id,
                    size = result.result_json.len(),
                    max = MAX_RESULT_SIZE,
                    "Tool result truncated"
                );
                result.result_json = result.result_json[..MAX_RESULT_SIZE].to_string();
            }
            result
        }
        Err(e) => {
            tracing::error!(
                call_id = %tool_call.call_id,
                error = %e,
                "Tool handler panicked"
            );
            error_result(
                &tool_call.call_id,
                "EXECUTION_ERROR",
                "Tool handler encountered an internal error",
            )
        }
    }
}

fn error_result(call_id: &str, code: &str, message: &str) -> ToolResult {
    ToolResult {
        call_id: call_id.to_string(),
        result_json: serde_json::json!({
            "error": code,
            "message": message
        })
        .to_string(),
        is_error: true,
    }
}

/// Validate tool call parameters against the tool's JSON schema.
fn validate_params(params_json: &str, schema_json: &str) -> Result<(), String> {
    let params: serde_json::Value =
        serde_json::from_str(params_json).map_err(|e| format!("Invalid JSON: {}", e))?;

    let schema: serde_json::Value =
        serde_json::from_str(schema_json).map_err(|e| format!("Invalid schema: {}", e))?;

    let validator =
        jsonschema::validator_for(&schema).map_err(|e| format!("Invalid JSON Schema: {}", e))?;

    let errors: Vec<String> = validator.iter_errors(&params).map(|e| e.to_string()).collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_params_valid() {
        let schema = r#"{
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            },
            "required": ["message"]
        }"#;
        let params = r#"{"message": "hello"}"#;
        assert!(validate_params(params, schema).is_ok());
    }

    #[test]
    fn test_validate_params_missing_required() {
        let schema = r#"{
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            },
            "required": ["message"]
        }"#;
        let params = r#"{}"#;
        assert!(validate_params(params, schema).is_err());
    }

    #[test]
    fn test_validate_params_wrong_type() {
        let schema = r#"{
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            }
        }"#;
        let params = r#"{"message": 42}"#;
        assert!(validate_params(params, schema).is_err());
    }

    #[test]
    fn test_authorization_no_requirements() {
        let reg = ToolRegistration {
            name: "test".to_string(),
            description: String::new(),
            input_schema_json: "{}".to_string(),
            zone: crate::tools::ToolZone::Sphere,
            required_scopes: vec![],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        };
        assert!(check_authorization(&reg, None).is_ok());
    }

    #[test]
    fn test_authorization_missing_scope() {
        let reg = ToolRegistration {
            name: "test".to_string(),
            description: String::new(),
            input_schema_json: "{}".to_string(),
            zone: crate::tools::ToolZone::Sphere,
            required_scopes: vec!["admin".to_string()],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        };
        let identity = Identity {
            sub: "user1".to_string(),
            roles: vec![],
            scopes: vec!["read".to_string()],
            metadata: std::collections::HashMap::new(),
        };
        assert!(check_authorization(&reg, Some(&identity)).is_err());
    }

    #[test]
    fn test_authorization_has_required_scope() {
        let reg = ToolRegistration {
            name: "test".to_string(),
            description: String::new(),
            input_schema_json: "{}".to_string(),
            zone: crate::tools::ToolZone::Sphere,
            required_scopes: vec!["admin".to_string()],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        };
        let identity = Identity {
            sub: "user1".to_string(),
            roles: vec![],
            scopes: vec!["admin".to_string()],
            metadata: std::collections::HashMap::new(),
        };
        assert!(check_authorization(&reg, Some(&identity)).is_ok());
    }

    #[test]
    fn test_authorization_no_identity_when_required() {
        let reg = ToolRegistration {
            name: "test".to_string(),
            description: String::new(),
            input_schema_json: "{}".to_string(),
            zone: crate::tools::ToolZone::Sphere,
            required_scopes: vec!["admin".to_string()],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        };
        assert!(check_authorization(&reg, None).is_err());
    }
}
