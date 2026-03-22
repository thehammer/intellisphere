use crate::config::PipelineConfig;
use crate::errors::SphereError;
use crate::pipeline::context::PipelineContext;
use crate::pipeline::inbound::{InboundFilter, InputSanitizationFilter};
use crate::pipeline::outbound::{
    HallucinationFlagFilter, InjectionEchoAction, InjectionEchoFilter, OutboundFilter,
    PIILeakAction, PIILeakDetectionFilter, ResponseClassifierAction, ResponseClassifierFilter,
    ResultSizeEnforcementFilter,
};
use crate::proto::intellisphere::v1::Message;

/// Executes the inbound and outbound filter chains in order.
pub struct PipelineChain {
    inbound_filters: Vec<Box<dyn InboundFilter>>,
    outbound_filters: Vec<Box<dyn OutboundFilter>>,
}

impl PipelineChain {
    pub fn from_config(config: &PipelineConfig) -> Self {
        let mut inbound_filters: Vec<Box<dyn InboundFilter>> = Vec::new();

        // Input sanitization is always first
        if config.inbound.input_sanitization.enabled {
            inbound_filters.push(Box::new(InputSanitizationFilter::new(
                config.inbound.input_sanitization.clone(),
            )));
        }

        let mut outbound_filters: Vec<Box<dyn OutboundFilter>> = Vec::new();

        // Result size enforcement runs first to cap output before other filters process it
        if config.outbound.result_size.enabled {
            outbound_filters.push(Box::new(ResultSizeEnforcementFilter::new(
                config.outbound.result_size.max_chars,
            )));
        }

        // PII leak detection
        if config.outbound.pii_leak_detection.enabled {
            outbound_filters.push(Box::new(PIILeakDetectionFilter::new(PIILeakAction::Redact)));
        }

        // Injection echo detection
        if config.outbound.injection_echo.enabled {
            outbound_filters.push(Box::new(InjectionEchoFilter::new(
                InjectionEchoAction::Flag,
            )));
        }

        // Response classifier
        if config.outbound.response_classifier.enabled {
            outbound_filters.push(Box::new(ResponseClassifierFilter::new(
                ResponseClassifierAction::Flag,
            )));
        }

        // Hallucination flag (always flag-only in v1)
        if config.outbound.hallucination_flag.enabled {
            outbound_filters.push(Box::new(HallucinationFlagFilter::new()));
        }

        Self {
            inbound_filters,
            outbound_filters,
        }
    }

    /// Run all inbound filters on a message.
    pub fn run_inbound(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        for filter in &self.inbound_filters {
            tracing::debug!(filter = filter.name(), "Running inbound filter");
            filter.apply(message, context)?;
        }
        Ok(())
    }

    /// Run all outbound filters on a response message.
    pub fn run_outbound(
        &self,
        message: &mut Message,
        context: &mut PipelineContext,
    ) -> Result<(), SphereError> {
        for filter in &self.outbound_filters {
            tracing::debug!(filter = filter.name(), "Running outbound filter");
            filter.apply(message, context)?;
        }
        Ok(())
    }
}
