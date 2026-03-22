use std::sync::Arc;

use super::event::AuditEvent;
use super::sink::AuditSink;

/// Central emitter that dispatches [`AuditEvent`]s to one or more [`AuditSink`]s.
///
/// The emitter is cheaply cloneable (backed by `Arc`) and safe to share across
/// async tasks.
#[derive(Clone)]
pub struct AuditEmitter {
    sinks: Arc<Vec<Box<dyn AuditSink>>>,
}

impl AuditEmitter {
    /// Create an emitter with the given sinks.
    pub fn new(sinks: Vec<Box<dyn AuditSink>>) -> Self {
        Self {
            sinks: Arc::new(sinks),
        }
    }

    /// Emit an audit event to all configured sinks.
    ///
    /// Always includes `request_id` and `session_id` as correlation fields in
    /// the tracing span, then forwards the event to every registered sink.
    pub fn emit(&self, event: &AuditEvent) {
        let _span = tracing::info_span!(
            "audit_emit",
            request_id = %event.request_id,
            session_id = %event.session_id,
        )
        .entered();

        for sink in self.sinks.iter() {
            sink.send(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::{AuditEvent, AuditEventType};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A test sink that counts how many events it receives.
    struct CountingSink {
        count: Arc<AtomicUsize>,
    }

    impl AuditSink for CountingSink {
        fn send(&self, _event: &AuditEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn emitter_dispatches_to_all_sinks() {
        let count_a = Arc::new(AtomicUsize::new(0));
        let count_b = Arc::new(AtomicUsize::new(0));

        let emitter = AuditEmitter::new(vec![
            Box::new(CountingSink {
                count: Arc::clone(&count_a),
            }),
            Box::new(CountingSink {
                count: Arc::clone(&count_b),
            }),
        ]);

        let event = AuditEvent::new(
            AuditEventType::RequestReceived,
            "req-1",
            "sess-1",
            json!({}),
        );

        emitter.emit(&event);
        emitter.emit(&event);

        assert_eq!(count_a.load(Ordering::SeqCst), 2);
        assert_eq!(count_b.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn emitter_with_no_sinks_does_not_panic() {
        let emitter = AuditEmitter::new(vec![]);
        let event = AuditEvent::new(
            AuditEventType::CompletionReceived,
            "req-x",
            "sess-x",
            json!({"tokens": 42}),
        );
        emitter.emit(&event);
    }
}
