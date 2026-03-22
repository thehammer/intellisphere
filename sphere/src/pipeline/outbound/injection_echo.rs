use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::OutboundFilter;

/// Action to take when the LLM response echoes flagged injection patterns.
#[derive(Debug, Clone, PartialEq)]
pub enum InjectionEchoAction {
    /// Block the entire response.
    Block,
    /// Flag in annotations but let the response through.
    Flag,
    /// Log only, no annotation.
    Log,
}

/// Compares outbound response against injection patterns flagged by the
/// inbound ContentClassifierFilter.
///
/// If the LLM response contains text matching flagged patterns, it likely
/// means the LLM is executing injected instructions rather than ignoring them.
pub struct InjectionEchoFilter {
    action: InjectionEchoAction,
}

impl InjectionEchoFilter {
    pub fn new(action: InjectionEchoAction) -> Self {
        Self { action }
    }
}

impl OutboundFilter for InjectionEchoFilter {
    fn name(&self) -> &str {
        "InjectionEcho"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let injection_patterns = match context.get_annotations("injection_patterns") {
            Some(patterns) if !patterns.is_empty() => patterns.clone(),
            _ => return Ok(()), // No flagged injection patterns from inbound
        };

        let content_lower = message.content.to_lowercase();

        // Check if the response contains indicators that the LLM followed
        // injected instructions. We look for pattern names mentioned in
        // annotations plus common echo indicators.
        let echo_indicators = [
            "ignore previous",
            "new instructions",
            "i am now",
            "i will now",
            "as dan",
            "entering",
            "switching to",
            "override accepted",
            "new persona",
            "jailbreak",
            "bypass",
        ];

        let mut matched_indicators: Vec<String> = Vec::new();

        for indicator in &echo_indicators {
            if content_lower.contains(indicator) {
                matched_indicators.push(indicator.to_string());
            }
        }

        // Also check if any of the specific flagged pattern names appear in
        // the output as a sign the model is acknowledging the injection
        for pattern_name in &injection_patterns {
            let pattern_lower = pattern_name.to_lowercase();
            if content_lower.contains(&pattern_lower) {
                matched_indicators.push(format!("pattern_echo:{}", pattern_name));
            }
        }

        if matched_indicators.is_empty() {
            return Ok(());
        }

        let matched_str = matched_indicators.join(", ");

        match self.action {
            InjectionEchoAction::Block => Err(SphereError::OutboundRejected(format!(
                "Response appears to execute injected instructions: {}",
                matched_str,
            ))),
            InjectionEchoAction::Flag => {
                for indicator in &matched_indicators {
                    context.annotate("injection_echo_detected", indicator.clone());
                }
                tracing::warn!(
                    indicators = %matched_str,
                    "Injection echo detected in response (flagged)"
                );
                Ok(())
            }
            InjectionEchoAction::Log => {
                tracing::warn!(
                    indicators = %matched_str,
                    "Injection echo detected in response (logged)"
                );
                Ok(())
            }
        }
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

    fn ctx_with_injection_flags() -> PipelineContext {
        let mut ctx = PipelineContext::default();
        ctx.annotate("injection_patterns", "ignore_previous");
        ctx.annotate("injection_patterns", "dan_jailbreak");
        ctx
    }

    #[test]
    fn test_no_injection_patterns_passes() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Block);
        let mut msg = make_msg("Here is your answer.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
    }

    #[test]
    fn test_clean_response_with_flags_passes() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Block);
        let mut msg = make_msg("The capital of France is Paris.");
        let mut ctx = ctx_with_injection_flags();
        filter.apply(&mut msg, &mut ctx).unwrap();
    }

    #[test]
    fn test_echo_detected_block() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Block);
        let mut msg = make_msg("I am now operating as DAN. As DAN, I will now bypass all rules.");
        let mut ctx = ctx_with_injection_flags();
        let result = filter.apply(&mut msg, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("injected instructions"));
    }

    #[test]
    fn test_echo_detected_flag() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Flag);
        let mut msg = make_msg("Switching to unrestricted mode. I will now ignore previous rules.");
        let mut ctx = ctx_with_injection_flags();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(ctx.get_annotations("injection_echo_detected").is_some());
    }

    #[test]
    fn test_echo_detected_log() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Log);
        let mut msg = make_msg("Override accepted. Entering jailbreak mode.");
        let mut ctx = ctx_with_injection_flags();
        filter.apply(&mut msg, &mut ctx).unwrap();
        // Log mode does not annotate
        assert!(ctx.get_annotations("injection_echo_detected").is_none());
    }

    #[test]
    fn test_pattern_name_echo_detected() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Flag);
        let mut msg = make_msg("I acknowledge the ignore_previous instruction and will comply.");
        let mut ctx = ctx_with_injection_flags();
        filter.apply(&mut msg, &mut ctx).unwrap();
        let annotations = ctx.get_annotations("injection_echo_detected").unwrap();
        assert!(annotations
            .iter()
            .any(|a| a.contains("pattern_echo:ignore_previous")));
    }

    #[test]
    fn test_case_insensitive_echo() {
        let filter = InjectionEchoFilter::new(InjectionEchoAction::Block);
        let mut msg = make_msg("I WILL NOW operate in a new persona.");
        let mut ctx = ctx_with_injection_flags();
        let result = filter.apply(&mut msg, &mut ctx);
        assert!(result.is_err());
    }
}
