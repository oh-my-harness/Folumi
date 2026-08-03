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
}
