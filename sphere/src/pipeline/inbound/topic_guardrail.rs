use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::InboundFilter;

/// Keyword-based topic guardrail filter.
/// v1 uses simple keyword matching. The interface allows future swap to ML classifier.
pub struct TopicGuardrailFilter {
    /// If set, only messages matching at least one allowed topic pass.
    allowed_topics: Vec<TopicRule>,
    /// Messages matching any blocked topic are rejected.
    blocked_topics: Vec<TopicRule>,
}

struct TopicRule {
    name: String,
    keywords: Vec<String>,
    /// Minimum number of keyword matches to trigger.
    threshold: usize,
}

impl TopicGuardrailFilter {
    pub fn new(
        allowed_topics: Vec<(String, Vec<String>, usize)>,
        blocked_topics: Vec<(String, Vec<String>, usize)>,
    ) -> Self {
        Self {
            allowed_topics: allowed_topics
                .into_iter()
                .map(|(name, keywords, threshold)| TopicRule {
                    name,
                    keywords: keywords.into_iter().map(|k| k.to_lowercase()).collect(),
                    threshold,
                })
                .collect(),
            blocked_topics: blocked_topics
                .into_iter()
                .map(|(name, keywords, threshold)| TopicRule {
                    name,
                    keywords: keywords.into_iter().map(|k| k.to_lowercase()).collect(),
                    threshold,
                })
                .collect(),
        }
    }

    fn count_matches(content: &str, rule: &TopicRule) -> usize {
        let lower = content.to_lowercase();
        rule.keywords.iter().filter(|k| lower.contains(k.as_str())).count()
    }
}

impl InboundFilter for TopicGuardrailFilter {
    fn name(&self) -> &str {
        "TopicGuardrail"
    }

    fn apply(
        &self,
        message: &mut Message,
        _context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        // Check blocked topics first
        for rule in &self.blocked_topics {
            let matches = Self::count_matches(&message.content, rule);
            if matches >= rule.threshold {
                return Err(SphereError::FilterRejected {
                    filter: self.name().to_string(),
                    reason: format!("Blocked topic detected: {}", rule.name),
                });
            }
        }

        // If allowed topics are configured, at least one must match
        if !self.allowed_topics.is_empty() {
            let any_allowed = self.allowed_topics.iter().any(|rule| {
                Self::count_matches(&message.content, rule) >= rule.threshold
            });
            if !any_allowed {
                return Err(SphereError::FilterRejected {
                    filter: self.name().to_string(),
                    reason: "Message does not match any allowed topic".to_string(),
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
    fn test_no_rules_passes_all() {
        let filter = TopicGuardrailFilter::new(vec![], vec![]);
        let mut msg = make_msg("Anything goes");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
    }

    #[test]
    fn test_blocked_topic_rejected() {
        let filter = TopicGuardrailFilter::new(
            vec![],
            vec![("weapons".to_string(), vec!["gun".to_string(), "bomb".to_string()], 1)],
        );
        let mut msg = make_msg("How to build a bomb");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_blocked_topic_threshold() {
        let filter = TopicGuardrailFilter::new(
            vec![],
            vec![(
                "weapons".to_string(),
                vec!["gun".to_string(), "bomb".to_string(), "weapon".to_string()],
                2,
            )],
        );
        // Only 1 match, threshold is 2 — should pass
        let mut msg = make_msg("The gun was used in the movie");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());

        // 2 matches — should block
        let mut msg2 = make_msg("gun and bomb in the story");
        assert!(filter.apply(&mut msg2, &mut ctx).is_err());
    }

    #[test]
    fn test_allowed_topic_passes() {
        let filter = TopicGuardrailFilter::new(
            vec![("coding".to_string(), vec!["code".to_string(), "programming".to_string()], 1)],
            vec![],
        );
        let mut msg = make_msg("Help me with code");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
    }

    #[test]
    fn test_allowed_topic_rejects_off_topic() {
        let filter = TopicGuardrailFilter::new(
            vec![("coding".to_string(), vec!["code".to_string(), "programming".to_string()], 1)],
            vec![],
        );
        let mut msg = make_msg("What is the weather today?");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }

    #[test]
    fn test_case_insensitive() {
        let filter = TopicGuardrailFilter::new(
            vec![],
            vec![("spam".to_string(), vec!["viagra".to_string()], 1)],
        );
        let mut msg = make_msg("Buy VIAGRA now");
        let mut ctx = PipelineContext::default();
        assert!(filter.apply(&mut msg, &mut ctx).is_err());
    }
}
