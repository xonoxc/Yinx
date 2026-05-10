use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::environments::Environment;

#[derive(Debug, Error)]
pub enum EnvironmentStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Environment not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, EnvironmentStoreError>;

pub trait EnvironmentStore: Send + Sync {
    fn save(&self, env: &Environment) -> Result<()>;
    fn get(&self, id: &str) -> Result<Option<Environment>>;
    fn delete(&self, id: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<Environment>>;
    fn path(&self) -> &Path;
}

pub struct JsonEnvironmentStore {
    dir: PathBuf,
}

impl JsonEnvironmentStore {
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        Ok(JsonEnvironmentStore { dir })
    }

    fn file_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }

    fn scan_environments(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }
}

impl EnvironmentStore for JsonEnvironmentStore {
    fn save(&self, env: &Environment) -> Result<()> {
        let path = self.file_path(&env.id);
        let json = serde_json::to_string_pretty(env)?;
        let tmp_path = path.with_extension("tmp");
        {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(json.as_bytes())?;
            tmp.flush()?;
        }
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Environment>> {
        let path = self.file_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let env: Environment = serde_json::from_str(&contents)?;
        Ok(Some(env))
    }

    fn delete(&self, id: &str) -> Result<()> {
        let path = self.file_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<Environment>> {
        let files = self.scan_environments()?;
        let mut envs = Vec::new();
        for file in files {
            let mut contents = String::new();
            File::open(&file)?.read_to_string(&mut contents)?;
            if let Ok(env) = serde_json::from_str::<Environment>(&contents) {
                envs.push(env);
            }
        }
        Ok(envs)
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use yinx_core::environments::EnvironmentVariable;

    fn setup_store() -> (JsonEnvironmentStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = JsonEnvironmentStore::new(dir.path().join("environments")).unwrap();
        (store, dir)
    }

    fn make_environment(name: &str) -> Environment {
        let mut env = Environment::new(name.to_string());
        env.add_variable(EnvironmentVariable::new(
            "base_url".to_string(),
            "https://api.example.com".to_string(),
        ));
        env
    }

    #[test]
    fn test_environment_store_save_and_get() {
        let (store, _dir) = setup_store();
        let env = make_environment("Staging");
        let id = env.id.clone();
        store.save(&env).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "Staging");
        assert_eq!(loaded.variable_count(), 1);
    }

    #[test]
    fn test_environment_store_get_nonexistent() {
        let (store, _dir) = setup_store();
        let loaded = store.get("nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_environment_store_delete() {
        let (store, _dir) = setup_store();
        let env = make_environment("Staging");
        let id = env.id.clone();
        store.save(&env).unwrap();
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }

    #[test]
    fn test_environment_store_list() {
        let (store, _dir) = setup_store();
        store.save(&make_environment("Staging")).unwrap();
        store.save(&make_environment("Production")).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_environment_store_list_empty() {
        let (store, _dir) = setup_store();
        let list = store.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_environment_store_overwrite() {
        let (store, _dir) = setup_store();
        let mut env = make_environment("Staging");
        let id = env.id.clone();
        store.save(&env).unwrap();
        env.name = "Updated".to_string();
        store.save(&env).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "Updated");
    }

    #[test]
    fn test_environment_store_atomic_write() {
        let (store, _dir) = setup_store();
        let env = make_environment("Staging");
        let path = store.file_path(&env.id);
        store.save(&env).unwrap();
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn test_environment_store_delete_nonexistent() {
        let (store, _dir) = setup_store();
        store.delete("nonexistent").unwrap();
    }

    #[test]
    fn test_environment_store_persistence_across_reloads() {
        let dir = TempDir::new().unwrap();
        let env_dir = dir.path().join("environments");
        let env = make_environment("Persistent");
        let id = env.id.clone();
        {
            let store = JsonEnvironmentStore::new(&env_dir).unwrap();
            store.save(&env).unwrap();
        }
        let store = JsonEnvironmentStore::new(&env_dir).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "Persistent");
    }

    #[test]
    fn test_environment_store_serde_roundtrip() {
        let (store, _dir) = setup_store();
        let env = make_environment("Staging");
        store.save(&env).unwrap();
        let loaded = store.get(&env.id).unwrap().unwrap();
        let json1 = serde_json::to_string(&env).unwrap();
        let json2 = serde_json::to_string(&loaded).unwrap();
        assert_eq!(json1, json2);
    }
}
