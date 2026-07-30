use std::{fs, path::PathBuf, sync::Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_MEMORY_TEXT_CHARS: usize = 2_000;
const MAX_NEXT_ACTION_CHARS: usize = 500;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TutorMemoryKind {
    Commitment,
    OpenLoop,
    LessonPlan,
    Reflection,
    Strategy,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TutorMemoryStatus {
    Active,
    Resolved,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TutorMemoryEntry {
    pub id: String,
    pub tutor_id: String,
    pub kind: TutorMemoryKind,
    pub text: String,
    pub status: TutorMemoryStatus,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
    #[serde(default)]
    pub resolution_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_provenance: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateTutorMemoryEntry {
    pub kind: TutorMemoryKind,
    pub text: String,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateTutorMemoryEntry {
    #[serde(default)]
    pub kind: Option<TutorMemoryKind>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub next_action: Option<Option<String>>,
    #[serde(default)]
    pub due_at: Option<Option<DateTime<Utc>>>,
    #[serde(default)]
    pub status: Option<TutorMemoryStatus>,
    #[serde(default)]
    pub resolution_note: Option<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct RuntimeTutorMemoryWrite {
    pub kind: TutorMemoryKind,
    pub text: String,
    pub next_action: Option<String>,
    pub source_session_id: Option<String>,
    pub source_message_id: Option<String>,
    pub idempotency_key: String,
    pub provenance: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExactTutorMemoryDeleteOutcome {
    Deleted,
    NotFound,
    Stale { latest_revision: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TutorMemoryFile {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default)]
    entries: Vec<TutorMemoryEntry>,
}

impl Default for TutorMemoryFile {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum TutorMemoryError {
    #[error("tutor memory entry not found")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

pub struct TutorMemoryStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl TutorMemoryStore {
    pub fn new_with_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        fs::create_dir_all(&root).expect("failed to create tutor memory root");
        Self {
            root,
            lock: Mutex::new(()),
        }
    }

    pub fn list(
        &self,
        tutor_id: &str,
        include_resolved: bool,
    ) -> Result<Vec<TutorMemoryEntry>, TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let _guard = self.lock.lock().unwrap();
        let mut entries = self.load_locked(tutor_id)?.entries;
        entries.retain(|entry| include_resolved || entry.status == TutorMemoryStatus::Active);
        entries.sort_by(|left, right| {
            status_rank(left.status)
                .cmp(&status_rank(right.status))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(entries)
    }

    pub fn get(
        &self,
        tutor_id: &str,
        entry_id: &str,
    ) -> Result<TutorMemoryEntry, TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let _guard = self.lock.lock().unwrap();
        self.load_locked(tutor_id)?
            .entries
            .into_iter()
            .find(|entry| entry.id == entry_id)
            .ok_or(TutorMemoryError::NotFound)
    }

    pub fn create(
        &self,
        tutor_id: &str,
        input: CreateTutorMemoryEntry,
    ) -> Result<TutorMemoryEntry, TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let now = Utc::now();
        let entry = TutorMemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            tutor_id: tutor_id.to_string(),
            kind: input.kind,
            text: clean_required(input.text, "memory text", MAX_MEMORY_TEXT_CHARS)?,
            status: TutorMemoryStatus::Active,
            next_action: clean_optional(input.next_action, "next action", MAX_NEXT_ACTION_CHARS)?,
            due_at: input.due_at,
            source_session_id: clean_source(input.source_session_id),
            source_message_id: clean_source(input.source_message_id),
            resolution_note: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
            runtime_idempotency_key: None,
            runtime_provenance: None,
        };
        let _guard = self.lock.lock().unwrap();
        let mut file = self.load_locked(tutor_id)?;
        file.entries.push(entry.clone());
        self.save_locked(tutor_id, &file)?;
        Ok(entry)
    }

    pub fn upsert_runtime(
        &self,
        tutor_id: &str,
        input: RuntimeTutorMemoryWrite,
    ) -> Result<TutorMemoryEntry, TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let idempotency_key = clean_required(input.idempotency_key, "idempotency key", 512)?;
        let _guard = self.lock.lock().unwrap();
        let mut file = self.load_locked(tutor_id)?;
        if let Some(existing) = file.entries.iter().find(|entry| {
            entry.runtime_idempotency_key.as_deref() == Some(idempotency_key.as_str())
        }) {
            return Ok(existing.clone());
        }
        let now = Utc::now();
        let entry = TutorMemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            tutor_id: tutor_id.to_string(),
            kind: input.kind,
            text: clean_required(input.text, "memory text", MAX_MEMORY_TEXT_CHARS)?,
            status: TutorMemoryStatus::Active,
            next_action: clean_optional(input.next_action, "next action", MAX_NEXT_ACTION_CHARS)?,
            due_at: None,
            source_session_id: clean_source(input.source_session_id),
            source_message_id: clean_source(input.source_message_id),
            resolution_note: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
            runtime_idempotency_key: Some(idempotency_key),
            runtime_provenance: Some(input.provenance),
        };
        file.entries.push(entry.clone());
        self.save_locked(tutor_id, &file)?;
        Ok(entry)
    }

    pub fn update(
        &self,
        tutor_id: &str,
        entry_id: &str,
        input: UpdateTutorMemoryEntry,
    ) -> Result<TutorMemoryEntry, TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let _guard = self.lock.lock().unwrap();
        let mut file = self.load_locked(tutor_id)?;
        let entry = file
            .entries
            .iter_mut()
            .find(|entry| entry.id == entry_id)
            .ok_or(TutorMemoryError::NotFound)?;
        if let Some(kind) = input.kind {
            entry.kind = kind;
        }
        if let Some(text) = input.text {
            entry.text = clean_required(text, "memory text", MAX_MEMORY_TEXT_CHARS)?;
        }
        if let Some(next_action) = input.next_action {
            entry.next_action = clean_optional(next_action, "next action", MAX_NEXT_ACTION_CHARS)?;
        }
        if let Some(due_at) = input.due_at {
            entry.due_at = due_at;
        }
        if let Some(note) = input.resolution_note {
            entry.resolution_note = clean_optional(note, "resolution note", MAX_NEXT_ACTION_CHARS)?;
        }
        if let Some(status) = input.status {
            entry.status = status;
            entry.resolved_at = (status == TutorMemoryStatus::Resolved).then(Utc::now);
            if status == TutorMemoryStatus::Active {
                entry.resolution_note = None;
            }
        }
        entry.updated_at = Utc::now();
        let updated = entry.clone();
        self.save_locked(tutor_id, &file)?;
        Ok(updated)
    }

    pub fn resolve(
        &self,
        tutor_id: &str,
        entry_id: &str,
        note: Option<String>,
    ) -> Result<TutorMemoryEntry, TutorMemoryError> {
        self.update(
            tutor_id,
            entry_id,
            UpdateTutorMemoryEntry {
                status: Some(TutorMemoryStatus::Resolved),
                resolution_note: Some(note),
                ..Default::default()
            },
        )
    }

    pub fn delete(&self, tutor_id: &str, entry_id: &str) -> Result<(), TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let _guard = self.lock.lock().unwrap();
        let mut file = self.load_locked(tutor_id)?;
        let before = file.entries.len();
        file.entries.retain(|entry| entry.id != entry_id);
        if file.entries.len() == before {
            return Err(TutorMemoryError::NotFound);
        }
        self.save_locked(tutor_id, &file)
    }

    pub fn delete_exact(
        &self,
        tutor_id: &str,
        entry_id: &str,
        expected_revision: &str,
    ) -> Result<ExactTutorMemoryDeleteOutcome, TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let _guard = self.lock.lock().unwrap();
        let mut file = self.load_locked(tutor_id)?;
        let Some(index) = file.entries.iter().position(|entry| entry.id == entry_id) else {
            return Ok(ExactTutorMemoryDeleteOutcome::NotFound);
        };
        let latest_revision = tutor_memory_entry_revision(&file.entries[index]);
        if latest_revision != expected_revision {
            return Ok(ExactTutorMemoryDeleteOutcome::Stale { latest_revision });
        }
        file.entries.remove(index);
        self.save_locked(tutor_id, &file)?;
        Ok(ExactTutorMemoryDeleteOutcome::Deleted)
    }

    pub fn reset(&self, tutor_id: &str) -> Result<(), TutorMemoryError> {
        validate_tutor_id(tutor_id)?;
        let _guard = self.lock.lock().unwrap();
        self.save_locked(tutor_id, &TutorMemoryFile::default())
    }

    fn load_locked(&self, tutor_id: &str) -> Result<TutorMemoryFile, TutorMemoryError> {
        let path = self.memory_path(tutor_id);
        match fs::read_to_string(path) {
            Ok(text) => {
                let mut file = serde_json::from_str::<TutorMemoryFile>(&text)
                    .map_err(anyhow::Error::from)
                    .map_err(TutorMemoryError::Storage)?;
                file.schema_version = schema_version();
                Ok(file)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(TutorMemoryFile::default())
            }
            Err(error) => Err(TutorMemoryError::Storage(error.into())),
        }
    }

    fn save_locked(&self, tutor_id: &str, file: &TutorMemoryFile) -> Result<(), TutorMemoryError> {
        let path = self.memory_path(tutor_id);
        let parent = path.parent().expect("tutor memory path has parent");
        fs::create_dir_all(parent).map_err(anyhow::Error::from)?;
        let temp = path.with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(file).map_err(anyhow::Error::from)?;
        fs::write(&temp, bytes).map_err(anyhow::Error::from)?;
        if let Err(rename_error) = fs::rename(&temp, &path) {
            fs::copy(&temp, &path).map_err(|_| anyhow::Error::from(rename_error))?;
            let _ = fs::remove_file(&temp);
        }
        Ok(())
    }

    fn memory_path(&self, tutor_id: &str) -> PathBuf {
        self.root.join(tutor_id).join("memory.json")
    }
}

fn schema_version() -> u32 {
    2
}

pub fn tutor_memory_entry_revision(entry: &TutorMemoryEntry) -> String {
    let normalized =
        serde_json::to_string(entry).expect("TutorMemoryEntry serialization is infallible");
    let mut hash = 0xcbf29ce484222325u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn status_rank(status: TutorMemoryStatus) -> u8 {
    match status {
        TutorMemoryStatus::Active => 0,
        TutorMemoryStatus::Resolved => 1,
    }
}

fn validate_tutor_id(tutor_id: &str) -> Result<(), TutorMemoryError> {
    if tutor_id.is_empty() || tutor_id == "." || tutor_id == ".." || tutor_id.contains(['/', '\\'])
    {
        return Err(TutorMemoryError::Validation("invalid tutor id".into()));
    }
    Ok(())
}

fn clean_required(
    value: String,
    label: &str,
    max_chars: usize,
) -> Result<String, TutorMemoryError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(TutorMemoryError::Validation(format!("{label} is required")));
    }
    if value.chars().count() > max_chars {
        return Err(TutorMemoryError::Validation(format!(
            "{label} exceeds {max_chars} characters"
        )));
    }
    Ok(value)
}

fn clean_optional(
    value: Option<String>,
    label: &str,
    max_chars: usize,
) -> Result<Option<String>, TutorMemoryError> {
    value
        .map(|value| clean_required(value, label, max_chars))
        .transpose()
}

fn clean_source(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str) -> CreateTutorMemoryEntry {
        CreateTutorMemoryEntry {
            kind: TutorMemoryKind::OpenLoop,
            text: text.into(),
            next_action: Some("Continue next time".into()),
            due_at: None,
            source_session_id: Some("session-1".into()),
            source_message_id: None,
        }
    }

    #[test]
    fn tutor_memories_are_persistent_and_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let store = TutorMemoryStore::new_with_root(dir.path());
        let created = store.create("tutor-a", input("Finish exercise 3")).unwrap();

        assert_eq!(store.list("tutor-a", false).unwrap(), vec![created.clone()]);
        assert!(store.list("tutor-b", true).unwrap().is_empty());

        let reopened = TutorMemoryStore::new_with_root(dir.path());
        assert_eq!(reopened.get("tutor-a", &created.id).unwrap(), created);
    }

    #[test]
    fn resolve_delete_and_reset_only_touch_the_bound_tutor() {
        let dir = tempfile::tempdir().unwrap();
        let store = TutorMemoryStore::new_with_root(dir.path());
        let first = store.create("tutor-a", input("First")).unwrap();
        let second = store.create("tutor-b", input("Second")).unwrap();

        let resolved = store
            .resolve("tutor-a", &first.id, Some("Done".into()))
            .unwrap();
        assert_eq!(resolved.status, TutorMemoryStatus::Resolved);
        assert!(store.list("tutor-a", false).unwrap().is_empty());
        assert_eq!(store.list("tutor-b", false).unwrap()[0].id, second.id);

        store.reset("tutor-a").unwrap();
        assert!(store.list("tutor-a", true).unwrap().is_empty());
        assert_eq!(store.list("tutor-b", true).unwrap().len(), 1);
        let reset_file: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("tutor-a/memory.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(reset_file["schema_version"], 2);
    }
}
