use regex::Regex;

use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::InboundFilter;

/// Strategy for handling detected PII.
#[derive(Debug, Clone)]
pub enum RedactionStrategy {
    /// Replace with placeholder like [EMAIL_REDACTED]
    Mask,
    /// Replace with safe equivalent
    Replace,
    /// Remove entirely
    Remove,
}

struct PiiPattern {
    name: &'static str,
    regex: Regex,
    mask_label: &'static str,
    replacement: &'static str,
}

/// Detects and redacts PII in input messages using regex patterns.
/// v1 is regex-only; the interface allows future swap to ML/NER.
pub struct PIIRedactionFilter {
    strategy: RedactionStrategy,
    patterns: Vec<PiiPattern>,
}

impl PIIRedactionFilter {
    pub fn new(strategy: RedactionStrategy) -> Self {
        let patterns = vec![
            PiiPattern {
                name: "EMAIL",
                regex: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
                mask_label: "[EMAIL_REDACTED]",
                replacement: "user@example.com",
            },
            PiiPattern {
                name: "PHONE",
                regex: Regex::new(r"(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").unwrap(),
                mask_label: "[PHONE_REDACTED]",
                replacement: "555-000-0000",
            },
            PiiPattern {
                name: "SSN",
                regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
                mask_label: "[SSN_REDACTED]",
                replacement: "000-00-0000",
            },
            PiiPattern {
                name: "CREDIT_CARD",
                regex: Regex::new(r"\b(?:\d[ -]*?){13,19}\b").unwrap(),
                mask_label: "[CREDIT_CARD_REDACTED]",
                replacement: "0000-0000-0000-0000",
            },
        ];

        Self { strategy, patterns }
    }
}

impl InboundFilter for PIIRedactionFilter {
    fn name(&self) -> &str {
        "PIIRedaction"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let mut content = message.content.clone();
        let mut detected_count = 0u32;

        for pattern in &self.patterns {
            let match_count = pattern.regex.find_iter(&content).count();
            if match_count > 0 {
                detected_count += match_count as u32;
                context.annotate("pii_detected", pattern.name.to_string());

                content = match self.strategy {
                    RedactionStrategy::Mask => pattern
                        .regex
                        .replace_all(&content, pattern.mask_label)
                        .to_string(),
                    RedactionStrategy::Replace => pattern
                        .regex
                        .replace_all(&content, pattern.replacement)
                        .to_string(),
                    RedactionStrategy::Remove => {
                        pattern.regex.replace_all(&content, "").to_string()
                    }
                };
            }
        }

        if detected_count > 0 {
            context.annotate("pii_entity_count", detected_count.to_string());
            tracing::info!(count = detected_count, "PII entities redacted from input");
        }

        message.content = content;
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
    fn test_no_pii_passes_through() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Mask);
        let mut msg = make_msg("Hello, how are you?");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "Hello, how are you?");
    }

    #[test]
    fn test_email_masked() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Mask);
        let mut msg = make_msg("Contact me at john@example.com please");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "Contact me at [EMAIL_REDACTED] please");
    }

    #[test]
    fn test_email_replaced() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Replace);
        let mut msg = make_msg("My email is john@example.com");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "My email is user@example.com");
    }

    #[test]
    fn test_email_removed() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Remove);
        let mut msg = make_msg("Email: john@example.com");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "Email: ");
    }

    #[test]
    fn test_ssn_masked() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Mask);
        let mut msg = make_msg("My SSN is 123-45-6789");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[SSN_REDACTED]"));
    }

    #[test]
    fn test_phone_masked() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Mask);
        let mut msg = make_msg("Call me at (555) 123-4567");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[PHONE_REDACTED]"));
    }

    #[test]
    fn test_multiple_pii_types() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Mask);
        let mut msg = make_msg("Email: test@test.com, SSN: 123-45-6789");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[EMAIL_REDACTED]"));
        assert!(msg.content.contains("[SSN_REDACTED]"));
        let annotations = ctx.get_annotations("pii_detected").unwrap();
        assert!(annotations.contains(&"EMAIL".to_string()));
        assert!(annotations.contains(&"SSN".to_string()));
    }

    #[test]
    fn test_context_annotated_with_count() {
        let filter = PIIRedactionFilter::new(RedactionStrategy::Mask);
        let mut msg = make_msg("a@b.com and c@d.com");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        let count = ctx.get_annotations("pii_entity_count").unwrap();
        assert_eq!(count[0], "2");
    }
}
