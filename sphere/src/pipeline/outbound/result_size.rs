use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::OutboundFilter;

const DEFAULT_MAX_CHARS: usize = 100_000;
const TRUNCATION_SUFFIX: &str = " [truncated]";

/// Enforces a maximum character limit on outbound responses.
/// If the response exceeds the limit, it is hard-truncated with a
/// "[truncated]" suffix.
pub struct ResultSizeEnforcementFilter {
    max_chars: usize,
}

impl ResultSizeEnforcementFilter {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }

    pub fn with_default() -> Self {
        Self {
            max_chars: DEFAULT_MAX_CHARS,
        }
    }
}

impl OutboundFilter for ResultSizeEnforcementFilter {
    fn name(&self) -> &str {
        "ResultSizeEnforcement"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let len = message.content.len();
        if len <= self.max_chars {
            return Ok(());
        }

        let truncate_at = self.max_chars.saturating_sub(TRUNCATION_SUFFIX.len());
        // Find the largest char boundary at or before truncate_at
        let mut boundary = truncate_at.min(message.content.len());
        while boundary > 0 && !message.content.is_char_boundary(boundary) {
            boundary -= 1;
        }

        let mut truncated = message.content[..boundary].to_string();
        truncated.push_str(TRUNCATION_SUFFIX);

        context.annotate(
            "result_truncated",
            format!("original_len={}, truncated_to={}", len, truncated.len()),
        );
        tracing::info!(
            original_len = len,
            max_chars = self.max_chars,
            "Response truncated to size limit"
        );

        message.content = truncated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(content: &str) -> Message {
        Message {
            role: "assistant".to_string(),
            content: content.to_string(),
            tool_results: vec![],
            tool_calls: vec![],
        }
    }

    #[test]
    fn test_short_response_passes_through() {
        let filter = ResultSizeEnforcementFilter::new(100);
        let mut msg = make_msg("Hello world");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "Hello world");
        assert!(ctx.get_annotations("result_truncated").is_none());
    }

    #[test]
    fn test_exact_limit_passes_through() {
        let filter = ResultSizeEnforcementFilter::new(5);
        let mut msg = make_msg("Hello");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_over_limit_truncated() {
        let filter = ResultSizeEnforcementFilter::new(30);
        let long_text = "a".repeat(100);
        let mut msg = make_msg(&long_text);
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.len() <= 30);
        assert!(msg.content.ends_with("[truncated]"));
        assert!(ctx.get_annotations("result_truncated").is_some());
    }

    #[test]
    fn test_truncation_with_multibyte_chars() {
        let filter = ResultSizeEnforcementFilter::new(20);
        // Each emoji is 4 bytes
        let content = "Hello \u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}";
        let mut msg = make_msg(content);
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.ends_with("[truncated]"));
        // Should not panic on multibyte boundary
    }

    #[test]
    fn test_default_constructor() {
        let filter = ResultSizeEnforcementFilter::with_default();
        assert_eq!(filter.max_chars, 100_000);
    }

    #[test]
    fn test_annotation_contains_lengths() {
        let filter = ResultSizeEnforcementFilter::new(25);
        let mut msg = make_msg(&"x".repeat(50));
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        let annotation = &ctx.get_annotations("result_truncated").unwrap()[0];
        assert!(annotation.contains("original_len=50"));
    }
}
