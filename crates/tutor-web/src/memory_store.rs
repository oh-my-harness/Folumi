use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const DEFAULT_FILES: &[(&str, &str)] = &[
    ("L2/chat.md", "# Chat memory\n\n"),
    ("L2/notebook.md", "# Notebook memory\n\n"),
    ("L2/knowledge.md", "# Knowledge memory\n\n"),
    ("L3/recent.md", "# Recent context\n\n"),
    (
        "L3/profile.md",
        "# User profile\n\n## Identity\n\n## Preferences\n\n",
    ),
    ("L3/scope.md", "# Working scope\n\n"),
    ("L3/preferences.md", "# User preferences\n\n"),
    (
        "L3/continuity.md",
        "# Assistant continuity\n\n## Commitments\n\n## Open loops\n\n## Strategies\n\n",
    ),
    (
        "L3/teaching_strategy.md",
        "# Assistant strategy\n\n## Preferred approach\n\n",
    ),
];

const DEFAULT_DIRS: &[&str] = &["L1", "L2", "L3"];
const MAX_L2_MEMORY_ENTRY_TEXT_CHARS: usize = 500;
const MAX_L3_MEMORY_ENTRY_TEXT_CHARS: usize = 1_200;
const MEMORY_ENTRY_METADATA_SCHEMA_VERSION: u32 = 1;
const MEMORY_ENTRY_METADATA_COMMENT_PREFIX: &str = "<!--llm-tutor-memory:";
const MEMORY_ENTRY_METADATA_V1_PREFIX: &str = "llm-tutor-memory:v1:";

type TargetLock = Arc<Mutex<()>>;
type TargetLocks = Arc<Mutex<HashMap<PathBuf, TargetLock>>>;

#[derive(Clone)]
pub struct FileMemoryBackend {
    root: PathBuf,
    target_locks: TargetLocks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFile {
    pub path: String,
    pub level: String,
    pub name: String,
    pub markdown: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUndoResult {
    pub file: MemoryFile,
    pub restored_from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEvent {
    pub id: String,
    pub category: MemoryEventCategory,
    pub action: String,
    pub summary: String,
    pub source_id: Option<String>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedMemorySource {
    pub reference: String,
    pub event: MemoryEvent,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEventPage {
    pub events: Vec<MemoryEvent>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEventContext {
    pub event: MemoryEvent,
    pub before: Vec<MemoryEvent>,
    pub after: Vec<MemoryEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEventCatalogItem {
    pub surface: String,
    pub count: usize,
    pub latest_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEventCategory {
    Chat,
    Notebook,
    Knowledge,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAssistAction {
    Update,
    Check,
    Dedupe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemorySourceRef {
    pub index: usize,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryEntryMetadata {
    pub schema_version: u32,
    pub item_id: String,
    pub kind: String,
    pub target: String,
    pub provenance: serde_json::Value,
    pub idempotency_key: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub line_number: usize,
    pub section: Option<String>,
    pub text: String,
    pub marker: String,
    pub source_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MemoryEntryMetadata>,
}

#[derive(Debug, Clone)]
pub struct DurableMemoryWrite {
    pub content: String,
    pub kind: String,
    pub provenance: serde_json::Value,
    pub idempotency_key: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactMemoryDeleteOutcome {
    Deleted,
    NotFound,
    Stale { latest_revision: String },
}

fn durable_memory_spec(kind: &str) -> Result<(&'static str, &'static str, &'static str)> {
    match kind {
        "profile" => Ok(("L3/profile.md", "Identity", "User profile")),
        "preference" => Ok(("L3/preferences.md", "Preferences", "User preferences")),
        "commitment" => Ok(("L3/continuity.md", "Commitments", "Assistant continuity")),
        "open_loop" => Ok(("L3/continuity.md", "Open loops", "Assistant continuity")),
        "strategy" => Ok(("L3/continuity.md", "Strategies", "Assistant continuity")),
        _ => Err(anyhow!("unsupported durable memory kind")),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryL2Entry {
    pub reference: String,
    pub path: String,
    pub revision: String,
    pub entry: MemoryEntry,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryL2EntryPage {
    pub entries: Vec<MemoryL2Entry>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryL2EntrySources {
    pub memory: MemoryL2Entry,
    pub sources: Vec<ResolvedMemorySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryTargetCatalog {
    pub title: String,
    #[serde(rename = "existingMarkdown")]
    pub existing_markdown: String,
    #[serde(rename = "allowedSections")]
    pub allowed_sections: Vec<String>,
    pub focus: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryChangeOp {
    Insert,
    Replace,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryFinding {
    pub id: String,
    pub entry_id: Option<String>,
    pub severity: String,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryChange {
    pub id: String,
    pub op: MemoryChangeOp,
    pub section: Option<String>,
    pub entry_id: Option<String>,
    pub after_entry_id: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
    pub reason: String,
    pub before_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryChangeSet {
    pub run_id: String,
    pub target_path: String,
    pub base_revision: String,
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<MemoryFinding>,
    #[serde(default)]
    pub changes: Vec<MemoryChange>,
}

impl FileMemoryBackend {
    pub fn new() -> Self {
        Self::new_with_root(default_root().join("memory"))
    }

    pub fn new_with_root(root: PathBuf) -> Self {
        let store = Self {
            root,
            target_locks: Arc::new(Mutex::new(HashMap::new())),
        };
        store
            .ensure_skeleton()
            .expect("failed to create memory directory skeleton");
        store
    }

    pub fn list(&self) -> Result<Vec<MemoryFile>> {
        self.ensure_skeleton()?;
        let mut files = Vec::new();
        for (path, _) in DEFAULT_FILES {
            files.push(self.read(path)?);
        }
        Ok(files)
    }

    pub fn read(&self, path: &str) -> Result<MemoryFile> {
        self.ensure_skeleton()?;
        let path = normalize_memory_path(path)?;
        self.read_normalized(&path)
    }

    fn read_normalized(&self, path: &Path) -> Result<MemoryFile> {
        let full_path = self.root.join(path);
        let markdown = fs::read_to_string(&full_path)?;
        let level = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        Ok(MemoryFile {
            path: path_to_slash(path),
            level,
            name,
            revision: memory_revision(&markdown),
            markdown,
        })
    }

    pub fn write(&self, path: &str, markdown: String) -> Result<MemoryFile> {
        self.ensure_skeleton()?;
        let path = normalize_memory_path(path)?;
        let markdown = normalize_memory_markdown(&markdown)?;
        if markdown.trim().is_empty() {
            return Err(anyhow!("memory markdown is empty"));
        }
        let target_lock = self.target_lock(&path)?;
        let _guard = lock(&target_lock)?;
        self.write_normalized(&path, markdown)
    }

    pub fn upsert_durable_memory(&self, write: DurableMemoryWrite) -> Result<MemoryEntry> {
        let (target, section, default_title) = durable_memory_spec(&write.kind)?;
        let content = write.content.trim();
        if content.is_empty() || content.chars().count() > MAX_L3_MEMORY_ENTRY_TEXT_CHARS {
            return Err(anyhow!("durable memory content is invalid"));
        }
        if write.idempotency_key.trim().is_empty() || !write.provenance.is_object() {
            return Err(anyhow!("durable memory metadata is invalid"));
        }

        self.ensure_skeleton()?;
        let path = normalize_memory_path(target)?;
        let target_lock = self.target_lock(&path)?;
        let _guard = lock(&target_lock)?;
        let current = self.read_normalized(&path)?;
        let mut entries = try_parse_memory_entries(&current.markdown)?;
        if let Some(existing) = entries.iter().find(|entry| {
            entry
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.idempotency_key == write.idempotency_key)
        }) {
            return Ok(existing.clone());
        }

        let marker = format!("m_{}", uuid::Uuid::new_v4().simple());
        let entry = MemoryEntry {
            line_number: 0,
            section: Some(section.into()),
            text: content.to_string(),
            marker: marker.clone(),
            source_refs: Vec::new(),
            metadata: Some(MemoryEntryMetadata {
                schema_version: MEMORY_ENTRY_METADATA_SCHEMA_VERSION,
                item_id: marker,
                kind: write.kind,
                target: target.into(),
                provenance: write.provenance,
                idempotency_key: write.idempotency_key,
                expires_at: write.expires_at,
            }),
        };
        entries.push(entry.clone());
        let title = memory_title(&current.markdown).unwrap_or_else(|| default_title.into());
        let markdown = serialize_memory_entries(&title, &entries)?;
        self.write_normalized(&path, markdown)?;
        Ok(entry)
    }

    #[cfg(test)]
    pub fn upsert_durable_preference(&self, write: DurableMemoryWrite) -> Result<MemoryEntry> {
        if write.kind != "preference" {
            return Err(anyhow!("unsupported durable memory kind"));
        }
        self.upsert_durable_memory(write)
    }

    pub fn delete_durable_memory(
        &self,
        kind: &str,
        marker: &str,
        expected_revision: &str,
    ) -> Result<ExactMemoryDeleteOutcome> {
        let (target, _, default_title) = durable_memory_spec(kind)?;
        serialize_memory_marker(marker)?;
        if expected_revision.trim().is_empty() {
            return Err(anyhow!("expected durable memory revision is empty"));
        }

        self.ensure_skeleton()?;
        let path = normalize_memory_path(target)?;
        let target_lock = self.target_lock(&path)?;
        let _guard = lock(&target_lock)?;
        let current = self.read_normalized(&path)?;
        let mut entries = try_parse_memory_entries(&current.markdown)?;
        let Some(index) = entries.iter().position(|entry| entry.marker == marker) else {
            return Ok(ExactMemoryDeleteOutcome::NotFound);
        };
        let latest_revision = memory_entry_revision(&entries[index])?;
        if latest_revision != expected_revision {
            return Ok(ExactMemoryDeleteOutcome::Stale { latest_revision });
        }
        entries.remove(index);
        let title = memory_title(&current.markdown).unwrap_or_else(|| default_title.into());
        let markdown = serialize_memory_entries(&title, &entries)?;
        self.write_normalized(&path, markdown)?;
        Ok(ExactMemoryDeleteOutcome::Deleted)
    }

    #[cfg(test)]
    pub fn delete_durable_preference(
        &self,
        marker: &str,
        expected_revision: &str,
    ) -> Result<ExactMemoryDeleteOutcome> {
        self.delete_durable_memory("preference", marker, expected_revision)
    }

    // Phase 4 will schedule this through the composed runtime boundary.
    #[allow(dead_code)]
    pub fn cleanup_expired_durable_preferences(&self, now: DateTime<Utc>) -> Result<usize> {
        const TARGET: &str = "L3/preferences.md";
        self.ensure_skeleton()?;
        let path = normalize_memory_path(TARGET)?;
        let target_lock = self.target_lock(&path)?;
        let _guard = lock(&target_lock)?;
        let current = self.read_normalized(&path)?;
        let mut entries = try_parse_memory_entries(&current.markdown)?;
        let original_len = entries.len();
        entries.retain(|entry| {
            !entry
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.expires_at)
                .is_some_and(|expires_at| expires_at <= now)
        });
        let removed = original_len - entries.len();
        if removed == 0 {
            return Ok(0);
        }
        let title = memory_title(&current.markdown).unwrap_or_else(|| "User preferences".into());
        let markdown = serialize_memory_entries(&title, &entries)?;
        self.write_normalized(&path, markdown)?;
        Ok(removed)
    }

    pub fn undo_latest_write(&self, path: &str) -> Result<MemoryUndoResult> {
        self.ensure_skeleton()?;
        let path = normalize_memory_path(path)?;
        let target_lock = self.target_lock(&path)?;
        let _guard = lock(&target_lock)?;
        let undo_path = self.undo_path(&path);
        if !undo_path.exists() {
            return Err(anyhow!("no memory undo snapshot exists for this file"));
        }
        let markdown = fs::read_to_string(&undo_path)?;
        atomic_write(&self.root.join(&path), markdown.as_bytes())?;
        fs::remove_file(&undo_path)?;
        let restored_from = path_to_slash(&path);
        Ok(MemoryUndoResult {
            file: self.read_normalized(&path)?,
            restored_from,
        })
    }

    pub fn record_event(
        &self,
        category: MemoryEventCategory,
        action: impl Into<String>,
        summary: impl Into<String>,
        source_id: Option<String>,
        payload: serde_json::Value,
    ) -> Result<MemoryEvent> {
        self.ensure_skeleton()?;
        let event = MemoryEvent {
            id: uuid::Uuid::new_v4().to_string(),
            category,
            action: action.into(),
            summary: summary.into().chars().take(500).collect(),
            source_id: source_id.and_then(clean_optional),
            payload,
            created_at: Utc::now(),
        };
        if event.summary.trim().is_empty() {
            return Err(anyhow!("memory event summary is empty"));
        }
        let relative_path = PathBuf::from(event_file(category));
        let target_lock = self.target_lock(&relative_path)?;
        let _guard = lock(&target_lock)?;
        let path = self.root.join(&relative_path);
        let mut line = serde_json::to_string(&event)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(line.as_bytes())?;
        file.sync_all()?;
        Ok(event)
    }

    pub fn recent_events(&self, limit: usize) -> Result<Vec<MemoryEvent>> {
        let mut events = self.all_events()?;
        events.sort_by_key(|event| std::cmp::Reverse(event.created_at));
        events.truncate(limit);
        Ok(events)
    }

    pub fn event_catalog(&self) -> Result<Vec<MemoryEventCatalogItem>> {
        let events = self.all_events()?;
        let mut catalog = Vec::new();
        for category in all_event_categories() {
            let matching = events
                .iter()
                .filter(|event| event.category == category)
                .collect::<Vec<_>>();
            catalog.push(MemoryEventCatalogItem {
                surface: event_surface(category).to_string(),
                count: matching.len(),
                latest_at: matching.iter().map(|event| event.created_at).max(),
            });
        }
        Ok(catalog)
    }

    #[cfg(test)]
    pub fn query_events(
        &self,
        surface: Option<&str>,
        query: Option<&str>,
        session_id: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<MemoryEventPage> {
        let category = match surface {
            Some(value) => Some(
                category_for_surface(value)
                    .ok_or_else(|| anyhow!("unsupported memory event surface `{value}`"))?,
            ),
            None => None,
        };
        let query = query.map(str::trim).filter(|value| !value.is_empty());
        let session_id = session_id.map(str::trim).filter(|value| !value.is_empty());
        let offset = cursor
            .map(str::parse::<usize>)
            .transpose()
            .map_err(|_| anyhow!("invalid memory event cursor"))?
            .unwrap_or(0);
        let limit = limit.clamp(1, 100);
        let mut events = self.all_events()?;
        events.sort_by_key(|event| std::cmp::Reverse(event.created_at));
        events.retain(|event| {
            category.is_none_or(|value| event.category == value)
                && session_id.is_none_or(|value| event.source_id.as_deref() == Some(value))
                && query.is_none_or(|value| event_matches_query(event, value))
        });
        let total = events.len();
        let page = events
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let next_offset = offset.saturating_add(page.len());
        Ok(MemoryEventPage {
            events: page,
            next_cursor: (next_offset < total).then(|| next_offset.to_string()),
            total,
        })
    }

    #[cfg(test)]
    pub fn read_event(&self, event_id: &str) -> Result<MemoryEvent> {
        let event_id = event_id.trim();
        self.all_events()?
            .into_iter()
            .find(|event| event.id == event_id)
            .ok_or_else(|| anyhow!("memory event `{event_id}` was not found"))
    }

    #[cfg(test)]
    pub fn event_context(
        &self,
        event_id: &str,
        before: usize,
        after: usize,
    ) -> Result<MemoryEventContext> {
        let event = self.read_event(event_id)?;
        let mut related = self
            .all_events()?
            .into_iter()
            .filter(|candidate| {
                candidate.category == event.category
                    && match event.source_id.as_deref() {
                        Some(source_id) => candidate.source_id.as_deref() == Some(source_id),
                        None => candidate.id == event.id,
                    }
            })
            .collect::<Vec<_>>();
        related.sort_by_key(|event| event.created_at);
        let index = related
            .iter()
            .position(|candidate| candidate.id == event.id)
            .ok_or_else(|| anyhow!("memory event context is unavailable"))?;
        let before_start = index.saturating_sub(before.min(20));
        let after_end = (index + 1 + after.min(20)).min(related.len());
        Ok(MemoryEventContext {
            event,
            before: related[before_start..index].to_vec(),
            after: related[index + 1..after_end].to_vec(),
        })
    }

    pub fn resolve_source_ref(&self, reference: &str) -> Result<ResolvedMemorySource> {
        self.ensure_skeleton()?;
        let reference = reference.trim();
        let (surface, id) = reference
            .split_once(':')
            .ok_or_else(|| anyhow!("memory source ref must look like surface:id"))?;
        let category = category_for_surface(surface)
            .ok_or_else(|| anyhow!("unsupported memory source surface `{surface}`"))?;
        let path = self.root.join(event_file(category));
        let text = fs::read_to_string(path)?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let event = serde_json::from_str::<MemoryEvent>(line)?;
            if event.id == id || event.source_id.as_deref() == Some(id) {
                return Ok(ResolvedMemorySource {
                    reference: reference.to_string(),
                    event,
                });
            }
        }
        Err(anyhow!("memory source ref `{reference}` was not found"))
    }

    #[cfg(test)]
    pub fn query_l2_entries(
        &self,
        paths: &[String],
        query: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<MemoryL2EntryPage> {
        let selected_paths = if paths.is_empty() {
            DEFAULT_FILES
                .iter()
                .map(|(path, _)| *path)
                .filter(|path| path.starts_with("L2/"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        } else {
            paths
                .iter()
                .map(|path| validate_l2_path(path))
                .collect::<Result<Vec<_>>>()?
        };
        let query = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);
        let mut results = Vec::new();
        for path in selected_paths {
            let file = self.read(&path)?;
            for entry in try_parse_memory_entries(&file.markdown)? {
                let matches = query.as_ref().is_none_or(|query| {
                    entry.text.to_lowercase().contains(query)
                        || entry
                            .section
                            .as_deref()
                            .is_some_and(|section| section.to_lowercase().contains(query))
                });
                if matches {
                    results.push(memory_l2_entry(&file, entry)?);
                }
            }
        }
        let total = results.len();
        let offset = cursor
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| anyhow!("invalid L2 memory cursor"))
            })
            .transpose()?
            .unwrap_or_default()
            .min(total);
        let limit = limit.clamp(1, 100);
        let entries = results
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let next_offset = offset + entries.len();
        Ok(MemoryL2EntryPage {
            entries,
            next_cursor: (next_offset < total).then(|| next_offset.to_string()),
            total,
        })
    }

    pub fn read_l2_entry(&self, reference: &str) -> Result<MemoryL2Entry> {
        let (path, marker) = parse_l2_entry_reference(reference)?;
        let file = self.read(&path)?;
        let entry = try_parse_memory_entries(&file.markdown)?
            .into_iter()
            .find(|entry| entry.marker == marker)
            .ok_or_else(|| anyhow!("L2 memory entry `{reference}` was not found"))?;
        memory_l2_entry(&file, entry)
    }

    #[cfg(test)]
    pub fn read_l2_entry_sources(&self, reference: &str) -> Result<MemoryL2EntrySources> {
        let memory = self.read_l2_entry(reference)?;
        let sources = memory
            .entry
            .source_refs
            .iter()
            .map(|reference| self.resolve_source_ref(reference))
            .collect::<Result<Vec<_>>>()?;
        Ok(MemoryL2EntrySources { memory, sources })
    }

    pub fn apply_memory_changes(
        &self,
        target_path: &str,
        base_revision: &str,
        changes: &[MemoryChange],
        accepted_change_ids: &[String],
    ) -> Result<MemoryFile> {
        self.ensure_skeleton()?;
        let normalized_target = normalize_memory_path(target_path)?;
        let target_lock = self.target_lock(&normalized_target)?;
        let _guard = lock(&target_lock)?;
        let current = self.read_normalized(&normalized_target)?;
        if current.revision != base_revision {
            return Err(anyhow!(
                "memory document changed since this run; rerun before applying"
            ));
        }
        let accepted = accepted_change_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let selected = changes
            .iter()
            .filter(|change| accepted.contains(change.id.as_str()))
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return if accepted_change_ids.is_empty() {
                Ok(current)
            } else {
                Err(anyhow!(
                    "none of the accepted memory change ids matched the review"
                ))
            };
        }
        let target = target_catalog(target_path, current.markdown.clone());
        let allowed_sections = target
            .allowed_sections
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let mut entries = try_parse_memory_entries(&current.markdown)?;
        for change in selected {
            validate_memory_change(target_path, change, &allowed_sections)?;
            for reference in &change.refs {
                if reference.starts_with("memory:L2/") {
                    self.read_l2_entry(reference)?;
                } else if reference.contains(':') {
                    self.resolve_source_ref(reference)?;
                }
            }
            match change.op {
                MemoryChangeOp::Insert => {
                    let entry = memory_entry_from_change(change)?;
                    let index = change
                        .after_entry_id
                        .as_deref()
                        .and_then(|id| entries.iter().position(|entry| entry.marker == id))
                        .map(|index| index + 1)
                        .unwrap_or(entries.len());
                    entries.insert(index, entry);
                }
                MemoryChangeOp::Replace => {
                    let entry_id = change.entry_id.as_deref().unwrap_or_default();
                    let entry = entries
                        .iter_mut()
                        .find(|entry| entry.marker == entry_id)
                        .ok_or_else(|| anyhow!("memory entry `{entry_id}` was not found"))?;
                    entry.text = change
                        .text
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if let Some(section) = change.section.as_deref() {
                        entry.section = Some(section.trim().to_string());
                    }
                    if !change.refs.is_empty() {
                        entry.source_refs = change.refs.clone();
                    }
                }
                MemoryChangeOp::Delete => {
                    let entry_id = change.entry_id.as_deref().unwrap_or_default();
                    let index = entries
                        .iter()
                        .position(|entry| entry.marker == entry_id)
                        .ok_or_else(|| anyhow!("memory entry `{entry_id}` was not found"))?;
                    entries.remove(index);
                }
            }
        }
        let title = memory_title(&current.markdown).unwrap_or(target.title);
        let markdown = serialize_memory_entries(&title, &entries)?;
        self.write_normalized(&normalized_target, markdown)
    }

    pub fn agent_context(&self, target_path: &str, current: &str) -> Result<serde_json::Value> {
        let target_path = path_to_slash(&normalize_memory_path(target_path)?);
        let target = target_catalog(&target_path, current.to_string());
        let default_surface = target_surface(&target_path);
        let mut context = serde_json::json!({
            "target": {
                "path": &target_path,
                "title": target.title,
                "focus": target.focus,
                "allowedSections": target.allowed_sections,
                "baseRevision": memory_revision(current),
                "defaultSurface": default_surface,
            },
        });
        if target_path.starts_with("L3/") {
            let allowed_paths = l3_source_paths(&target_path);
            context["l2Catalog"] = serde_json::Value::Array(
                allowed_paths
                    .iter()
                    .map(|path| {
                        let file = self.read(path)?;
                        Ok(serde_json::json!({
                            "path": file.path,
                            "revision": file.revision,
                            "entryCount": try_parse_memory_entries(&file.markdown)?.len(),
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
            context["instructions"] = serde_json::json!({
                "evidenceLayer": "L2",
                "allowedL2Paths": allowed_paths,
                "readBeforeCiting": true,
                "boundedL1Exception": target_path == "L3/recent.md",
            });
            if target_path == "L3/recent.md" {
                context["l1Catalog"] = serde_json::to_value(self.event_catalog()?)?;
            }
        } else {
            context["l1Catalog"] = serde_json::to_value(self.event_catalog()?)?;
            context["instructions"] = serde_json::json!({
                "evidenceLayer": "L1",
                "allL1Addressable": true,
                "startWithTargetSurface": true,
                "readBeforeCiting": true,
            });
        }
        Ok(context)
    }

    pub(crate) fn all_events(&self) -> Result<Vec<MemoryEvent>> {
        self.ensure_skeleton()?;
        let mut events = Vec::new();
        for category in all_event_categories() {
            let path = self.root.join(event_file(category));
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
                if let Ok(event) = serde_json::from_str::<MemoryEvent>(line) {
                    events.push(event);
                }
            }
        }
        Ok(events)
    }

    fn ensure_skeleton(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        for dir in DEFAULT_DIRS {
            fs::create_dir_all(self.root.join(dir))?;
        }
        fs::create_dir_all(self.root.join(".undo"))?;
        for (path, default_markdown) in DEFAULT_FILES {
            let full_path = self.root.join(path);
            if !full_path.exists() {
                fs::write(full_path, default_markdown)?;
            }
        }
        for obsolete_path in ["L1/research_events.jsonl", "L2/research.md"] {
            match fs::remove_file(self.root.join(obsolete_path)) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    fn target_lock(&self, path: &Path) -> Result<TargetLock> {
        let mut locks = self
            .target_locks
            .lock()
            .map_err(|_| anyhow!("memory target lock registry is poisoned"))?;
        Ok(locks
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone())
    }

    fn write_normalized(&self, path: &Path, markdown: String) -> Result<MemoryFile> {
        let full_path = self.root.join(path);
        let previous = full_path
            .exists()
            .then(|| fs::read(&full_path))
            .transpose()?;
        let undo_path = self.undo_path(path);
        let previous_undo = undo_path
            .exists()
            .then(|| fs::read(&undo_path))
            .transpose()?;

        if let Some(previous) = previous.as_deref() {
            self.write_undo_snapshot(path, previous)?;
        }
        if let Err(error) = atomic_write(&full_path, markdown.as_bytes()) {
            restore_undo_after_failed_write(&undo_path, previous_undo.as_deref())?;
            return Err(error);
        }
        self.read_normalized(path)
    }

    fn write_undo_snapshot(&self, path: &Path, markdown: &[u8]) -> Result<()> {
        let undo_path = self.undo_path(path);
        if let Some(parent) = undo_path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&undo_path, markdown)?;
        Ok(())
    }

    fn undo_path(&self, path: &Path) -> PathBuf {
        self.root.join(".undo").join(path).with_extension("md.bak")
    }
}

impl Default for FileMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn lock(target: &Mutex<()>) -> Result<std::sync::MutexGuard<'_, ()>> {
    target
        .lock()
        .map_err(|_| anyhow!("memory target lock is poisoned"))
}

fn restore_undo_after_failed_write(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    match previous {
        Some(previous) => atomic_write(path, previous),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
    }
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("memory target has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("memory target file name is invalid"))?;
    let temporary = parent.join(format!(
        ".{file_name}.llm-tutor-memory-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)?;
        sync_parent_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // Both UTF-16 buffers are NUL-terminated and remain alive for the call.
    // The temporary file is created beside the target, so replacement stays
    // on one volume and never falls back to a copy/delete sequence.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target)?;
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<()> {
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<()> {
    Ok(())
}

pub fn parse_source_refs(markdown: &str) -> Vec<MemorySourceRef> {
    markdown.lines().filter_map(parse_source_ref_line).collect()
}

// Kept as a tolerant boundary for recovery-oriented UI callers. Runtime and
// mutation paths must use `try_parse_memory_entries` and fail closed.
#[allow(dead_code)]
pub fn parse_memory_entries(markdown: &str) -> Vec<MemoryEntry> {
    parse_memory_entries_with_mode(markdown, false)
        .expect("compatible memory parsing is infallible")
}

pub fn try_parse_memory_entries(markdown: &str) -> Result<Vec<MemoryEntry>> {
    parse_memory_entries_with_mode(markdown, true)
}

fn parse_memory_entries_with_mode(
    markdown: &str,
    strict_metadata: bool,
) -> Result<Vec<MemoryEntry>> {
    let definitions = parse_source_refs(markdown)
        .into_iter()
        .map(|reference| (reference.index, reference.target))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut section = None::<String>;
    let mut entries = Vec::new();
    for (index, line) in markdown.lines().enumerate() {
        if let Some(heading) = memory_section_heading(line) {
            section = Some(heading);
            continue;
        }
        if let Some(entry) = parse_memory_entry_line(
            index + 1,
            line,
            section.clone(),
            &definitions,
            strict_metadata,
        )? {
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub fn l2_entry_reference(path: &str, marker: &str) -> Result<String> {
    let path = validate_l2_path(path)?;
    let marker = marker.trim();
    serialize_memory_marker(marker)?;
    Ok(format!("memory:{path}#{marker}"))
}

pub fn parse_l2_entry_reference(reference: &str) -> Result<(String, String)> {
    let value = reference
        .trim()
        .strip_prefix("memory:")
        .ok_or_else(|| anyhow!("L2 memory ref must start with `memory:`"))?;
    let (path, marker) = value
        .split_once('#')
        .ok_or_else(|| anyhow!("L2 memory ref must look like memory:L2/path.md#m_id"))?;
    let path = validate_l2_path(path)?;
    serialize_memory_marker(marker)?;
    Ok((path, marker.to_string()))
}

pub fn serialize_memory_entries(title: &str, entries: &[MemoryEntry]) -> Result<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(anyhow!("memory title is empty"));
    }
    let mut refs_by_target = std::collections::BTreeMap::<String, usize>::new();
    let mut next_ref = 1usize;
    let mut lines = vec![format!("# {title}")];
    let mut last_section = None::<String>;
    for entry in entries {
        if entry.text.trim().is_empty() {
            return Err(anyhow!("memory entry text is empty"));
        }
        let section = entry
            .section
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if lines.len() == 1 || section != last_section {
            lines.push(String::new());
            if let Some(section) = &section {
                lines.push(format!("## {section}"));
                lines.push(String::new());
            }
            last_section = section;
        }
        let marker = serialize_memory_marker(&entry.marker)?;
        let metadata = entry
            .metadata
            .as_ref()
            .map(|metadata| serialize_memory_entry_metadata(&entry.marker, metadata))
            .transpose()?
            .map(|metadata| format!(" {metadata}"))
            .unwrap_or_default();
        let refs = entry
            .source_refs
            .iter()
            .filter_map(|target| {
                let target = target.trim();
                if target.is_empty() {
                    return None;
                }
                let index = if let Some(index) = refs_by_target.get(target) {
                    *index
                } else {
                    let index = next_ref;
                    next_ref += 1;
                    refs_by_target.insert(target.to_string(), index);
                    index
                };
                Some(format!("[^{index}]"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let refs = if refs.is_empty() {
            String::new()
        } else {
            format!(" {refs}")
        };
        lines.push(format!(
            "- {}{} {}{}",
            entry.text.trim(),
            refs,
            marker,
            metadata
        ));
    }
    if !refs_by_target.is_empty() {
        lines.push(String::new());
        lines.push("---".into());
        lines.push(String::new());
        let refs = refs_by_target
            .iter()
            .map(|(target, index)| {
                serialize_source_ref(&MemorySourceRef {
                    index: *index,
                    target: target.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        lines.extend(refs);
    }
    Ok(lines.join("\n"))
}

pub fn serialize_memory_marker(id: &str) -> Result<String> {
    let id = id.trim();
    if id.is_empty() || !id.starts_with("m_") || id.contains("-->") {
        return Err(anyhow!("invalid memory marker id"));
    }
    Ok(format!("<!--{id}-->"))
}

pub fn memory_entry_revision(entry: &MemoryEntry) -> Result<String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RevisionInput<'a> {
        marker: &'a str,
        section: Option<&'a str>,
        text: &'a str,
        source_refs: &'a [String],
        metadata: &'a Option<MemoryEntryMetadata>,
    }

    let input = RevisionInput {
        marker: entry.marker.trim(),
        section: entry.section.as_deref().map(str::trim),
        text: entry.text.trim(),
        source_refs: &entry.source_refs,
        metadata: &entry.metadata,
    };
    let normalized = serde_json::to_string(&input)?;
    Ok(memory_revision(&normalized))
}

fn serialize_memory_entry_metadata(marker: &str, metadata: &MemoryEntryMetadata) -> Result<String> {
    validate_memory_entry_metadata(marker, metadata)?;
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(metadata)?);
    Ok(format!("<!--{MEMORY_ENTRY_METADATA_V1_PREFIX}{encoded}-->"))
}

pub fn serialize_source_ref(reference: &MemorySourceRef) -> Result<String> {
    if reference.index == 0 || reference.target.trim().is_empty() {
        return Err(anyhow!("invalid memory source reference"));
    }
    Ok(format!(
        "[^{}]: {}",
        reference.index,
        reference.target.trim()
    ))
}

pub fn normalize_memory_markdown(markdown: &str) -> Result<String> {
    let mut definitions = std::collections::BTreeMap::<usize, String>::new();
    let mut body_lines = Vec::new();
    for line in markdown.lines() {
        if let Some(reference) = parse_source_ref_line(line) {
            definitions
                .entry(reference.index)
                .or_insert(reference.target);
        } else {
            body_lines.push(line.to_string());
        }
    }

    let mut old_to_new = std::collections::BTreeMap::<usize, usize>::new();
    let mut target_to_new = std::collections::BTreeMap::<String, usize>::new();
    let mut normalized_refs = Vec::<MemorySourceRef>::new();
    for line in &body_lines {
        for old_index in footnote_indices_in_line(line) {
            let Some(target) = definitions.get(&old_index).cloned() else {
                continue;
            };
            if let Some(new_index) = target_to_new.get(&target).copied() {
                old_to_new.insert(old_index, new_index);
                continue;
            }
            let new_index = normalized_refs.len() + 1;
            target_to_new.insert(target.clone(), new_index);
            old_to_new.insert(old_index, new_index);
            normalized_refs.push(MemorySourceRef {
                index: new_index,
                target,
            });
        }
    }

    let mut normalized_body = body_lines
        .iter()
        .map(|line| replace_footnote_indices(line, &old_to_new))
        .collect::<Vec<_>>();
    while normalized_body
        .last()
        .is_some_and(|line| line.trim().is_empty())
    {
        normalized_body.pop();
    }
    let mut result = normalized_body.join("\n");
    if !normalized_refs.is_empty() {
        if !result.trim().is_empty() {
            result.push_str("\n\n");
        }
        result.push_str("---\n\n");
        let refs = normalized_refs
            .iter()
            .map(serialize_source_ref)
            .collect::<Result<Vec<_>>>()?;
        result.push_str(&refs.join("\n"));
    }
    Ok(result)
}

fn parse_source_ref_line(line: &str) -> Option<MemorySourceRef> {
    let line = line.trim();
    let rest = line.strip_prefix("[^")?;
    let (index, target) = rest.split_once("]:")?;
    Some(MemorySourceRef {
        index: index.parse().ok()?,
        target: target.trim().to_string(),
    })
}

fn parse_memory_entry_line(
    line_number: usize,
    line: &str,
    section: Option<String>,
    definitions: &std::collections::BTreeMap<usize, String>,
    strict_metadata: bool,
) -> Result<Option<MemoryEntry>> {
    let trimmed = line.trim();
    let Some(bullet) = trimmed.strip_prefix("- ") else {
        return Ok(None);
    };
    let Some(marker) = marker_in_line(bullet) else {
        return Ok(None);
    };
    let metadata = match parse_memory_entry_metadata(bullet, &marker) {
        Ok(metadata) => metadata,
        Err(error) if strict_metadata => {
            return Err(anyhow!(
                "invalid memory metadata on line {line_number}: {error}"
            ));
        }
        Err(_) => None,
    };
    let source_refs = footnote_indices_in_line(bullet)
        .into_iter()
        .filter_map(|index| definitions.get(&index).cloned())
        .collect::<Vec<_>>();
    let text = strip_entry_markup(bullet).trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(MemoryEntry {
        line_number,
        section,
        text,
        marker,
        source_refs,
        metadata,
    }))
}

fn parse_memory_entry_metadata(line: &str, marker: &str) -> Result<Option<MemoryEntryMetadata>> {
    let mut rest = line;
    let mut encoded = None::<&str>;
    while let Some(start) = rest.find(MEMORY_ENTRY_METADATA_COMMENT_PREFIX) {
        let envelope = &rest[start + "<!--".len()..];
        let end = envelope
            .find("-->")
            .ok_or_else(|| anyhow!("metadata comment is not terminated"))?;
        if encoded.is_some() {
            return Err(anyhow!("entry has more than one metadata envelope"));
        }
        let body = &envelope[..end];
        encoded = Some(
            body.strip_prefix(MEMORY_ENTRY_METADATA_V1_PREFIX)
                .ok_or_else(|| anyhow!("unsupported memory metadata schema"))?,
        );
        rest = &envelope[end + "-->".len()..];
    }
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    if encoded.is_empty() {
        return Err(anyhow!("memory metadata payload is empty"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|error| anyhow!("memory metadata is not valid base64url: {error}"))?;
    let metadata = serde_json::from_slice::<MemoryEntryMetadata>(&bytes)
        .map_err(|error| anyhow!("memory metadata is not valid JSON: {error}"))?;
    validate_memory_entry_metadata(marker, &metadata)?;
    Ok(Some(metadata))
}

fn validate_memory_entry_metadata(marker: &str, metadata: &MemoryEntryMetadata) -> Result<()> {
    if metadata.schema_version != MEMORY_ENTRY_METADATA_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported memory metadata schema version {}",
            metadata.schema_version
        ));
    }
    if metadata.item_id.trim() != marker.trim() {
        return Err(anyhow!("memory metadata item id does not match its marker"));
    }
    if metadata.kind.trim().is_empty() {
        return Err(anyhow!("memory metadata kind is empty"));
    }
    let normalized_target = path_to_slash(&normalize_memory_path(&metadata.target)?);
    if normalized_target != metadata.target.trim().replace('\\', "/") {
        return Err(anyhow!("memory metadata target is not canonical"));
    }
    if !metadata.provenance.is_object() {
        return Err(anyhow!("memory metadata provenance must be an object"));
    }
    if metadata.idempotency_key.trim().is_empty() {
        return Err(anyhow!("memory metadata idempotency key is empty"));
    }
    Ok(())
}

fn memory_title(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("# ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn memory_section_heading(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let heading = trimmed
        .strip_prefix("## ")
        .or_else(|| trimmed.strip_prefix("### "))?
        .trim();
    (!heading.is_empty()).then(|| heading.to_string())
}

fn marker_in_line(line: &str) -> Option<String> {
    let mut rest = line;
    while let Some(start) = rest.find("<!--") {
        let comment = &rest[start + "<!--".len()..];
        let end = comment.find("-->")?;
        let marker = comment[..end].trim();
        if marker.starts_with("m_") {
            return Some(marker.to_string());
        }
        rest = &comment[end + "-->".len()..];
    }
    None
}

fn strip_entry_markup(line: &str) -> String {
    let mut output = String::new();
    let mut rest = line;
    loop {
        let footnote_pos = rest.find("[^");
        let marker_pos = rest.find("<!--");
        let Some(next_pos) = (match (footnote_pos, marker_pos) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }) else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..next_pos]);
        rest = &rest[next_pos..];
        if rest.starts_with("[^") {
            if let Some(end) = rest.find(']') {
                rest = &rest[end + 1..];
            } else {
                output.push_str(rest);
                break;
            }
        } else if rest.starts_with("<!--") {
            if let Some(end) = rest.find("-->") {
                rest = &rest[end + 3..];
            } else if rest.starts_with(MEMORY_ENTRY_METADATA_COMMENT_PREFIX) {
                break;
            } else {
                output.push_str(rest);
                break;
            }
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn footnote_indices_in_line(line: &str) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("[^") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find(']') else {
            break;
        };
        if let Ok(index) = rest[..end].parse::<usize>() {
            indices.push(index);
        }
        rest = &rest[end + 1..];
    }
    indices
}

fn replace_footnote_indices(
    line: &str,
    old_to_new: &std::collections::BTreeMap<usize, usize>,
) -> String {
    let mut output = String::new();
    let mut rest = line;
    loop {
        let Some(start) = rest.find("[^") else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..start]);
        output.push_str("[^");
        rest = &rest[start + 2..];
        let Some(end) = rest.find(']') else {
            output.push_str(rest);
            break;
        };
        if let Ok(old_index) = rest[..end].parse::<usize>() {
            if let Some(new_index) = old_to_new.get(&old_index) {
                output.push_str(&new_index.to_string());
            } else {
                output.push_str(&rest[..end]);
            }
        } else {
            output.push_str(&rest[..end]);
        }
        output.push(']');
        rest = &rest[end + 1..];
    }
    output
}

fn event_file(category: MemoryEventCategory) -> &'static str {
    match category {
        MemoryEventCategory::Chat => "L1/chat_events.jsonl",
        MemoryEventCategory::Notebook => "L1/notebook_events.jsonl",
        MemoryEventCategory::Knowledge => "L1/knowledge_events.jsonl",
    }
}

fn all_event_categories() -> [MemoryEventCategory; 3] {
    [
        MemoryEventCategory::Chat,
        MemoryEventCategory::Notebook,
        MemoryEventCategory::Knowledge,
    ]
}

fn event_surface(category: MemoryEventCategory) -> &'static str {
    match category {
        MemoryEventCategory::Chat => "chat",
        MemoryEventCategory::Notebook => "notebook",
        MemoryEventCategory::Knowledge => "knowledge",
    }
}

fn category_for_surface(surface: &str) -> Option<MemoryEventCategory> {
    match surface {
        "chat" => Some(MemoryEventCategory::Chat),
        "notebook" => Some(MemoryEventCategory::Notebook),
        "knowledge" => Some(MemoryEventCategory::Knowledge),
        _ => None,
    }
}

#[cfg(test)]
fn event_matches_query(event: &MemoryEvent, query: &str) -> bool {
    let query = query.to_lowercase();
    event.summary.to_lowercase().contains(&query)
        || event.action.to_lowercase().contains(&query)
        || event.payload.to_string().to_lowercase().contains(&query)
}

pub fn memory_revision(markdown: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in markdown.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn validate_memory_change(
    target_path: &str,
    change: &MemoryChange,
    allowed_sections: &std::collections::BTreeSet<&str>,
) -> Result<()> {
    if change.id.trim().is_empty() {
        return Err(anyhow!("memory change id is empty"));
    }
    if change.reason.trim().is_empty() {
        return Err(anyhow!("memory change requires a reason"));
    }
    match change.op {
        MemoryChangeOp::Insert => {
            let section = change.section.as_deref().unwrap_or_default().trim();
            if !allowed_sections.contains(section) {
                return Err(anyhow!("memory insert uses unknown section `{section}`"));
            }
            validate_change_text(target_path, change)?;
            if change.refs.is_empty() {
                return Err(anyhow!("memory insert requires evidence refs"));
            }
        }
        MemoryChangeOp::Replace => {
            if change
                .entry_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(anyhow!("memory replace requires an entry id"));
            }
            if let Some(section) = change.section.as_deref()
                && !allowed_sections.contains(section.trim())
            {
                return Err(anyhow!("memory replace uses unknown section `{section}`"));
            }
            validate_change_text(target_path, change)?;
        }
        MemoryChangeOp::Delete => {
            if change
                .entry_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(anyhow!("memory delete requires an entry id"));
            }
            if change.text.is_some() {
                return Err(anyhow!("memory delete must not include replacement text"));
            }
        }
    }
    Ok(())
}

fn validate_change_text(target_path: &str, change: &MemoryChange) -> Result<()> {
    let text = change.text.as_deref().unwrap_or_default().trim();
    if text.is_empty() {
        return Err(anyhow!("memory change text is empty"));
    }
    let count = text.chars().count();
    let limit = memory_entry_text_limit(target_path);
    if count > limit {
        return Err(anyhow!(
            "memory change `{}` has {count} characters, exceeding the {limit}-character limit for {target_path}; shorten it or split it into multiple changes",
            change.id
        ));
    }
    Ok(())
}

pub fn memory_entry_text_limit(target_path: &str) -> usize {
    if target_path.starts_with("L3/") {
        MAX_L3_MEMORY_ENTRY_TEXT_CHARS
    } else {
        MAX_L2_MEMORY_ENTRY_TEXT_CHARS
    }
}

fn memory_entry_from_change(change: &MemoryChange) -> Result<MemoryEntry> {
    Ok(MemoryEntry {
        line_number: 0,
        section: change.section.as_deref().map(str::trim).map(str::to_string),
        text: change
            .text
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string(),
        marker: format!("m_{}", uuid::Uuid::new_v4().simple()),
        source_refs: change.refs.clone(),
        metadata: None,
    })
}

fn target_surface(target_path: &str) -> Option<&'static str> {
    if target_path.contains("chat") {
        Some("chat")
    } else if target_path.contains("profile")
        || target_path.contains("preferences")
        || target_path.contains("recent")
        || target_path.contains("teaching_strategy")
        || target_path.contains("continuity")
    {
        Some("chat")
    } else if target_path.contains("notebook") || target_path.contains("scope") {
        Some("notebook")
    } else if target_path.contains("knowledge") {
        Some("knowledge")
    } else {
        None
    }
}

fn l3_source_paths(target_path: &str) -> Vec<&'static str> {
    match target_path {
        "L3/profile.md" | "L3/scope.md" | "L3/recent.md" => {
            vec!["L2/chat.md", "L2/notebook.md", "L2/knowledge.md"]
        }
        "L3/preferences.md" => vec!["L2/chat.md", "L2/notebook.md"],
        "L3/continuity.md" => vec!["L2/chat.md", "L2/notebook.md"],
        "L3/teaching_strategy.md" => vec!["L2/chat.md", "L2/notebook.md", "L2/knowledge.md"],
        _ => Vec::new(),
    }
}

fn target_catalog(target_path: &str, existing_markdown: String) -> MemoryTargetCatalog {
    let (title, focus, sections) = match target_path {
        "L2/chat.md" => (
            "Chat memory",
            "Stable misconceptions, demonstrated mastery, and recurring topics.",
            vec!["Misconceptions", "Mastery", "Topics"],
        ),
        "L2/notebook.md" => (
            "Notebook memory",
            "Recurring note and saved research themes, organization habits, preferred formats, report preferences, and open questions.",
            vec![
                "Themes",
                "Organization",
                "Formats",
                "Report preferences",
                "Open questions",
            ],
        ),
        "L2/knowledge.md" => (
            "Knowledge memory",
            "Document interests, frequent queries, and knowledge gaps.",
            vec!["Interests", "Frequent queries", "Gaps"],
        ),
        "L3/recent.md" => (
            "Recent context",
            "Rolling timeline of recent user activity relevant to future work.",
            vec!["This week", "Earlier"],
        ),
        "L3/profile.md" => (
            "User profile",
            "Durable user identity and explicitly supported profile facts.",
            vec!["Identity", "Preferences"],
        ),
        "L3/scope.md" => (
            "Working scope",
            "Topics, projects, and domains the user is actively working with.",
            vec!["Active", "Background", "Open questions"],
        ),
        "L3/preferences.md" => (
            "User preferences",
            "Explicit user-stated long-term preferences.",
            vec!["Preferences"],
        ),
        "L3/continuity.md" => (
            "Assistant continuity",
            "User-approved commitments, open loops, and future response strategies.",
            vec!["Commitments", "Open loops", "Strategies"],
        ),
        "L3/teaching_strategy.md" => (
            "Assistant strategy",
            "How the assistant should adapt explanations, structure, and follow-up.",
            vec![
                "Explanation style",
                "Working strategy",
                "Follow-up strategy",
            ],
        ),
        _ => ("Memory", "Durable user memory.", vec!["Notes"]),
    };
    MemoryTargetCatalog {
        title: title.into(),
        existing_markdown,
        allowed_sections: sections.into_iter().map(str::to_string).collect(),
        focus: focus.into(),
    }
}

fn clean_optional(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn normalize_memory_path(path: &str) -> Result<PathBuf> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains("..")
        || !normalized.ends_with(".md")
    {
        return Err(anyhow!("invalid memory path"));
    }
    if !DEFAULT_FILES.iter().any(|(item, _)| *item == normalized) {
        return Err(anyhow!("memory file is not editable"));
    }
    Ok(PathBuf::from(normalized))
}

fn validate_l2_path(path: &str) -> Result<String> {
    let path = path_to_slash(&normalize_memory_path(path)?);
    if !path.starts_with("L2/") {
        return Err(anyhow!("memory entry path must target an L2 file"));
    }
    Ok(path)
}

fn memory_l2_entry(file: &MemoryFile, entry: MemoryEntry) -> Result<MemoryL2Entry> {
    let revision = memory_entry_revision(&entry)?;
    Ok(MemoryL2Entry {
        reference: l2_entry_reference(&file.path, &entry.marker)?,
        path: file.path.clone(),
        revision,
        entry,
    })
}

fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn default_root() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".llm-tutor")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_store_creates_skeleton_and_updates_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let files = store.list().unwrap();
        assert!(files.iter().any(|file| file.path == "L3/profile.md"));
        assert!(!files.iter().any(|file| file.path == "L2/research.md"));

        let updated = store
            .write(
                "L3/profile.md",
                "# Student profile\n\n- Needs review. <!--m_01-->\n\n[^1]: quiz:q1".into(),
            )
            .unwrap();
        assert_eq!(updated.path, "L3/profile.md");
        assert!(updated.markdown.contains("Needs review"));
    }

    #[test]
    fn memory_entry_text_limits_are_layer_specific() {
        assert_eq!(memory_entry_text_limit("L2/chat.md"), 500);
        assert_eq!(memory_entry_text_limit("L3/profile.md"), 1_200);
    }

    #[test]
    fn memory_store_removes_retired_research_memory_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("memory");
        fs::create_dir_all(root.join("L1")).unwrap();
        fs::create_dir_all(root.join("L2")).unwrap();
        fs::write(root.join("L1/research_events.jsonl"), "legacy").unwrap();
        fs::write(root.join("L2/research.md"), "legacy").unwrap();

        let store = FileMemoryBackend::new_with_root(root.clone());

        assert!(!root.join("L1/research_events.jsonl").exists());
        assert!(!root.join("L2/research.md").exists());
        assert_eq!(store.event_catalog().unwrap().len(), 3);
    }

    #[test]
    fn memory_parser_extracts_markers_and_refs() {
        let markdown = "- Prefers diagrams. [^1] <!--m_01ABC-->\n\n[^1]: chat:session:turn-1";
        assert_eq!(parse_memory_entries(markdown)[0].marker, "m_01ABC");
        assert_eq!(
            parse_source_refs(markdown),
            vec![MemorySourceRef {
                index: 1,
                target: "chat:session:turn-1".into()
            }]
        );
        assert_eq!(
            serialize_memory_marker("m_01ABC").unwrap(),
            "<!--m_01ABC-->"
        );
        assert_eq!(
            serialize_source_ref(&MemorySourceRef {
                index: 1,
                target: "chat:session:turn-1".into()
            })
            .unwrap(),
            "[^1]: chat:session:turn-1"
        );
    }

    #[test]
    fn memory_entry_metadata_round_trips_with_legacy_entries() {
        let metadata = MemoryEntryMetadata {
            schema_version: MEMORY_ENTRY_METADATA_SCHEMA_VERSION,
            item_id: "m_visual".into(),
            kind: "preference".into(),
            target: "L3/preferences.md".into(),
            provenance: json!({
                "principalId": "local-user",
                "sessionId": "session-1",
                "origin": "explicit"
            }),
            idempotency_key: "idem-visual".into(),
            expires_at: Some("2026-08-01T00:00:00Z".parse().unwrap()),
        };
        let entries = vec![
            MemoryEntry {
                line_number: 3,
                section: Some("Format".into()),
                text: "Prefers visual examples.".into(),
                marker: "m_visual".into(),
                source_refs: vec!["chat:event-1".into()],
                metadata: Some(metadata),
            },
            MemoryEntry {
                line_number: 4,
                section: Some("Format".into()),
                text: "Legacy preference.".into(),
                marker: "m_legacy".into(),
                source_refs: Vec::new(),
                metadata: None,
            },
        ];

        let markdown = serialize_memory_entries("Learning preferences", &entries).unwrap();
        let reparsed = try_parse_memory_entries(&markdown).unwrap();

        assert!(markdown.contains("<!--llm-tutor-memory:v1:"));
        assert_eq!(reparsed[0].metadata, entries[0].metadata);
        assert!(reparsed[1].metadata.is_none());
        assert_eq!(reparsed[0].text, "Prefers visual examples.");
        assert_eq!(reparsed[1].text, "Legacy preference.");
    }

    #[test]
    fn maintenance_replace_preserves_runtime_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let metadata = MemoryEntryMetadata {
            schema_version: MEMORY_ENTRY_METADATA_SCHEMA_VERSION,
            item_id: "m_visual".into(),
            kind: "preference".into(),
            target: "L3/preferences.md".into(),
            provenance: json!({ "principalId": "local-user" }),
            idempotency_key: "idem-visual".into(),
            expires_at: None,
        };
        let markdown = serialize_memory_entries(
            "Learning preferences",
            &[MemoryEntry {
                line_number: 3,
                section: Some("Preferences".into()),
                text: "Prefers visual examples.".into(),
                marker: "m_visual".into(),
                source_refs: Vec::new(),
                metadata: Some(metadata.clone()),
            }],
        )
        .unwrap();
        let original = store.write("L3/preferences.md", markdown).unwrap();
        let change = MemoryChange {
            id: "replace-visual".into(),
            op: MemoryChangeOp::Replace,
            section: Some("Preferences".into()),
            entry_id: Some("m_visual".into()),
            after_entry_id: None,
            text: Some("Prefers diagrams and visual examples.".into()),
            refs: Vec::new(),
            reason: "Clarify the explicit preference.".into(),
            before_text: Some("Prefers visual examples.".into()),
        };

        let updated = store
            .apply_memory_changes(
                "L3/preferences.md",
                &original.revision,
                &[change],
                &["replace-visual".into()],
            )
            .unwrap();
        let entries = try_parse_memory_entries(&updated.markdown).unwrap();

        assert_eq!(entries[0].metadata.as_ref(), Some(&metadata));
        assert_eq!(entries[0].text, "Prefers diagrams and visual examples.");
    }

    #[test]
    fn durable_preference_is_idempotent_and_exact_delete_is_undoable() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let write = DurableMemoryWrite {
            content: "Prefers diagrams.".into(),
            kind: "preference".into(),
            provenance: json!({ "runId": "run-1" }),
            idempotency_key: "idem-1".into(),
            expires_at: None,
        };

        let first = backend.upsert_durable_preference(write.clone()).unwrap();
        let replay = backend.upsert_durable_preference(write).unwrap();
        let revision = memory_entry_revision(&first).unwrap();

        assert_eq!(replay.marker, first.marker);
        assert_eq!(
            try_parse_memory_entries(&backend.read("L3/preferences.md").unwrap().markdown)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            backend
                .delete_durable_preference(&first.marker, "stale")
                .unwrap(),
            ExactMemoryDeleteOutcome::Stale {
                latest_revision: revision.clone()
            }
        );
        assert_eq!(
            backend
                .delete_durable_preference(&first.marker, &revision)
                .unwrap(),
            ExactMemoryDeleteOutcome::Deleted
        );
        assert!(
            try_parse_memory_entries(&backend.read("L3/preferences.md").unwrap().markdown)
                .unwrap()
                .is_empty()
        );

        backend
            .undo_latest_write("L3/preferences.md")
            .expect("delete should create one exact undo snapshot");
        let restored =
            try_parse_memory_entries(&backend.read("L3/preferences.md").unwrap().markdown).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(memory_entry_revision(&restored[0]).unwrap(), revision);
    }

    #[test]
    fn concurrent_durable_retries_create_one_entry() {
        use std::sync::Barrier;

        let dir = tempfile::tempdir().unwrap();
        let backend = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let barrier = Arc::new(Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let backend = backend.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    backend
                        .upsert_durable_preference(DurableMemoryWrite {
                            content: "Prefers diagrams.".into(),
                            kind: "preference".into(),
                            provenance: json!({ "runId": "run-1" }),
                            idempotency_key: "same-key".into(),
                            expires_at: None,
                        })
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let entries = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(entries[0].marker, entries[1].marker);
        assert_eq!(
            try_parse_memory_entries(&backend.read("L3/preferences.md").unwrap().markdown)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn concurrent_durable_write_and_exact_delete_serialize_without_lost_updates() {
        use std::sync::Barrier;

        let dir = tempfile::tempdir().unwrap();
        let backend = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let original = backend
            .upsert_durable_preference(DurableMemoryWrite {
                content: "Original preference.".into(),
                kind: "preference".into(),
                provenance: json!({ "runId": "run-original" }),
                idempotency_key: "original-key".into(),
                expires_at: None,
            })
            .unwrap();
        let revision = memory_entry_revision(&original).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let write_backend = backend.clone();
        let write_barrier = barrier.clone();
        let writer = std::thread::spawn(move || {
            write_barrier.wait();
            write_backend
                .upsert_durable_preference(DurableMemoryWrite {
                    content: "Concurrent preference.".into(),
                    kind: "preference".into(),
                    provenance: json!({ "runId": "run-concurrent" }),
                    idempotency_key: "concurrent-key".into(),
                    expires_at: None,
                })
                .unwrap();
        });
        let delete_backend = backend.clone();
        let delete_barrier = barrier.clone();
        let marker = original.marker;
        let deleter = std::thread::spawn(move || {
            delete_barrier.wait();
            delete_backend
                .delete_durable_preference(&marker, &revision)
                .unwrap()
        });
        barrier.wait();
        writer.join().unwrap();
        assert_eq!(deleter.join().unwrap(), ExactMemoryDeleteOutcome::Deleted);

        let entries =
            try_parse_memory_entries(&backend.read("L3/preferences.md").unwrap().markdown).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "Concurrent preference.");
    }

    #[test]
    fn expired_durable_preferences_are_cleaned_without_touching_legacy_entries() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        backend
            .write(
                "L3/preferences.md",
                "# Learning preferences\n\n## Preferences\n\n- Legacy preference. <!--m_legacy-->"
                    .into(),
            )
            .unwrap();
        backend
            .upsert_durable_preference(DurableMemoryWrite {
                content: "Expired preference.".into(),
                kind: "preference".into(),
                provenance: json!({ "runId": "run-1" }),
                idempotency_key: "expired-key".into(),
                expires_at: Some(Utc::now() - chrono::Duration::seconds(1)),
            })
            .unwrap();

        assert_eq!(
            backend
                .cleanup_expired_durable_preferences(Utc::now())
                .unwrap(),
            1
        );
        let entries =
            try_parse_memory_entries(&backend.read("L3/preferences.md").unwrap().markdown).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].marker, "m_legacy");
        assert_eq!(
            backend
                .cleanup_expired_durable_preferences(Utc::now())
                .unwrap(),
            0
        );
    }

    #[test]
    fn entry_revision_is_scoped_to_one_normalized_entry() {
        let first = "# Chat memory\n\n- Stable entry. [^1] <!--m_stable-->\n- Other entry. <!--m_other-->\n\n[^1]: chat:event-1";
        let second = "# Chat memory\n\n- Stable entry. [^1] <!--m_stable-->\n- Changed other entry. <!--m_other-->\n\n[^1]: chat:event-1";
        let changed =
            "# Chat memory\n\n- Changed stable entry. [^1] <!--m_stable-->\n\n[^1]: chat:event-1";
        let first_entries = try_parse_memory_entries(first).unwrap();
        let second_entries = try_parse_memory_entries(second).unwrap();
        let changed_entries = try_parse_memory_entries(changed).unwrap();

        assert_eq!(
            memory_entry_revision(&first_entries[0]).unwrap(),
            memory_entry_revision(&second_entries[0]).unwrap()
        );
        assert_ne!(
            memory_entry_revision(&first_entries[0]).unwrap(),
            memory_entry_revision(&changed_entries[0]).unwrap()
        );
        assert_ne!(memory_revision(first), memory_revision(second));
    }

    #[test]
    fn malformed_metadata_fails_closed_but_tolerant_parse_recovers_text() {
        let markdown = "# Chat memory\n\n- Learns visually. <!--m_visual--> <!--llm-tutor-memory:v1:not-base64!!-->";

        let error = try_parse_memory_entries(markdown).unwrap_err();
        let recovered = parse_memory_entries(markdown);

        assert!(error.to_string().contains("not valid base64url"));
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].text, "Learns visually.");
        assert!(recovered[0].metadata.is_none());
    }

    #[test]
    fn strict_backend_reads_reject_malformed_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n- Learns visually. <!--m_visual--> <!--llm-tutor-memory:v1:not-base64!!-->"
                    .into(),
            )
            .unwrap();

        let error = store
            .query_l2_entries(&["L2/chat.md".into()], None, None, 10)
            .unwrap_err();

        assert!(error.to_string().contains("invalid memory metadata"));
        assert!(
            store
                .read("L2/chat.md")
                .unwrap()
                .markdown
                .contains("visually")
        );
    }

    #[test]
    fn unsupported_metadata_schema_is_rejected() {
        let metadata = json!({
            "schemaVersion": 2,
            "itemId": "m_visual",
            "kind": "preference",
            "target": "L3/preferences.md",
            "provenance": {},
            "idempotencyKey": "idem-visual",
            "expiresAt": null
        });
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&metadata).unwrap());
        let markdown = format!(
            "- Learns visually. <!--m_visual--> <!--{MEMORY_ENTRY_METADATA_V1_PREFIX}{encoded}-->"
        );

        let error = try_parse_memory_entries(&markdown).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported memory metadata schema version 2")
        );
    }

    #[test]
    fn l2_entry_references_round_trip_and_resolve_sources() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let event = store
            .record_event(
                MemoryEventCategory::Chat,
                "answered",
                "Explained vectors visually",
                Some("session-1".into()),
                json!({ "answer": "complete evidence" }),
            )
            .unwrap();
        store
            .write(
                "L2/chat.md",
                format!(
                    "# Chat memory\n\n## Topics\n\n- Learns vectors visually. [^1] <!--m_visual-->\n\n---\n\n[^1]: chat:{}",
                    event.id
                ),
            )
            .unwrap();

        let reference = l2_entry_reference("L2/chat.md", "m_visual").unwrap();
        assert_eq!(reference, "memory:L2/chat.md#m_visual");
        assert_eq!(
            parse_l2_entry_reference(&reference).unwrap(),
            ("L2/chat.md".into(), "m_visual".into())
        );
        let matches = store
            .query_l2_entries(&["L2/chat.md".into()], Some("vectors"), None, 10)
            .unwrap();
        assert_eq!(matches.entries[0].reference, reference);
        let resolved = store.read_l2_entry_sources(&reference).unwrap();
        assert_eq!(resolved.sources.len(), 1);
        assert_eq!(resolved.sources[0].event.id, event.id);
    }

    #[test]
    fn l3_context_uses_l2_catalog_except_for_recent_l1_exception() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));

        let profile = store
            .agent_context("L3/profile.md", "# User profile")
            .unwrap();
        assert!(profile.get("l1Catalog").is_none());
        assert_eq!(profile["instructions"]["evidenceLayer"], "L2");
        assert_eq!(profile["l2Catalog"].as_array().unwrap().len(), 3);

        let preferences = store
            .agent_context("L3/preferences.md", "# User preferences")
            .unwrap();
        let paths = preferences["l2Catalog"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["L2/chat.md", "L2/notebook.md"]);

        let recent = store
            .agent_context("L3/recent.md", "# Recent context")
            .unwrap();
        assert!(recent.get("l1Catalog").is_some());
        assert_eq!(recent["instructions"]["boundedL1Exception"], true);
    }

    #[test]
    fn normalize_memory_markdown_dedupes_and_removes_unused_refs() {
        let markdown = "# Chat memory\n\n- First fact. [^2]\n- Second fact. [^3]\n- Unknown fact. [^9]\n\n[^1]: chat:unused\n[^2]: chat:q1\n[^3]: chat:q1\n[^4]: chat:unused";

        let normalized = normalize_memory_markdown(markdown).unwrap();

        assert!(normalized.contains("- First fact. [^1]"));
        assert!(normalized.contains("- Second fact. [^1]"));
        assert!(normalized.contains("- Unknown fact. [^9]"));
        assert!(normalized.contains("[^1]: chat:q1"));
        assert!(!normalized.contains("chat:unused"));
        assert!(!normalized.contains("[^2]:"));
    }

    #[test]
    fn memory_entries_round_trip_with_shared_source_refs() {
        let markdown = "# Chat memory\n\n- First fact. [^2] <!--m_1-->\n- Second fact. [^3] <!--m_2-->\n\n---\n\n[^2]: chat:q1\n[^3]: chat:q1\n[^4]: chat:unused";

        let entries = parse_memory_entries(markdown);
        let serialized = serialize_memory_entries("Chat memory", &entries).unwrap();
        let reparsed = parse_memory_entries(&serialized);

        assert_eq!(entries, reparsed);
        assert!(serialized.contains("- First fact. [^1] <!--m_1-->"));
        assert!(serialized.contains("- Second fact. [^1] <!--m_2-->"));
        assert!(serialized.contains("[^1]: chat:q1"));
        assert!(!serialized.contains("chat:unused"));
        assert!(!serialized.contains("[^2]:"));
    }

    #[test]
    fn memory_entry_serializer_removes_refs_for_deleted_entries() {
        let markdown = "# Chat memory\n\n- Keep this. [^1] <!--m_keep-->\n- Delete this. [^2] <!--m_drop-->\n\n---\n\n[^1]: chat:keep\n[^2]: chat:drop";
        let entries = parse_memory_entries(markdown)
            .into_iter()
            .filter(|entry| entry.marker != "m_drop")
            .collect::<Vec<_>>();

        let serialized = serialize_memory_entries("Chat memory", &entries).unwrap();

        assert!(serialized.contains("chat:keep"));
        assert!(!serialized.contains("chat:drop"));
        assert_eq!(parse_memory_entries(&serialized).len(), 1);
    }

    #[test]
    fn memory_entry_serializer_preserves_sections() {
        let markdown = "# Chat memory\n\n## Topics\n\n- Needs OPC review. [^1] <!--m_1-->\n\n## Mastery\n\n- Understands basic lithography. [^2] <!--m_2-->\n\n---\n\n[^1]: chat:q1\n[^2]: chat:q2";

        let entries = parse_memory_entries(markdown);
        let serialized = serialize_memory_entries("Chat memory", &entries).unwrap();

        assert_eq!(entries[0].section.as_deref(), Some("Topics"));
        assert_eq!(entries[1].section.as_deref(), Some("Mastery"));
        assert!(serialized.contains("## Topics\n\n- Needs OPC review."));
        assert!(serialized.contains("## Mastery\n\n- Understands basic lithography."));
        assert_eq!(
            parse_memory_entries(&serialized)
                .iter()
                .map(|entry| entry.section.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("Topics"), Some("Mastery")]
        );
    }

    #[test]
    fn memory_store_write_normalizes_source_refs() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let updated = store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n- Same source. [^7]\n\n[^7]: chat:q1\n[^8]: chat:unused".into(),
            )
            .unwrap();

        assert!(updated.markdown.contains("- Same source. [^1]"));
        assert!(updated.markdown.contains("[^1]: chat:q1"));
        assert!(!updated.markdown.contains("chat:unused"));
    }

    #[test]
    fn memory_store_atomic_write_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("memory");
        let store = FileMemoryBackend::new_with_root(root.clone());

        store
            .write(
                "L3/preferences.md",
                "# Learning preferences\n\n- Prefer visual examples. <!--m_visual-->".into(),
            )
            .unwrap();

        let temporary_files = fs::read_dir(root.join("L3"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".llm-tutor-memory-")
            })
            .collect::<Vec<_>>();
        assert!(temporary_files.is_empty());
    }

    #[test]
    fn memory_store_ignores_an_interrupted_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("memory");
        let store = FileMemoryBackend::new_with_root(root.clone());
        let canonical = store
            .write(
                "L3/preferences.md",
                "# Learning preferences\n\n- Canonical preference. <!--m_canonical-->".into(),
            )
            .unwrap();
        fs::write(
            root.join("L3/.preferences.md.llm-tutor-memory-interrupted.tmp"),
            "# Learning preferences\n\n- Partial write",
        )
        .unwrap();

        let reopened = FileMemoryBackend::new_with_root(root);
        let recovered = reopened.read("L3/preferences.md").unwrap();

        assert_eq!(recovered.revision, canonical.revision);
        assert!(recovered.markdown.contains("Canonical preference"));
        assert!(!recovered.markdown.contains("Partial write"));
    }

    #[test]
    fn memory_store_can_undo_latest_write_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        store
            .write("L2/chat.md", "# Chat memory\n\n- Original.".into())
            .unwrap();
        store
            .write("L2/chat.md", "# Chat memory\n\n- Changed.".into())
            .unwrap();

        let restored = store.undo_latest_write("L2/chat.md").unwrap();

        assert!(restored.file.markdown.contains("Original"));
        assert!(!restored.file.markdown.contains("Changed"));
        let err = store.undo_latest_write("L2/chat.md").unwrap_err();
        assert!(err.to_string().contains("no memory undo snapshot"));
    }

    #[test]
    fn memory_store_records_and_lists_events() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let event = store
            .record_event(
                MemoryEventCategory::Chat,
                "message",
                "Asked for a concise OPC explanation",
                Some("chat-1".into()),
                json!({ "topic": "opc" }),
            )
            .unwrap();
        assert_eq!(event.category, MemoryEventCategory::Chat);
        let events = store.recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Asked for a concise OPC explanation");
    }

    #[test]
    fn memory_store_lists_knowledge_surface_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let files = store.list().unwrap();

        assert!(files.iter().any(|file| file.path == "L2/knowledge.md"));
        assert!(dir.path().join("memory/L2/knowledge.md").exists());
    }

    #[test]
    fn memory_store_resolves_source_refs_to_l1_events() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        store
            .record_event(
                MemoryEventCategory::Chat,
                "message",
                "Asked for a concise OPC explanation",
                Some("chat-1".into()),
                json!({ "topic": "opc" }),
            )
            .unwrap();

        let source = store.resolve_source_ref("chat:chat-1").unwrap();

        assert_eq!(source.reference, "chat:chat-1");
        assert_eq!(source.event.summary, "Asked for a concise OPC explanation");
    }

    #[test]
    fn event_queries_paginate_with_event_scoped_references() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let first = store
            .record_event(
                MemoryEventCategory::Chat,
                "asked",
                "First question about vectors",
                Some("session-1".into()),
                json!({ "content": "full first question" }),
            )
            .unwrap();
        let second = store
            .record_event(
                MemoryEventCategory::Chat,
                "answered",
                "Second answer about vectors",
                Some("session-1".into()),
                json!({ "answer": "full second answer" }),
            )
            .unwrap();

        let first_page = store
            .query_events(Some("chat"), Some("vectors"), Some("session-1"), None, 1)
            .unwrap();
        let second_page = store
            .query_events(
                Some("chat"),
                Some("vectors"),
                Some("session-1"),
                first_page.next_cursor.as_deref(),
                1,
            )
            .unwrap();

        assert_eq!(first_page.total, 2);
        assert_eq!(first_page.events.len(), 1);
        assert_eq!(second_page.events.len(), 1);
        let refs = [
            format!("chat:{}", first_page.events[0].id),
            format!("chat:{}", second_page.events[0].id),
        ];
        assert_ne!(refs[0], refs[1]);
        assert!(refs.contains(&format!("chat:{}", first.id)));
        assert!(refs.contains(&format!("chat:{}", second.id)));
        assert!(second_page.next_cursor.is_none());
    }

    #[test]
    fn event_context_is_bounded_to_the_same_source_session() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let before = store
            .record_event(
                MemoryEventCategory::Chat,
                "asked",
                "Question",
                Some("session-1".into()),
                json!({}),
            )
            .unwrap();
        let focus = store
            .record_event(
                MemoryEventCategory::Chat,
                "answered",
                "Answer",
                Some("session-1".into()),
                json!({}),
            )
            .unwrap();
        store
            .record_event(
                MemoryEventCategory::Chat,
                "asked",
                "Unrelated question",
                Some("session-2".into()),
                json!({}),
            )
            .unwrap();

        let context = store.event_context(&focus.id, 2, 2).unwrap();

        assert_eq!(context.event.id, focus.id);
        assert!(context.before.iter().any(|event| event.id == before.id));
        assert!(
            context
                .before
                .iter()
                .chain(context.after.iter())
                .all(|event| event.source_id.as_deref() == Some("session-1"))
        );
    }

    #[test]
    fn memory_change_apply_supports_partial_acceptance_and_stale_revision_checks() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let event = store
            .record_event(
                MemoryEventCategory::Chat,
                "answered",
                "Explained vectors visually",
                Some("session-1".into()),
                json!({ "answer": "complete evidence" }),
            )
            .unwrap();
        let original = store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n## Topics\n\n- Old vector note. <!--m_old-->".into(),
            )
            .unwrap();
        let changes = vec![
            MemoryChange {
                id: "replace-old".into(),
                op: MemoryChangeOp::Replace,
                section: Some("Topics".into()),
                entry_id: Some("m_old".into()),
                after_entry_id: None,
                text: Some("Improved vector note.".into()),
                refs: vec![format!("chat:{}", event.id)],
                reason: "The read evidence is more specific.".into(),
                before_text: Some("Old vector note.".into()),
            },
            MemoryChange {
                id: "insert-new".into(),
                op: MemoryChangeOp::Insert,
                section: Some("Mastery".into()),
                entry_id: None,
                after_entry_id: None,
                text: Some("Understands vector addition.".into()),
                refs: vec![format!("chat:{}", event.id)],
                reason: "The answer demonstrates mastery.".into(),
                before_text: None,
            },
        ];

        let applied = store
            .apply_memory_changes(
                "L2/chat.md",
                &original.revision,
                &changes,
                &["replace-old".into()],
            )
            .unwrap();

        assert!(applied.markdown.contains("Improved vector note"));
        assert!(!applied.markdown.contains("Understands vector addition"));
        let stale = store
            .apply_memory_changes(
                "L2/chat.md",
                &original.revision,
                &changes,
                &["insert-new".into()],
            )
            .unwrap_err();
        assert!(stale.to_string().contains("changed since this run"));
    }

    #[test]
    fn memory_change_apply_is_atomic_when_one_selected_change_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let original = store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n## Topics\n\n- Original note. <!--m_original-->".into(),
            )
            .unwrap();
        let changes = vec![
            MemoryChange {
                id: "valid".into(),
                op: MemoryChangeOp::Replace,
                section: Some("Topics".into()),
                entry_id: Some("m_original".into()),
                after_entry_id: None,
                text: Some("Changed note.".into()),
                refs: vec![],
                reason: "Clarify the note.".into(),
                before_text: Some("Original note.".into()),
            },
            MemoryChange {
                id: "invalid".into(),
                op: MemoryChangeOp::Delete,
                section: None,
                entry_id: Some("m_missing".into()),
                after_entry_id: None,
                text: None,
                refs: vec![],
                reason: "Remove a duplicate.".into(),
                before_text: None,
            },
        ];

        let error = store
            .apply_memory_changes(
                "L2/chat.md",
                &original.revision,
                &changes,
                &["valid".into(), "invalid".into()],
            )
            .unwrap_err();

        assert!(error.to_string().contains("m_missing"));
        let unchanged = store.read("L2/chat.md").unwrap();
        assert_eq!(unchanged.revision, original.revision);
        assert!(unchanged.markdown.contains("Original note"));
        assert!(!unchanged.markdown.contains("Changed note"));
    }

    #[test]
    fn concurrent_change_sets_with_one_base_revision_cannot_both_commit() {
        use std::sync::Barrier;

        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let event = store
            .record_event(
                MemoryEventCategory::Chat,
                "answered",
                "Concurrent change-set evidence",
                Some("session-1".into()),
                json!({}),
            )
            .unwrap();
        let original = store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n## Topics\n\n- Original note. <!--m_original-->".into(),
            )
            .unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["first", "second"]
            .into_iter()
            .map(|name| {
                let store = store.clone();
                let barrier = barrier.clone();
                let base_revision = original.revision.clone();
                let evidence_ref = format!("chat:{}", event.id);
                std::thread::spawn(move || {
                    let change_id = format!("insert-{name}");
                    let changes = vec![MemoryChange {
                        id: change_id.clone(),
                        op: MemoryChangeOp::Insert,
                        section: Some("Topics".into()),
                        entry_id: None,
                        after_entry_id: None,
                        text: Some(format!("{name} concurrent note.")),
                        refs: vec![evidence_ref],
                        reason: "Exercise compare-and-swap serialization.".into(),
                        before_text: None,
                    }];
                    barrier.wait();
                    store.apply_memory_changes("L2/chat.md", &base_revision, &changes, &[change_id])
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .all(|error| error.to_string().contains("changed since this run"))
        );
        let committed = store.read("L2/chat.md").unwrap();
        assert_ne!(committed.revision, original.revision);
        assert_eq!(
            ["first concurrent note", "second concurrent note"]
                .into_iter()
                .filter(|text| committed.markdown.contains(text))
                .count(),
            1
        );
        let restored = store.undo_latest_write("L2/chat.md").unwrap();
        assert_eq!(restored.file.revision, original.revision);
    }

    #[test]
    fn memory_change_apply_allows_review_to_finish_without_accepting_changes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileMemoryBackend::new_with_root(dir.path().join("memory"));
        let original = store
            .write(
                "L2/chat.md",
                "# Chat memory\n\n## Topics\n\n- Keep this note. <!--m_original-->".into(),
            )
            .unwrap();

        let reviewed = store
            .apply_memory_changes("L2/chat.md", &original.revision, &[], &[])
            .unwrap();

        assert_eq!(reviewed.revision, original.revision);
        assert_eq!(reviewed.markdown, original.markdown);
    }
}
