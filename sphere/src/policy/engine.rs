use crate::config::PolicyConfig;
use crate::pipeline::PipelineContext;

use super::types::*;

/// The policy engine evaluates policies against pipeline context and events.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policies: Vec<Policy>,
}

impl PolicyEngine {
    /// Create a new empty policy engine.
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
        }
    }

    /// Load policies from configuration.
    pub fn load_from_config(configs: &[PolicyConfig]) -> Self {
        let policies = configs
            .iter()
            .map(|cfg| Policy {
                name: cfg.name.clone(),
                scope: cfg.scope.clone(),
                rules: cfg.rules.clone(),
                action: cfg.action.clone(),
            })
            .collect();

        Self { policies }
    }

    /// Evaluate all applicable policies against the given context and event.
    ///
    /// Returns a list of violations (empty if everything passes).
    pub fn evaluate(&self, context: &PipelineContext, event: &PolicyEvent) -> Vec<PolicyViolation> {
        let mut violations = Vec::new();

        for policy in &self.policies {
            if !Self::scope_matches(&policy.scope, context, event) {
                continue;
            }

            for rule in &policy.rules {
                if let Some(reason) = Self::evaluate_rule(rule, context, event) {
                    violations.push(PolicyViolation {
                        policy_name: policy.name.clone(),
                        action: policy.action.clone(),
                        reason,
                    });
                }
            }
        }

        violations
    }

    /// Check whether the policy scope matches the current event and context.
    fn scope_matches(scope: &PolicyScope, _context: &PipelineContext, event: &PolicyEvent) -> bool {
        match scope {
            PolicyScope::Global => true,
            PolicyScope::Session => true, // session policies always apply within a session
            PolicyScope::Identity => true, // identity policies always apply when identity present
            PolicyScope::Tool { name } => {
                matches!(event, PolicyEvent::ToolCallRequested { tool_name } if tool_name == name)
            }
        }
    }

    /// Evaluate a single rule against the context and event.
    /// Returns `Some(reason)` if the rule is violated, `None` otherwise.
    fn evaluate_rule(
        rule: &PolicyRule,
        context: &PipelineContext,
        event: &PolicyEvent,
    ) -> Option<String> {
        match rule {
            PolicyRule::MaxToolCallsPerSession { limit } => {
                if let PolicyEvent::ToolCallRequested { .. } | PolicyEvent::SessionUpdate = event {
                    if let Some(turn_count) = context.turn_count {
                        if turn_count > *limit {
                            return Some(format!(
                                "Tool call count {} exceeds session limit of {}",
                                turn_count, limit
                            ));
                        }
                    }
                }
                None
            }

            PolicyRule::MaxTokensPerIdentityPerHour { limit } => {
                if let Some(session_tokens) = context.session_token_count {
                    if session_tokens > *limit {
                        return Some(format!(
                            "Token count {} exceeds hourly limit of {}",
                            session_tokens, limit
                        ));
                    }
                }
                None
            }

            PolicyRule::RequireMfa => {
                if let Some(ref identity) = context.identity {
                    let has_mfa = identity
                        .metadata
                        .get("mfa_verified")
                        .map(|v| v == "true")
                        .unwrap_or(false);
                    if !has_mfa {
                        return Some("MFA is required but not verified".to_string());
                    }
                } else {
                    return Some("MFA is required but no identity present".to_string());
                }
                None
            }

            PolicyRule::RequireRoles { roles } => {
                if let Some(ref identity) = context.identity {
                    let missing: Vec<&str> = roles
                        .iter()
                        .filter(|r| !identity.roles.contains(r))
                        .map(|r| r.as_str())
                        .collect();
                    if !missing.is_empty() {
                        return Some(format!("Missing required roles: {}", missing.join(", ")));
                    }
                } else {
                    return Some("Roles required but no identity present".to_string());
                }
                None
            }

            PolicyRule::MaxResultSize { limit } => {
                // Result size is typically checked during outbound processing.
                // Here we check annotations if a filter has recorded the result size.
                if let Some(sizes) = context.get_annotations("result_size") {
                    if let Some(size_str) = sizes.last() {
                        if let Ok(size) = size_str.parse::<usize>() {
                            if size > *limit {
                                return Some(format!(
                                    "Result size {} exceeds limit of {}",
                                    size, limit
                                ));
                            }
                        }
                    }
                }
                None
            }
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Identity;
    use std::collections::HashMap;

    fn make_context() -> PipelineContext {
        PipelineContext::new("req-1".into(), "sess-1".into())
    }

    fn make_identity(roles: Vec<String>, mfa: bool) -> Identity {
        let mut metadata = HashMap::new();
        if mfa {
            metadata.insert("mfa_verified".to_string(), "true".to_string());
        }
        Identity {
            sub: "user-1".to_string(),
            roles,
            scopes: vec![],
            metadata,
        }
    }

    #[test]
    fn test_empty_engine_no_violations() {
        let engine = PolicyEngine::new();
        let ctx = make_context();
        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_max_tool_calls_within_limit() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "tool-limit".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::MaxToolCallsPerSession { limit: 10 }],
            action: PolicyAction::Block,
        }]);

        let mut ctx = make_context();
        ctx.turn_count = Some(5);

        let violations = engine.evaluate(
            &ctx,
            &PolicyEvent::ToolCallRequested {
                tool_name: "test".into(),
            },
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn test_max_tool_calls_exceeded() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "tool-limit".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::MaxToolCallsPerSession { limit: 5 }],
            action: PolicyAction::Block,
        }]);

        let mut ctx = make_context();
        ctx.turn_count = Some(6);

        let violations = engine.evaluate(
            &ctx,
            &PolicyEvent::ToolCallRequested {
                tool_name: "test".into(),
            },
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].policy_name, "tool-limit");
        assert_eq!(violations[0].action, PolicyAction::Block);
    }

    #[test]
    fn test_max_tokens_exceeded() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "token-limit".into(),
            scope: PolicyScope::Identity,
            rules: vec![PolicyRule::MaxTokensPerIdentityPerHour { limit: 1000 }],
            action: PolicyAction::Throttle,
        }]);

        let mut ctx = make_context();
        ctx.session_token_count = Some(1500);

        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].action, PolicyAction::Throttle);
    }

    #[test]
    fn test_require_mfa_passes() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "mfa-required".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::RequireMfa],
            action: PolicyAction::Block,
        }]);

        let mut ctx = make_context();
        ctx.identity = Some(make_identity(vec![], true));

        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_require_mfa_fails_no_mfa() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "mfa-required".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::RequireMfa],
            action: PolicyAction::Block,
        }]);

        let mut ctx = make_context();
        ctx.identity = Some(make_identity(vec![], false));

        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].reason.contains("MFA"));
    }

    #[test]
    fn test_require_mfa_fails_no_identity() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "mfa-required".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::RequireMfa],
            action: PolicyAction::Block,
        }]);

        let ctx = make_context();
        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].reason.contains("no identity"));
    }

    #[test]
    fn test_require_roles_passes() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "admin-only".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::RequireRoles {
                roles: vec!["admin".into()],
            }],
            action: PolicyAction::Block,
        }]);

        let mut ctx = make_context();
        ctx.identity = Some(make_identity(vec!["admin".into()], false));

        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_require_roles_missing() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "admin-only".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::RequireRoles {
                roles: vec!["admin".into(), "superuser".into()],
            }],
            action: PolicyAction::Block,
        }]);

        let mut ctx = make_context();
        ctx.identity = Some(make_identity(vec!["admin".into()], false));

        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].reason.contains("superuser"));
    }

    #[test]
    fn test_tool_scope_matches_correct_tool() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "tool-specific".into(),
            scope: PolicyScope::Tool {
                name: "dangerous_tool".into(),
            },
            rules: vec![PolicyRule::RequireRoles {
                roles: vec!["admin".into()],
            }],
            action: PolicyAction::Block,
        }]);

        let ctx = make_context();

        // Should not match a different tool
        let violations = engine.evaluate(
            &ctx,
            &PolicyEvent::ToolCallRequested {
                tool_name: "safe_tool".into(),
            },
        );
        assert!(violations.is_empty());

        // Should match the targeted tool
        let violations = engine.evaluate(
            &ctx,
            &PolicyEvent::ToolCallRequested {
                tool_name: "dangerous_tool".into(),
            },
        );
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_max_result_size_exceeded() {
        let engine = PolicyEngine::load_from_config(&[PolicyConfig {
            name: "size-limit".into(),
            scope: PolicyScope::Global,
            rules: vec![PolicyRule::MaxResultSize { limit: 1000 }],
            action: PolicyAction::Flag,
        }]);

        let mut ctx = make_context();
        ctx.annotate("result_size", "2000");

        let violations = engine.evaluate(&ctx, &PolicyEvent::SessionUpdate);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].action, PolicyAction::Flag);
    }

    #[test]
    fn test_multiple_policies_multiple_violations() {
        let engine = PolicyEngine::load_from_config(&[
            PolicyConfig {
                name: "tool-limit".into(),
                scope: PolicyScope::Global,
                rules: vec![PolicyRule::MaxToolCallsPerSession { limit: 5 }],
                action: PolicyAction::Block,
            },
            PolicyConfig {
                name: "mfa-required".into(),
                scope: PolicyScope::Global,
                rules: vec![PolicyRule::RequireMfa],
                action: PolicyAction::TerminateSession,
            },
        ]);

        let mut ctx = make_context();
        ctx.turn_count = Some(10);
        // No identity → MFA check also fails

        let violations = engine.evaluate(
            &ctx,
            &PolicyEvent::ToolCallRequested {
                tool_name: "test".into(),
            },
        );
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_load_from_empty_config() {
        let engine = PolicyEngine::load_from_config(&[]);
        let ctx = make_context();
        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert!(violations.is_empty());
    }
}
