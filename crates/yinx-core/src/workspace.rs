use serde::{Deserialize, Serialize};

use crate::collections::Collection;
use crate::environments::Environment;
use crate::request::Header;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    pub restore_tabs: bool,
    pub auto_save: bool,
    pub default_headers: Vec<Header>,
    pub max_tabs: usize,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            restore_tabs: true,
            auto_save: false,
            default_headers: Vec::new(),
            max_tabs: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub collections: Vec<Collection>,
    pub environments: Vec<Environment>,
    pub active_environment: Option<String>,
    pub settings: WorkspaceSettings,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new("Default".to_string())
    }
}

impl Workspace {
    pub fn new(name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            collections: Vec::new(),
            environments: Vec::new(),
            active_environment: None,
            settings: WorkspaceSettings::default(),
        }
    }

    pub fn add_collection(&mut self, collection: Collection) {
        self.collections.push(collection);
    }

    pub fn remove_collection(&mut self, id: &str) -> Option<Collection> {
        let idx = self.collections.iter().position(|c| c.id == id)?;
        Some(self.collections.remove(idx))
    }

    pub fn get_collection(&self, id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    pub fn get_collection_mut(&mut self, id: &str) -> Option<&mut Collection> {
        self.collections.iter_mut().find(|c| c.id == id)
    }

    pub fn add_environment(&mut self, env: Environment) {
        self.environments.push(env);
    }

    pub fn remove_environment(&mut self, id: &str) -> Option<Environment> {
        let idx = self.environments.iter().position(|e| e.id == id)?;
        if self.active_environment.as_deref() == Some(id) {
            self.active_environment = None;
        }
        Some(self.environments.remove(idx))
    }

    pub fn get_environment(&self, id: &str) -> Option<&Environment> {
        self.environments.iter().find(|e| e.id == id)
    }

    pub fn get_environment_mut(&mut self, id: &str) -> Option<&mut Environment> {
        self.environments.iter_mut().find(|e| e.id == id)
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.active_environment
            .as_ref()
            .and_then(|id| self.get_environment(id))
    }

    pub fn set_active_environment(&mut self, id: Option<String>) {
        self.active_environment = id;
    }

    pub fn collection_count(&self) -> usize {
        self.collections.len()
    }

    pub fn environment_count(&self) -> usize {
        self.environments.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::{Collection, CollectionItem};
    use crate::environments::EnvironmentVariable;
    use crate::request::RequestBuilder;
    use crate::state::SavedRequest;

    fn make_request(name: &str) -> SavedRequest {
        SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            request: RequestBuilder::new()
                .url("https://example.com")
                .build()
                .unwrap(),
            tags: Vec::new(),
        }
    }

    fn make_collection(name: &str) -> Collection {
        let mut c = Collection::new(name.to_string());
        c.add_item(CollectionItem::Request(Box::new(make_request("GET /users"))));
        c
    }

    fn make_environment(name: &str) -> Environment {
        let mut env = Environment::new(name.to_string());
        env.add_variable(EnvironmentVariable::new("base_url".to_string(), "https://api.example.com".to_string()));
        env
    }

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new("My Workspace".to_string());
        assert_eq!(ws.name, "My Workspace");
        assert!(ws.collections.is_empty());
        assert!(ws.environments.is_empty());
        assert!(ws.active_environment.is_none());
    }

    #[test]
    fn test_workspace_add_collection() {
        let mut ws = Workspace::new("Test".to_string());
        ws.add_collection(make_collection("API v2"));
        assert_eq!(ws.collection_count(), 1);
    }

    #[test]
    fn test_workspace_remove_collection() {
        let mut ws = Workspace::new("Test".to_string());
        let c = make_collection("API v2");
        let id = c.id.clone();
        ws.add_collection(c);
        let removed = ws.remove_collection(&id);
        assert!(removed.is_some());
        assert_eq!(ws.collection_count(), 0);
    }

    #[test]
    fn test_workspace_remove_collection_not_found() {
        let mut ws = Workspace::new("Test".to_string());
        assert!(ws.remove_collection("nonexistent").is_none());
    }

    #[test]
    fn test_workspace_get_collection() {
        let mut ws = Workspace::new("Test".to_string());
        let c = make_collection("API v2");
        let id = c.id.clone();
        ws.add_collection(c);
        let found = ws.get_collection(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "API v2");
    }

    #[test]
    fn test_workspace_get_collection_mut() {
        let mut ws = Workspace::new("Test".to_string());
        let c = make_collection("API v2");
        let id = c.id.clone();
        ws.add_collection(c);
        let found = ws.get_collection_mut(&id);
        assert!(found.is_some());
        found.unwrap().name = "Updated".to_string();
        assert_eq!(ws.get_collection(&id).unwrap().name, "Updated");
    }

    #[test]
    fn test_workspace_add_environment() {
        let mut ws = Workspace::new("Test".to_string());
        ws.add_environment(make_environment("Staging"));
        assert_eq!(ws.environment_count(), 1);
    }

    #[test]
    fn test_workspace_remove_environment() {
        let mut ws = Workspace::new("Test".to_string());
        let env = make_environment("Staging");
        let id = env.id.clone();
        ws.add_environment(env);
        let removed = ws.remove_environment(&id);
        assert!(removed.is_some());
        assert_eq!(ws.environment_count(), 0);
    }

    #[test]
    fn test_workspace_remove_environment_clears_active() {
        let mut ws = Workspace::new("Test".to_string());
        let env = make_environment("Staging");
        let id = env.id.clone();
        ws.add_environment(env);
        ws.set_active_environment(Some(id.clone()));
        ws.remove_environment(&id);
        assert!(ws.active_environment.is_none());
    }

    #[test]
    fn test_workspace_get_environment() {
        let mut ws = Workspace::new("Test".to_string());
        let env = make_environment("Staging");
        let id = env.id.clone();
        ws.add_environment(env);
        let found = ws.get_environment(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Staging");
    }

    #[test]
    fn test_workspace_get_environment_mut() {
        let mut ws = Workspace::new("Test".to_string());
        let env = make_environment("Staging");
        let id = env.id.clone();
        ws.add_environment(env);
        let found = ws.get_environment_mut(&id);
        assert!(found.is_some());
        found.unwrap().name = "Production".to_string();
        assert_eq!(ws.get_environment(&id).unwrap().name, "Production");
    }

    #[test]
    fn test_workspace_active_environment() {
        let mut ws = Workspace::new("Test".to_string());
        assert!(ws.active_environment().is_none());
        let env = make_environment("Staging");
        let id = env.id.clone();
        ws.add_environment(env);
        ws.set_active_environment(Some(id));
        assert!(ws.active_environment().is_some());
        assert_eq!(ws.active_environment().unwrap().name, "Staging");
    }

    #[test]
    fn test_workspace_set_active_environment_none() {
        let mut ws = Workspace::new("Test".to_string());
        ws.set_active_environment(None);
        assert!(ws.active_environment().is_none());
    }

    #[test]
    fn test_workspace_settings_default() {
        let settings = WorkspaceSettings::default();
        assert!(settings.restore_tabs);
        assert!(!settings.auto_save);
        assert!(settings.default_headers.is_empty());
        assert_eq!(settings.max_tabs, 20);
    }

    #[test]
    fn test_workspace_serde_roundtrip() {
        let mut ws = Workspace::new("Test".to_string());
        ws.add_collection(make_collection("API v2"));
        ws.add_environment(make_environment("Staging"));
        let json = serde_json::to_string(&ws).unwrap();
        let decoded: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, ws.name);
        assert_eq!(decoded.collection_count(), ws.collection_count());
        assert_eq!(decoded.environment_count(), ws.environment_count());
    }

    #[test]
    fn test_workspace_active_environment_unknown_id_returns_none() {
        let ws = Workspace::new("Test".to_string());
        let ws = Workspace {
            active_environment: Some("nonexistent".to_string()),
            ..ws
        };
        assert!(ws.active_environment().is_none());
    }
}
