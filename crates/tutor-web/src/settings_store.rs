use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use serde_json::Value;

pub struct SettingsStore {
    path: PathBuf,
    value: Mutex<Value>,
}

impl SettingsStore {
    pub fn new_with_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create settings store directory");
        }
        let value = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Default::default()));
        Self {
            path,
            value: Mutex::new(value),
        }
    }

    pub fn get(&self) -> Value {
        self.value.lock().unwrap().clone()
    }

    pub fn has_llm_config(&self, id: &str) -> bool {
        self.value
            .lock()
            .unwrap()
            .get("llmConfigs")
            .and_then(Value::as_array)
            .is_some_and(|configs| {
                configs
                    .iter()
                    .any(|config| config.get("id").and_then(Value::as_str) == Some(id))
            })
    }

    pub fn active_embedding_config(&self) -> Option<tutor_rag::EmbeddingConfig> {
        let settings = self.value.lock().unwrap();
        let active_id = settings
            .get("activeEmbeddingConfigId")
            .and_then(Value::as_str)?;
        let config = settings
            .get("embeddingConfigs")
            .and_then(Value::as_array)?
            .iter()
            .find(|config| config.get("id").and_then(Value::as_str) == Some(active_id))?;
        let provider = required_string(config, "provider")?;
        let model = required_string(config, "model")?;
        Some(tutor_rag::EmbeddingConfig {
            provider,
            model,
            api_key: optional_string(config, "apiKey").unwrap_or_default(),
            base_url: optional_string(config, "baseUrl"),
            embeddings_path: optional_string(config, "embeddingsPath"),
            dimensions: config
                .get("dimensions")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0),
            send_dimensions: config
                .get("sendDimensions")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub fn replace(&self, value: Value) -> Result<Value> {
        let value = if value.is_object() {
            value
        } else {
            Value::Object(Default::default())
        };
        let mut current = self.value.lock().unwrap();
        *current = value;
        self.save_locked(&current)?;
        Ok(current.clone())
    }

    fn save_locked(&self, value: &Value) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(value)?)?;
        Ok(())
    }
}

fn required_string(value: &Value, key: &str) -> Option<String> {
    optional_string(value, key)
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_store_persists_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::new_with_path(&path);
        store
            .replace(json!({
                "llmConfigs": [{ "id": "m1", "model": "gpt" }],
                "activeLlmConfigId": "m1"
            }))
            .unwrap();

        let reloaded = SettingsStore::new_with_path(&path);
        assert_eq!(reloaded.get()["activeLlmConfigId"], "m1");
    }

    #[test]
    fn resolves_the_active_embedding_for_runtime_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new_with_path(dir.path().join("settings.json"));
        store
            .replace(json!({
                "embeddingConfigs": [{
                    "id": "e1",
                    "provider": "openai",
                    "model": "text-embedding-3-small",
                    "apiKey": "test-key",
                    "baseUrl": "https://example.test",
                    "embeddingsPath": "/v1/embeddings",
                    "dimensions": 1536,
                    "sendDimensions": true
                }],
                "activeEmbeddingConfigId": "e1"
            }))
            .unwrap();

        let config = store.active_embedding_config().unwrap();
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.dimensions, Some(1536));
        assert!(config.send_dimensions);
    }
}
