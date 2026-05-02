use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Key not found: {0}")]
    NotFound(String),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

pub trait Store {
    type Key: ToString + Clone + std::hash::Hash + Eq;
    type Value: Serialize + DeserializeOwned + Clone;

    fn get(&self, key: &Self::Key) -> Result<Option<Self::Value>>;
    fn set(&mut self, key: Self::Key, value: Self::Value) -> Result<()>;
    fn list(&self) -> Result<Vec<(Self::Key, Self::Value)>>;
    fn delete(&mut self, key: &Self::Key) -> Result<bool>;
    fn contains(&self, key: &Self::Key) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonFileStoreData {
    version: u32,
    entries: HashMap<String, serde_json::Value>,
}

pub struct JsonFileStore {
    path: PathBuf,
    data: JsonFileStoreData,
}

impl JsonFileStore {
    const CURRENT_VERSION: u32 = 1;

    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            Self::load(&path)
        } else {
            let store = JsonFileStore {
                path,
                data: JsonFileStoreData {
                    version: Self::CURRENT_VERSION,
                    entries: HashMap::new(),
                },
            };
            store.save()?;
            Ok(store)
        }
    }

    fn load(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let data: JsonFileStoreData = serde_json::from_str(&contents)?;
        Ok(JsonFileStore {
            path: path.to_path_buf(),
            data,
        })
    }

    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.data)?;
        let tmp_path = self.path.with_extension("tmp");
        {
            let mut tmp_file = File::create(&tmp_path)?;
            tmp_file.write_all(json.as_bytes())?;
            tmp_file.flush()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    pub fn migrate_to_v2(&mut self) -> Result<()> {
        if self.data.version == 1 {
            self.data.version = 2;
            self.save()?;
            Ok(())
        } else {
            Err(StorageError::Migration(format!(
                "Cannot migrate from version {} to 2",
                self.data.version
            )))
        }
    }
}

impl Store for JsonFileStore {
    type Key = String;
    type Value = serde_json::Value;

    fn get(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
        Ok(self.data.entries.get(key).cloned())
    }

    fn set(&mut self, key: Self::Key, value: Self::Value) -> Result<()> {
        self.data.entries.insert(key, value);
        self.save()
    }

    fn list(&self) -> Result<Vec<(Self::Key, Self::Value)>> {
        Ok(self
            .data
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn delete(&mut self, key: &Self::Key) -> Result<bool> {
        let removed = self.data.entries.remove(key);
        if removed.is_some() {
            self.save()?;
        }
        Ok(removed.is_some())
    }

    fn contains(&self, key: &Self::Key) -> bool {
        self.data.entries.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_store() -> (JsonFileStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test_store.json");
        let store = JsonFileStore::new(&path).unwrap();
        (store, dir)
    }

    #[test]
    fn test_store_new_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("new_store.json");
        let store = JsonFileStore::new(&path).unwrap();
        assert_eq!(store.data.version, 1);
        assert!(path.exists());
    }

    #[test]
    fn test_store_set_and_get() {
        let (mut store, _dir) = setup_store();
        let key = "test_key".to_string();
        let value = serde_json::json!({"name": "test", "value": 42});
        store.set(key.clone(), value.clone()).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved, Some(value));
    }

    #[test]
    fn test_store_get_nonexistent() {
        let (store, _dir) = setup_store();
        let result = store.get(&"nonexistent".to_string()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_store_list() {
        let (mut store, _dir) = setup_store();
        store.set("key1".to_string(), serde_json::json!(1)).unwrap();
        store.set("key2".to_string(), serde_json::json!(2)).unwrap();
        let items = store.list().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_store_delete() {
        let (mut store, _dir) = setup_store();
        store.set("key1".to_string(), serde_json::json!(1)).unwrap();
        assert!(store.contains(&"key1".to_string()));
        let deleted = store.delete(&"key1".to_string()).unwrap();
        assert!(deleted);
        assert!(!store.contains(&"key1".to_string()));
    }

    #[test]
    fn test_store_delete_nonexistent() {
        let (mut store, _dir) = setup_store();
        let deleted = store.delete(&"nonexistent".to_string()).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_atomic_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("atomic_test.json");
        let mut store = JsonFileStore::new(&path).unwrap();
        store.set("key".to_string(), serde_json::json!("value")).unwrap();
        assert!(!path.with_extension("tmp").exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: JsonFileStoreData = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.entries.get("key"), Some(&serde_json::json!("value")));
    }

    #[test]
    fn test_persistence_across_reloads() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("persist_test.json");
        {
            let mut store = JsonFileStore::new(&path).unwrap();
            store.set("key".to_string(), serde_json::json!({"data": [1,2,3]})).unwrap();
        }
        let store = JsonFileStore::new(&path).unwrap();
        let value = store.get(&"key".to_string()).unwrap();
        assert_eq!(value, Some(serde_json::json!({"data": [1,2,3]})));
    }
}
