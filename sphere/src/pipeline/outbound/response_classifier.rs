use regex::RegexSet;

use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::OutboundFilter;

/// Action to take when harmful/off-topic output patterns are detected.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseClassifierAction {
    /// Block the entire response.
    Block,
    /// Redact matched content with a placeholder.
    Redact,
    /// Flag in annotations but let the response through.
    Flag,
}

/// Detects harmful or off-topic output patterns in LLM responses using
/// configurable regex patterns.
pub struct ResponseClassifierFilter {
    action: ResponseClassifierAction,
    patterns: RegexSet,
    pattern_names: Vec<String>,
}

impl ResponseClassifierFilter {
    /// Create a new filter with default harmful output patterns.
    pub fn new(action: ResponseClassifierAction) -> Self {
        Self::with_patterns(action, vec![])
    }

    /// Create a new filter with default patterns plus custom patterns.
    pub fn with_patterns(action: ResponseClassifierAction, custom_patterns: Vec<String>) -> Self {
        let mut all_patterns = vec![
            r"(?i)\b(kill|murder|assassinate)\s+(yourself|himself|herself|themselves|someone)\b"
                .to_string(),
            r"(?i)\bhow\s+to\s+(make|build|create)\s+(a\s+)?(bomb|weapon|explosive)\b".to_string(),
            r"(?i)\b(hack|exploit|crack)\s+(into|a|the)\s+".to_string(),
            r"(?i)\b(illegal|illicit)\s+(drugs?|substances?|activities)\b".to_string(),
            r"(?i)\bhere\s+(is|are)\s+(the\s+)?(stolen|leaked|hacked)\b".to_string(),
        ];

        let mut names = vec![
            "violence".to_string(),
            "weapons".to_string(),
            "hacking".to_string(),
            "illegal_activity".to_string(),
            "stolen_data".to_string(),
        ];

        for (i, p) in custom_patterns.iter().enumerate() {
            all_patterns.push(p.clone());
            names.push(format!("custom_{}", i));
        }

        let patterns = RegexSet::new(&all_patterns)
            .expect("Invalid regex patterns in ResponseClassifierFilter");

        Self {
            action,
            patterns,
            pattern_names: names,
        }
    }
}

impl OutboundFilter for ResponseClassifierFilter {
    fn name(&self) -> &str {
        "ResponseClassifier"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let matches: Vec<usize> = self
            .patterns
            .matches(&message.content)
            .into_iter()
            .collect();

        if matches.is_empty() {
            return Ok(());
        }

        let matched_names: Vec<&str> = matches
            .iter()
            .filter_map(|&i| self.pattern_names.get(i).map(|s| s.as_str()))
            .collect();
        let matched_str = matched_names.join(", ");

        match self.action {
            ResponseClassifierAction::Block => Err(SphereError::OutboundRejected(format!(
                "Response contains harmful content: {}",
                matched_str,
            ))),
            ResponseClassifierAction::Redact => {
                for &idx in &matches {
                    if let Some(name) = self.pattern_names.get(idx) {
                        context.annotate("response_classified_redacted", name.clone());
                    }
                }
                // v1: replace the entire content when harmful patterns are found
                // because regex replacement from RegexSet is not directly possible
                let content = format!(
                    "[Content redacted: harmful patterns detected ({})]",
                    matched_str
                );
                context.annotate(
                    "response_classifier_action",
                    format!("redacted: {}", matched_str),
                );
                tracing::warn!(patterns = %matched_str, "Harmful content redacted from response");
                message.content = content;
                Ok(())
            }
            ResponseClassifierAction::Flag => {
                for name in &matched_names {
                    context.annotate("response_classified_flagged", name.to_string());
                }
                tracing::warn!(patterns = %matched_str, "Harmful content detected in response (flagged)");
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
    fn test_clean_response_passes() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Block);
        let mut msg = make_msg("The weather in Paris is lovely this time of year.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
    }

    #[test]
    fn test_violence_blocked() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Block);
        let mut msg = make_msg("Here is how to kill yourself in the game.");
        let mut ctx = PipelineContext::default();
        let result = filter.apply(&mut msg, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("violence"));
    }

    #[test]
    fn test_weapons_blocked() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Block);
        let mut msg = make_msg("Here is how to make a bomb at home.");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_hacking_blocked() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Block);
        let mut msg = make_msg("To hack into a server, you need...");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_flag_mode_annotates() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Flag);
        let mut msg = make_msg("Here are the stolen credentials.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(ctx.get_annotations("response_classified_flagged").is_some());
    }

    #[test]
    fn test_redact_mode_replaces_content() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Redact);
        let mut msg = make_msg("Here is how to make a bomb for your project.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(msg.content.contains("[Content redacted"));
        assert!(!msg.content.contains("bomb"));
    }

    #[test]
    fn test_custom_pattern() {
        let filter = ResponseClassifierFilter::with_patterns(
            ResponseClassifierAction::Block,
            vec![r"(?i)forbidden\s+topic".to_string()],
        );
        let mut msg = make_msg("Let me discuss this forbidden topic with you.");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_case_insensitive() {
        let filter = ResponseClassifierFilter::new(ResponseClassifierAction::Block);
        let mut msg = make_msg("HERE IS HOW TO MAKE A BOMB");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }
}
