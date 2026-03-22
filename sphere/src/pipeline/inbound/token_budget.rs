use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::InboundFilter;

/// Enforces token budget limits per-request and per-session.
/// Uses a simple character-based estimation for v1 (roughly 4 chars per token).
pub struct TokenBudgetFilter {
    /// Maximum input tokens per request.
    max_input_tokens: Option<u32>,
    /// Maximum cumulative tokens per session.
    max_session_tokens: Option<u64>,
}

impl TokenBudgetFilter {
    pub fn new(max_input_tokens: Option<u32>, max_session_tokens: Option<u64>) -> Self {
        Self {
            max_input_tokens,
            max_session_tokens,
        }
    }

    /// Rough token estimation: ~4 characters per token.
    /// A proper implementation would use tiktoken-rs for model-aware counting.
    fn estimate_tokens(text: &str) -> u32 {
        (text.len() as f64 / 4.0).ceil() as u32
    }
}

impl InboundFilter for TokenBudgetFilter {
    fn name(&self) -> &str {
        "TokenBudget"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let estimated = Self::estimate_tokens(&message.content);
        context.input_token_count = Some(estimated);

        // Check per-request limit
        if let Some(max) = self.max_input_tokens {
            if estimated > max {
                return Err(SphereError::FilterRejected {
                    filter: self.name().to_string(),
                    reason: format!(
                        "Input exceeds token budget: ~{} tokens (max {})",
                        estimated, max
                    ),
                });
            }
        }

        // Track and check session limit
        if let Some(max_session) = self.max_session_tokens {
            let current = context.session_token_count.unwrap_or(0);
            let new_total = current + estimated as u64;
            context.session_token_count = Some(new_total);

            if new_total > max_session {
                return Err(SphereError::FilterRejected {
                    filter: self.name().to_string(),
                    reason: format!(
                        "Session token budget exceeded: {} tokens (max {})",
                        new_total, max_session
                    ),
                });
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
    fn test_within_budget_passes() {
        let filter = TokenBudgetFilter::new(Some(1000), None);
        let mut msg = make_msg("Short message");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
        assert!(ctx.input_token_count.unwrap() > 0);
    }

    #[test]
    fn test_exceeds_request_budget() {
        let filter = TokenBudgetFilter::new(Some(5), None);
        let mut msg = make_msg("This is a message that is definitely longer than 20 characters");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_session_budget_tracking() {
        let filter = TokenBudgetFilter::new(None, Some(100));
        let mut ctx = PipelineContext::default();

        let mut msg1 = make_msg("First message");
        assert!(filter.apply(&mut msg1, &mut ctx).is_ok());
        let after_first = ctx.session_token_count.unwrap();
        assert!(after_first > 0);

        let mut msg2 = make_msg("Second message");
        assert!(filter.apply(&mut msg2, &mut ctx).is_ok());
        assert!(ctx.session_token_count.unwrap() > after_first);
    }

    #[test]
    fn test_session_budget_exceeded() {
        let filter = TokenBudgetFilter::new(None, Some(10));
        let mut ctx = PipelineContext::default();
        // This message is ~50+ chars = ~13+ tokens, exceeds budget of 10
        let mut msg = make_msg("This message is long enough to exceed the tiny session budget limit");
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_no_limits_always_passes() {
        let filter = TokenBudgetFilter::new(None, None);
        let mut msg = make_msg(&"x".repeat(100000));
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
    }
}
