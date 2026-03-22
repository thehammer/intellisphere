use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

/// Trait for inbound pipeline filters.
///
/// Each filter receives the full message and pipeline context, and returns
/// either the (potentially modified) message or an error.
///
/// Filters can:
/// - Pass through the message unchanged
/// - Modify the message (e.g., redact PII, strip control chars)
/// - Reject the message (return Err)
/// - Annotate the context for downstream filters or outbound filters
pub trait InboundFilter: Send + Sync {
    /// Human-readable name of this filter.
    fn name(&self) -> &str;

    /// Process a message through this filter.
    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError>;
}
