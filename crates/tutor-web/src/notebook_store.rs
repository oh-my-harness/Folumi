use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotebookEntryType {
    Note,
    ResearchReport,
    ChatAnswer,
    SourceSnippet,
    QuizSummary,
    DeepSolveResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookEntry {
    pub id: String,
    pub space_id: String,
    pub entry_type: NotebookEntryType,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub markdown: String,
    pub metadata: Option<serde_json::Value>,
    pub source_session_id: Option<String>,
    pub source_message_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookLink {
    pub raw: String,
    pub target: String,
    pub alias: Option<String>,
    pub target_id: Option<String>,
    pub target_title: Option<String>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookBacklink {
    pub source_entry_id: String,
    pub source_title: String,
    pub raw: String,
    pub alias: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookEntryView {
    #[serde(flatten)]
    pub entry: NotebookEntry,
    pub tags: Vec<String>,
    pub links: Vec<NotebookLink>,
    pub backlinks: Vec<NotebookBacklink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookEntrySummary {
    #[serde(flatten)]
    pub entry: NotebookEntry,
    pub tags: Vec<String>,
    pub links: Vec<NotebookLink>,
    pub backlinks: Vec<NotebookBacklink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookEntryListItem {
    pub id: String,
    pub space_id: String,
    pub entry_type: NotebookEntryType,
    pub title: String,
    pub path: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub source_session_id: Option<String>,
    pub source_message_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub file_size: Option<u64>,
    pub file_modified_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum ExactNotebookMutationOutcome {
    Updated(Box<NotebookEntry>),
    NotFound,
    Stale { latest_revision: String },
}

pub struct NotebookStore {
    index_path: PathBuf,
    config_path: PathBuf,
    vault_root: Mutex<PathBuf>,
    items: Mutex<Vec<NotebookEntry>>,
    folders: Mutex<HashSet<String>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    watcher_generation: AtomicU64,
    watch_status: Mutex<NotebookWatchStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NotebookConfig {
    vault_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotebookIndex {
    entries: Vec<NotebookIndexEntry>,
    #[serde(default)]
    folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotebookIndexEntry {
    id: String,
    space_id: String,
    entry_type: NotebookEntryType,
    title: String,
    path: String,
    metadata: Option<serde_json::Value>,
    source_session_id: Option<String>,
    source_message_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    file_size: Option<u64>,
    #[serde(default)]
    file_modified_ms: Option<i64>,
}

impl NotebookStore {
    pub fn new() -> Self {
        Self::new_with_path(default_root().join("notebook"))
    }

    pub fn new_with_path(path: PathBuf) -> Self {
        let index_path = path.join("index.json");
        let config_path = path.join("config.json");
        let configured_vault_root = load_config(&config_path).and_then(|config| config.vault_root);
        let vault_root = configured_vault_root.unwrap_or_else(|| path.join("vault"));
        fs::create_dir_all(&vault_root).expect("failed to create notebook vault directory");
        let (items, folders) =
            load_file_backed_entries(&index_path, &vault_root).unwrap_or_default();
        Self {
            index_path,
            config_path,
            vault_root: Mutex::new(vault_root),
            items: Mutex::new(items),
            folders: Mutex::new(folders.into_iter().collect()),
            watcher: Mutex::new(None),
            watcher_generation: AtomicU64::new(0),
            watch_status: Mutex::new(NotebookWatchStatus::default()),
        }
    }

    pub fn vault_info(&self) -> NotebookVaultInfo {
        let root = self.vault_root.lock().unwrap().clone();
        NotebookVaultInfo {
            root: root.to_string_lossy().to_string(),
            external: load_config(&self.config_path)
                .and_then(|config| config.vault_root)
                .is_some(),
            entries: self.items.lock().unwrap().len(),
        }
    }

    pub fn set_vault_root(&self, path: PathBuf) -> Result<NotebookVaultMount> {
        if !path.is_dir() {
            return Err(anyhow!(
                "notebook vault folder does not exist or is not a directory"
            ));
        }
        let root = path.canonicalize()?;
        let previous_items = self.items.lock().unwrap().clone();
        let scan = scan_vault_root(&root, &previous_items)?;
        let entries = scan.entries;
        let folders = scan.folders;
        fs::create_dir_all(
            self.config_path
                .parent()
                .ok_or_else(|| anyhow!("notebook config path has no parent"))?,
        )?;
        fs::write(
            &self.config_path,
            serde_json::to_string_pretty(&NotebookConfig {
                vault_root: Some(root.clone()),
            })?,
        )?;
        {
            let mut vault_root = self.vault_root.lock().unwrap();
            *vault_root = root;
        }
        {
            let mut items = self.items.lock().unwrap();
            *items = entries.clone();
        }
        {
            let mut stored_folders = self.folders.lock().unwrap();
            *stored_folders = folders.iter().cloned().collect();
            self.save_index_locked(&entries, &stored_folders)?;
        }
        Ok(NotebookVaultMount {
            vault: self.vault_info(),
            entries,
            folders,
        })
    }

    pub fn list(&self, space_id: Option<&str>) -> Vec<NotebookEntry> {
        let mut items: Vec<_> = self
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|item| space_id.is_none_or(|space_id| item.space_id == space_id))
            .cloned()
            .collect();
        items.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
        items
    }

    pub fn get(&self, id: &str) -> Option<NotebookEntry> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(|entry| self.hydrate_entry(entry))
    }

    pub fn refresh_from_vault(&self) -> Result<NotebookRefreshResult> {
        let root = self.vault_root.lock().unwrap().clone();
        let previous_items = self.items.lock().unwrap().clone();
        let scan = scan_vault_root(&root, &previous_items)?;
        let entries = scan.entries;
        let folders = scan.folders;
        let entry_count = entries.len();
        let folder_count = folders.len();
        {
            let mut items = self.items.lock().unwrap();
            *items = entries.clone();
        }
        {
            let mut stored_folders = self.folders.lock().unwrap();
            stored_folders.extend(folders);
            self.save_index_locked(&entries, &stored_folders)?;
        }
        Ok(NotebookRefreshResult {
            entries: entry_count,
            folders: folder_count,
            added: scan.added,
            changed: scan.changed,
            unchanged: scan.unchanged,
            removed: scan.removed,
        })
    }

    pub fn start_watcher(self: &Arc<Self>) -> Result<NotebookWatchInfo> {
        let root = self.vault_root.lock().unwrap().clone();
        let generation = self.watcher_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel::<()>();
        let store = Arc::downgrade(self);
        thread::spawn(move || {
            while rx.recv().is_ok() {
                thread::sleep(Duration::from_millis(350));
                while rx.try_recv().is_ok() {}
                let Some(store) = store.upgrade() else {
                    break;
                };
                if store.watcher_generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                let result = store.refresh_from_vault();
                let mut status = store.watch_status.lock().unwrap();
                match result {
                    Ok(refresh) => {
                        status.watching = true;
                        status.root = Some(
                            store
                                .vault_root
                                .lock()
                                .unwrap()
                                .to_string_lossy()
                                .to_string(),
                        );
                        status.last_refreshed_at = Some(Utc::now());
                        status.last_result = Some(refresh);
                        status.last_error = None;
                    }
                    Err(error) => {
                        status.last_error = Some(error.to_string());
                    }
                }
            }
        });

        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                if event.is_ok() {
                    let _ = tx.send(());
                }
            },
            Config::default(),
        )?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        {
            let mut status = self.watch_status.lock().unwrap();
            status.watching = true;
            status.root = Some(root.to_string_lossy().to_string());
            status.last_error = None;
        }
        {
            let mut current = self.watcher.lock().unwrap();
            *current = Some(watcher);
        }
        Ok(self.watch_info())
    }

    pub fn watch_info(&self) -> NotebookWatchInfo {
        self.watch_status.lock().unwrap().clone().into()
    }

    pub fn list_views(&self, space_id: Option<&str>) -> Vec<NotebookEntryView> {
        let entries = self.list_hydrated(space_id);
        entry_views(&entries)
    }

    pub fn list_items(&self, space_id: Option<&str>) -> Vec<NotebookEntryListItem> {
        self.list(space_id)
            .into_iter()
            .map(|entry| NotebookEntryListItem {
                tags: metadata_tags(entry.metadata.as_ref()),
                id: entry.id,
                space_id: entry.space_id,
                entry_type: entry.entry_type,
                title: entry.title,
                path: entry.path,
                metadata: entry.metadata,
                source_session_id: entry.source_session_id,
                source_message_id: entry.source_message_id,
                created_at: entry.created_at,
                updated_at: entry.updated_at,
                file_size: entry.file_size,
                file_modified_ms: entry.file_modified_ms,
            })
            .collect()
    }

    pub fn list_summaries(&self, space_id: Option<&str>) -> Vec<NotebookEntrySummary> {
        let entries = self.list_hydrated(space_id);
        let all_entries = self.list_hydrated(None);
        entries
            .into_iter()
            .map(|entry| entry_summary(entry, &all_entries))
            .collect()
    }

    pub fn get_view(&self, id: &str) -> Option<NotebookEntryView> {
        let entries = self.list_hydrated(None);
        let entry = entries.iter().find(|item| item.id == id)?.clone();
        Some(entry_view(entry, &entries))
    }

    fn list_hydrated(&self, space_id: Option<&str>) -> Vec<NotebookEntry> {
        self.list(space_id)
            .into_iter()
            .map(|entry| self.hydrate_entry(entry))
            .collect()
    }

    fn hydrate_entry(&self, mut entry: NotebookEntry) -> NotebookEntry {
        if let Some(path) = entry.path.as_ref() {
            let vault_root = self.vault_root.lock().unwrap().clone();
            if let Ok(markdown) = fs::read_to_string(vault_root.join(path)) {
                entry.markdown = markdown;
            }
        }
        entry
    }

    pub fn list_folders(&self) -> Vec<String> {
        let mut folders = self
            .folders
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        folders.sort_by_key(|folder| folder.to_lowercase());
        folders
    }

    pub fn create_folder(&self, path: &str) -> Result<String> {
        let Some(folder) = normalize_folder_path(path) else {
            return Err(anyhow!("notebook folder path is empty"));
        };
        let vault_root = self.vault_root.lock().unwrap().clone();
        fs::create_dir_all(vault_root.join(&folder))?;
        let items = self.items.lock().unwrap();
        let mut folders = self.folders.lock().unwrap();
        folders.insert(folder.clone());
        self.save_index_locked(&items, &folders)?;
        Ok(folder)
    }

    pub fn create(&self, input: NotebookEntryInput) -> Result<NotebookEntry> {
        validate_notebook_entry_input(&input)?;
        let mut items = self.items.lock().unwrap();
        let mut used_paths = used_entry_paths(&items);
        let entry = entry_from_input(input, Utc::now(), &mut used_paths);
        items.push(entry.clone());
        let mut folders = self.folders.lock().unwrap();
        add_parent_folders(&entry, &mut folders);
        self.save_locked(&items, &folders)?;
        Ok(entry)
    }

    pub fn create_strict(&self, mut input: NotebookEntryInput) -> Result<NotebookEntry> {
        validate_notebook_entry_input(&input)?;
        let requested_path = input
            .path
            .as_deref()
            .map(|path| {
                normalize_note_path(path).ok_or_else(|| anyhow!("notebook path is invalid"))
            })
            .transpose()?;
        let mut items = self.items.lock().unwrap();
        if let Some(path) = requested_path.as_deref()
            && items.iter().any(|entry| {
                entry
                    .path
                    .as_deref()
                    .is_some_and(|existing| existing.eq_ignore_ascii_case(path))
            })
        {
            return Err(anyhow!("notebook path already exists"));
        }
        input.path = requested_path;
        let mut used_paths = used_entry_paths(&items);
        let entry = entry_from_input(input, Utc::now(), &mut used_paths);
        items.push(entry.clone());
        let mut folders = self.folders.lock().unwrap();
        add_parent_folders(&entry, &mut folders);
        self.save_locked(&items, &folders)?;
        Ok(entry)
    }

    pub fn create_many(&self, inputs: Vec<NotebookEntryInput>) -> Result<Vec<NotebookEntry>> {
        let now = Utc::now();
        let mut entries = Vec::with_capacity(inputs.len());
        let mut items = self.items.lock().unwrap();
        let mut used_paths = used_entry_paths(&items);
        for input in inputs {
            validate_notebook_entry_input(&input)?;
            entries.push(entry_from_input(input, now, &mut used_paths));
        }
        if entries.is_empty() {
            return Ok(entries);
        }
        items.extend(entries.iter().cloned());
        let mut folders = self.folders.lock().unwrap();
        for entry in &entries {
            add_parent_folders(entry, &mut folders);
        }
        self.save_locked(&items, &folders)?;
        Ok(entries)
    }

    pub fn update(&self, id: &str, input: NotebookEntryUpdate) -> Result<NotebookEntry> {
        let mut items = self.items.lock().unwrap();
        let Some(entry_index) = items.iter().position(|item| item.id == id) else {
            return Err(anyhow!("notebook entry not found"));
        };
        let updated = apply_notebook_entry_update(&mut items, entry_index, input)?;
        let folders = self.folders.lock().unwrap();
        self.save_locked(&items, &folders)?;
        Ok(updated)
    }

    pub fn update_exact(
        &self,
        id: &str,
        expected_revision: &str,
        input: NotebookEntryUpdate,
    ) -> Result<ExactNotebookMutationOutcome> {
        let mut items = self.items.lock().unwrap();
        let Some(entry_index) = items.iter().position(|item| item.id == id) else {
            return Ok(ExactNotebookMutationOutcome::NotFound);
        };
        let latest_revision = notebook_entry_revision(&items[entry_index]);
        if latest_revision != expected_revision {
            return Ok(ExactNotebookMutationOutcome::Stale { latest_revision });
        }
        let updated = apply_notebook_entry_update(&mut items, entry_index, input)?;
        let folders = self.folders.lock().unwrap();
        self.save_locked(&items, &folders)?;
        Ok(ExactNotebookMutationOutcome::Updated(Box::new(updated)))
    }

    pub fn move_exact(
        &self,
        id: &str,
        expected_revision: &str,
        requested_path: &str,
    ) -> Result<ExactNotebookMutationOutcome> {
        let mut items = self.items.lock().unwrap();
        let Some(entry_index) = items.iter().position(|item| item.id == id) else {
            return Ok(ExactNotebookMutationOutcome::NotFound);
        };
        let latest_revision = notebook_entry_revision(&items[entry_index]);
        if latest_revision != expected_revision {
            return Ok(ExactNotebookMutationOutcome::Stale { latest_revision });
        }
        let path = normalize_note_path(requested_path)
            .ok_or_else(|| anyhow!("notebook path is invalid"))?;
        if items.iter().enumerate().any(|(index, entry)| {
            index != entry_index
                && entry
                    .path
                    .as_deref()
                    .is_some_and(|existing| existing.eq_ignore_ascii_case(&path))
        }) {
            return Err(anyhow!("notebook path already exists"));
        }
        let previous_path = items[entry_index].path.clone();
        items[entry_index].path = Some(path);
        items[entry_index].updated_at = Utc::now();
        items[entry_index].file_size = None;
        items[entry_index].file_modified_ms = None;
        let updated = items[entry_index].clone();
        let mut folders = self.folders.lock().unwrap();
        add_parent_folders(&updated, &mut folders);
        self.save_locked(&items, &folders)?;
        if let Some(previous_path) = previous_path
            && updated.path.as_deref() != Some(previous_path.as_str())
        {
            let vault_root = self.vault_root.lock().unwrap().clone();
            let previous_file = vault_root.join(previous_path);
            if previous_file.is_file() {
                fs::remove_file(previous_file)?;
            }
        }
        Ok(ExactNotebookMutationOutcome::Updated(Box::new(updated)))
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        let removed = items.iter().find(|item| item.id == id).cloned();
        items.retain(|item| item.id != id);
        let deleted = items.len() != before;
        if deleted {
            let folders = self.folders.lock().unwrap();
            let _ = self.save_locked(&items, &folders);
            if let Some(entry) = removed.and_then(|entry| entry.path) {
                let vault_root = self.vault_root.lock().unwrap().clone();
                let _ = fs::remove_file(vault_root.join(entry));
            }
        }
        deleted
    }

    fn save_locked(&self, items: &[NotebookEntry], folders: &HashSet<String>) -> Result<()> {
        let vault_root = self.vault_root.lock().unwrap().clone();
        fs::create_dir_all(&vault_root)?;
        for entry in items {
            let path = entry
                .path
                .clone()
                .unwrap_or_else(|| safe_markdown_file_name(&entry.title));
            let markdown_path = vault_root.join(&path);
            if let Some(parent) = markdown_path.parent() {
                fs::create_dir_all(parent)?;
            }
            if should_skip_markdown_write(entry, &markdown_path) {
                continue;
            }
            fs::write(markdown_path, &entry.markdown)?;
        }
        self.save_index_locked(items, folders)?;
        Ok(())
    }

    fn save_index_locked(&self, items: &[NotebookEntry], folders: &HashSet<String>) -> Result<()> {
        if let Some(parent) = self.index_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut index = Vec::with_capacity(items.len());
        for entry in items {
            let path = entry
                .path
                .clone()
                .unwrap_or_else(|| safe_markdown_file_name(&entry.title));
            index.push(NotebookIndexEntry::from_entry(entry, path));
        }
        let mut folders = folders.iter().cloned().collect::<Vec<_>>();
        folders.sort_by_key(|folder| folder.to_lowercase());
        fs::write(
            &self.index_path,
            serde_json::to_string_pretty(&NotebookIndex {
                entries: index,
                folders,
            })?,
        )?;
        Ok(())
    }
}

fn apply_notebook_entry_update(
    items: &mut [NotebookEntry],
    entry_index: usize,
    input: NotebookEntryUpdate,
) -> Result<NotebookEntry> {
    let needs_path = items[entry_index].path.is_none();
    let entry_id = items[entry_index].id.clone();
    let mut used_paths = if needs_path {
        Some(used_entry_paths_excluding(items, &entry_id))
    } else {
        None
    };
    let entry = &mut items[entry_index];
    if let Some(title) = input.title {
        entry.title = normalize_title(&title);
        if entry.path.is_none() {
            let used_paths = used_paths.get_or_insert_with(HashSet::new);
            entry.path = Some(unique_note_path(&entry.title, None, used_paths));
        }
    }
    if let Some(markdown) = input.markdown {
        if markdown.trim().is_empty() {
            return Err(anyhow!("notebook markdown is empty"));
        }
        entry.markdown = markdown;
        entry.file_size = None;
        entry.file_modified_ms = None;
    }
    if let Some(metadata) = input.metadata {
        entry.metadata = Some(metadata);
    }
    if let Some(source_session_id) = input.source_session_id {
        entry.source_session_id = clean_optional(Some(source_session_id));
    }
    if let Some(source_message_id) = input.source_message_id {
        entry.source_message_id = clean_optional(Some(source_message_id));
    }
    entry.updated_at = Utc::now();
    Ok(entry.clone())
}

pub fn notebook_entry_revision(entry: &NotebookEntry) -> String {
    let normalized =
        serde_json::to_string(entry).expect("NotebookEntry serialization is infallible");
    let mut hash = 0xcbf29ce484222325u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookVaultInfo {
    pub root: String,
    pub external: bool,
    pub entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookVaultMount {
    pub vault: NotebookVaultInfo,
    pub entries: Vec<NotebookEntry>,
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookRefreshResult {
    pub entries: usize,
    pub folders: usize,
    pub added: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Default)]
struct NotebookWatchStatus {
    watching: bool,
    root: Option<String>,
    last_refreshed_at: Option<DateTime<Utc>>,
    last_result: Option<NotebookRefreshResult>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookWatchInfo {
    pub watching: bool,
    pub root: Option<String>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub last_result: Option<NotebookRefreshResult>,
    pub last_error: Option<String>,
}

impl From<NotebookWatchStatus> for NotebookWatchInfo {
    fn from(value: NotebookWatchStatus) -> Self {
        Self {
            watching: value.watching,
            root: value.root,
            last_refreshed_at: value.last_refreshed_at,
            last_result: value.last_result,
            last_error: value.last_error,
        }
    }
}

impl NotebookIndexEntry {
    fn from_entry(entry: &NotebookEntry, path: String) -> Self {
        Self {
            id: entry.id.clone(),
            space_id: entry.space_id.clone(),
            entry_type: entry.entry_type.clone(),
            title: entry.title.clone(),
            path,
            metadata: entry.metadata.clone(),
            source_session_id: entry.source_session_id.clone(),
            source_message_id: entry.source_message_id.clone(),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            file_size: entry.file_size,
            file_modified_ms: entry.file_modified_ms,
        }
    }

    fn into_entry(self, markdown: String) -> NotebookEntry {
        NotebookEntry {
            id: self.id,
            space_id: self.space_id,
            entry_type: self.entry_type,
            title: self.title,
            path: Some(self.path),
            markdown,
            metadata: self.metadata,
            source_session_id: self.source_session_id,
            source_message_id: self.source_message_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            file_size: self.file_size,
            file_modified_ms: self.file_modified_ms,
        }
    }
}

fn entry_from_input(
    input: NotebookEntryInput,
    now: DateTime<Utc>,
    used_paths: &mut HashSet<String>,
) -> NotebookEntry {
    let title = normalize_title(&input.title);
    let path = unique_note_path(&title, input.path.as_deref(), used_paths);
    NotebookEntry {
        id: uuid::Uuid::new_v4().to_string(),
        space_id: normalize_space_id(input.space_id),
        entry_type: input.entry_type,
        title,
        path: Some(path),
        markdown: input.markdown,
        metadata: input.metadata,
        source_session_id: clean_optional(input.source_session_id),
        source_message_id: clean_optional(input.source_message_id),
        created_at: now,
        updated_at: now,
        file_size: None,
        file_modified_ms: None,
    }
}

fn load_file_backed_entries(
    index_path: &Path,
    _vault_root: &Path,
) -> Result<(Vec<NotebookEntry>, Vec<String>)> {
    if !index_path.exists() {
        return Err(anyhow!("notebook index not found"));
    }
    let text = fs::read_to_string(index_path)?;
    let index = parse_notebook_index(&text)?;
    let mut entries = Vec::with_capacity(index.entries.len());
    for item in index.entries {
        entries.push(item.into_entry(String::new()));
    }
    Ok((entries, normalize_folder_paths(index.folders)))
}

fn parse_notebook_index(text: &str) -> Result<NotebookIndex> {
    if let Ok(index) = serde_json::from_str::<NotebookIndex>(text) {
        return Ok(index);
    }
    Ok(NotebookIndex {
        entries: serde_json::from_str::<Vec<NotebookIndexEntry>>(text)?,
        folders: Vec::new(),
    })
}

fn load_config(config_path: &Path) -> Option<NotebookConfig> {
    fs::read_to_string(config_path)
        .ok()
        .and_then(|text| serde_json::from_str::<NotebookConfig>(&text).ok())
}

struct VaultScan {
    entries: Vec<NotebookEntry>,
    folders: Vec<String>,
    added: usize,
    changed: usize,
    unchanged: usize,
    removed: usize,
}

#[derive(Debug, Clone)]
struct VaultFile {
    relative_path: String,
    size: Option<u64>,
    modified_ms: Option<i64>,
}

fn scan_vault_root(root: &Path, previous_items: &[NotebookEntry]) -> Result<VaultScan> {
    let mut files = Vec::new();
    collect_markdown_files(root, root, &mut files)?;
    files.sort_by_key(|file| file.relative_path.to_lowercase());

    let previous_by_path = previous_items
        .iter()
        .filter_map(|entry| {
            entry
                .path
                .as_ref()
                .map(|path| (path.to_lowercase(), entry.clone()))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let now = Utc::now();
    let mut entries = Vec::with_capacity(files.len());
    let mut folders = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut added = 0usize;
    let mut changed = 0usize;
    let mut unchanged = 0usize;

    for file in files {
        let relative_path = file.relative_path;
        let lower_path = relative_path.to_lowercase();
        seen_paths.insert(lower_path.clone());
        let title = title_from_note_path(&relative_path);
        let previous = previous_by_path.get(&lower_path).cloned();
        let unchanged_file = previous.as_ref().is_some_and(|entry| {
            entry.file_size == file.size && entry.file_modified_ms == file.modified_ms
        });
        let mut entry = if let Some(mut entry) = previous {
            if unchanged_file {
                unchanged += 1;
            } else {
                changed += 1;
                entry.markdown = fs::read_to_string(root.join(&relative_path))?;
                entry.updated_at = now;
            }
            entry
        } else {
            added += 1;
            NotebookEntry {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                entry_type: NotebookEntryType::Note,
                title: title.clone(),
                path: Some(relative_path.clone()),
                markdown: fs::read_to_string(root.join(&relative_path))?,
                metadata: Some(serde_json::json!({
                    "source": "external_vault",
                    "source_path": relative_path,
                })),
                source_session_id: None,
                source_message_id: None,
                created_at: now,
                updated_at: now,
                file_size: None,
                file_modified_ms: None,
            }
        };
        entry.title = title;
        entry.path = Some(relative_path);
        entry.file_size = file.size;
        entry.file_modified_ms = file.modified_ms;
        add_parent_folders(&entry, &mut folders);
        entries.push(entry);
    }

    let mut folders = folders.into_iter().collect::<Vec<_>>();
    folders.sort_by_key(|folder| folder.to_lowercase());
    let removed = previous_by_path
        .keys()
        .filter(|path| !seen_paths.contains(*path))
        .count();
    Ok(VaultScan {
        entries,
        folders,
        added,
        changed,
        unchanged,
        removed,
    })
}

fn collect_markdown_files(root: &Path, dir: &Path, files: &mut Vec<VaultFile>) -> Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let path = item.path();
        let file_type = item.file_type()?;
        if file_type.is_dir() {
            collect_markdown_files(root, &path, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let lower = relative_path.to_lowercase();
        if lower.ends_with(".md") || lower.ends_with(".markdown") {
            let metadata = item.metadata().ok();
            files.push(VaultFile {
                relative_path,
                size: metadata.as_ref().map(|metadata| metadata.len()),
                modified_ms: metadata
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(system_time_to_epoch_ms),
            });
        }
    }
    Ok(())
}

fn should_skip_markdown_write(entry: &NotebookEntry, path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    if entry.markdown.is_empty() {
        return true;
    }
    let Some(expected_size) = entry.file_size else {
        return false;
    };
    let Some(expected_modified_ms) = entry.file_modified_ms else {
        return false;
    };
    let Some(stats) = vault_file_stats(path) else {
        return false;
    };
    stats.size == Some(expected_size) && stats.modified_ms == Some(expected_modified_ms)
}

fn vault_file_stats(path: &Path) -> Option<VaultFileStats> {
    let metadata = fs::metadata(path).ok()?;
    Some(VaultFileStats {
        size: Some(metadata.len()),
        modified_ms: metadata.modified().ok().and_then(system_time_to_epoch_ms),
    })
}

struct VaultFileStats {
    size: Option<u64>,
    modified_ms: Option<i64>,
}

fn system_time_to_epoch_ms(time: SystemTime) -> Option<i64> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn title_from_note_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(normalize_title)
        .unwrap_or_else(|| "Untitled note".to_string())
}

pub fn entry_views(entries: &[NotebookEntry]) -> Vec<NotebookEntryView> {
    entries
        .iter()
        .cloned()
        .map(|entry| entry_view(entry, entries))
        .collect()
}

pub fn entry_view(entry: NotebookEntry, entries: &[NotebookEntry]) -> NotebookEntryView {
    let tags = merge_tags(
        parse_tags(&entry.markdown),
        metadata_tags(entry.metadata.as_ref()),
    );
    let links = parse_links(&entry.markdown)
        .into_iter()
        .map(|link| resolve_link(link, entries))
        .collect::<Vec<_>>();
    let backlinks = entries
        .iter()
        .filter(|source| source.id != entry.id)
        .flat_map(|source| backlinks_from_source(source, &entry, entries))
        .collect::<Vec<_>>();
    NotebookEntryView {
        entry,
        tags,
        links,
        backlinks,
    }
}

pub fn entry_summary(entry: NotebookEntry, entries: &[NotebookEntry]) -> NotebookEntrySummary {
    let tags = merge_tags(
        parse_tags(&entry.markdown),
        metadata_tags(entry.metadata.as_ref()),
    );
    let links = parse_links(&entry.markdown)
        .into_iter()
        .map(|link| resolve_link(link, entries))
        .collect::<Vec<_>>();
    NotebookEntrySummary {
        entry,
        tags,
        links,
        backlinks: Vec::new(),
    }
}

pub fn parse_links(markdown: &str) -> Vec<NotebookLink> {
    let mut links = Vec::new();
    let mut rest = markdown;
    while let Some(start) = rest.find("[[") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("]]") else {
            break;
        };
        let raw_inner = after_start[..end].trim();
        if !raw_inner.is_empty() {
            let (target, alias) = parse_link_inner(raw_inner);
            if !target.is_empty() {
                links.push(NotebookLink {
                    raw: format!("[[{raw_inner}]]"),
                    target,
                    alias,
                    target_id: None,
                    target_title: None,
                    resolved: false,
                });
            }
        }
        rest = &after_start[end + 2..];
    }
    dedupe_links(links)
}

pub fn parse_tags(markdown: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let chars = markdown.char_indices().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if ch != '#' || is_escaped_hash(markdown, byte_index) {
            index += 1;
            continue;
        }
        if byte_index > 0 {
            let previous = markdown[..byte_index].chars().next_back();
            if previous.is_some_and(|value| value.is_alphanumeric() || value == '_') {
                index += 1;
                continue;
            }
        }
        let mut end = byte_index + ch.len_utf8();
        let mut next_index = index + 1;
        while next_index < chars.len() {
            let (next_byte, next_ch) = chars[next_index];
            if next_ch.is_alphanumeric() || matches!(next_ch, '_' | '-' | '/') {
                end = next_byte + next_ch.len_utf8();
                next_index += 1;
            } else {
                break;
            }
        }
        if end > byte_index + 1 {
            tags.push(markdown[byte_index + 1..end].to_string());
        }
        index = next_index.max(index + 1);
    }
    normalize_tags(tags)
}

pub fn metadata_tags(metadata: Option<&serde_json::Value>) -> Vec<String> {
    let Some(metadata) = metadata else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    if let Some(value) = metadata.get("tags") {
        collect_metadata_tags(value, &mut tags);
    }
    if let Some(value) = metadata.get("tag") {
        collect_metadata_tags(value, &mut tags);
    }
    normalize_tags(tags)
}

fn collect_metadata_tags(value: &serde_json::Value, tags: &mut Vec<String>) {
    match value {
        serde_json::Value::String(tag) => {
            tags.extend(split_tag_string(tag));
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_metadata_tags(item, tags);
            }
        }
        _ => {}
    }
}

fn split_tag_string(value: &str) -> Vec<String> {
    value
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(|tag| tag.trim().trim_start_matches('#').trim())
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn merge_tags(markdown_tags: Vec<String>, metadata_tags: Vec<String>) -> Vec<String> {
    normalize_tags(markdown_tags.into_iter().chain(metadata_tags).collect())
}

fn normalize_tags(mut tags: Vec<String>) -> Vec<String> {
    for tag in &mut tags {
        *tag = tag.trim().trim_start_matches('#').trim().to_string();
    }
    tags.retain(|tag| !tag.is_empty());
    tags.sort_by_key(|tag| tag.to_lowercase());
    tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    tags
}

fn parse_link_inner(raw: &str) -> (String, Option<String>) {
    let mut parts = raw.splitn(2, '|');
    let target = parts.next().unwrap_or_default().trim().to_string();
    let alias = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    (target, alias)
}

fn resolve_link(mut link: NotebookLink, entries: &[NotebookEntry]) -> NotebookLink {
    let target_key = normalize_lookup_key(&link.target);
    let resolved = entries
        .iter()
        .find(|entry| entry.id == link.target || normalize_lookup_key(&entry.title) == target_key);
    if let Some(entry) = resolved {
        link.target_id = Some(entry.id.clone());
        link.target_title = Some(entry.title.clone());
        link.resolved = true;
    }
    link
}

fn backlinks_from_source(
    source: &NotebookEntry,
    target: &NotebookEntry,
    entries: &[NotebookEntry],
) -> Vec<NotebookBacklink> {
    parse_links_with_positions(&source.markdown)
        .into_iter()
        .filter_map(|(link, start, end)| {
            let resolved = resolve_link(link, entries);
            if resolved.target_id.as_deref() != Some(target.id.as_str()) {
                return None;
            }
            Some(NotebookBacklink {
                source_entry_id: source.id.clone(),
                source_title: source.title.clone(),
                raw: resolved.raw,
                alias: resolved.alias,
                snippet: snippet_around(&source.markdown, start, end),
            })
        })
        .collect()
}

fn parse_links_with_positions(markdown: &str) -> Vec<(NotebookLink, usize, usize)> {
    let mut links = Vec::new();
    let mut offset = 0usize;
    let mut rest = markdown;
    while let Some(start) = rest.find("[[") {
        let absolute_start = offset + start;
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("]]") else {
            break;
        };
        let absolute_end = absolute_start + 2 + end + 2;
        let raw_inner = after_start[..end].trim();
        if !raw_inner.is_empty() {
            let (target, alias) = parse_link_inner(raw_inner);
            if !target.is_empty() {
                links.push((
                    NotebookLink {
                        raw: format!("[[{raw_inner}]]"),
                        target,
                        alias,
                        target_id: None,
                        target_title: None,
                        resolved: false,
                    },
                    absolute_start,
                    absolute_end,
                ));
            }
        }
        offset = absolute_end;
        rest = &markdown[offset..];
    }
    links
}

fn dedupe_links(links: Vec<NotebookLink>) -> Vec<NotebookLink> {
    let mut deduped = Vec::new();
    for link in links {
        let seen = deduped.iter().any(|existing: &NotebookLink| {
            normalize_lookup_key(&existing.target) == normalize_lookup_key(&link.target)
                && existing.alias == link.alias
        });
        if !seen {
            deduped.push(link);
        }
    }
    deduped
}

fn normalize_lookup_key(value: &str) -> String {
    value.trim().trim_matches('#').trim().to_lowercase()
}

fn snippet_around(markdown: &str, start: usize, end: usize) -> String {
    let left = markdown[..start].chars().rev().take(48).collect::<String>();
    let left = left.chars().rev().collect::<String>();
    let right = markdown[end..].chars().take(72).collect::<String>();
    format!("{left}{}{right}", &markdown[start..end])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_escaped_hash(markdown: &str, byte_index: usize) -> bool {
    markdown[..byte_index]
        .chars()
        .rev()
        .take_while(|ch| *ch == '\\')
        .count()
        % 2
        == 1
}

impl Default for NotebookStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NotebookEntryInput {
    pub space_id: Option<String>,
    pub entry_type: NotebookEntryType,
    pub title: String,
    pub path: Option<String>,
    pub markdown: String,
    pub metadata: Option<serde_json::Value>,
    pub source_session_id: Option<String>,
    pub source_message_id: Option<String>,
}

pub struct NotebookEntryUpdate {
    pub title: Option<String>,
    pub markdown: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub source_session_id: Option<String>,
    pub source_message_id: Option<String>,
}

fn validate_notebook_entry_input(input: &NotebookEntryInput) -> Result<()> {
    if input.markdown.trim().is_empty() {
        return Err(anyhow!("notebook markdown is empty"));
    }
    if input
        .path
        .as_deref()
        .is_some_and(|path| normalize_note_path(path).is_none())
    {
        return Err(anyhow!("notebook path is invalid"));
    }
    Ok(())
}

fn default_root() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".llm-tutor")
}

fn normalize_space_id(space_id: Option<String>) -> String {
    space_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

fn normalize_title(title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        "Untitled note".to_string()
    } else {
        title.chars().take(120).collect()
    }
}

fn used_entry_paths(items: &[NotebookEntry]) -> HashSet<String> {
    items
        .iter()
        .filter_map(|entry| entry.path.as_ref())
        .map(|path| path.to_lowercase())
        .collect()
}

fn used_entry_paths_excluding(items: &[NotebookEntry], excluded_id: &str) -> HashSet<String> {
    items
        .iter()
        .filter(|entry| entry.id != excluded_id)
        .filter_map(|entry| entry.path.as_ref())
        .map(|path| path.to_lowercase())
        .collect()
}

fn unique_note_path(
    title: &str,
    preferred_path: Option<&str>,
    used_paths: &mut HashSet<String>,
) -> String {
    let base = preferred_path
        .and_then(normalize_note_path)
        .unwrap_or_else(|| safe_markdown_file_name(title));
    let mut candidate = base.clone();
    if !used_paths.contains(&candidate.to_lowercase()) {
        used_paths.insert(candidate.to_lowercase());
        return candidate;
    }
    let extension = Path::new(&base)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("md");
    let parent = Path::new(&base)
        .parent()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let stem = Path::new(&base)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("note");
    for index in 2.. {
        let file_name = format!("{stem}-{index}.{extension}");
        candidate = if parent.is_empty() {
            file_name
        } else {
            format!("{}/{}", parent.replace('\\', "/"), file_name)
        };
        if !used_paths.contains(&candidate.to_lowercase()) {
            used_paths.insert(candidate.to_lowercase());
            return candidate;
        }
    }
    unreachable!()
}

fn normalize_note_path(path: &str) -> Option<String> {
    let requested = path.trim().replace('\\', "/");
    if requested.is_empty()
        || requested.starts_with('/')
        || Path::new(&requested).components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return None;
    }
    let mut parts = Vec::new();
    for part in requested.split('/') {
        let part = safe_file_stem(part);
        if !part.is_empty() {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut normalized = parts.join("/");
    if !normalized.to_lowercase().ends_with(".md")
        && !normalized.to_lowercase().ends_with(".markdown")
    {
        normalized.push_str(".md");
    }
    Some(normalized)
}

fn normalize_folder_path(path: &str) -> Option<String> {
    let parts = path
        .replace('\\', "/")
        .split('/')
        .map(safe_file_stem)
        .filter(|part| !part.is_empty() && part != "." && part != "..")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn normalize_folder_paths(paths: Vec<String>) -> Vec<String> {
    let mut folders = paths
        .iter()
        .filter_map(|path| normalize_folder_path(path))
        .collect::<Vec<_>>();
    folders.sort_by_key(|folder| folder.to_lowercase());
    folders.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    folders
}

fn add_parent_folders(entry: &NotebookEntry, folders: &mut HashSet<String>) {
    let Some(path) = entry.path.as_ref() else {
        return;
    };
    let normalized = path.replace('\\', "/");
    let mut parts = normalized.split('/').collect::<Vec<_>>();
    parts.pop();
    let mut current = Vec::new();
    for part in parts {
        if part.trim().is_empty() {
            continue;
        }
        current.push(part);
        if let Some(folder) = normalize_folder_path(&current.join("/")) {
            folders.insert(folder);
        }
    }
}

fn safe_markdown_file_name(title: &str) -> String {
    format!("{}.md", safe_file_stem(title))
}

fn safe_file_stem(title: &str) -> String {
    let name = title
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if name.is_empty() { "note".into() } else { name }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notebook_store_creates_updates_and_deletes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = NotebookStore::new_with_path(dir.path().join("notebook"));
        let entry = store
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::ResearchReport,
                path: None,
                title: " Report ".into(),
                markdown: "# Report".into(),
                metadata: None,
                source_session_id: Some("session-1".into()),
                source_message_id: None,
            })
            .unwrap();

        assert_eq!(entry.space_id, "default");
        assert_eq!(store.list(Some("default")).len(), 1);

        let updated = store
            .update(
                &entry.id,
                NotebookEntryUpdate {
                    title: Some("Updated".into()),
                    markdown: Some("# Updated".into()),
                    metadata: None,
                    source_session_id: None,
                    source_message_id: None,
                },
            )
            .unwrap();
        assert_eq!(updated.title, "Updated");
        assert!(store.delete(&entry.id));
        assert!(store.list(Some("default")).is_empty());
    }

    #[test]
    fn notebook_store_persists_markdown_files_and_index() {
        let dir = tempfile::tempdir().unwrap();
        let notebook_root = dir.path().join("notebook");
        let store = NotebookStore::new_with_path(notebook_root.clone());
        assert_eq!(store.create_folder("empty/folder").unwrap(), "empty/folder");
        let entry = store
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: Some("notes/TCC.md".into()),
                title: "TCC".into(),
                markdown: "# TCC".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();

        let index_path = dir.path().join("notebook").join("index.json");
        let note_path = dir
            .path()
            .join("notebook")
            .join("vault")
            .join("notes")
            .join("TCC.md");
        assert!(index_path.exists());
        assert_eq!(std::fs::read_to_string(&note_path).unwrap(), "# TCC");

        store
            .update(
                &entry.id,
                NotebookEntryUpdate {
                    title: None,
                    markdown: Some("# TCC\n\nUpdated".into()),
                    metadata: None,
                    source_session_id: None,
                    source_message_id: None,
                },
            )
            .unwrap();
        assert!(
            std::fs::read_to_string(&note_path)
                .unwrap()
                .contains("Updated")
        );

        let reloaded = NotebookStore::new_with_path(dir.path().join("notebook"));
        let loaded = reloaded.get(&entry.id).unwrap();
        assert_eq!(loaded.path.as_deref(), Some("notes/TCC.md"));
        assert!(loaded.markdown.contains("Updated"));
        assert_eq!(
            reloaded.list_folders(),
            vec!["empty/folder".to_string(), "notes".to_string()]
        );

        assert!(reloaded.delete(&entry.id));
        assert!(!note_path.exists());
    }

    #[test]
    fn notebook_store_binds_external_vault_and_writes_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let app_root = dir.path().join("app-notebook");
        let vault_root = dir.path().join("external-vault");
        std::fs::create_dir_all(vault_root.join("optics")).unwrap();
        std::fs::write(vault_root.join("optics").join("TCC.md"), "# TCC").unwrap();

        let store = NotebookStore::new_with_path(app_root.clone());
        let mounted = store.set_vault_root(vault_root.clone()).unwrap();
        assert!(mounted.vault.external);
        assert_eq!(mounted.entries.len(), 1);
        assert_eq!(mounted.entries[0].title, "TCC");

        let entry = store
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: Some("new/OPC.md".into()),
                title: "OPC".into(),
                markdown: "# OPC".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(vault_root.join("new").join("OPC.md")).unwrap(),
            "# OPC"
        );
        assert!(!app_root.join("vault").join("new").join("OPC.md").exists());

        std::fs::write(vault_root.join("External.md"), "# External").unwrap();
        let listed = store.list(Some("default"));
        assert!(!listed.iter().any(|item| item.title == "External"));
        let refreshed = store.refresh_from_vault().unwrap();
        assert_eq!(refreshed.entries, 3);
        assert_eq!(refreshed.added, 1);
        assert!(refreshed.unchanged >= 1);
        let listed = store.list(Some("default"));
        assert!(listed.iter().any(|item| item.title == "External"));
        assert!(listed.iter().any(|item| item.id == entry.id));

        let refreshed = store.refresh_from_vault().unwrap();
        assert_eq!(refreshed.added, 0);
        assert_eq!(refreshed.changed, 0);
        assert_eq!(refreshed.unchanged, 3);

        std::fs::remove_file(vault_root.join("External.md")).unwrap();
        let refreshed = store.refresh_from_vault().unwrap();
        assert_eq!(refreshed.entries, 2);
        assert_eq!(refreshed.removed, 1);
    }

    #[test]
    fn notebook_store_watcher_indexes_external_changes() {
        let dir = tempfile::tempdir().unwrap();
        let vault_root = dir.path().join("watched-vault");
        std::fs::create_dir_all(&vault_root).unwrap();
        let store = std::sync::Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        store.set_vault_root(vault_root.clone()).unwrap();
        let watch = store.start_watcher().unwrap();
        assert!(watch.watching);

        std::fs::write(vault_root.join("Watched.md"), "# Watched").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if store
                .list(Some("default"))
                .iter()
                .any(|entry| entry.title == "Watched")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "watcher did not index created note"
            );
            thread::sleep(Duration::from_millis(100));
        }
        let watch = store.watch_info();
        assert!(watch.last_result.is_some());
    }

    #[test]
    fn parses_wiki_links_and_tags() {
        let links = parse_links("See [[Lithography]] and [[note-1|OPC notes]].");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Lithography");
        assert_eq!(links[1].target, "note-1");
        assert_eq!(links[1].alias.as_deref(), Some("OPC notes"));

        let tags = parse_tags("Study #lithography and #weak-point. Ignore email#a and \\#escaped.");
        assert_eq!(tags, vec!["lithography", "weak-point"]);

        let metadata = serde_json::json!({
            "tags": ["#lithography", "opc weak-point"],
            "tag": "research",
        });
        assert_eq!(
            metadata_tags(Some(&metadata)),
            vec!["lithography", "opc", "research", "weak-point"]
        );
    }

    #[test]
    fn entry_view_resolves_links_and_backlinks() {
        let target = NotebookEntry {
            id: "target-1".into(),
            space_id: "default".into(),
            entry_type: NotebookEntryType::Note,
            title: "Lithography".into(),
            path: None,
            markdown: "# Lithography\n\n#process".into(),
            metadata: Some(serde_json::json!({ "tags": ["#semiconductor", "process"] })),
            source_session_id: None,
            source_message_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            file_size: None,
            file_modified_ms: None,
        };
        let source = NotebookEntry {
            id: "source-1".into(),
            space_id: "default".into(),
            entry_type: NotebookEntryType::Note,
            title: "OPC".into(),
            path: None,
            markdown: "OPC is related to [[Lithography|litho]].".into(),
            metadata: None,
            source_session_id: None,
            source_message_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            file_size: None,
            file_modified_ms: None,
        };
        let entries = vec![target.clone(), source.clone()];
        let target_view = entry_view(target, &entries);
        let source_view = entry_view(source, &entries);

        assert_eq!(target_view.tags, vec!["process", "semiconductor"]);
        assert_eq!(target_view.backlinks.len(), 1);
        assert_eq!(target_view.backlinks[0].source_title, "OPC");
        assert!(
            target_view.backlinks[0]
                .snippet
                .contains("[[Lithography|litho]]")
        );
        assert_eq!(source_view.links.len(), 1);
        assert!(source_view.links[0].resolved);
        assert_eq!(source_view.links[0].target_id.as_deref(), Some("target-1"));
    }
}
