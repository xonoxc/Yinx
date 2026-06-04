use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::workspace::Workspace;
use yinx_core::workspace_manager::WorkspaceManager;
use crate::workspace_store::WorkspaceStore;

#[derive(Debug, Error)]
pub enum WorkspaceManagerStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Workspace manager data corrupted: {0}")]
    Corrupted(String),
    #[error("Workspace store error: {0}")]
    WorkspaceStore(String),
}

pub type Result<T> = std::result::Result<T, WorkspaceManagerStoreError>;

pub struct WorkspaceManagerStore {
    base_dir: PathBuf,
}

impl WorkspaceManagerStore {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    fn workspace_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    fn workspace_file(&self, id: &str) -> PathBuf {
        self.workspace_dir(id).join("workspace.json")
    }

    pub fn load_index(&self) -> Result<WorkspaceManager> {
        let path = self.index_path();
        if !path.exists() {
            let mut manager = WorkspaceManager::new();
            let ws = Workspace::new("Default".to_string());
            manager.create("Default");
            self.save_index(&manager)?;
            self.save_workspace(&ws)?;
            return Ok(manager);
        }

        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        serde_json::from_str(&contents)
            .map_err(|e| WorkspaceManagerStoreError::Corrupted(e.to_string()))
    }

    pub fn save_index(&self, manager: &WorkspaceManager) -> Result<()> {
        let json = serde_json::to_string_pretty(manager)?;
        let tmp_path = self.index_path().with_extension("tmp");
        {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(json.as_bytes())?;
            tmp.flush()?;
        }
        fs::rename(&tmp_path, self.index_path())?;
        Ok(())
    }

    pub fn load_workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let path = self.workspace_file(id);
        if !path.exists() {
            return Ok(None);
        }
        let store = WorkspaceStore::new(&path)
            .map_err(|e| WorkspaceManagerStoreError::WorkspaceStore(e.to_string()))?;
        store.load()
            .map_err(|e| WorkspaceManagerStoreError::WorkspaceStore(e.to_string()))
    }

    pub fn save_workspace(&self, workspace: &Workspace) -> Result<()> {
        let dir = self.workspace_dir(&workspace.id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("workspace.json");
        let store = WorkspaceStore::new(&path)
            .map_err(|e| WorkspaceManagerStoreError::WorkspaceStore(e.to_string()))?;
        store.save(workspace)
            .map_err(|e| WorkspaceManagerStoreError::WorkspaceStore(e.to_string()))
    }

    pub fn delete_workspace(&self, id: &str) -> Result<()> {
        let dir = self.workspace_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn workspace_exists(&self, id: &str) -> bool {
        self.workspace_file(id).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_store() -> (WorkspaceManagerStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = WorkspaceManagerStore::new(dir.path().join("workspaces")).unwrap();
        (store, dir)
    }

    #[test]
    fn test_load_index_creates_default() {
        let (store, _dir) = setup_store();
        let manager = store.load_index().unwrap();
        assert_eq!(manager.workspaces.len(), 1);
        assert_eq!(manager.workspaces[0].name, "Default");
        assert!(manager.active_id.is_some());
    }

    #[test]
    fn test_save_and_load_index() {
        let (store, _dir) = setup_store();
        let mut manager = WorkspaceManager::new();
        manager.create("Test");
        store.save_index(&manager).unwrap();

        let loaded = store.load_index().unwrap();
        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].name, "Test");
    }

    #[test]
    fn test_save_and_load_workspace() {
        let (store, _dir) = setup_store();
        let ws = Workspace::new("My Workspace".to_string());
        let id = ws.id.clone();
        store.save_workspace(&ws).unwrap();

        let loaded = store.load_workspace(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "My Workspace");
    }

    #[test]
    fn test_delete_workspace() {
        let (store, _dir) = setup_store();
        let ws = Workspace::new("To Delete".to_string());
        let id = ws.id.clone();
        store.save_workspace(&ws).unwrap();
        assert!(store.workspace_exists(&id));

        store.delete_workspace(&id).unwrap();
        assert!(!store.workspace_exists(&id));
    }

    #[test]
    fn test_load_workspace_nonexistent() {
        let (store, _dir) = setup_store();
        let loaded = store.load_workspace("nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_index_atomic_write() {
        let (store, _dir) = setup_store();
        let mut manager = WorkspaceManager::new();
        manager.create("Test");
        store.save_index(&manager).unwrap();
        assert!(!store.index_path().with_extension("tmp").exists());
    }
}
