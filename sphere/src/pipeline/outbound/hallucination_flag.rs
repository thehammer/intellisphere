use regex::Regex;

use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

use super::OutboundFilter;

/// v1 heuristic hallucination detector.
///
/// Flags responses containing URLs not present in any user message or tool result
/// stored in the pipeline context. This is low-confidence and annotation-only;
/// it never blocks in v1.
pub struct HallucinationFlagFilter {
    url_regex: Regex,
}

impl HallucinationFlagFilter {
    pub fn new() -> Self {
        Self {
            url_regex: Regex::new(r"https?://[a-zA-Z0-9\-._~:/?#\[\]@!$&'()*+,;=%]+").unwrap(),
        }
    }
}

impl Default for HallucinationFlagFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutboundFilter for HallucinationFlagFilter {
    fn name(&self) -> &str {
        "HallucinationFlag"
    }

    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        let response_urls: Vec<String> = self
            .url_regex
            .find_iter(&message.content)
            .map(|m| m.as_str().to_string())
            .collect();

        if response_urls.is_empty() {
            return Ok(());
        }

        // Collect all known URLs from user messages and tool results stored in annotations
        let known_urls: Vec<String> = context
            .get_annotations("known_urls")
            .cloned()
            .unwrap_or_default();

        let mut hallucinated_urls: Vec<String> = Vec::new();

        for url in &response_urls {
            // Check if this URL (or a prefix of it) appears in known URLs
            let is_known = known_urls
                .iter()
                .any(|known| url.starts_with(known.as_str()) || known.starts_with(url.as_str()));

            if !is_known {
                hallucinated_urls.push(url.clone());
            }
        }

        if !hallucinated_urls.is_empty() {
            for url in &hallucinated_urls {
                context.annotate("hallucination_flagged_url", url.clone());
            }
            context.annotate("hallucination_confidence", "low".to_string());
            tracing::info!(
                count = hallucinated_urls.len(),
                "Potential hallucinated URLs detected in response (flagged)"
            );
        }

        // v1: Never block, always pass through
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
    fn test_no_urls_passes() {
        let filter = HallucinationFlagFilter::new();
        let mut msg = make_msg("The answer is 42.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(ctx.get_annotations("hallucination_flagged_url").is_none());
    }

    #[test]
    fn test_known_url_not_flagged() {
        let filter = HallucinationFlagFilter::new();
        let mut msg = make_msg("Check out https://example.com/docs for more info.");
        let mut ctx = PipelineContext::default();
        ctx.annotate("known_urls", "https://example.com/docs");
        filter.apply(&mut msg, &mut ctx).unwrap();
        assert!(ctx.get_annotations("hallucination_flagged_url").is_none());
    }

    #[test]
    fn test_unknown_url_flagged() {
        let filter = HallucinationFlagFilter::new();
        let mut msg = make_msg("Visit https://made-up-site.com/fake for details.");
        let mut ctx = PipelineContext::default();
        filter.apply(&mut msg, &mut ctx).unwrap();
        let flagged = ctx.get_annotations("hallucination_flagged_url").unwrap();
        assert!(flagged.contains(&"https://made-up-site.com/fake".to_string()));
        assert!(ctx.get_annotations("hallucination_confidence").is_some());
    }

    #[test]
    fn test_mixed_known_and_unknown() {
        let filter = HallucinationFlagFilter::new();
        let mut msg = make_msg("See https://real.com/page and https://fake.com/invented for info.");
        let mut ctx = PipelineContext::default();
        ctx.annotate("known_urls", "https://real.com/page");
        filter.apply(&mut msg, &mut ctx).unwrap();
        let flagged = ctx.get_annotations("hallucination_flagged_url").unwrap();
        assert_eq!(flagged.len(), 1);
        assert!(flagged.contains(&"https://fake.com/invented".to_string()));
    }

    #[test]
    fn test_never_blocks() {
        let filter = HallucinationFlagFilter::new();
        let mut msg = make_msg("https://a.com https://b.com https://c.com all hallucinated");
        let mut ctx = PipelineContext::default();
        // Should always return Ok
        assert!(filter.apply(&mut msg, &mut ctx).is_ok());
    }

    #[test]
    fn test_url_prefix_matching() {
        let filter = HallucinationFlagFilter::new();
        let mut msg = make_msg("See https://example.com/docs/page for more.");
        let mut ctx = PipelineContext::default();
        ctx.annotate("known_urls", "https://example.com/docs");
        filter.apply(&mut msg, &mut ctx).unwrap();
        // Should not flag because the URL starts with a known URL prefix
        assert!(ctx.get_annotations("hallucination_flagged_url").is_none());
    }
}
