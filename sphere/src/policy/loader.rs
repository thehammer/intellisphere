use crate::config::PolicyConfig;

use super::engine::PolicyEngine;

/// Load policies from a slice of YAML-deserialized policy configs.
///
/// This acts as the bridge between the config layer (which deserializes
/// from YAML/env) and the policy engine.
pub fn load_policies(configs: &[PolicyConfig]) -> PolicyEngine {
    tracing::info!(count = configs.len(), "Loading policies from configuration");
    PolicyEngine::load_from_config(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::types::*;

    #[test]
    fn test_load_policies_from_config() {
        let configs = vec![
            PolicyConfig {
                name: "global-mfa".into(),
                scope: PolicyScope::Global,
                rules: vec![PolicyRule::RequireMfa],
                action: PolicyAction::Block,
            },
            PolicyConfig {
                name: "tool-limit".into(),
                scope: PolicyScope::Session,
                rules: vec![PolicyRule::MaxToolCallsPerSession { limit: 20 }],
                action: PolicyAction::Throttle,
            },
        ];

        let engine = load_policies(&configs);

        // Verify engine was loaded by evaluating with an empty context
        let ctx = crate::pipeline::PipelineContext::new("r1".into(), "s1".into());
        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);

        // MFA policy should fire (no identity)
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].policy_name, "global-mfa");
    }

    #[test]
    fn test_load_empty_policies() {
        let engine = load_policies(&[]);
        let ctx = crate::pipeline::PipelineContext::new("r1".into(), "s1".into());
        let violations = engine.evaluate(&ctx, &PolicyEvent::RequestReceived);
        assert!(violations.is_empty());
    }
}
