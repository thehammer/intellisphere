use serde::{Deserialize, Serialize};

use crate::errors::SphereError;
use crate::pipeline::PipelineChain;
use crate::pipeline::PipelineContext;
use crate::proto::intellisphere::v1::Message;
use crate::satellite::session::SatelliteSession;
use crate::satellite::trust_budget::{
    SUSPICION_ADJUDICATION_FAILURE, SUSPICION_INJECTION, SUSPICION_OVERSIZED,
    SUSPICION_UNREGISTERED_TOOL,
};
use crate::tools::ToolRegistry;

/// Maximum allowed result payload size in bytes (256 KiB).
const MAX_RESULT_BYTES: usize = 256 * 1024;

/// Cost deducted from the trust budget per tool proposal.
const DEFAULT_TRUST_COST: f64 = 1.0;

/// A tool proposal submitted by a satellite for adjudication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProposal {
    /// The tool name being invoked.
    pub tool_name: String,
    /// The result payload from the satellite execution.
    pub result_payload: String,
}

/// The outcome of adjudicating a tool proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjudicationResult {
    /// Whether the result was accepted.
    pub accepted: bool,
    /// Reason for rejection, if any.
    pub reason: Option<String>,
}

impl AdjudicationResult {
    fn accepted() -> Self {
        Self {
            accepted: true,
            reason: None,
        }
    }

    fn rejected(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason: Some(reason.into()),
        }
    }
}

/// Validates tool proposal results from satellite edge nodes.
///
/// The adjudication pipeline:
/// 1. Deduct trust budget for the proposal
/// 2. Validate the tool is registered in the Satellite zone
/// 3. Validate the result payload size
/// 4. Run the result through the inbound pipeline filters
pub struct Adjudicator;

impl Adjudicator {
    /// Adjudicate a tool proposal from a satellite session.
    ///
    /// Mutates the session's trust budget and suspicion score as needed.
    /// Returns an [`AdjudicationResult`] indicating acceptance or rejection.
    pub fn adjudicate(
        session: &mut SatelliteSession,
        proposal: &ToolProposal,
        tool_registry: &ToolRegistry,
        pipeline: &PipelineChain,
    ) -> AdjudicationResult {
        // Step 1: Deduct trust budget
        if !session.trust_budget.deduct(DEFAULT_TRUST_COST) {
            return AdjudicationResult::rejected("trust budget exhausted");
        }

        // Step 2: Validate tool is registered
        match tool_registry.get(&proposal.tool_name) {
            Some(reg) => {
                if reg.zone != crate::tools::ToolZone::Satellite {
                    session
                        .trust_budget
                        .add_suspicion(SUSPICION_UNREGISTERED_TOOL);
                    return AdjudicationResult::rejected(format!(
                        "tool '{}' is not in the Satellite zone",
                        proposal.tool_name
                    ));
                }
            }
            None => {
                session
                    .trust_budget
                    .add_suspicion(SUSPICION_UNREGISTERED_TOOL);
                return AdjudicationResult::rejected(format!(
                    "tool '{}' is not registered",
                    proposal.tool_name
                ));
            }
        }

        // Step 3: Validate result size
        if proposal.result_payload.len() > MAX_RESULT_BYTES {
            session.trust_budget.add_suspicion(SUSPICION_OVERSIZED);
            return AdjudicationResult::rejected(format!(
                "result payload exceeds maximum size ({} > {} bytes)",
                proposal.result_payload.len(),
                MAX_RESULT_BYTES
            ));
        }

        // Step 4: Run through inbound pipeline
        let mut message = Message {
            role: "tool".to_string(),
            content: proposal.result_payload.clone(),
            tool_calls: vec![],
            tool_results: vec![],
        };
        let mut ctx = PipelineContext::new(
            uuid::Uuid::new_v4().to_string(),
            session.session_id.clone(),
        );

        match pipeline.run_inbound(&mut message, &mut ctx) {
            Ok(()) => {}
            Err(SphereError::FilterRejected { filter, reason }) => {
                // Injection-like rejections get a higher suspicion bump
                if filter.contains("sanitiz") {
                    session.trust_budget.add_suspicion(SUSPICION_INJECTION);
                } else {
                    session
                        .trust_budget
                        .add_suspicion(SUSPICION_ADJUDICATION_FAILURE);
                }
                return AdjudicationResult::rejected(format!(
                    "inbound filter '{}' rejected: {}",
                    filter, reason
                ));
            }
            Err(e) => {
                session
                    .trust_budget
                    .add_suspicion(SUSPICION_ADJUDICATION_FAILURE);
                return AdjudicationResult::rejected(format!("pipeline error: {}", e));
            }
        }

        AdjudicationResult::accepted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PipelineConfig;
    use crate::satellite::trust_budget::TrustBudget;
    use crate::tools::{ToolRegistration, ToolZone};
    use chrono::{Duration, Utc};

    fn make_session(budget: f64) -> SatelliteSession {
        SatelliteSession {
            session_id: "test-session".into(),
            identity_sub: "test-user".into(),
            trust_budget: TrustBudget::new(budget, 1.0),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        }
    }

    fn make_registry_with_satellite_tool() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(ToolRegistration {
            name: "sat_tool".into(),
            description: "A satellite tool".into(),
            input_schema_json: "{}".into(),
            zone: ToolZone::Satellite,
            required_scopes: vec![],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        })
        .unwrap();
        reg
    }

    fn make_registry_with_sphere_tool() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(ToolRegistration {
            name: "sphere_tool".into(),
            description: "A sphere-zone tool".into(),
            input_schema_json: "{}".into(),
            zone: ToolZone::Sphere,
            required_scopes: vec![],
            required_roles: vec![],
            handler_url: None,
            timeout_ms: 5000,
        })
        .unwrap();
        reg
    }

    fn make_pipeline() -> PipelineChain {
        PipelineChain::from_config(&PipelineConfig::default())
    }

    #[test]
    fn accepts_valid_proposal() {
        let mut session = make_session(10.0);
        let registry = make_registry_with_satellite_tool();
        let pipeline = make_pipeline();

        let proposal = ToolProposal {
            tool_name: "sat_tool".into(),
            result_payload: "hello world".into(),
        };

        let result = Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        assert!(result.accepted);
        assert!(result.reason.is_none());
        // Trust budget should have been deducted
        assert!((session.trust_budget.remaining - 9.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_when_budget_exhausted() {
        let mut session = make_session(0.5);
        let registry = make_registry_with_satellite_tool();
        let pipeline = make_pipeline();

        let proposal = ToolProposal {
            tool_name: "sat_tool".into(),
            result_payload: "hello".into(),
        };

        let result = Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        assert!(!result.accepted);
        assert_eq!(result.reason.as_deref(), Some("trust budget exhausted"));
    }

    #[test]
    fn rejects_unregistered_tool() {
        let mut session = make_session(10.0);
        let registry = ToolRegistry::new();
        let pipeline = make_pipeline();

        let proposal = ToolProposal {
            tool_name: "unknown_tool".into(),
            result_payload: "data".into(),
        };

        let result = Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        assert!(!result.accepted);
        assert!(result.reason.as_deref().unwrap().contains("not registered"));
        assert!(session.trust_budget.suspicion_score > 0.0);
    }

    #[test]
    fn rejects_tool_in_wrong_zone() {
        let mut session = make_session(10.0);
        let registry = make_registry_with_sphere_tool();
        let pipeline = make_pipeline();

        let proposal = ToolProposal {
            tool_name: "sphere_tool".into(),
            result_payload: "data".into(),
        };

        let result = Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        assert!(!result.accepted);
        assert!(result
            .reason
            .as_deref()
            .unwrap()
            .contains("not in the Satellite zone"));
    }

    #[test]
    fn rejects_oversized_payload() {
        let mut session = make_session(10.0);
        let registry = make_registry_with_satellite_tool();
        let pipeline = make_pipeline();

        let big_payload = "x".repeat(MAX_RESULT_BYTES + 1);
        let proposal = ToolProposal {
            tool_name: "sat_tool".into(),
            result_payload: big_payload,
        };

        let result = Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        assert!(!result.accepted);
        assert!(result
            .reason
            .as_deref()
            .unwrap()
            .contains("exceeds maximum size"));
    }

    #[test]
    fn suspicion_accumulates_across_rejections() {
        let mut session = make_session(100.0);
        let registry = ToolRegistry::new();
        let pipeline = make_pipeline();

        // Each unregistered tool adds 0.3 suspicion
        for _ in 0..3 {
            let proposal = ToolProposal {
                tool_name: "bad_tool".into(),
                result_payload: "x".into(),
            };
            Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        }

        assert!((session.trust_budget.suspicion_score - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn inbound_pipeline_rejection_with_null_bytes() {
        let mut session = make_session(10.0);
        let registry = make_registry_with_satellite_tool();
        let pipeline = make_pipeline();

        let proposal = ToolProposal {
            tool_name: "sat_tool".into(),
            result_payload: "hello\0world".into(),
        };

        let result = Adjudicator::adjudicate(&mut session, &proposal, &registry, &pipeline);
        // The default pipeline has input sanitization enabled with reject_null_bytes
        assert!(!result.accepted);
    }
}
