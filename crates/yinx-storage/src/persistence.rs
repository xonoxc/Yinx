use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::state::{AppState, WorkflowDefinition};

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct WorkflowFile {
    version: u32,
    workflows: Vec<WorkflowDefinition>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SessionFile {
    version: u32,
    timestamp: DateTime<Utc>,
    state: AppState,
}

pub struct WorkflowStore {
    path: PathBuf,
}

impl WorkflowStore {
    const CURRENT_VERSION: u32 = 1;

    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            let file = WorkflowFile {
                version: Self::CURRENT_VERSION,
                workflows: Vec::new(),
            };
            let json = serde_json::to_string_pretty(&file)?;
            let mut f = File::create(&path)?;
            f.write_all(json.as_bytes())?;
        }
        Ok(WorkflowStore { path })
    }

    pub fn load_all(&self) -> Result<Vec<WorkflowDefinition>> {
        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let file_data: WorkflowFile = serde_json::from_str(&contents)?;
        Ok(file_data.workflows)
    }

    pub fn save_all(&mut self, workflows: &[WorkflowDefinition]) -> Result<()> {
        let file = WorkflowFile {
            version: Self::CURRENT_VERSION,
            workflows: workflows.to_vec(),
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

    pub fn save(&mut self, workflow: WorkflowDefinition) -> Result<()> {
        let mut workflows = self.load_all()?;
        if let Some(idx) = workflows.iter().position(|w| w.id == workflow.id) {
            workflows[idx] = workflow;
        } else {
            workflows.push(workflow);
        }
        self.save_all(&workflows)
    }

    pub fn delete(&mut self, workflow_id: &str) -> Result<bool> {
        let mut workflows = self.load_all()?;
        let original_len = workflows.len();
        workflows.retain(|w| w.id != workflow_id);
        if workflows.len() < original_len {
            self.save_all(&workflows)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get(&self, workflow_id: &str) -> Result<Option<WorkflowDefinition>> {
        let workflows = self.load_all()?;
        Ok(workflows.into_iter().find(|w| w.id == workflow_id))
    }

    pub fn migrate_v1_to_v2(&mut self) -> Result<()> {
        let mut workflows = self.load_all()?;
        for wf in &mut workflows {
            if wf.nodes.is_empty() && wf.edges.is_empty() {
                continue;
            }
        }
        self.save_all(&workflows)?;
        Ok(())
    }
}

pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    const CURRENT_VERSION: u32 = 1;

    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(SessionStore { path })
    }

    pub fn save(&self, state: &AppState) -> Result<()> {
        let file = SessionFile {
            version: Self::CURRENT_VERSION,
            timestamp: Utc::now(),
            state: state.clone(),
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

    pub fn load(&self) -> Result<Option<AppState>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let file_data: SessionFile = serde_json::from_str(&contents)?;
        Ok(Some(file_data.state))
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use yinx_core::state::WorkflowDefinition;

    fn create_test_workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            id: "test-workflow-1".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec!["node1".to_string(), "node2".to_string()],
            edges: vec![("node1".to_string(), "node2".to_string())],
        }
    }

    fn setup_workflow_store() -> (WorkflowStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("workflows.json");
        let store = WorkflowStore::new(&path).unwrap();
        (store, dir)
    }

    fn setup_session_store() -> (SessionStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");
        let store = SessionStore::new(&path).unwrap();
        (store, dir)
    }

    #[test]
    fn test_workflow_save_and_load() {
        let (mut store, _dir) = setup_workflow_store();
        let workflow = create_test_workflow();
        let id = workflow.id.clone();
        store.save(workflow).unwrap();
        let loaded = store.get(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, id);
    }

    #[test]
    fn test_workflow_load_all() {
        let (mut store, _dir) = setup_workflow_store();
        let mut wf1 = create_test_workflow();
        wf1.id = "workflow-1".to_string();
        let mut wf2 = create_test_workflow();
        wf2.id = "workflow-2".to_string();
        store.save(wf1).unwrap();
        store.save(wf2).unwrap();
        let workflows = store.load_all().unwrap();
        assert_eq!(workflows.len(), 2);
    }

    #[test]
    fn test_workflow_delete() {
        let (mut store, _dir) = setup_workflow_store();
        let workflow = create_test_workflow();
        let id = workflow.id.clone();
        store.save(workflow).unwrap();
        let deleted = store.delete(&id).unwrap();
        assert!(deleted);
        let loaded = store.get(&id).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_workflow_update() {
        let (mut store, _dir) = setup_workflow_store();
        let mut workflow = create_test_workflow();
        let id = workflow.id.clone();
        store.save(workflow.clone()).unwrap();
        workflow.name = "Updated Name".to_string();
        store.save(workflow).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "Updated Name");
        let all = store.load_all().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_session_save_and_load() {
        let (store, _dir) = setup_session_store();
        let state = AppState::default();
        store.save(&state).unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn test_session_clear() {
        let (store, _dir) = setup_session_store();
        let state = AppState::default();
        store.save(&state).unwrap();
        store.clear().unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_atomic_workflow_write() {
        let (store, _dir) = setup_workflow_store();
        let path = store.path.clone();
        let mut store = store;
        store.save(create_test_workflow()).unwrap();
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn test_persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let workflow_path = dir.path().join("workflows.json");
        let session_path = dir.path().join("session.json");
        let mut workflow_store = WorkflowStore::new(&workflow_path).unwrap();
        let session_store = SessionStore::new(&session_path).unwrap();
        let workflow = create_test_workflow();
        let workflow_id = workflow.id.clone();
        workflow_store.save(workflow).unwrap();
        let state = AppState::default();
        session_store.save(&state).unwrap();
        let loaded_workflow = workflow_store.get(&workflow_id).unwrap();
        assert!(loaded_workflow.is_some());
        let loaded_session = session_store.load().unwrap();
        assert!(loaded_session.is_some());
    }
}
