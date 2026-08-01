use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub id: String,
    pub name: String,
    pub status: KnowledgeBaseStatus,
    pub embedding: tutor_rag::EmbeddingConfig,
    pub documents: Vec<KnowledgeDocument>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeBaseStatus {
    Ready,
    Draft,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub index_source: Option<String>,
    pub size_bytes: usize,
    pub chunks: usize,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub content_path: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KnowledgeBaseView {
    pub id: String,
    pub name: String,
    pub status: KnowledgeBaseStatus,
    pub embedding: EmbeddingConfigView,
    pub documents: Vec<KnowledgeDocument>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmbeddingConfigView {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub embeddings_path: Option<String>,
    pub dimensions: Option<usize>,
    pub send_dimensions: bool,
    pub api_key_configured: bool,
}

#[derive(Clone)]
pub struct KnowledgeStore {
    path: PathBuf,
    documents_root: PathBuf,
    items: Arc<Mutex<Vec<KnowledgeBase>>>,
}

impl KnowledgeStore {
    #[allow(dead_code)]
    pub fn new() -> Arc<Self> {
        let root = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".llm-tutor");
        std::fs::create_dir_all(&root).expect("failed to create .llm-tutor directory");
        Self::new_with_path(root.join("knowledge-bases.json"))
    }

    pub fn new_with_path(path: impl Into<PathBuf>) -> Arc<Self> {
        let path = path.into();
        let documents_root = path
            .parent()
            .map(|parent| parent.join("documents"))
            .unwrap_or_else(|| PathBuf::from(".llm-tutor").join("documents"));
        let items = read_items(&path).unwrap_or_default();
        Arc::new(Self {
            path,
            documents_root,
            items: Arc::new(Mutex::new(items)),
        })
    }

    pub fn list(&self) -> Vec<KnowledgeBaseView> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .map(KnowledgeBaseView::from)
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<KnowledgeBase> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.id == id)
            .cloned()
    }

    pub fn create(
        &self,
        name: impl Into<String>,
        embedding: tutor_rag::EmbeddingConfig,
    ) -> Result<KnowledgeBaseView> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(anyhow!("knowledge base name is empty"));
        }
        validate_embedding(&embedding)?;

        let now = Utc::now();
        let item = KnowledgeBase {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            status: KnowledgeBaseStatus::Draft,
            embedding,
            documents: Vec::new(),
            created_at: now,
            updated_at: now,
        };

        let mut items = self.items.lock().unwrap();
        items.push(item.clone());
        self.persist_locked(&items)?;
        Ok(KnowledgeBaseView::from(item))
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut items = self.items.lock().unwrap();
        let original_len = items.len();
        items.retain(|item| item.id != id);
        let deleted = items.len() != original_len;
        if deleted {
            self.persist_locked(&items)?;
            let _ = std::fs::remove_dir_all(self.documents_root.join(id));
        }
        Ok(deleted)
    }

    pub fn add_document(&self, id: &str, document: KnowledgeDocument) -> Result<KnowledgeBaseView> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.iter_mut().find(|item| item.id == id) else {
            return Err(anyhow!("knowledge base not found"));
        };

        item.status = KnowledgeBaseStatus::Ready;
        item.updated_at = Utc::now();
        item.documents.insert(0, document);
        let view = KnowledgeBaseView::from(item.clone());
        self.persist_locked(&items)?;
        Ok(view)
    }

    pub fn delete_document(
        &self,
        kb: &str,
        document_id: &str,
    ) -> Result<Option<KnowledgeDocument>> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.iter_mut().find(|item| item.id == kb) else {
            return Err(anyhow!("knowledge base not found"));
        };
        let Some(index) = item.documents.iter().position(|doc| doc.id == document_id) else {
            return Ok(None);
        };
        let document = item.documents.remove(index);
        item.status = if item.documents.is_empty() {
            KnowledgeBaseStatus::Draft
        } else {
            KnowledgeBaseStatus::Ready
        };
        item.updated_at = Utc::now();
        self.persist_locked(&items)?;
        self.remove_document_files(&document);
        Ok(Some(document))
    }

    pub fn update_document_chunks(
        &self,
        kb: &str,
        document_id: &str,
        chunks: usize,
    ) -> Result<KnowledgeBaseView> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.iter_mut().find(|item| item.id == kb) else {
            return Err(anyhow!("knowledge base not found"));
        };
        let Some(document) = item.documents.iter_mut().find(|doc| doc.id == document_id) else {
            return Err(anyhow!("document not found"));
        };
        document.chunks = chunks;
        item.status = KnowledgeBaseStatus::Ready;
        item.updated_at = Utc::now();
        let view = KnowledgeBaseView::from(item.clone());
        self.persist_locked(&items)?;
        Ok(view)
    }

    pub fn update_document_file_metadata(
        &self,
        kb: &str,
        document_id: &str,
        file_path: String,
        mime_type: Option<String>,
    ) -> Result<KnowledgeBaseView> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.iter_mut().find(|item| item.id == kb) else {
            return Err(anyhow!("knowledge base not found"));
        };
        let Some(document) = item.documents.iter_mut().find(|doc| doc.id == document_id) else {
            return Err(anyhow!("document not found"));
        };
        document.file_path = Some(file_path);
        document.mime_type = mime_type;
        item.updated_at = Utc::now();
        let view = KnowledgeBaseView::from(item.clone());
        self.persist_locked(&items)?;
        Ok(view)
    }

    pub fn store_document_text(&self, kb: &str, document_id: &str, text: &str) -> Result<String> {
        let relative = PathBuf::from(kb).join(format!("{document_id}.txt"));
        let path = self.documents_root.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
        Ok(relative.to_string_lossy().replace('\\', "/"))
    }

    pub fn store_document_file(
        &self,
        kb: &str,
        document_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<String> {
        let extension = PathBuf::from(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("bin")
            .to_ascii_lowercase();
        let relative = PathBuf::from(kb).join(format!("{document_id}.{extension}"));
        let path = self.documents_root.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;
        Ok(relative.to_string_lossy().replace('\\', "/"))
    }

    pub fn document_text(&self, kb: &str, document_id: &str) -> Result<Option<String>> {
        let Some(item) = self.get(kb) else {
            return Ok(None);
        };
        let Some(document) = item.documents.iter().find(|doc| doc.id == document_id) else {
            return Ok(None);
        };
        let Some(relative) = &document.content_path else {
            return Ok(None);
        };
        let path = self.documents_root.join(relative);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(path)?))
    }

    pub fn document_file(&self, kb: &str, document_id: &str) -> Result<Option<Vec<u8>>> {
        let Some(item) = self.get(kb) else {
            return Ok(None);
        };
        let Some(document) = item.documents.iter().find(|doc| doc.id == document_id) else {
            return Ok(None);
        };
        let Some(relative) = &document.file_path else {
            return Ok(None);
        };
        let path = self.documents_root.join(relative);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read(path)?))
    }

    pub fn find_duplicate_document(
        &self,
        kb: &str,
        content_sha256: &str,
        text: &str,
    ) -> Result<Option<KnowledgeDocument>> {
        let Some(item) = self.get(kb) else {
            return Err(anyhow!("knowledge base not found"));
        };
        for document in item.documents {
            if document.content_sha256.as_deref() == Some(content_sha256) {
                return Ok(Some(document));
            }
            if document.content_sha256.is_none()
                && self.document_text(kb, &document.id)?.as_deref() == Some(text)
            {
                return Ok(Some(document));
            }
        }
        Ok(None)
    }

    fn remove_document_files(&self, document: &KnowledgeDocument) {
        if let Some(relative) = &document.content_path {
            let _ = std::fs::remove_file(self.documents_root.join(relative));
        }
        if let Some(relative) = &document.file_path {
            let _ = std::fs::remove_file(self.documents_root.join(relative));
        }
    }

    fn persist_locked(&self, items: &[KnowledgeBase]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(items)?;
        std::fs::write(&self.path, text)?;
        Ok(())
    }
}

impl From<KnowledgeBase> for KnowledgeBaseView {
    fn from(value: KnowledgeBase) -> Self {
        Self {
            id: value.id,
            name: value.name,
            status: value.status,
            embedding: EmbeddingConfigView {
                provider: value.embedding.provider,
                model: value.embedding.model,
                base_url: value.embedding.base_url,
                embeddings_path: value.embedding.embeddings_path,
                dimensions: value.embedding.dimensions,
                send_dimensions: value.embedding.send_dimensions,
                api_key_configured: !value.embedding.api_key.trim().is_empty(),
            },
            documents: value.documents,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

pub fn normalize_embedding_config(
    config: tutor_rag::EmbeddingConfig,
) -> tutor_rag::EmbeddingConfig {
    tutor_rag::EmbeddingConfig {
        provider: config.provider.trim().to_string(),
        model: config.model.trim().to_string(),
        api_key: config.api_key.trim().to_string(),
        base_url: config
            .base_url
            .and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_string())),
        embeddings_path: config
            .embeddings_path
            .and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_string())),
        dimensions: config.dimensions.filter(|value| *value > 0),
        send_dimensions: config.send_dimensions,
    }
}

fn validate_embedding(config: &tutor_rag::EmbeddingConfig) -> Result<()> {
    if config.provider.trim().is_empty() {
        return Err(anyhow!("embedding provider is empty"));
    }
    if config.model.trim().is_empty() {
        return Err(anyhow!("embedding model is empty"));
    }
    if config.api_key.trim().is_empty() {
        return Err(anyhow!("embedding API key is empty"));
    }
    Ok(())
}

fn read_items(path: &PathBuf) -> Result<Vec<KnowledgeBase>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_persists_embedding_metadata() {
        let path = std::env::temp_dir().join(format!("llm-tutor-kb-{}.json", uuid::Uuid::new_v4()));
        let store = KnowledgeStore::new_with_path(&path);
        let item = store
            .create(
                "Calculus",
                tutor_rag::EmbeddingConfig {
                    provider: "openai".into(),
                    model: "text-embedding-3-small".into(),
                    api_key: "sk-test".into(),
                    base_url: Some("https://api.openai.com".into()),
                    embeddings_path: Some("/v1/embeddings".into()),
                    dimensions: Some(1536),
                    send_dimensions: false,
                },
            )
            .unwrap();

        assert_eq!(item.name, "Calculus");
        assert!(item.embedding.api_key_configured);
        assert_eq!(KnowledgeStore::new_with_path(&path).list().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn document_text_round_trips() {
        let root = std::env::temp_dir().join(format!("llm-tutor-kb-{}", uuid::Uuid::new_v4()));
        let path = root.join("knowledge-bases.json");
        let store = KnowledgeStore::new_with_path(&path);
        let content_path = store
            .store_document_text("kb-1", "doc-1", "hello knowledge")
            .unwrap();
        let document = KnowledgeDocument {
            id: "doc-1".into(),
            name: "doc.txt".into(),
            source: "doc.txt".into(),
            index_source: None,
            size_bytes: 15,
            chunks: 1,
            mime_type: Some("text/plain".into()),
            content_path: Some(content_path),
            file_path: None,
            content_sha256: None,
            created_at: Utc::now(),
        };
        store
            .create(
                "Calculus",
                tutor_rag::EmbeddingConfig {
                    provider: "openai".into(),
                    model: "text-embedding-3-small".into(),
                    api_key: "sk-test".into(),
                    base_url: None,
                    embeddings_path: None,
                    dimensions: Some(1536),
                    send_dimensions: false,
                },
            )
            .unwrap();
        {
            let mut items = store.items.lock().unwrap();
            items[0].id = "kb-1".into();
            items[0].documents.push(document);
            store.persist_locked(&items).unwrap();
        }

        assert_eq!(
            store.document_text("kb-1", "doc-1").unwrap().as_deref(),
            Some("hello knowledge")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_lookup_supports_fingerprinted_and_legacy_documents() {
        let root = std::env::temp_dir().join(format!("folumi-kb-{}", uuid::Uuid::new_v4()));
        let path = root.join("knowledge-bases.json");
        let store = KnowledgeStore::new_with_path(&path);
        let item = store
            .create(
                "Library",
                tutor_rag::EmbeddingConfig {
                    provider: "openai".into(),
                    model: "embedding".into(),
                    api_key: "test".into(),
                    base_url: None,
                    embeddings_path: None,
                    dimensions: Some(8),
                    send_dimensions: false,
                },
            )
            .unwrap();
        let content_path = store
            .store_document_text(&item.id, "legacy", "same content")
            .unwrap();
        store
            .add_document(
                &item.id,
                KnowledgeDocument {
                    id: "legacy".into(),
                    name: "legacy.md".into(),
                    source: "legacy.md".into(),
                    index_source: None,
                    size_bytes: 12,
                    chunks: 1,
                    mime_type: Some("text/markdown".into()),
                    content_path: Some(content_path),
                    file_path: None,
                    content_sha256: None,
                    created_at: Utc::now(),
                },
            )
            .unwrap();

        let duplicate = store
            .find_duplicate_document(&item.id, "sha256:new", "same content")
            .unwrap()
            .expect("legacy content should be compared");
        assert_eq!(duplicate.name, "legacy.md");
        assert!(
            store
                .find_duplicate_document(&item.id, "sha256:other", "different")
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
