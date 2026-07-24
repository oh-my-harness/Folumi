#![allow(clippy::result_large_err)] // Tool helpers must return the runtime-owned ToolFailure value.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use llm_harness_types::{DataBlock, Tool, ToolContext, ToolFailure, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::memory_store::{MemoryEvent, MemoryL2Entry, MemoryStore};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEvidenceActivity {
    pub stage: String,
    pub tool: String,
    pub summary: String,
    pub refs: Vec<String>,
}

#[derive(Clone, Default)]
pub struct MemoryEvidenceTracker {
    read_l1_refs: Arc<Mutex<BTreeSet<String>>>,
    read_l2_refs: Arc<Mutex<BTreeSet<String>>>,
    resolved_aliases: Arc<Mutex<BTreeMap<String, String>>>,
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

    #[cfg(test)]
    pub fn read_refs(&self) -> Vec<String> {
        let mut refs = BTreeSet::new();
        if let Ok(read) = self.read_l1_refs.lock() {
            refs.extend(read.iter().cloned());
        }
        if let Ok(read) = self.read_l2_refs.lock() {
            refs.extend(read.iter().cloned());
        }
        refs.into_iter().collect()
    }

    #[cfg(test)]
    pub fn read_l1_refs(&self) -> Vec<String> {
        self.read_l1_refs
            .lock()
            .map(|refs| refs.iter().cloned().collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn read_l2_refs(&self) -> Vec<String> {
        self.read_l2_refs
            .lock()
            .map(|refs| refs.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn canonical_reference(&self, reference: &str) -> Option<String> {
        if self
            .read_l1_refs
            .lock()
            .is_ok_and(|refs| refs.contains(reference))
            || self
                .read_l2_refs
                .lock()
                .is_ok_and(|refs| refs.contains(reference))
        {
            return Some(reference.to_string());
        }
        self.resolved_aliases
            .lock()
            .ok()
            .and_then(|aliases| aliases.get(reference).cloned())
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

    fn record_l1(&self, stage: &str, tool: &str, summary: String, refs: Vec<String>) {
        if let Ok(mut read_refs) = self.read_l1_refs.lock() {
            read_refs.extend(refs.iter().cloned());
        }
        self.record_activity(stage, tool, summary, refs);
    }

    fn record_l2(&self, stage: &str, tool: &str, summary: String, refs: Vec<String>) {
        if let Ok(mut read_refs) = self.read_l2_refs.lock() {
            read_refs.extend(refs.iter().cloned());
        }
        self.record_activity(stage, tool, summary, refs);
    }

    pub(crate) fn record_resolution(
        &self,
        stage: &str,
        tool: &str,
        summary: String,
        requested_reference: &str,
        canonical_reference: &str,
    ) {
        if requested_reference != canonical_reference
            && let Ok(mut aliases) = self.resolved_aliases.lock()
        {
            aliases.insert(
                requested_reference.to_string(),
                canonical_reference.to_string(),
            );
        }
        self.record_l1(stage, tool, summary, vec![canonical_reference.to_string()]);
    }

    pub(crate) fn record_stage(&self, stage: &str, summary: String) {
        self.record_activity(stage, "memory_workflow", summary, Vec::new());
    }
}

#[derive(Clone)]
pub struct ListMemoryEventsTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
}

#[derive(Clone)]
pub struct SearchMemoryEventsTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
}

#[derive(Clone)]
pub struct ReadMemoryEventTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
}

#[derive(Clone)]
pub struct ReadMemoryContextTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
}

#[derive(Clone)]
pub struct ReadMemorySourceTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
}

#[derive(Clone)]
pub struct ListMemoryEntriesTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
    allowed_paths: Vec<String>,
}

#[derive(Clone)]
pub struct SearchMemoryEntriesTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
    allowed_paths: Vec<String>,
}

#[derive(Clone)]
pub struct ReadMemoryEntryTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
    allowed_paths: Vec<String>,
}

#[derive(Clone)]
pub struct ReadMemoryEntrySourcesTool {
    store: Arc<MemoryStore>,
    tracker: MemoryEvidenceTracker,
    allowed_paths: Vec<String>,
}

macro_rules! tool_constructor {
    ($tool:ident) => {
        impl $tool {
            pub fn new(store: Arc<MemoryStore>, tracker: MemoryEvidenceTracker) -> Self {
                Self { store, tracker }
            }
        }
    };
}

tool_constructor!(ListMemoryEventsTool);
tool_constructor!(SearchMemoryEventsTool);
tool_constructor!(ReadMemoryEventTool);
tool_constructor!(ReadMemoryContextTool);
tool_constructor!(ReadMemorySourceTool);
macro_rules! l2_tool_constructor {
    ($tool:ident) => {
        impl $tool {
            pub fn new(
                store: Arc<MemoryStore>,
                tracker: MemoryEvidenceTracker,
                allowed_paths: Vec<String>,
            ) -> Self {
                Self {
                    store,
                    tracker,
                    allowed_paths,
                }
            }
        }
    };
}

l2_tool_constructor!(ListMemoryEntriesTool);
l2_tool_constructor!(SearchMemoryEntriesTool);
l2_tool_constructor!(ReadMemoryEntryTool);
l2_tool_constructor!(ReadMemoryEntrySourcesTool);

impl Tool for ListMemoryEventsTool {
    fn name(&self) -> &str {
        "list_memory_events"
    }

    fn description(&self) -> &str {
        "List a bounded page of L1 learner activity summaries. Start with the target surface before expanding to other surfaces. Use read_memory_event for full evidence."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| event_page_schema(false))
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let surface = optional_string(&args, "surface");
            let session_id = optional_string(&args, "session_id");
            let cursor = optional_string(&args, "cursor");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let page = self
                .store
                .query_events(surface, None, session_id, cursor, limit)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            self.tracker.record_activity(
                "discovering_sources",
                self.name(),
                format!(
                    "Listed {} of {} matching {} L1 events",
                    page.events.len(),
                    page.total,
                    surface.unwrap_or("all-surface")
                ),
                Vec::new(),
            );
            Ok(json_tool_result(json!({
                "events": event_summaries(&page.events),
                "next_cursor": page.next_cursor,
                "total": page.total,
            })))
        })
    }
}

impl Tool for SearchMemoryEventsTool {
    fn name(&self) -> &str {
        "search_memory_events"
    }

    fn description(&self) -> &str {
        "Search L1 learner activity summaries by text, surface, or session. Search results are candidates; call read_memory_event before citing one."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| event_page_schema(true))
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let query = optional_string(&args, "query")
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ToolFailure::invalid_arguments("query is required"))?;
            let surface = optional_string(&args, "surface");
            let session_id = optional_string(&args, "session_id");
            let cursor = optional_string(&args, "cursor");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let page = self
                .store
                .query_events(surface, Some(query), session_id, cursor, limit)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            self.tracker.record_activity(
                "discovering_sources",
                self.name(),
                format!(
                    "Found {} {} L1 event candidates for `{query}`",
                    page.total,
                    surface.unwrap_or("all-surface")
                ),
                Vec::new(),
            );
            Ok(json_tool_result(json!({
                "events": event_summaries(&page.events),
                "next_cursor": page.next_cursor,
                "total": page.total,
            })))
        })
    }
}

impl Tool for ReadMemoryEventTool {
    fn name(&self) -> &str {
        "read_memory_event"
    }

    fn description(&self) -> &str {
        "Read one complete L1 event by its event id. Only events read with this tool or a context/source read may be cited in proposed memory changes."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {"event_id": {"type": "string"}},
                "required": ["event_id"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let event_id = required_string(&args, "event_id")?;
            let event = self
                .store
                .read_event(event_id)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            let reference = event_reference(&event);
            self.tracker.record_l1(
                "reading_evidence",
                self.name(),
                format!("Read {} evidence", reference),
                vec![reference.clone()],
            );
            Ok(json_ephemeral_result(
                json!({"reference": reference, "event": event}),
                "Read one L1 memory event.",
            ))
        })
    }
}

impl Tool for ReadMemoryContextTool {
    fn name(&self) -> &str {
        "read_memory_context"
    }

    fn description(&self) -> &str {
        "Read bounded events before and after one L1 event in the same source session. Use this when an isolated event lacks enough context."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "event_id": {"type": "string"},
                    "before": {"type": "integer", "minimum": 0, "maximum": 20},
                    "after": {"type": "integer", "minimum": 0, "maximum": 20}
                },
                "required": ["event_id"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let event_id = required_string(&args, "event_id")?;
            let context = self
                .store
                .event_context(
                    event_id,
                    args["before"].as_u64().unwrap_or(2) as usize,
                    args["after"].as_u64().unwrap_or(2) as usize,
                )
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            let refs = std::iter::once(&context.event)
                .chain(context.before.iter())
                .chain(context.after.iter())
                .map(event_reference)
                .collect::<Vec<_>>();
            self.tracker.record_l1(
                "reading_evidence",
                self.name(),
                format!("Read {} contextual L1 events", refs.len()),
                refs.clone(),
            );
            Ok(json_ephemeral_result(
                json!({"references": refs, "context": context}),
                "Read contextual L1 memory events.",
            ))
        })
    }
}

impl Tool for ReadMemorySourceTool {
    fn name(&self) -> &str {
        "read_memory_source"
    }

    fn description(&self) -> &str {
        "Resolve a memory source reference to its complete L1 event snapshot. Supports legacy session references, but returns the canonical event-level reference."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {"reference": {"type": "string"}},
                "required": ["reference"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let reference = required_string(&args, "reference")?;
            let source = self
                .store
                .resolve_source_ref(reference)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            let canonical_reference = event_reference(&source.event);
            self.tracker.record_resolution(
                "reading_evidence",
                self.name(),
                format!("Resolved {reference} to {canonical_reference}"),
                reference,
                &canonical_reference,
            );
            Ok(json_ephemeral_result(
                json!({
                    "requested_reference": reference,
                    "canonical_reference": canonical_reference,
                    "event": source.event,
                }),
                "Resolved one L1 memory source.",
            ))
        })
    }
}

impl Tool for ListMemoryEntriesTool {
    fn name(&self) -> &str {
        "list_memory_entries"
    }

    fn description(&self) -> &str {
        "List bounded L2 memory entry summaries from the source files allowed for this L3 target. Results are candidates; call read_memory_entry before citing one."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| l2_entry_page_schema(false))
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let paths = selected_l2_paths(&args, &self.allowed_paths)?;
            let cursor = optional_string(&args, "cursor");
            let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 100) as usize;
            let page = self
                .store
                .query_l2_entries(&paths, None, cursor, limit)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            self.tracker.record_activity(
                "discovering_l2_sources",
                self.name(),
                format!(
                    "Listed {} of {} L2 memory entry candidates",
                    page.entries.len(),
                    page.total
                ),
                Vec::new(),
            );
            Ok(json_tool_result(json!({
                "entries": l2_entry_summaries(&page.entries),
                "next_cursor": page.next_cursor,
                "total": page.total,
            })))
        })
    }
}

impl Tool for SearchMemoryEntriesTool {
    fn name(&self) -> &str {
        "search_memory_entries"
    }

    fn description(&self) -> &str {
        "Search L2 memory entry summaries within the source files allowed for this L3 target. Call read_memory_entry before citing a result."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| l2_entry_page_schema(true))
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let query = required_string(&args, "query")?;
            let paths = selected_l2_paths(&args, &self.allowed_paths)?;
            let cursor = optional_string(&args, "cursor");
            let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 100) as usize;
            let page = self
                .store
                .query_l2_entries(&paths, Some(query), cursor, limit)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            self.tracker.record_activity(
                "discovering_l2_sources",
                self.name(),
                format!("Found {} L2 memory candidates for `{query}`", page.total),
                Vec::new(),
            );
            Ok(json_tool_result(json!({
                "entries": l2_entry_summaries(&page.entries),
                "next_cursor": page.next_cursor,
                "total": page.total,
            })))
        })
    }
}

impl Tool for ReadMemoryEntryTool {
    fn name(&self) -> &str {
        "read_memory_entry"
    }

    fn description(&self) -> &str {
        "Read one complete L2 memory entry by canonical memory:L2/path.md#m_id reference. A successful read makes that L2 reference citeable."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(l2_entry_read_schema)
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let reference = required_string(&args, "reference")?;
            let entry = self
                .store
                .read_l2_entry(reference)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            ensure_l2_path_allowed(&entry.path, &self.allowed_paths)?;
            self.tracker.record_l2(
                "reading_l2_evidence",
                self.name(),
                format!("Read {} L2 evidence", entry.reference),
                vec![entry.reference.clone()],
            );
            Ok(json_ephemeral_result(
                json!({ "memory": entry }),
                "Read one L2 memory entry.",
            ))
        })
    }
}

impl Tool for ReadMemoryEntrySourcesTool {
    fn name(&self) -> &str {
        "read_memory_entry_sources"
    }

    fn description(&self) -> &str {
        "Drill through one L2 memory entry to its complete L1 source events. Use this only when the L2 summary needs verification or more detail."
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(l2_entry_read_schema)
    }

    fn execute<'a>(
        &'a self,
        args: Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let reference = required_string(&args, "reference")?;
            let sources = self
                .store
                .read_l2_entry_sources(reference)
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            ensure_l2_path_allowed(&sources.memory.path, &self.allowed_paths)?;
            let l1_refs = sources
                .sources
                .iter()
                .map(|source| event_reference(&source.event))
                .collect::<Vec<_>>();
            self.tracker.record_l2(
                "verifying_l1_evidence",
                self.name(),
                format!(
                    "Verified {} through {} L1 source events",
                    sources.memory.reference,
                    l1_refs.len()
                ),
                vec![sources.memory.reference.clone()],
            );
            self.tracker.record_l1(
                "verifying_l1_evidence",
                self.name(),
                format!("Read {} supporting L1 source events", l1_refs.len()),
                l1_refs.clone(),
            );
            Ok(json_ephemeral_result(
                json!({
                    "memory": sources.memory,
                    "source_references": l1_refs,
                    "sources": sources.sources,
                }),
                "Read supporting L1 memory sources.",
            ))
        })
    }
}

fn l2_entry_page_schema(require_query: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "paths": {"type": "array", "items": {"type": "string"}, "maxItems": 4},
            "cursor": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100}
        }
    });
    if require_query {
        schema["properties"]["query"] = json!({"type": "string"});
        schema["required"] = json!(["query"]);
    }
    schema
}

fn l2_entry_read_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"reference": {"type": "string"}},
        "required": ["reference"]
    })
}

fn selected_l2_paths(args: &Value, allowed_paths: &[String]) -> Result<Vec<String>, ToolFailure> {
    let mut selected = args["paths"]
        .as_array()
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(path) = optional_string(args, "path") {
        selected.push(path.to_string());
    }
    if selected.is_empty() {
        return Ok(allowed_paths.to_vec());
    }
    selected.sort();
    selected.dedup();
    for path in &selected {
        ensure_l2_path_allowed(path, allowed_paths)?;
    }
    Ok(selected)
}

fn ensure_l2_path_allowed(path: &str, allowed_paths: &[String]) -> Result<(), ToolFailure> {
    if allowed_paths.iter().any(|allowed| allowed == path) {
        Ok(())
    } else {
        Err(ToolFailure::invalid_arguments(format!(
            "L2 memory path `{path}` is not available for this target"
        )))
    }
}

fn l2_entry_summaries(entries: &[MemoryL2Entry]) -> Vec<Value> {
    entries
        .iter()
        .map(|item| {
            json!({
                "reference": item.reference,
                "path": item.path,
                "revision": item.revision,
                "section": item.entry.section,
                "text": item.entry.text,
            })
        })
        .collect()
}

fn event_page_schema(require_query: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "surface": {"type": "string", "enum": ["chat", "quiz", "notebook", "knowledge"]},
            "session_id": {"type": "string"},
            "cursor": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100}
        }
    });
    if require_query {
        schema["properties"]["query"] = json!({"type": "string"});
        schema["required"] = json!(["query"]);
    }
    schema
}

fn event_summaries(events: &[MemoryEvent]) -> Vec<Value> {
    events
        .iter()
        .map(|event| {
            json!({
                "event_id": event.id,
                "reference": event_reference(event),
                "surface": format!("{:?}", event.category).to_lowercase(),
                "action": event.action,
                "summary": event.summary,
                "session_id": event.source_id,
                "created_at": event.created_at,
            })
        })
        .collect()
}

fn event_reference(event: &MemoryEvent) -> String {
    let surface = format!("{:?}", event.category).to_lowercase();
    format!("{surface}:{}", event.id)
}

fn optional_string<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolFailure> {
    optional_string(args, key)
        .ok_or_else(|| ToolFailure::invalid_arguments(format!("{key} is required")))
}

fn json_tool_result(value: Value) -> ToolResult {
    let content = vec![DataBlock::text(value.to_string())];
    ToolResult::projected(content.clone(), content, value, false)
}

fn json_ephemeral_result(value: Value, summary: impl Into<String>) -> ToolResult {
    ToolResult::ephemeral(
        vec![DataBlock::text(value.to_string())],
        summary,
        value,
        false,
    )
}

fn tool_execution_failure(message: impl Into<String>) -> ToolFailure {
    ToolFailure::new("memory_tool_failed", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use llm_harness_types::UnsupportedEnv;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn make_ctx() -> ToolContext {
        let (update_tx, _update_rx) = mpsc::channel(1);
        ToolContext {
            env: Arc::new(UnsupportedEnv::new()),
            run: Arc::new(llm_harness_types::RunContext::new(
                llm_harness_types::RunRequest::default(),
            )),
            abort: CancellationToken::new(),
            tool_use_id: "memory-tool-test".into(),
            turn_index: 0,
            assistant_message: Arc::new(llm_harness_types::AssistantMessage {
                kind: llm_harness_types::AssistantMessageKind::FinalAnswer,
                message_id: "message".into(),
                turn_id: "turn".into(),
                content: vec![],
                usage: None,
                stop_reason: None,
                timestamp: Utc::now(),
                provider: None,
                api: None,
                model: None,
                error_message: None,
            }),
            update_tx,
        }
    }

    #[test]
    fn event_schema_only_exposes_active_l1_surfaces() {
        let schema = event_page_schema(false);
        let values = schema["properties"]["surface"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["chat", "quiz", "notebook", "knowledge"]);
    }

    #[tokio::test]
    async fn listing_does_not_make_unread_events_citeable() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        store
            .record_event(
                crate::memory_store::MemoryEventCategory::Chat,
                "asked",
                "Asked about vector addition",
                Some("session-1".into()),
                json!({ "content": "full question" }),
            )
            .unwrap();
        let tracker = MemoryEvidenceTracker::default();
        let tool = ListMemoryEventsTool::new(store, tracker.clone());

        let result = tool
            .execute(json!({ "surface": "chat", "limit": 10 }), &make_ctx())
            .await
            .unwrap();

        assert_eq!(result.details["total"], 1);
        assert_eq!(tracker.read_refs(), Vec::<String>::new());
        assert!(result.details["events"][0].get("payload").is_none());
    }

    #[tokio::test]
    async fn reading_tracks_the_canonical_event_reference() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        let event = store
            .record_event(
                crate::memory_store::MemoryEventCategory::Quiz,
                "answered",
                "Answered a vector question",
                Some("quiz-1".into()),
                json!({ "answer": "complete answer" }),
            )
            .unwrap();
        let tracker = MemoryEvidenceTracker::default();
        let tool = ReadMemoryEventTool::new(store, tracker.clone());

        let result = tool
            .execute(json!({ "event_id": event.id }), &make_ctx())
            .await
            .unwrap();
        let reference = result.details["reference"].as_str().unwrap().to_string();

        assert_eq!(tracker.read_refs(), vec![reference]);
        assert_eq!(
            result.details["event"]["payload"]["answer"],
            "complete answer"
        );
    }

    #[tokio::test]
    async fn source_reads_map_product_id_aliases_to_canonical_event_refs() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        let entry_id = "4747cc47-597a-410b-a073-5881480bb4c6";
        let event = store
            .record_event(
                crate::memory_store::MemoryEventCategory::Notebook,
                "created",
                "Created notebook entry",
                Some(entry_id.into()),
                json!({ "title": "Transformer notes" }),
            )
            .unwrap();
        let tracker = MemoryEvidenceTracker::default();
        let tool = ReadMemorySourceTool::new(store, tracker.clone());
        let alias = format!("notebook:{entry_id}");
        let canonical = format!("notebook:{}", event.id);

        let result = tool
            .execute(json!({ "reference": alias }), &make_ctx())
            .await
            .unwrap();

        assert_eq!(result.details["canonical_reference"], canonical);
        assert_eq!(tracker.read_refs(), vec![canonical.clone()]);
        assert_eq!(tracker.canonical_reference(&alias), Some(canonical));
    }

    #[tokio::test]
    async fn l2_listing_is_not_citeable_until_the_entry_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n## Topics\n\n- Learns vectors visually. <!--m_visual-->".into(),
            )
            .unwrap();
        let tracker = MemoryEvidenceTracker::default();
        let allowed = vec!["L2/chat.md".into()];
        let list = ListMemoryEntriesTool::new(store.clone(), tracker.clone(), allowed.clone());

        let listed = list
            .execute(json!({ "path": "L2/chat.md" }), &make_ctx())
            .await
            .unwrap();
        let reference = listed.details["entries"][0]["reference"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(tracker.read_l2_refs().is_empty());
        assert_eq!(tracker.canonical_reference(&reference), None);

        let read = ReadMemoryEntryTool::new(store, tracker.clone(), allowed);
        read.execute(json!({ "reference": reference }), &make_ctx())
            .await
            .unwrap();
        assert_eq!(tracker.read_l2_refs().len(), 1);
    }

    #[tokio::test]
    async fn l2_source_drill_down_tracks_l2_and_l1_reads_separately() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        let event = store
            .record_event(
                crate::memory_store::MemoryEventCategory::Notebook,
                "created",
                "Saved a transformer note",
                Some("note-1".into()),
                json!({ "title": "Transformer" }),
            )
            .unwrap();
        store
            .write(
                "L2/notebook.md",
                format!(
                    "# Notebook memory\n\n## Themes\n\n- Studies transformers. [^1] <!--m_transformer-->\n\n---\n\n[^1]: notebook:{}",
                    event.id
                ),
            )
            .unwrap();
        let tracker = MemoryEvidenceTracker::default();
        let reference = "memory:L2/notebook.md#m_transformer";
        let tool =
            ReadMemoryEntrySourcesTool::new(store, tracker.clone(), vec!["L2/notebook.md".into()]);

        tool.execute(json!({ "reference": reference }), &make_ctx())
            .await
            .unwrap();

        assert_eq!(tracker.read_l2_refs(), vec![reference.to_string()]);
        assert_eq!(
            tracker.read_l1_refs(),
            vec![format!("notebook:{}", event.id)]
        );
    }
}
