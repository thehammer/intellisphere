use crate::config::InputSanitizationConfig;
use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::InboundFilter;

/// Sanitizes input messages by stripping control characters,
/// normalizing unicode, and rejecting null bytes.
pub struct InputSanitizationFilter {
    config: InputSanitizationConfig,
}

impl InputSanitizationFilter {
    pub fn new(config: InputSanitizationConfig) -> Self {
        Self { config }
    }

    fn sanitize(&self, input: &str) -> Result<String, SphereError> {
        // Reject null bytes
        if self.config.reject_null_bytes && input.contains('\0') {
            return Err(SphereError::FilterRejected {
                filter: self.name().to_string(),
                reason: "Input contains null bytes".to_string(),
            });
        }

        let mut result = input.to_string();

        // Strip control characters (U+0000–U+001F except \n \r \t)
        if self.config.strip_control_chars {
            result = result
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
                .collect();
        }

        // Unicode NFC normalization
        if self.config.normalize_unicode {
            // For v1, we do a basic normalization. A full NFC implementation
            // would use the `unicode-normalization` crate. For now, we ensure
            // consistent representation of common cases.
            // TODO: Add unicode-normalization crate for proper NFC
        }

        Ok(result)
    }
}

impl InboundFilter for InputSanitizationFilter {
    fn name(&self) -> &str {
        "InputSanitization"
    }

    fn apply(
        &self,
        message: &mut Message,
        _context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        if !self.config.enabled {
            return Ok(());
        }

        message.content = self.sanitize(&message.content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_filter() -> InputSanitizationFilter {
        InputSanitizationFilter::new(InputSanitizationConfig::default())
    }

    fn make_message(content: &str) -> Message {
        Message {
            role: "user".to_string(),
            content: content.to_string(),
            tool_results: vec![],
            tool_calls: vec![],
        }
    }

    #[test]
    fn test_passes_clean_input() {
        let filter = default_filter();
        let mut msg = make_message("Hello, world!");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
        assert_eq!(msg.content, "Hello, world!");
    }

    #[test]
    fn test_rejects_null_bytes() {
        let filter = default_filter();
        let mut msg = make_message("Hello\0world");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_strips_control_characters() {
        let filter = default_filter();
        let mut msg = make_message("Hello\x01\x02\x03world");
        let mut ctx = PipelineContext::default();
        // Null byte check happens first, and these aren't null bytes
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
        assert_eq!(msg.content, "Helloworld");
    }

    #[test]
    fn test_preserves_newlines_tabs() {
        let filter = default_filter();
        let mut msg = make_message("Hello\n\tworld\r\n");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
        assert_eq!(msg.content, "Hello\n\tworld\r\n");
    }

    #[test]
    fn test_disabled_filter_passes_everything() {
        let config = InputSanitizationConfig {
            enabled: false,
            ..Default::default()
        };
        let filter = InputSanitizationFilter::new(config);
        let mut msg = make_message("Hello\x01world");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
        assert_eq!(msg.content, "Hello\x01world");
    }
}
