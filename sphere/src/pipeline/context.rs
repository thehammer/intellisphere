use std::collections::HashMap;

use crate::auth::Identity;

/// Context that flows through the pipeline, carrying metadata and annotations
/// from filter to filter.
#[derive(Debug, Clone, Default)]
pub struct PipelineContext {
    /// Unique request ID
    pub request_id: String,

    /// Session ID for multi-turn conversations
    pub session_id: String,

    /// Authenticated identity (None if auth is disabled or not yet checked)
    pub identity: Option<Identity>,

    /// Annotations added by filters (e.g., flagged injection patterns)
    pub annotations: HashMap<String, Vec<String>>,

    /// Token counts tracked through the pipeline
    pub input_token_count: Option<u32>,

    /// Cumulative session token count
    pub session_token_count: Option<u64>,

    /// Turn count for conversation boundary enforcement
    pub turn_count: Option<u32>,
}

impl PipelineContext {
    pub fn new(request_id: String, session_id: String) -> Self {
        Self {
            request_id,
            session_id,
            ..Default::default()
        }
    }

    /// Add an annotation from a filter. Multiple annotations can exist per key.
    pub fn annotate(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.annotations
            .entry(key.into())
            .or_default()
            .push(value.into());
    }

    /// Get annotations for a given key.
    pub fn get_annotations(&self, key: &str) -> Option<&Vec<String>> {
        self.annotations.get(key)
    }
}
