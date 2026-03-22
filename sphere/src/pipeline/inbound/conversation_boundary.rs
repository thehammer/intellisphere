use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::InboundFilter;

/// Enforces conversation boundary limits: max turns per session and
/// max context window tokens.
pub struct ConversationBoundaryFilter {
    /// Maximum number of turns (messages) per session.
    max_turns: Option<u32>,
    /// Maximum total context window tokens (triggers rejection).
    max_context_tokens: Option<u64>,
}

impl ConversationBoundaryFilter {
    pub fn new(max_turns: Option<u32>, max_context_tokens: Option<u64>) -> Self {
        Self {
            max_turns,
            max_context_tokens,
        }
    }
}

impl InboundFilter for ConversationBoundaryFilter {
    fn name(&self) -> &str {
        "ConversationBoundary"
    }

    fn apply(
        &self,
        _message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        // Check turn count
        if let Some(max) = self.max_turns {
            let current = context.turn_count.unwrap_or(0);
            let new_count = current + 1;
            context.turn_count = Some(new_count);

            if new_count > max {
                return Err(SphereError::FilterRejected {
                    filter: self.name().to_string(),
                    reason: format!(
                        "Session exceeded maximum turns: {} (max {})",
                        new_count, max
                    ),
                });
            }
        }

        // Check context window
        if let Some(max) = self.max_context_tokens {
            if let Some(session_tokens) = context.session_token_count {
                if session_tokens > max {
                    return Err(SphereError::FilterRejected {
                        filter: self.name().to_string(),
                        reason: format!(
                            "Context window exceeded: {} tokens (max {})",
                            session_tokens, max
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(content: &str) -> Message {
        Message {
            role: "user".to_string(),
            content: content.to_string(),
            tool_results: vec![],
            tool_calls: vec![],
        }
    }

    #[test]
    fn test_within_limits_passes() {
        let filter = ConversationBoundaryFilter::new(Some(10), None);
        let mut msg = make_msg("Hello");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
        assert_eq!(ctx.turn_count, Some(1));
    }

    #[test]
    fn test_max_turns_exceeded() {
        let filter = ConversationBoundaryFilter::new(Some(2), None);
        let mut ctx = PipelineContext::default();

        let mut msg1 = make_msg("Turn 1");
        assert!(filter.apply(&mut msg1, &mut ctx).is_ok());
        let mut msg2 = make_msg("Turn 2");
        assert!(filter.apply(&mut msg2, &mut ctx).is_ok());
        let mut msg3 = make_msg("Turn 3");
        assert!(filter.apply(&mut msg3, &mut ctx).is_err());
    }

    #[test]
    fn test_context_tokens_exceeded() {
        let filter = ConversationBoundaryFilter::new(None, Some(100));
        let mut ctx = PipelineContext::default();
        ctx.session_token_count = Some(200);

        let mut msg = make_msg("Hello");
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_no_limits_always_passes() {
        let filter = ConversationBoundaryFilter::new(None, None);
        let mut msg = make_msg("Hello");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
    }
}
