use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tutor_agent::event_sink::EventSink;

/// Events pushed from the agent harness to the WebSocket handler.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum StreamEvent {
    /// Final-answer text chunk or final message.
    Content { text: String, chunk: bool },
    /// Transient model narration/progress that must not be persisted as the
    /// final assistant answer.
    ProgressContent { text: String, chunk: bool },
    /// Model-provided displayable reasoning, kept separate from final text.
    ThinkingContent { text: String, chunk: bool },
    /// Internal event for the TracePanel.
    Trace {
        kind: String,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    /// Status notification.
    Status {
        kind: String,
        #[serde(flatten)]
        data: serde_json::Value,
    },
}

/// WebSocket event bus — one per active session.
#[derive(Clone)]
pub struct TutorStream {
    tx: broadcast::Sender<StreamEvent>,
    snapshot: Arc<Mutex<StreamSnapshot>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamSnapshot {
    pub generation: u64,
    pub content: String,
    pub progress_content: String,
    pub thinking_content: String,
    pub completed: bool,
}

impl TutorStream {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self {
            tx,
            snapshot: Arc::new(Mutex::new(StreamSnapshot::default())),
        }
    }

    #[cfg(test)]
    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.tx.subscribe()
    }

    pub fn subscribe_with_snapshot(&self) -> (broadcast::Receiver<StreamEvent>, StreamSnapshot) {
        let snapshot = self.snapshot.lock().unwrap();
        let receiver = self.tx.subscribe();
        (receiver, snapshot.clone())
    }

    pub fn begin_run(&self) {
        let mut snapshot = self.snapshot.lock().unwrap();
        let generation = snapshot.generation.wrapping_add(1);
        *snapshot = StreamSnapshot {
            generation,
            ..StreamSnapshot::default()
        };
    }

    pub fn acknowledge_completed(&self, generation: u64) {
        let mut snapshot = self.snapshot.lock().unwrap();
        if snapshot.generation == generation {
            snapshot.completed = false;
        }
    }

    pub async fn content(&self, text: &str, chunk: bool) {
        let mut snapshot = self.snapshot.lock().unwrap();
        if chunk {
            snapshot.content.push_str(text);
        } else {
            // The terminal event is authoritative. Text deltas emitted before
            // runtime message classification can belong to a Progress message
            // that precedes a tool call, so a non-empty final answer replaces
            // provisional content instead of being appended to it.
            if !text.is_empty() {
                snapshot.content = text.to_string();
            }
            snapshot.completed = true;
        }
        let _ = self.tx.send(StreamEvent::Content {
            text: text.to_string(),
            chunk,
        });
    }

    pub async fn progress_content(&self, text: &str, chunk: bool) {
        let mut snapshot = self.snapshot.lock().unwrap();
        if chunk {
            snapshot.progress_content.push_str(text);
        } else {
            snapshot.progress_content = text.to_string();
        }
        let _ = self.tx.send(StreamEvent::ProgressContent {
            text: text.to_string(),
            chunk,
        });
    }

    pub async fn thinking_content(&self, text: &str, chunk: bool) {
        let mut snapshot = self.snapshot.lock().unwrap();
        if chunk {
            snapshot.thinking_content.push_str(text);
        } else {
            snapshot.thinking_content = text.to_string();
        }
        let _ = self.tx.send(StreamEvent::ThinkingContent {
            text: text.to_string(),
            chunk,
        });
    }

    pub async fn trace(&self, kind: &str, data: impl Serialize) {
        let _ = self.tx.send(StreamEvent::Trace {
            kind: kind.to_string(),
            data: serde_json::to_value(data).unwrap_or_default(),
        });
    }

    pub async fn status(&self, kind: &str, data: impl Serialize) {
        let _ = self.tx.send(StreamEvent::Status {
            kind: kind.to_string(),
            data: serde_json::to_value(data).unwrap_or_default(),
        });
    }
}

impl EventSink for TutorStream {
    fn trace(
        &self,
        kind: String,
        data: serde_json::Value,
    ) -> futures::future::BoxFuture<'static, ()> {
        let stream = self.clone();
        Box::pin(async move {
            stream.trace(&kind, data).await;
        })
    }

    fn content(&self, text: String, chunk: bool) -> futures::future::BoxFuture<'static, ()> {
        let stream = self.clone();
        Box::pin(async move {
            stream.content(&text, chunk).await;
        })
    }

    fn progress_content(
        &self,
        text: String,
        chunk: bool,
    ) -> futures::future::BoxFuture<'static, ()> {
        let stream = self.clone();
        Box::pin(async move {
            stream.progress_content(&text, chunk).await;
        })
    }

    fn thinking_content(
        &self,
        text: String,
        chunk: bool,
    ) -> futures::future::BoxFuture<'static, ()> {
        let stream = self.clone();
        Box::pin(async move {
            stream.thinking_content(&text, chunk).await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_event_serializes_to_json() {
        let event = StreamEvent::Content {
            text: "hello".into(),
            chunk: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content");
        assert_eq!(json["payload"]["text"], "hello");
        assert_eq!(json["payload"]["chunk"], true);
    }

    #[test]
    fn trace_event_serializes_correctly() {
        let event = StreamEvent::Trace {
            kind: "phase_start".into(),
            data: serde_json::json!({ "phase": "plan" }),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "trace");
        assert_eq!(json["payload"]["kind"], "phase_start");
        assert_eq!(json["payload"]["phase"], "plan");
    }

    #[test]
    fn progress_content_event_serializes_to_json() {
        let event = StreamEvent::ProgressContent {
            text: "working".into(),
            chunk: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "progress_content");
        assert_eq!(json["payload"]["text"], "working");
        assert_eq!(json["payload"]["chunk"], true);
    }

    #[test]
    fn thinking_content_event_serializes_to_json() {
        let event = StreamEvent::ThinkingContent {
            text: "considering context".into(),
            chunk: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "thinking_content");
        assert_eq!(json["payload"]["text"], "considering context");
        assert_eq!(json["payload"]["chunk"], true);
    }

    #[tokio::test]
    async fn tutor_stream_sends_content() {
        let stream = TutorStream::new(16);
        let mut rx = stream.subscribe();
        stream.content("hello", true).await;
        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::Content { text, chunk } => {
                assert_eq!(text, "hello");
                assert!(chunk);
            }
            _ => panic!("expected Content"),
        }
    }

    #[tokio::test]
    async fn tutor_stream_sends_progress_content() {
        let stream = TutorStream::new(16);
        let mut rx = stream.subscribe();
        stream.progress_content("working", true).await;
        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ProgressContent { text, chunk } => {
                assert_eq!(text, "working");
                assert!(chunk);
            }
            _ => panic!("expected ProgressContent"),
        }
    }

    #[tokio::test]
    async fn tutor_stream_sends_thinking_content() {
        let stream = TutorStream::new(16);
        let mut rx = stream.subscribe();
        stream.thinking_content("thinking", true).await;
        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ThinkingContent { text, chunk } => {
                assert_eq!(text, "thinking");
                assert!(chunk);
            }
            _ => panic!("expected ThinkingContent"),
        }
    }

    #[tokio::test]
    async fn snapshot_restores_content_before_continuing_live_stream() {
        let stream = TutorStream::new(16);
        stream.begin_run();
        stream.content("hello ", true).await;

        let (mut rx, snapshot) = stream.subscribe_with_snapshot();
        assert_eq!(snapshot.content, "hello ");
        assert!(!snapshot.completed);

        stream.content("world", true).await;
        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::Content { text, chunk } => {
                assert_eq!(text, "world");
                assert!(chunk);
            }
            _ => panic!("expected Content"),
        }
    }

    #[tokio::test]
    async fn begin_run_clears_previous_stream_snapshot() {
        let stream = TutorStream::new(16);
        stream.content("old answer", true).await;
        stream.progress_content("old progress", false).await;
        stream.thinking_content("old thinking", false).await;

        stream.begin_run();
        let (_, snapshot) = stream.subscribe_with_snapshot();

        assert_eq!(snapshot.content, "");
        assert_eq!(snapshot.progress_content, "");
        assert_eq!(snapshot.thinking_content, "");
        assert!(!snapshot.completed);
    }

    #[tokio::test]
    async fn final_content_marks_snapshot_as_completed() {
        let stream = TutorStream::new(16);
        stream.begin_run();
        stream.content("complete answer", false).await;

        let (_, snapshot) = stream.subscribe_with_snapshot();

        assert_eq!(snapshot.content, "complete answer");
        assert!(snapshot.completed);
    }

    #[tokio::test]
    async fn final_content_replaces_provisional_progress_in_snapshot() {
        let stream = TutorStream::new(16);
        stream.begin_run();
        stream
            .content("I remembered it before the tool.", true)
            .await;
        stream.content("The preference was saved.", false).await;

        let (_, snapshot) = stream.subscribe_with_snapshot();

        assert_eq!(snapshot.content, "The preference was saved.");
        assert!(snapshot.completed);
    }

    #[tokio::test]
    async fn completed_snapshot_can_be_acknowledged_after_rejoin() {
        let stream = TutorStream::new(16);
        stream.content("complete answer", false).await;

        let (_, completed) = stream.subscribe_with_snapshot();
        stream.acknowledge_completed(completed.generation);
        let (_, snapshot) = stream.subscribe_with_snapshot();

        assert_eq!(snapshot.content, "complete answer");
        assert!(!snapshot.completed);
    }

    #[tokio::test]
    async fn acknowledgement_does_not_clear_a_newer_run() {
        let stream = TutorStream::new(16);
        stream.content("old answer", false).await;
        let (_, old) = stream.subscribe_with_snapshot();
        stream.begin_run();
        stream.content("new answer", false).await;

        stream.acknowledge_completed(old.generation);
        let (_, snapshot) = stream.subscribe_with_snapshot();

        assert_eq!(snapshot.content, "new answer");
        assert!(snapshot.completed);
    }
}
