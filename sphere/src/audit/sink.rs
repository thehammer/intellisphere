use std::path::Path;

use tracing_subscriber::fmt::MakeWriter;

use super::event::AuditEvent;

/// Trait for audit event destinations.
///
/// Implementations must be thread-safe (`Send + Sync`) so they can be shared
/// across async tasks.
pub trait AuditSink: Send + Sync {
    fn send(&self, event: &AuditEvent);
}

// ---------------------------------------------------------------------------
// StdoutSink
// ---------------------------------------------------------------------------

/// Emits audit events to stdout via the `tracing` infrastructure.
pub struct StdoutSink;

impl AuditSink for StdoutSink {
    fn send(&self, event: &AuditEvent) {
        let json = serde_json::to_string(event).unwrap_or_else(|e| {
            format!("{{\"error\":\"failed to serialize audit event: {e}\"}}")
        });

        tracing::info!(
            target: "audit",
            event_type = %event.event_type,
            request_id = %event.request_id,
            session_id = %event.session_id,
            audit_payload = %json,
            "audit_event"
        );
    }
}

// ---------------------------------------------------------------------------
// FileSink
// ---------------------------------------------------------------------------

/// Writes audit events as newline-delimited JSON to a rotating log file.
///
/// Uses `tracing_appender` for non-blocking, daily-rotated file output.
pub struct FileSink {
    _guard: tracing_appender::non_blocking::WorkerGuard,
    writer: tracing_appender::non_blocking::NonBlocking,
}

impl FileSink {
    /// Create a new `FileSink` that writes daily-rotated files into `directory`
    /// with the given `file_name_prefix`.
    pub fn new(directory: impl AsRef<Path>, file_name_prefix: impl AsRef<str>) -> Self {
        let file_appender = tracing_appender::rolling::daily(
            directory.as_ref(),
            file_name_prefix.as_ref(),
        );
        let (writer, guard) = tracing_appender::non_blocking(file_appender);
        Self {
            _guard: guard,
            writer,
        }
    }
}

impl AuditSink for FileSink {
    fn send(&self, event: &AuditEvent) {
        use std::io::Write;

        let json = serde_json::to_string(event).unwrap_or_else(|e| {
            format!("{{\"error\":\"failed to serialize audit event: {e}\"}}")
        });

        // Write directly to the non-blocking writer; drop errors silently
        // since audit logging should never block or crash the pipeline.
        let mut writer = self.writer.make_writer();
        let _ = writeln!(writer, "{json}");
    }
}

// ---------------------------------------------------------------------------
// OtlpSink (stub)
// ---------------------------------------------------------------------------

/// Placeholder for an OpenTelemetry (OTLP) audit sink.
///
/// Logs a warning on each call — a real implementation will be added in a
/// future phase.
pub struct OtlpSink;

impl AuditSink for OtlpSink {
    fn send(&self, _event: &AuditEvent) {
        tracing::warn!(target: "audit", "OTLP sink not yet configured");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::{AuditEvent, AuditEventType};
    use serde_json::json;

    #[test]
    fn stdout_sink_does_not_panic() {
        let sink = StdoutSink;
        let event = AuditEvent::new(
            AuditEventType::ResponseSent,
            "req-1",
            "sess-1",
            json!({"status": "ok"}),
        );
        // Just verify it doesn't panic — actual log output goes to tracing.
        sink.send(&event);
    }

    #[test]
    fn file_sink_writes_event() {
        let dir = tempfile::tempdir().unwrap();
        let sink = FileSink::new(dir.path(), "audit-test");
        let event = AuditEvent::new(
            AuditEventType::PolicyEvaluated,
            "req-2",
            "sess-2",
            json!({"policy": "allow"}),
        );
        sink.send(&event);
        // FileSink uses non-blocking IO — drop the sink to flush.
        drop(sink);

        // Verify at least one file was created in the directory.
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(!files.is_empty(), "Expected at least one audit log file");

        // Read the file and verify it contains valid JSON with our event.
        let content = std::fs::read_to_string(files[0].path()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed["event_type"], "policy_evaluated");
        assert_eq!(parsed["request_id"], "req-2");
    }

    #[test]
    fn otlp_sink_does_not_panic() {
        let sink = OtlpSink;
        let event = AuditEvent::new(
            AuditEventType::ToolCallDenied,
            "req-3",
            "sess-3",
            json!({}),
        );
        sink.send(&event);
    }
}
