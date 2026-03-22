use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::proto::intellisphere::v1::Message;

/// Trait for outbound pipeline filters.
///
/// Each filter receives the full response message and pipeline context, and returns
/// either the (potentially modified) message or an error.
///
/// Filters can:
/// - Pass through the message unchanged
/// - Modify the message (e.g., redact PII leaks, truncate)
/// - Reject the message (return Err with OutboundRejected)
/// - Annotate the context for logging or downstream consumers
pub trait OutboundFilter: Send + Sync {
    /// Human-readable name of this filter.
    fn name(&self) -> &str;

    /// Process a response message through this filter.
    fn apply(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError>;
}
