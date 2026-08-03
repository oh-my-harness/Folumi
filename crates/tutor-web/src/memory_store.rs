use std::path::PathBuf;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: i64 = 1;
const MAX_CONTENT_CHARS: usize = 1_200;
const MAX_TOPIC_KEY_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeType {
    #[default]
    Global,
    Workspace,
}

impl MemoryScopeType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Workspace => "workspace",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "global" => Ok(Self::Global),
            "workspace" => Ok(Self::Workspace),
            _ => Err(invalid_column("scope_type", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Preference,
    Goal,
    Continuity,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Goal => "goal",
            Self::Continuity => "continuity",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "fact" => Some(Self::Fact),
            "preference" => Some(Self::Preference),
            "goal" => Some(Self::Goal),
            "continuity" => Some(Self::Continuity),
            _ => None,
        }
    }

    fn from_column(value: &str) -> rusqlite::Result<Self> {
        Self::parse(value).ok_or_else(|| invalid_column("kind", value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Resolved,
    Superseded,
}

impl MemoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Resolved => "resolved",
            Self::Superseded => "superseded",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "resolved" => Ok(Self::Resolved),
            "superseded" => Ok(Self::Superseded),
            _ => Err(invalid_column("status", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPriority {
    #[default]
    Normal,
    Pinned,
}

impl MemoryPriority {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Pinned => "pinned",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "normal" => Ok(Self::Normal),
            "pinned" => Ok(Self::Pinned),
            _ => Err(invalid_column("priority", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOrigin {
    #[default]
    UserExplicit,
    AssistantSuggested,
}

impl MemoryOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::UserExplicit => "user_explicit",
            Self::AssistantSuggested => "assistant_suggested",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "user_explicit" => Ok(Self::UserExplicit),
            "assistant_suggested" => Ok(Self::AssistantSuggested),
            _ => Err(invalid_column("origin", value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySourceRef {
    pub source_type: String,
    pub source_id: String,
    pub source_revision: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub scope_type: MemoryScopeType,
    pub scope_id: Option<String>,
    pub kind: MemoryKind,
    pub content: String,
    pub topic_key: Option<String>,
    pub status: MemoryStatus,
    pub priority: MemoryPriority,
    pub origin: MemoryOrigin,
    pub source_refs: Vec<MemorySourceRef>,
    pub provenance: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_confirmed_at: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub revision: String,
    pub supersedes: Option<String>,
    pub expired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictAction {
    #[default]
    Reject,
    Replace,
    KeepBoth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryItem {
    #[serde(default)]
    pub scope_type: MemoryScopeType,
    pub scope_id: Option<String>,
    pub kind: MemoryKind,
    pub content: String,
    pub topic_key: Option<String>,
    #[serde(default)]
    pub priority: MemoryPriority,
    #[serde(default)]
    pub origin: MemoryOrigin,
    #[serde(default)]
    pub source_refs: Vec<MemorySourceRef>,
    #[serde(default)]
    pub provenance: serde_json::Value,
    pub valid_until: Option<DateTime<Utc>>,
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub conflict_action: ConflictAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryItem {
    pub revision: String,
    pub content: Option<String>,
    pub kind: Option<MemoryKind>,
    pub topic_key: Option<String>,
    #[serde(default)]
    pub clear_topic_key: bool,
    pub priority: Option<MemoryPriority>,
    pub status: Option<MemoryStatus>,
    pub valid_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub clear_valid_until: bool,
    #[serde(default)]
    pub reconfirm: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryListFilter {
    pub kind: Option<MemoryKind>,
    pub status: Option<MemoryStatus>,
    pub include_expired: bool,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MemorySettings {
    pub enabled: bool,
    pub history_recall_enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    #[error("memory database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid memory item: {0}")]
    Validation(String),
    #[error("memory item was not found")]
    NotFound,
    #[error("memory item revision is stale")]
    Stale { latest: Box<MemoryItem> },
    #[error("memory item conflicts with an active item")]
    Conflict { existing: Box<MemoryItem> },
}

#[derive(Clone)]
pub struct MemoryStore {
    path: PathBuf,
}

impl MemoryStore {
    pub fn new_with_path(path: impl Into<PathBuf>) -> Result<Self, MemoryStoreError> {
        let store = Self { path: path.into() };
        if let Some(parent) = store.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MemoryStoreError::Validation(format!("cannot create memory directory: {error}"))
            })?;
        }
        store.initialize()?;
        Ok(store)
    }

    fn open(&self) -> Result<Connection, MemoryStoreError> {
        let connection = Connection::open(&self.path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(connection)
    }

    fn initialize(&self) -> Result<(), MemoryStoreError> {
        let connection = self.open()?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memory_items (
                id TEXT PRIMARY KEY,
                scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'workspace')),
                scope_id TEXT,
                kind TEXT NOT NULL CHECK(kind IN ('fact', 'preference', 'goal', 'continuity')),
                content TEXT NOT NULL,
                topic_key TEXT,
                status TEXT NOT NULL CHECK(status IN ('active', 'resolved', 'superseded')),
                priority TEXT NOT NULL CHECK(priority IN ('normal', 'pinned')),
                origin TEXT NOT NULL CHECK(origin IN ('user_explicit', 'assistant_suggested')),
                provenance TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_confirmed_at TEXT NOT NULL,
                valid_until TEXT,
                resolved_at TEXT,
                revision TEXT NOT NULL,
                CHECK((scope_type = 'global' AND scope_id IS NULL) OR
                      (scope_type = 'workspace' AND scope_id IS NOT NULL))
            );
            CREATE INDEX IF NOT EXISTS memory_items_recall
                ON memory_items(scope_type, scope_id, status, priority, updated_at);
            CREATE INDEX IF NOT EXISTS memory_items_topic
                ON memory_items(scope_type, scope_id, topic_key, status);

            CREATE TABLE IF NOT EXISTS memory_sources (
                memory_id TEXT NOT NULL REFERENCES memory_items(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                source_type TEXT NOT NULL,
                source_id TEXT NOT NULL,
                source_revision TEXT,
                metadata TEXT NOT NULL,
                PRIMARY KEY(memory_id, position)
            );

            CREATE TABLE IF NOT EXISTS memory_relations (
                from_id TEXT NOT NULL REFERENCES memory_items(id) ON DELETE CASCADE,
                relation_type TEXT NOT NULL CHECK(relation_type IN ('supersedes')),
                to_id TEXT NOT NULL REFERENCES memory_items(id) ON DELETE CASCADE,
                PRIMARY KEY(from_id, relation_type, to_id)
            );

            CREATE TABLE IF NOT EXISTS memory_history (
                memory_id TEXT NOT NULL REFERENCES memory_items(id) ON DELETE CASCADE,
                revision TEXT NOT NULL,
                operation TEXT NOT NULL,
                prior_value TEXT NOT NULL,
                changed_at TEXT NOT NULL,
                origin TEXT NOT NULL,
                PRIMARY KEY(memory_id, revision)
            );

            CREATE TABLE IF NOT EXISTS memory_idempotency (
                policy_scope TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                result_id TEXT NOT NULL,
                result_revision TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY(policy_scope, idempotency_key)
            );

            CREATE TABLE IF NOT EXISTS memory_tombstones (
                id TEXT PRIMARY KEY,
                deleted_at TEXT NOT NULL,
                content_hash TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memory_settings (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                enabled INTEGER NOT NULL,
                history_recall_enabled INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO memory_settings(singleton, enabled, history_recall_enabled)
                VALUES(1, 0, 0);

            CREATE TABLE IF NOT EXISTS memory_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memory_items_fts USING fts5(
                memory_id UNINDEXED,
                content,
                topic_key,
                tokenize = 'unicode61'
            );
            "#,
        )?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        if connection
            .query_row(
                "SELECT value FROM memory_meta WHERE key = 'policy_secret'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .is_none()
        {
            let secret = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
            connection.execute(
                "INSERT INTO memory_meta(key, value) VALUES('policy_secret', ?1)",
                [secret],
            )?;
        }
        Ok(())
    }

    pub fn policy_secret(&self) -> Result<Vec<u8>, MemoryStoreError> {
        let connection = self.open()?;
        let value: String = connection.query_row(
            "SELECT value FROM memory_meta WHERE key = 'policy_secret'",
            [],
            |row| row.get(0),
        )?;
        Ok(value.into_bytes())
    }

    pub fn settings(&self) -> Result<MemorySettings, MemoryStoreError> {
        let connection = self.open()?;
        connection
            .query_row(
                "SELECT enabled, history_recall_enabled FROM memory_settings WHERE singleton = 1",
                [],
                |row| {
                    Ok(MemorySettings {
                        enabled: row.get::<_, i64>(0)? != 0,
                        history_recall_enabled: row.get::<_, i64>(1)? != 0,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn update_settings(
        &self,
        enabled: bool,
        history_recall_enabled: bool,
    ) -> Result<MemorySettings, MemoryStoreError> {
        if history_recall_enabled {
            return Err(MemoryStoreError::Validation(
                "history recall is unavailable until the runtime session recall boundary exists"
                    .into(),
            ));
        }
        let connection = self.open()?;
        connection.execute(
            "UPDATE memory_settings SET enabled = ?1, history_recall_enabled = ?2 WHERE singleton = 1",
            params![enabled as i64, history_recall_enabled as i64],
        )?;
        self.settings()
    }

    pub fn list(&self, filter: &MemoryListFilter) -> Result<Vec<MemoryItem>, MemoryStoreError> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, scope_type, scope_id, kind, content, topic_key, status, priority, origin,
                    provenance, created_at, updated_at, last_confirmed_at, valid_until,
                    resolved_at, revision
             FROM memory_items
             ORDER BY CASE priority WHEN 'pinned' THEN 0 ELSE 1 END, updated_at DESC, id ASC",
        )?;
        let mut items = statement
            .query_map([], row_to_item)?
            .collect::<Result<Vec<_>, _>>()?;
        for item in &mut items {
            hydrate_item(&connection, item)?;
        }
        let query = filter
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);
        items.retain(|item| {
            filter.kind.is_none_or(|kind| item.kind == kind)
                && filter.status.is_none_or(|status| item.status == status)
                && (filter.include_expired || !item.expired)
                && query.as_ref().is_none_or(|query| {
                    item.content.to_lowercase().contains(query)
                        || item
                            .topic_key
                            .as_deref()
                            .is_some_and(|topic| topic.to_lowercase().contains(query))
                })
        });
        Ok(items)
    }

    pub fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>, MemoryStoreError> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let terms = query.split_whitespace().collect::<Vec<_>>();
        let connection = self.open()?;
        let fts_query = terms
            .iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut statement = connection.prepare(
            "SELECT memory_id FROM memory_items_fts
             WHERE memory_items_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let ids = statement
            .query_map(params![fts_query, limit.min(50) as i64], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            let item = get_item(&connection, &id)?;
            if item.status == MemoryStatus::Active && !item.expired {
                items.push(item);
            }
        }
        if items.is_empty() {
            items = self.list(&MemoryListFilter {
                status: Some(MemoryStatus::Active),
                include_expired: false,
                ..Default::default()
            })?;
        }
        items.retain(|item| {
            let haystack = format!(
                "{} {}",
                item.topic_key.as_deref().unwrap_or_default(),
                item.content
            )
            .to_lowercase();
            haystack.contains(&query) || terms.iter().any(|term| haystack.contains(term))
        });
        items.sort_by(|left, right| {
            right
                .priority
                .eq(&MemoryPriority::Pinned)
                .cmp(&left.priority.eq(&MemoryPriority::Pinned))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        items.truncate(limit.min(50));
        Ok(items)
    }

    pub fn get(&self, id: &str) -> Result<MemoryItem, MemoryStoreError> {
        let connection = self.open()?;
        get_item(&connection, id)
    }

    pub fn create(&self, mut input: CreateMemoryItem) -> Result<MemoryItem, MemoryStoreError> {
        validate_create(&mut input)?;
        let mut connection = self.open()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = input.idempotency_key.as_deref().map(str::trim).filter(|v| !v.is_empty())
            && let Some(id) = transaction
                .query_row(
                    "SELECT result_id FROM memory_idempotency WHERE policy_scope = 'global' AND idempotency_key = ?1",
                    [key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
        {
            let item = get_item(&transaction, &id)?;
            transaction.commit()?;
            return Ok(item);
        }

        if let Some(equivalent) = find_equivalent(&transaction, &input)? {
            let updated = reconfirm_equivalent(&transaction, equivalent, &input)?;
            record_idempotency(&transaction, input.idempotency_key.as_deref(), &updated)?;
            transaction.commit()?;
            return Ok(updated);
        }

        let conflict = find_topic_conflict(&transaction, &input)?;
        if let Some(existing) = conflict.as_ref()
            && input.conflict_action == ConflictAction::Reject
        {
            return Err(MemoryStoreError::Conflict {
                existing: Box::new(existing.clone()),
            });
        }
        if conflict.is_some() && input.conflict_action == ConflictAction::KeepBoth {
            input.topic_key = None;
        }

        let now = Utc::now();
        let id = uuid::Uuid::new_v4().to_string();
        let revision = uuid::Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO memory_items(
                id, scope_type, scope_id, kind, content, topic_key, status, priority, origin,
                provenance, created_at, updated_at, last_confirmed_at, valid_until,
                resolved_at, revision
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8, ?9, ?10, ?10, ?10, ?11, NULL, ?12)",
            params![
                id,
                input.scope_type.as_str(),
                input.scope_id,
                input.kind.as_str(),
                input.content,
                input.topic_key,
                input.priority.as_str(),
                input.origin.as_str(),
                serde_json::to_string(&input.provenance).map_err(json_error)?,
                format_time(now),
                input.valid_until.map(format_time),
                revision,
            ],
        )?;
        replace_sources(&transaction, &id, &input.source_refs)?;
        index_item(
            &transaction,
            &id,
            &input.content,
            input.topic_key.as_deref(),
        )?;

        if let Some(existing) = conflict
            && input.conflict_action == ConflictAction::Replace
        {
            record_history(&transaction, &existing, "supersede", input.origin)?;
            let superseded_revision = uuid::Uuid::new_v4().to_string();
            transaction.execute(
                "UPDATE memory_items SET status = 'superseded', updated_at = ?1, revision = ?2 WHERE id = ?3",
                params![format_time(now), superseded_revision, existing.id],
            )?;
            transaction.execute(
                "INSERT INTO memory_relations(from_id, relation_type, to_id) VALUES(?1, 'supersedes', ?2)",
                params![id, existing.id],
            )?;
        }

        let item = get_item(&transaction, &id)?;
        record_idempotency(&transaction, input.idempotency_key.as_deref(), &item)?;
        transaction.commit()?;
        Ok(item)
    }

    pub fn update(
        &self,
        id: &str,
        patch: UpdateMemoryItem,
        origin: MemoryOrigin,
    ) -> Result<MemoryItem, MemoryStoreError> {
        let mut connection = self.open()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get_item(&transaction, id)?;
        if current.revision != patch.revision {
            return Err(MemoryStoreError::Stale {
                latest: Box::new(current),
            });
        }
        if current.status == MemoryStatus::Superseded {
            return Err(MemoryStoreError::Validation(
                "superseded memory cannot be edited".into(),
            ));
        }
        let content = patch.content.unwrap_or_else(|| current.content.clone());
        let kind = patch.kind.unwrap_or(current.kind);
        let topic_key = if patch.clear_topic_key {
            None
        } else {
            patch.topic_key.or_else(|| current.topic_key.clone())
        };
        let priority = patch.priority.unwrap_or(current.priority);
        let status = patch.status.unwrap_or(current.status);
        let valid_until = if patch.clear_valid_until {
            None
        } else {
            patch.valid_until.or(current.valid_until)
        };
        validate_values(
            current.scope_type,
            current.scope_id.as_deref(),
            kind,
            &content,
            topic_key.as_deref(),
            valid_until,
        )?;
        if status == MemoryStatus::Resolved
            && !matches!(kind, MemoryKind::Goal | MemoryKind::Continuity)
        {
            return Err(MemoryStoreError::Validation(
                "only goal or continuity memory can be resolved".into(),
            ));
        }
        if status == MemoryStatus::Superseded {
            return Err(MemoryStoreError::Validation(
                "superseded status is created only by conflict replacement".into(),
            ));
        }
        if status == MemoryStatus::Active
            && let Some(topic) = topic_key.as_deref()
            && let Some(existing) = find_active_topic(
                &transaction,
                current.scope_type,
                current.scope_id.as_deref(),
                topic,
                Some(id),
            )?
        {
            return Err(MemoryStoreError::Conflict {
                existing: Box::new(existing),
            });
        }

        record_history(&transaction, &current, "update", origin)?;
        let now = Utc::now();
        let revision = uuid::Uuid::new_v4().to_string();
        let resolved_at = if status == MemoryStatus::Resolved {
            current.resolved_at.or(Some(now))
        } else {
            None
        };
        let last_confirmed_at = if patch.reconfirm || content != current.content {
            now
        } else {
            current.last_confirmed_at
        };
        transaction.execute(
            "UPDATE memory_items SET kind = ?1, content = ?2, topic_key = ?3, status = ?4,
                priority = ?5, updated_at = ?6, last_confirmed_at = ?7, valid_until = ?8,
                resolved_at = ?9, revision = ?10 WHERE id = ?11",
            params![
                kind.as_str(),
                content,
                topic_key,
                status.as_str(),
                priority.as_str(),
                format_time(now),
                format_time(last_confirmed_at),
                valid_until.map(format_time),
                resolved_at.map(format_time),
                revision,
                id,
            ],
        )?;
        index_item(&transaction, id, &content, topic_key.as_deref())?;
        let item = get_item(&transaction, id)?;
        transaction.commit()?;
        Ok(item)
    }

    pub fn forget(&self, id: &str, revision: &str) -> Result<(), MemoryStoreError> {
        let mut connection = self.open()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get_item(&transaction, id)?;
        if current.revision != revision {
            return Err(MemoryStoreError::Stale {
                latest: Box::new(current),
            });
        }
        let content_hash = format!("{:x}", Sha256::digest(current.content.as_bytes()));
        transaction.execute("DELETE FROM memory_items_fts WHERE memory_id = ?1", [id])?;
        transaction.execute("DELETE FROM memory_history WHERE memory_id = ?1", [id])?;
        transaction.execute("DELETE FROM memory_items WHERE id = ?1", [id])?;
        transaction.execute(
            "INSERT INTO memory_tombstones(id, deleted_at, content_hash)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET deleted_at = excluded.deleted_at, content_hash = excluded.content_hash",
            params![id, format_time(Utc::now()), content_hash],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn validate_create(input: &mut CreateMemoryItem) -> Result<(), MemoryStoreError> {
    input.content = input.content.trim().replace("\r\n", "\n");
    input.topic_key = normalize_optional(input.topic_key.take());
    input.scope_id = normalize_optional(input.scope_id.take());
    validate_values(
        input.scope_type,
        input.scope_id.as_deref(),
        input.kind,
        &input.content,
        input.topic_key.as_deref(),
        input.valid_until,
    )
}

fn validate_values(
    scope_type: MemoryScopeType,
    scope_id: Option<&str>,
    _kind: MemoryKind,
    content: &str,
    topic_key: Option<&str>,
    valid_until: Option<DateTime<Utc>>,
) -> Result<(), MemoryStoreError> {
    if scope_type != MemoryScopeType::Global || scope_id.is_some() {
        return Err(MemoryStoreError::Validation(
            "workspace scope is reserved until a logical workspace product decision is implemented"
                .into(),
        ));
    }
    let chars = content.chars().count();
    if chars == 0 || chars > MAX_CONTENT_CHARS {
        return Err(MemoryStoreError::Validation(format!(
            "content must contain 1 to {MAX_CONTENT_CHARS} characters"
        )));
    }
    if let Some(topic) = topic_key
        && (topic.chars().count() > MAX_TOPIC_KEY_CHARS
            || !topic
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || "._-".contains(ch)))
    {
        return Err(MemoryStoreError::Validation(
            "topic_key must contain only lowercase ASCII letters, numbers, dot, underscore, or hyphen"
                .into(),
        ));
    }
    if valid_until.is_some_and(|value| value <= Utc::now()) {
        return Err(MemoryStoreError::Validation(
            "valid_until must be in the future".into(),
        ));
    }
    Ok(())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn find_equivalent(
    transaction: &Transaction<'_>,
    input: &CreateMemoryItem,
) -> Result<Option<MemoryItem>, MemoryStoreError> {
    let mut statement = transaction.prepare(
        "SELECT id FROM memory_items
         WHERE scope_type = ?1 AND scope_id IS ?2 AND kind = ?3 AND status = 'active'",
    )?;
    let ids = statement
        .query_map(
            params![
                input.scope_type.as_str(),
                input.scope_id,
                input.kind.as_str()
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let expected = normalized_content(&input.content);
    for id in ids {
        let item = get_item(transaction, &id)?;
        if normalized_content(&item.content) == expected {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

fn normalized_content(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn find_topic_conflict(
    transaction: &Transaction<'_>,
    input: &CreateMemoryItem,
) -> Result<Option<MemoryItem>, MemoryStoreError> {
    let Some(topic) = input.topic_key.as_deref() else {
        return Ok(None);
    };
    find_active_topic(
        transaction,
        input.scope_type,
        input.scope_id.as_deref(),
        topic,
        None,
    )
}

fn find_active_topic(
    connection: &Connection,
    scope_type: MemoryScopeType,
    scope_id: Option<&str>,
    topic: &str,
    exclude_id: Option<&str>,
) -> Result<Option<MemoryItem>, MemoryStoreError> {
    let id = connection
        .query_row(
            "SELECT id FROM memory_items
             WHERE scope_type = ?1 AND scope_id IS ?2 AND topic_key = ?3 AND status = 'active'
               AND (?4 IS NULL OR id != ?4)
             LIMIT 1",
            params![scope_type.as_str(), scope_id, topic, exclude_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    id.map(|id| get_item(connection, &id)).transpose()
}

fn reconfirm_equivalent(
    transaction: &Transaction<'_>,
    current: MemoryItem,
    input: &CreateMemoryItem,
) -> Result<MemoryItem, MemoryStoreError> {
    record_history(transaction, &current, "reconfirm", input.origin)?;
    let now = Utc::now();
    let revision = uuid::Uuid::new_v4().to_string();
    transaction.execute(
        "UPDATE memory_items SET updated_at = ?1, last_confirmed_at = ?1,
            valid_until = COALESCE(?2, valid_until), revision = ?3 WHERE id = ?4",
        params![
            format_time(now),
            input.valid_until.map(format_time),
            revision,
            current.id,
        ],
    )?;
    if !input.source_refs.is_empty() {
        replace_sources(transaction, &current.id, &input.source_refs)?;
    }
    get_item(transaction, &current.id)
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryItem> {
    let scope_type = MemoryScopeType::parse(&row.get::<_, String>(1)?)?;
    let kind = MemoryKind::from_column(&row.get::<_, String>(3)?)?;
    let status = MemoryStatus::parse(&row.get::<_, String>(6)?)?;
    let priority = MemoryPriority::parse(&row.get::<_, String>(7)?)?;
    let origin = MemoryOrigin::parse(&row.get::<_, String>(8)?)?;
    let provenance_text = row.get::<_, String>(9)?;
    let provenance = serde_json::from_str(&provenance_text)
        .map_err(|error| invalid_json_column("provenance", error))?;
    let valid_until = parse_optional_time(row.get::<_, Option<String>>(13)?, "valid_until")?;
    Ok(MemoryItem {
        id: row.get(0)?,
        scope_type,
        scope_id: row.get(2)?,
        kind,
        content: row.get(4)?,
        topic_key: row.get(5)?,
        status,
        priority,
        origin,
        source_refs: Vec::new(),
        provenance,
        created_at: parse_time(&row.get::<_, String>(10)?, "created_at")?,
        updated_at: parse_time(&row.get::<_, String>(11)?, "updated_at")?,
        last_confirmed_at: parse_time(&row.get::<_, String>(12)?, "last_confirmed_at")?,
        valid_until,
        resolved_at: parse_optional_time(row.get::<_, Option<String>>(14)?, "resolved_at")?,
        revision: row.get(15)?,
        supersedes: None,
        expired: valid_until.is_some_and(|value| value <= Utc::now()),
    })
}

fn get_item(connection: &Connection, id: &str) -> Result<MemoryItem, MemoryStoreError> {
    let mut item = connection
        .query_row(
            "SELECT id, scope_type, scope_id, kind, content, topic_key, status, priority, origin,
                    provenance, created_at, updated_at, last_confirmed_at, valid_until,
                    resolved_at, revision
             FROM memory_items WHERE id = ?1",
            [id],
            row_to_item,
        )
        .optional()?
        .ok_or(MemoryStoreError::NotFound)?;
    hydrate_item(connection, &mut item)?;
    Ok(item)
}

fn hydrate_item(connection: &Connection, item: &mut MemoryItem) -> Result<(), MemoryStoreError> {
    let mut statement = connection.prepare(
        "SELECT source_type, source_id, source_revision, metadata
         FROM memory_sources WHERE memory_id = ?1 ORDER BY position",
    )?;
    item.source_refs = statement
        .query_map([&item.id], |row| {
            let metadata = serde_json::from_str(&row.get::<_, String>(3)?)
                .map_err(|error| invalid_json_column("metadata", error))?;
            Ok(MemorySourceRef {
                source_type: row.get(0)?,
                source_id: row.get(1)?,
                source_revision: row.get(2)?,
                metadata,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    item.supersedes = connection
        .query_row(
            "SELECT to_id FROM memory_relations
             WHERE from_id = ?1 AND relation_type = 'supersedes' LIMIT 1",
            [&item.id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(())
}

fn replace_sources(
    transaction: &Transaction<'_>,
    id: &str,
    sources: &[MemorySourceRef],
) -> Result<(), MemoryStoreError> {
    transaction.execute("DELETE FROM memory_sources WHERE memory_id = ?1", [id])?;
    for (position, source) in sources.iter().enumerate() {
        if source.source_type.trim().is_empty() || source.source_id.trim().is_empty() {
            return Err(MemoryStoreError::Validation(
                "memory source type and ID must not be empty".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO memory_sources(memory_id, position, source_type, source_id, source_revision, metadata)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                position as i64,
                source.source_type.trim(),
                source.source_id.trim(),
                source.source_revision,
                serde_json::to_string(&source.metadata).map_err(json_error)?,
            ],
        )?;
    }
    Ok(())
}

fn index_item(
    transaction: &Transaction<'_>,
    id: &str,
    content: &str,
    topic_key: Option<&str>,
) -> Result<(), MemoryStoreError> {
    transaction.execute("DELETE FROM memory_items_fts WHERE memory_id = ?1", [id])?;
    transaction.execute(
        "INSERT INTO memory_items_fts(memory_id, content, topic_key) VALUES(?1, ?2, ?3)",
        params![id, content, topic_key.unwrap_or_default()],
    )?;
    Ok(())
}

fn record_history(
    transaction: &Transaction<'_>,
    item: &MemoryItem,
    operation: &str,
    origin: MemoryOrigin,
) -> Result<(), MemoryStoreError> {
    transaction.execute(
        "INSERT INTO memory_history(memory_id, revision, operation, prior_value, changed_at, origin)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            item.id,
            item.revision,
            operation,
            serde_json::to_string(item).map_err(json_error)?,
            format_time(Utc::now()),
            origin.as_str(),
        ],
    )?;
    Ok(())
}

fn record_idempotency(
    transaction: &Transaction<'_>,
    key: Option<&str>,
    item: &MemoryItem,
) -> Result<(), MemoryStoreError> {
    let Some(key) = key.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    transaction.execute(
        "INSERT OR REPLACE INTO memory_idempotency(
            policy_scope, idempotency_key, result_id, result_revision, created_at
         ) VALUES('global', ?1, ?2, ?3, ?4)",
        params![key, item.id, item.revision, format_time(Utc::now())],
    )?;
    Ok(())
}

fn format_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_time(value: &str, column: &'static str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid {column}: {error}"),
                )),
            )
        })
}

fn parse_optional_time(
    value: Option<String>,
    column: &'static str,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value.map(|value| parse_time(&value, column)).transpose()
}

fn invalid_column(column: &'static str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid {column}: {value}"),
        )),
    )
}

fn invalid_json_column(column: &'static str, error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid {column}: {error}"),
        )),
    )
}

fn json_error(error: serde_json::Error) -> MemoryStoreError {
    MemoryStoreError::Validation(format!("memory metadata is not valid JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, MemoryStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = MemoryStore::new_with_path(directory.path().join("memory.sqlite3")).unwrap();
        (directory, store)
    }

    fn input(content: &str, topic_key: Option<&str>) -> CreateMemoryItem {
        CreateMemoryItem {
            scope_type: MemoryScopeType::Global,
            scope_id: None,
            kind: MemoryKind::Preference,
            content: content.into(),
            topic_key: topic_key.map(str::to_string),
            priority: MemoryPriority::Normal,
            origin: MemoryOrigin::UserExplicit,
            source_refs: vec![],
            provenance: serde_json::json!({ "origin": "memory_ui" }),
            valid_until: None,
            idempotency_key: None,
            conflict_action: ConflictAction::Reject,
        }
    }

    #[test]
    fn creates_lists_and_reopens_sqlite_memory() {
        let (directory, store) = store();
        let created = store
            .create(input("请使用简洁的中文回答。", Some("response_language")))
            .unwrap();
        assert_eq!(created.kind, MemoryKind::Preference);
        assert_eq!(store.list(&Default::default()).unwrap().len(), 1);
        drop(store);

        let reopened = MemoryStore::new_with_path(directory.path().join("memory.sqlite3")).unwrap();
        assert_eq!(reopened.get(&created.id).unwrap().content, created.content);
    }

    #[test]
    fn equivalent_write_reconfirms_without_duplication() {
        let (_directory, store) = store();
        let first = store
            .create(input("Prefers concise answers.", None))
            .unwrap();
        let second = store
            .create(input("  prefers   concise answers. ", None))
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_ne!(first.revision, second.revision);
        assert_eq!(store.list(&Default::default()).unwrap().len(), 1);
    }

    #[test]
    fn conflict_requires_resolution_and_replace_is_atomic() {
        let (_directory, store) = store();
        let old = store
            .create(input("Answer in Chinese.", Some("response_language")))
            .unwrap();
        let conflict = store
            .create(input("Answer in English.", Some("response_language")))
            .unwrap_err();
        assert!(matches!(conflict, MemoryStoreError::Conflict { .. }));

        let mut replacement = input("Answer in English.", Some("response_language"));
        replacement.conflict_action = ConflictAction::Replace;
        let new = store.create(replacement).unwrap();
        assert_eq!(new.supersedes.as_deref(), Some(old.id.as_str()));
        assert_eq!(store.get(&old.id).unwrap().status, MemoryStatus::Superseded);
        let recalled = store.recall("Answer", 10).unwrap();
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].id, new.id);
    }

    #[test]
    fn stale_update_fails_and_forget_removes_recoverable_content() {
        let (_directory, store) = store();
        let item = store.create(input("Prefers diagrams.", None)).unwrap();
        let updated = store
            .update(
                &item.id,
                UpdateMemoryItem {
                    revision: item.revision.clone(),
                    content: Some("Prefers diagrams with labels.".into()),
                    kind: None,
                    topic_key: None,
                    clear_topic_key: false,
                    priority: None,
                    status: None,
                    valid_until: None,
                    clear_valid_until: false,
                    reconfirm: false,
                },
                MemoryOrigin::UserExplicit,
            )
            .unwrap();
        let stale = store.forget(&item.id, &item.revision).unwrap_err();
        assert!(matches!(stale, MemoryStoreError::Stale { .. }));
        store.forget(&updated.id, &updated.revision).unwrap();
        assert!(matches!(
            store.get(&updated.id),
            Err(MemoryStoreError::NotFound)
        ));
        assert!(store.recall("diagrams", 10).unwrap().is_empty());

        let connection = store.open().unwrap();
        let history_count: i64 = connection
            .query_row("SELECT count(*) FROM memory_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(history_count, 0);
    }

    #[test]
    fn workspace_scope_and_history_recall_fail_closed() {
        let (_directory, store) = store();
        let mut workspace = input("Project fact", None);
        workspace.scope_type = MemoryScopeType::Workspace;
        workspace.scope_id = Some("project-a".into());
        assert!(matches!(
            store.create(workspace),
            Err(MemoryStoreError::Validation(_))
        ));
        assert!(store.update_settings(true, true).is_err());
    }
}
