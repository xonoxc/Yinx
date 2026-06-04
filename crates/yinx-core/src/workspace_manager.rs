use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub collection_count: usize,
    pub environment_count: usize,
    pub last_opened: DateTime<Utc>,
}

impl WorkspaceSummary {
    pub fn from_workspace(ws: &Workspace) -> Self {
        Self {
            id: ws.id.clone(),
            name: ws.name.clone(),
            collection_count: ws.collection_count(),
            environment_count: ws.environment_count(),
            last_opened: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManager {
    pub workspaces: Vec<WorkspaceSummary>,
    pub active_id: Option<String>,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            active_id: None,
        }
    }

    pub fn create(&mut self, name: &str) -> Workspace {
        let ws = Workspace::new(name.to_string());
        let summary = WorkspaceSummary::from_workspace(&ws);
        self.workspaces.push(summary);
        self.active_id = Some(ws.id.clone());
        ws
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let pos = self.workspaces.iter().position(|w| w.id == id)
            .ok_or_else(|| format!("Workspace '{}' not found", id))?;

        if self.workspaces.len() <= 1 {
            return Err("Cannot delete the last workspace".to_string());
        }

        self.workspaces.remove(pos);

        if self.active_id.as_deref() == Some(id) {
            self.active_id = self.workspaces.first().map(|w| w.id.clone());
        }

        Ok(())
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let ws = self.workspaces.iter_mut().find(|w| w.id == id)
            .ok_or_else(|| format!("Workspace '{}' not found", id))?;
        ws.name = name.to_string();
        Ok(())
    }

    pub fn switch(&mut self, id: &str) -> Result<(), String> {
        if !self.workspaces.iter().any(|w| w.id == id) {
            return Err(format!("Workspace '{}' not found", id));
        }
        self.active_id = Some(id.to_string());
        Ok(())
    }

    pub fn list(&self) -> &[WorkspaceSummary] {
        &self.workspaces
    }

    pub fn active(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub fn active_summary(&self) -> Option<&WorkspaceSummary> {
        let id = self.active_id.as_ref()?;
        self.workspaces.iter().find(|w| w.id == *id)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&WorkspaceSummary> {
        self.workspaces.iter().find(|w| w.name == name)
    }

    pub fn update_summary(&mut self, ws: &Workspace) {
        if let Some(summary) = self.workspaces.iter_mut().find(|w| w.id == ws.id) {
            *summary = WorkspaceSummary::from_workspace(ws);
        }
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_manager_new() {
        let mgr = WorkspaceManager::new();
        assert!(mgr.workspaces.is_empty());
        assert!(mgr.active_id.is_none());
    }

    #[test]
    fn test_workspace_manager_create() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("Test Workspace");
        assert_eq!(ws.name, "Test Workspace");
        assert_eq!(mgr.workspaces.len(), 1);
        assert_eq!(mgr.active_id.as_deref(), Some(ws.id.as_str()));
    }

    #[test]
    fn test_workspace_manager_delete() {
        let mut mgr = WorkspaceManager::new();
        mgr.create("First");
        let second_id = {
            let ws = mgr.create("Second");
            ws.id.clone()
        };
        assert_eq!(mgr.workspaces.len(), 2);

        mgr.delete(&second_id).unwrap();
        assert_eq!(mgr.workspaces.len(), 1);
    }

    #[test]
    fn test_workspace_manager_delete_last_fails() {
        let mut mgr = WorkspaceManager::new();
        mgr.create("Only");
        assert!(mgr.delete("nonexistent").is_err());
    }

    #[test]
    fn test_workspace_manager_rename() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("Old Name");
        mgr.rename(&ws.id, "New Name").unwrap();
        assert_eq!(mgr.workspaces[0].name, "New Name");
    }

    #[test]
    fn test_workspace_manager_switch() {
        let mut mgr = WorkspaceManager::new();
        let ws1 = mgr.create("First");
        let id1 = ws1.id.clone();
        let ws2 = mgr.create("Second");
        let id2 = ws2.id.clone();

        mgr.switch(&id1).unwrap();
        assert_eq!(mgr.active(), Some(id1.as_str()));

        mgr.switch(&id2).unwrap();
        assert_eq!(mgr.active(), Some(id2.as_str()));
    }

    #[test]
    fn test_workspace_manager_list() {
        let mut mgr = WorkspaceManager::new();
        mgr.create("A");
        mgr.create("B");
        assert_eq!(mgr.list().len(), 2);
    }

    #[test]
    fn test_workspace_manager_find_by_name() {
        let mut mgr = WorkspaceManager::new();
        mgr.create("My Workspace");
        let found = mgr.find_by_name("My Workspace");
        assert!(found.is_some());
        assert!(mgr.find_by_name("Nonexistent").is_none());
    }

    #[test]
    fn test_workspace_manager_delete_active_switches_to_first() {
        let mut mgr = WorkspaceManager::new();
        let ws1 = mgr.create("First");
        let id1 = ws1.id.clone();
        mgr.create("Second");
        let id2 = mgr.workspaces[1].id.clone();

        mgr.switch(&id2).unwrap();
        mgr.delete(&id2).unwrap();
        assert_eq!(mgr.active(), Some(id1.as_str()));
    }
}
