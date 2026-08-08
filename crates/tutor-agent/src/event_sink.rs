use std::sync::Arc;

use futures::future::BoxFuture;
use serde::Serialize;

/// Optional event bridge used by web sessions to receive agent trace events.
pub trait EventSink: Send + Sync {
    fn trace(&self, kind: String, data: serde_json::Value) -> BoxFuture<'static, ()>;

    fn content(&self, _text: String, _chunk: bool) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }

    fn progress_content(&self, _text: String, _chunk: bool) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }

    fn thinking_content(&self, _text: String, _chunk: bool) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}

pub type SharedEventSink = Arc<dyn EventSink>;

pub async fn emit_trace(sink: &Option<SharedEventSink>, kind: &str, data: impl Serialize) {
    if let Some(sink) = sink {
        sink.trace(
            kind.to_string(),
            serde_json::to_value(data).unwrap_or_default(),
        )
        .await;
    }
}

pub async fn emit_content(sink: &Option<SharedEventSink>, text: impl Into<String>, chunk: bool) {
    if let Some(sink) = sink {
        sink.content(text.into(), chunk).await;
    }
}

pub async fn emit_progress_content(
    sink: &Option<SharedEventSink>,
    text: impl Into<String>,
    chunk: bool,
) {
    if let Some(sink) = sink {
        sink.progress_content(text.into(), chunk).await;
    }
}

pub async fn emit_thinking_content(
    sink: &Option<SharedEventSink>,
    text: impl Into<String>,
    chunk: bool,
) {
    if let Some(sink) = sink {
        sink.thinking_content(text.into(), chunk).await;
    }
}
