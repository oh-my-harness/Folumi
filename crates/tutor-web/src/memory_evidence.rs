use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    KnowledgeContent, KnowledgeError, KnowledgeReadRequest, KnowledgeRequestContext,
    KnowledgeSource, KnowledgeSourceDescriptor, SourceSearchPage, SourceSearchRequest,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemoryEvidenceActivity {
    pub stage: String,
    pub tool: String,
    pub summary: String,
    pub refs: Vec<String>,
}

#[derive(Clone, Default)]
pub struct MemoryEvidenceTracker {
    read_refs: Arc<Mutex<BTreeSet<String>>>,
    activities: Arc<Mutex<Vec<MemoryEvidenceActivity>>>,
    activity_sender: Option<tokio::sync::mpsc::UnboundedSender<MemoryEvidenceActivity>>,
}

impl MemoryEvidenceTracker {
    pub fn with_activity_sender(
        activity_sender: tokio::sync::mpsc::UnboundedSender<MemoryEvidenceActivity>,
    ) -> Self {
        Self {
            activity_sender: Some(activity_sender),
            ..Self::default()
        }
    }

    pub fn canonical_reference(&self, reference: &str) -> Option<String> {
        self.read_refs
            .lock()
            .ok()
            .filter(|refs| refs.contains(reference))
            .map(|_| reference.to_string())
    }

    pub(crate) fn record_stage(&self, stage: &str, summary: String) {
        self.record_activity(stage, "memory_workflow", summary, Vec::new());
    }

    fn record_search(&self, count: usize) {
        self.record_activity(
            "discovering_sources",
            "knowledge_search",
            format!("Found {count} Learner Memory evidence candidates"),
            Vec::new(),
        );
    }

    fn record_read(&self, content: &KnowledgeContent) {
        let Some(reference) = content
            .metadata
            .get("canonical_reference")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if let Ok(mut refs) = self.read_refs.lock() {
            refs.insert(reference.clone());
        }
        self.record_activity(
            "reading_evidence",
            "knowledge_read",
            format!("Read {reference} through runtime Knowledge"),
            vec![reference],
        );
    }

    fn record_activity(&self, stage: &str, tool: &str, summary: String, refs: Vec<String>) {
        let activity = MemoryEvidenceActivity {
            stage: stage.into(),
            tool: tool.into(),
            summary,
            refs,
        };
        if let Ok(mut activities) = self.activities.lock() {
            activities.push(activity.clone());
        }
        if let Some(sender) = &self.activity_sender {
            let _ = sender.send(activity);
        }
    }

    #[cfg(test)]
    fn read_refs(&self) -> Vec<String> {
        self.read_refs
            .lock()
            .map(|refs| refs.iter().cloned().collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn record_test_reference(&self, reference: &str) {
        if let Ok(mut refs) = self.read_refs.lock() {
            refs.insert(reference.to_string());
        }
    }
}

#[derive(Clone)]
pub struct TrackedMemoryKnowledgeSource {
    inner: Arc<dyn KnowledgeSource>,
    tracker: MemoryEvidenceTracker,
}

impl TrackedMemoryKnowledgeSource {
    pub fn new(inner: Arc<dyn KnowledgeSource>, tracker: MemoryEvidenceTracker) -> Self {
        Self { inner, tracker }
    }
}

impl KnowledgeSource for TrackedMemoryKnowledgeSource {
    fn descriptor(&self) -> &KnowledgeSourceDescriptor {
        self.inner.descriptor()
    }

    fn search<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: SourceSearchRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<SourceSearchPage, KnowledgeError>> {
        Box::pin(async move {
            let page = self.inner.search(ctx, request, abort).await?;
            self.tracker.record_search(page.hits.len());
            Ok(page)
        })
    }

    fn read<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: KnowledgeReadRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<KnowledgeContent, KnowledgeError>> {
        Box::pin(async move {
            let content = self.inner.read(ctx, request, abort).await?;
            self.tracker.record_read(&content);
            Ok(content)
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use llm_harness_runtime_knowledge::{ContentSelector, KnowledgeRef};
    use llm_harness_types::DataBlock;

    use super::*;

    #[test]
    fn only_runtime_read_metadata_makes_a_reference_citeable() {
        let tracker = MemoryEvidenceTracker::default();
        assert!(tracker.canonical_reference("chat:event-1").is_none());
        tracker.record_read(&KnowledgeContent {
            reference: KnowledgeRef {
                source_id: "llm-tutor.learner-memory".into(),
                item_id: "l1/chat/event-1".into(),
                revision: Some("rev-1".into()),
            },
            selector: ContentSelector::Document,
            title: None,
            blocks: vec![DataBlock::text("private body")],
            uri: None,
            updated_at: None,
            obtained_at: Utc::now(),
            truncated: false,
            metadata: [(
                "canonical_reference".into(),
                serde_json::json!("chat:event-1"),
            )]
            .into_iter()
            .collect(),
        });

        assert_eq!(tracker.read_refs(), vec!["chat:event-1"]);
        assert_eq!(
            tracker.canonical_reference("chat:event-1").as_deref(),
            Some("chat:event-1")
        );
        assert!(tracker.canonical_reference("chat:event-2").is_none());
    }
}
