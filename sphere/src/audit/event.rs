use chrono::{DateTime, Utc};
use serde::Serialize;

/// Types of auditable events in the Sphere pipeline.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    RequestReceived,
    FilterApplied,
    FilterBlocked,
    CompletionRequested,
    CompletionReceived,
    ToolCallIntercepted,
    ToolCallAuthorized,
    ToolCallDenied,
    ToolCallExecuted,
    ToolCallTimeout,
    ResponseFiltered,
    ResponseSent,
    PolicyEvaluated,
    PolicyViolated,
}

/// A structured audit event capturing a discrete action within the pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub event_type: AuditEventType,
    pub request_id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_sub: Option<String>,
    pub details: serde_json::Value,
}

impl AuditEvent {
    /// Create a new audit event with the current UTC timestamp.
    pub fn new(
        event_type: AuditEventType,
        request_id: impl Into<String>,
        session_id: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            event_type,
            request_id: request_id.into(),
            session_id: session_id.into(),
            timestamp: Utc::now(),
            identity_sub: None,
            details,
        }
    }

    /// Set the identity subject (e.g. JWT `sub` claim) on this event.
    pub fn with_identity(mut self, sub: impl Into<String>) -> Self {
        self.identity_sub = Some(sub.into());
        self
    }
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Leverage serde's snake_case serialization for the Display impl.
        let json = serde_json::to_value(self).unwrap_or_default();
        let s = json.as_str().unwrap_or("unknown");
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_type_serializes_as_snake_case() {
        let cases = vec![
            (AuditEventType::RequestReceived, "request_received"),
            (AuditEventType::FilterApplied, "filter_applied"),
            (AuditEventType::FilterBlocked, "filter_blocked"),
            (AuditEventType::CompletionRequested, "completion_requested"),
            (AuditEventType::CompletionReceived, "completion_received"),
            (AuditEventType::ToolCallIntercepted, "tool_call_intercepted"),
            (AuditEventType::ToolCallAuthorized, "tool_call_authorized"),
            (AuditEventType::ToolCallDenied, "tool_call_denied"),
            (AuditEventType::ToolCallExecuted, "tool_call_executed"),
            (AuditEventType::ToolCallTimeout, "tool_call_timeout"),
            (AuditEventType::ResponseFiltered, "response_filtered"),
            (AuditEventType::ResponseSent, "response_sent"),
            (AuditEventType::PolicyEvaluated, "policy_evaluated"),
            (AuditEventType::PolicyViolated, "policy_violated"),
        ];

        for (variant, expected) in cases {
            let serialized = serde_json::to_value(&variant).unwrap();
            assert_eq!(serialized, json!(expected), "Failed for {:?}", variant);
        }
    }

    #[test]
    fn audit_event_serializes_to_json() {
        let event = AuditEvent::new(
            AuditEventType::RequestReceived,
            "req-123",
            "sess-456",
            json!({"model": "claude-3"}),
        )
        .with_identity("user-789");

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event_type"], "request_received");
        assert_eq!(json["request_id"], "req-123");
        assert_eq!(json["session_id"], "sess-456");
        assert_eq!(json["identity_sub"], "user-789");
        assert_eq!(json["details"]["model"], "claude-3");
        assert!(json["timestamp"].is_string());
    }

    #[test]
    fn audit_event_omits_identity_sub_when_none() {
        let event = AuditEvent::new(
            AuditEventType::FilterApplied,
            "req-1",
            "sess-1",
            json!({}),
        );

        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("identity_sub").is_none());
    }
}
