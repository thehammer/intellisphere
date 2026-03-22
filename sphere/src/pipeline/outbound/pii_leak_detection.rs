use regex::Regex;

use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::OutboundFilter;

/// Action to take when PII is detected in outbound responses.
#[derive(Debug, Clone, PartialEq)]
pub enum PIILeakAction {
    /// Replace detected PII with placeholders in the output.
    Redact,
    /// Block the entire response.
    Block,
    /// Flag in annotations but let the response through.
    Flag,
}

struct PiiPattern {
    name: &'static str,
    regex: Regex,
    mask_label: &'static str,
}

/// Detects PII that the LLM may have generated in its response.
/// Uses the same entity patterns as the inbound PII redaction filter.
pub struct PIILeakDetectionFilter {
    action: PIILeakAction,
    patterns: Vec<PiiPattern>,
}

impl PIILeakDetectionFilter {
    pub fn new(action: PIILeakAction) -> Self {
        let patterns = vec![
            PiiPattern {
                name: "EMAIL",
                regex: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
                mask_label: "[EMAIL_REDACTED]",
            },
            PiiPattern {
                name: "PHONE",
                regex: Regex::new(
                    r"(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}",
                )
                .unwrap(),
                mask_label: "[PHONE_REDACTED]",
            },
            PiiPattern {
                name: "SSN",
                regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
                mask_label: "[SSN_REDACTED]",
            },
            PiiPattern {
                name: "CREDIT_CARD",
                regex: Regex::new(r"\b(?:\d[ -]*?){13,19}\b").unwrap(),
                mask_label: "[CREDIT_CARD_REDACTED]",
            },
        ];

        Self { action, patterns }
    }
}

impl OutboundFilter for PIILeakDetectionFilter {
    fn name(&self) -> &str {
        "PIILeakDetection"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let mut content = message.content.clone();
        let mut detected: Vec<String> = Vec::new();

        for pattern in &self.patterns {
            if pattern.regex.is_match(&content) {
                detected.push(pattern.name.to_string());

                if self.action == PIILeakAction::Redact {
                    content = pattern
                        .regex
                        .replace_all(&content, pattern.mask_label)
                        .to_string();
                }
            }
        }

        if detected.is_empty() {
            return Ok(());
        }

        let detected_str = detected.join(", ");

        match self.action {
            PIILeakAction::Redact => {
                for d in &detected {
                    context.annotate("outbound_pii_redacted", d.clone());
                }
                tracing::warn!(types = %detected_str, "PII detected and redacted in outbound response");
                message.content = content;
                Ok(())
            }
            PIILeakAction::Block => {
                Err(SphereError::OutboundRejected(format!(
                    "Response contains PII: {}",
                    detected_str,
                )))
            }
            PIILeakAction::Flag => {
                for d in &detected {
                    context.annotate("outbound_pii_flagged", d.clone());
                }
                tracing::warn!(types = %detected_str, "PII detected in outbound response (flagged)");
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

    #[test]
    fn test_no_pii_passes_through() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Flag);
        let mut msg = make_msg("The weather is sunny today.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "The weather is sunny today.");
        assert!(ctx.get_annotations("outbound_pii_flagged").is_none());
    }

    #[test]
    fn test_email_redacted() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Redact);
        let mut msg = make_msg("Contact john@example.com for details.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert_eq!(msg.content, "Contact [EMAIL_REDACTED] for details.");
        assert!(ctx
            .get_annotations("outbound_pii_redacted")
            .unwrap()
            .contains(&"EMAIL".to_string()));
    }

    #[test]
    fn test_ssn_redacted() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Redact);
        let mut msg = make_msg("SSN is 123-45-6789.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[SSN_REDACTED]"));
    }

    #[test]
    fn test_phone_redacted() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Redact);
        let mut msg = make_msg("Call (555) 123-4567.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[PHONE_REDACTED]"));
    }

    #[test]
    fn test_block_rejects_response() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Block);
        let mut msg = make_msg("Email me at leak@corp.com");
        let mut ctx = PipelineContext::default();
        let result = filter.apply(&mut msg, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("EMAIL"));
    }

    #[test]
    fn test_flag_annotates_context() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Flag);
        let mut msg = make_msg("SSN: 111-22-3333");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(ctx.get_annotations("outbound_pii_flagged").is_some());
        // Content should be unchanged in flag mode
        assert_eq!(msg.content, "SSN: 111-22-3333");
    }

    #[test]
    fn test_multiple_pii_types_redacted() {
        let filter = PIILeakDetectionFilter::new(PIILeakAction::Redact);
        let mut msg = make_msg("Email: a@b.com, SSN: 123-45-6789");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[EMAIL_REDACTED]"));
        assert!(msg.content.contains("[SSN_REDACTED]"));
        let annotations = ctx.get_annotations("outbound_pii_redacted").unwrap();
        assert!(annotations.contains(&"EMAIL".to_string()));
        assert!(annotations.contains(&"SSN".to_string()));
    }
}
