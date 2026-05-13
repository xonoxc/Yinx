use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::workspace::Workspace;

#[derive(Debug, Error)]
pub enum WorkspaceStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Workspace not found: {0}")]
    NotFound(String),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub type Result<T> = std::result::Result<T, WorkspaceStoreError>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WorkspaceFile {
    version: u32,
    workspace: Workspace,
}

pub struct WorkspaceStore {
    path: PathBuf,
}

impl WorkspaceStore {
    const CURRENT_VERSION: u32 = 1;

    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(WorkspaceStore { path })
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn save(&self, workspace: &Workspace) -> Result<()> {
        let file = WorkspaceFile {
            version: Self::CURRENT_VERSION,
            workspace: workspace.clone(),
        };
        let json = serde_json::to_string_pretty(&file)?;
        let tmp_path = self.path.with_extension("tmp");
        {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(json.as_bytes())?;
            tmp.flush()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<Workspace>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let file_data: WorkspaceFile = serde_json::from_str(&contents)?;
        Ok(Some(file_data.workspace))
    }

    pub fn delete(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn migrate_to_v2(&self) -> Result<()> {
        let workspace = self.load()?.ok_or_else(|| {
            WorkspaceStoreError::NotFound("Cannot migrate: no workspace file".to_string())
        })?;
        let file = WorkspaceFile {
            version: 2,
            workspace,
        };
        let json = serde_json::to_string_pretty(&file)?;
        let tmp_path = self.path.with_extension("tmp");
        {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(json.as_bytes())?;
            tmp.flush()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use yinx_core::collections::Collection;
    use yinx_core::environments::Environment;

    fn setup_store() -> (WorkspaceStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("workspace.json");
        let store = WorkspaceStore::new(&path).unwrap();
        (store, dir)
    }

    fn create_workspace() -> Workspace {
        let mut ws = Workspace::new("Test Workspace".to_string());
        ws.add_collection(Collection::new("API v2".to_string()));
        ws.add_environment(Environment::new("Staging".to_string()));
        ws
    }

    #[test]
    fn test_workspace_store_save_and_load() {
        let (store, _dir) = setup_store();
        let ws = create_workspace();
        let id = ws.id.clone();
        store.save(&ws).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.name, "Test Workspace");
        assert_eq!(loaded.collection_count(), 1);
        assert_eq!(loaded.environment_count(), 1);
    }

    #[test]
    fn test_workspace_store_load_nonexistent() {
        let (store, _dir) = setup_store();
        let loaded = store.load().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_workspace_store_exists() {
        let (store, _dir) = setup_store();
        assert!(!store.exists());
        let ws = create_workspace();
        store.save(&ws).unwrap();
        assert!(store.exists());
    }

    #[test]
    fn test_workspace_store_delete() {
        let (store, _dir) = setup_store();
        let ws = create_workspace();
        store.save(&ws).unwrap();
        assert!(store.exists());
        store.delete().unwrap();
        assert!(!store.exists());
    }

    #[test]
    fn test_workspace_store_overwrite() {
        let (store, _dir) = setup_store();
        let ws1 = Workspace::new("First".to_string());
        store.save(&ws1).unwrap();
        let ws2 = Workspace::new("Second".to_string());
        store.save(&ws2).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.name, "Second");
    }

    #[test]
    fn test_workspace_store_atomic_write() {
        let (store, _dir) = setup_store();
        let path = store.path().to_path_buf();
        let ws = create_workspace();
        store.save(&ws).unwrap();
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn test_workspace_store_serde_roundtrip() {
        let (store, _dir) = setup_store();
        let ws = create_workspace();
        store.save(&ws).unwrap();
        let loaded = store.load().unwrap().unwrap();
        let json1 = serde_json::to_string(&ws).unwrap();
        let json2 = serde_json::to_string(&loaded).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn test_workspace_store_roundtrip_preserves_collections() {
        let (store, _dir) = setup_store();
        let mut ws = create_workspace();
        let mut c = Collection::new("Auth".to_string());
        let saved = yinx_core::state::SavedRequest {
            id: "req-1".to_string(),
            name: "Login".to_string(),
            request: yinx_core::request::RequestBuilder::new()
                .url("https://example.com/login")
                .build()
                .unwrap(),
            tags: Vec::new(),
        };
        c.add_item(yinx_core::collections::CollectionItem::Request(Box::new(
            saved,
        )));
        ws.add_collection(c);
        store.save(&ws).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.collections.len(), 2);
    }

    #[test]
    fn test_workspace_store_roundtrip_preserves_environments() {
        let (store, _dir) = setup_store();
        let mut ws = create_workspace();
        let mut env = Environment::new("Production".to_string());
        env.add_variable(yinx_core::environments::EnvironmentVariable::new(
            "key".to_string(),
            "value".to_string(),
        ));
        ws.add_environment(env);
        store.save(&ws).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.environments.len(), 2);
    }

    #[test]
    fn test_workspace_store_create_dir() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("nested").join("dir").join("workspace.json");
        let store = WorkspaceStore::new(&nested).unwrap();
        assert!(nested.parent().unwrap().exists());
        store.save(&create_workspace()).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn test_workspace_store_migrate() {
        let (store, _dir) = setup_store();
        store.save(&create_workspace()).unwrap();
        store.migrate_to_v2().unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.name, "Test Workspace");
    }
}
