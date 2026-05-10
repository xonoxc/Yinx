use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub request_id: Option<String>,
    pub title: String,
    pub dirty: bool,
    pub modified: chrono::DateTime<chrono::Utc>,
}

impl Tab {
    pub fn new(title: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: None,
            title,
            dirty: false,
            modified: chrono::Utc::now(),
        }
    }

    pub fn with_request(title: String, request_id: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: Some(request_id),
            title,
            dirty: false,
            modified: chrono::Utc::now(),
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.modified = chrono::Utc::now();
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
        self.modified = chrono::Utc::now();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SerializedTab {
    pub id: String,
    pub request_id: Option<String>,
    pub title: String,
    pub dirty: bool,
}

impl From<&Tab> for SerializedTab {
    fn from(tab: &Tab) -> Self {
        SerializedTab {
            id: tab.id.clone(),
            request_id: tab.request_id.clone(),
            title: tab.title.clone(),
            dirty: tab.dirty,
        }
    }
}

impl From<Tab> for SerializedTab {
    fn from(tab: Tab) -> Self {
        SerializedTab {
            id: tab.id,
            request_id: tab.request_id,
            title: tab.title,
            dirty: tab.dirty,
        }
    }
}

impl From<SerializedTab> for Tab {
    fn from(st: SerializedTab) -> Self {
        Tab {
            id: st.id,
            request_id: st.request_id,
            title: st.title,
            dirty: st.dirty,
            modified: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabManager {
    tabs: Vec<Tab>,
    active_idx: usize,
    max_tabs: usize,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new(20)
    }
}

impl TabManager {
    pub fn new(max_tabs: usize) -> Self {
        Self {
            tabs: Vec::new(),
            active_idx: 0,
            max_tabs,
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_idx)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_idx)
    }

    pub fn active_idx(&self) -> usize {
        self.active_idx
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.tabs.len() >= self.max_tabs
    }

    pub fn open_tab(&mut self, tab: Tab) -> String {
        let id = tab.id.clone();
        if self.is_full() {
            return id;
        }
        self.tabs.push(tab);
        self.active_idx = self.tabs.len() - 1;
        id
    }

    pub fn open_blank(&mut self) -> String {
        let tab = Tab::new("Untitled".to_string());
        self.open_tab(tab)
    }

    pub fn close(&mut self, id: &str) -> Option<Tab> {
        let idx = self.tabs.iter().position(|t| t.id == id)?;
        let tab = self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.active_idx = 0;
        } else if idx <= self.active_idx {
            self.active_idx = self.active_idx.saturating_sub(1);
        }
        Some(tab)
    }

    pub fn close_active(&mut self) -> Option<Tab> {
        if self.tabs.is_empty() {
            return None;
        }
        let id = self.tabs[self.active_idx].id.clone();
        self.close(&id)
    }

    pub fn close_others(&mut self, id: &str) {
        self.tabs.retain(|t| t.id == id);
        self.active_idx = 0;
    }

    pub fn mark_dirty(&mut self, id: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.mark_dirty();
        }
    }

    pub fn mark_clean(&mut self, id: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.mark_clean();
        }
    }

    pub fn navigate(&mut self, delta: i32) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as i32;
        let new_idx = (self.active_idx as i32 + delta).rem_euclid(len);
        self.active_idx = new_idx as usize;
    }

    pub fn go_to(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active_idx = idx;
        }
    }

    pub fn find_tab(&self, id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn dirty_tabs(&self) -> Vec<&Tab> {
        self.tabs.iter().filter(|t| t.dirty).collect()
    }

    pub fn has_dirty_tabs(&self) -> bool {
        self.tabs.iter().any(|t| t.dirty)
    }

    pub fn save_state(&self) -> Vec<SerializedTab> {
        self.tabs.iter().map(SerializedTab::from).collect()
    }

    pub fn restore_state(&mut self, saved: Vec<SerializedTab>) {
        self.tabs = saved.into_iter().map(Tab::from).collect();
        if !self.tabs.is_empty() {
            self.active_idx = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_new() {
        let tab = Tab::new("users".to_string());
        assert_eq!(tab.title, "users");
        assert!(!tab.dirty);
        assert!(tab.request_id.is_none());
        assert!(!tab.id.is_empty());
    }

    #[test]
    fn test_tab_with_request() {
        let tab = Tab::with_request("users".to_string(), "req-1".to_string());
        assert_eq!(tab.request_id, Some("req-1".to_string()));
    }

    #[test]
    fn test_tab_mark_dirty() {
        let mut tab = Tab::new("users".to_string());
        assert!(!tab.dirty);
        tab.mark_dirty();
        assert!(tab.dirty);
    }

    #[test]
    fn test_tab_mark_clean() {
        let mut tab = Tab::new("users".to_string());
        tab.mark_dirty();
        assert!(tab.dirty);
        tab.mark_clean();
        assert!(!tab.dirty);
    }

    #[test]
    fn test_tab_serde_roundtrip() {
        let tab = Tab::new("users".to_string());
        let json = serde_json::to_string(&tab).unwrap();
        let decoded: Tab = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.title, tab.title);
        assert_eq!(decoded.dirty, tab.dirty);
    }

    #[test]
    fn test_tab_manager_new() {
        let tm = TabManager::new(10);
        assert!(tm.is_empty());
        assert_eq!(tm.max_tabs, 10);
    }

    #[test]
    fn test_tab_manager_open_tab() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        assert_eq!(tm.len(), 1);
        assert!(tm.active_tab().is_some());
        assert_eq!(tm.active_tab().unwrap().title, "Untitled");
        assert_eq!(tm.active_tab().unwrap().id, id);
    }

    #[test]
    fn test_tab_manager_open_custom_tab() {
        let mut tm = TabManager::new(10);
        let tab = Tab::new("users".to_string());
        tm.open_tab(tab);
        assert_eq!(tm.len(), 1);
        assert_eq!(tm.active_tab().unwrap().title, "users");
    }

    #[test]
    fn test_tab_manager_max_tabs() {
        let mut tm = TabManager::new(2);
        tm.open_blank();
        tm.open_blank();
        assert!(tm.is_full());
        tm.open_blank();
        assert_eq!(tm.len(), 2);
    }

    #[test]
    fn test_tab_manager_close() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        let closed = tm.close(&id);
        assert!(closed.is_some());
        assert!(tm.is_empty());
    }

    #[test]
    fn test_tab_manager_close_active() {
        let mut tm = TabManager::new(10);
        tm.open_blank();
        tm.open_blank();
        assert_eq!(tm.active_idx(), 1);
        let closed = tm.close_active();
        assert!(closed.is_some());
    }

    #[test]
    fn test_tab_manager_close_active_empty() {
        let mut tm = TabManager::new(10);
        assert!(tm.close_active().is_none());
    }

    #[test]
    fn test_tab_manager_close_nonexistent() {
        let mut tm = TabManager::new(10);
        assert!(tm.close("nonexistent").is_none());
    }

    #[test]
    fn test_tab_manager_close_active_updates_index() {
        let mut tm = TabManager::new(10);
        tm.open_blank(); // idx 0
        tm.open_blank(); // idx 1
        tm.open_blank(); // idx 2
        let tab_id = tm.tabs()[1].id.clone();
        tm.close(&tab_id);
        assert!(tm.active_idx() < tm.len());
    }

    #[test]
    fn test_tab_manager_mark_dirty() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        assert!(!tm.active_tab().unwrap().dirty);
        tm.mark_dirty(&id);
        assert!(tm.active_tab().unwrap().dirty);
    }

    #[test]
    fn test_tab_manager_mark_clean() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        tm.mark_dirty(&id);
        assert!(tm.active_tab().unwrap().dirty);
        tm.mark_clean(&id);
        assert!(!tm.active_tab().unwrap().dirty);
    }

    #[test]
    fn test_tab_manager_navigate() {
        let mut tm = TabManager::new(10);
        tm.open_blank();
        tm.open_blank();
        tm.open_blank();
        assert_eq!(tm.active_idx(), 2);
        tm.navigate(-1);
        assert_eq!(tm.active_idx(), 1);
        tm.navigate(-1);
        assert_eq!(tm.active_idx(), 0);
        tm.navigate(-1);
        assert_eq!(tm.active_idx(), 2);
    }

    #[test]
    fn test_tab_manager_navigate_forward() {
        let mut tm = TabManager::new(10);
        tm.open_blank();
        tm.open_blank();
        tm.go_to(0);
        tm.navigate(1);
        assert_eq!(tm.active_idx(), 1);
    }

    #[test]
    fn test_tab_manager_navigate_empty() {
        let mut tm = TabManager::new(10);
        tm.navigate(1);
        assert_eq!(tm.active_idx(), 0);
    }

    #[test]
    fn test_tab_manager_go_to() {
        let mut tm = TabManager::new(10);
        tm.open_blank();
        tm.open_blank();
        tm.open_blank();
        tm.go_to(1);
        assert_eq!(tm.active_idx(), 1);
        tm.go_to(5);
        assert_eq!(tm.active_idx(), 1);
    }

    #[test]
    fn test_tab_manager_find_tab() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        assert!(tm.find_tab(&id).is_some());
        assert!(tm.find_tab("nonexistent").is_none());
    }

    #[test]
    fn test_tab_manager_close_others() {
        let mut tm = TabManager::new(10);
        let _id1 = tm.open_blank();
        let id2 = tm.open_blank();
        tm.open_blank();
        tm.close_others(&id2);
        assert_eq!(tm.len(), 1);
        assert_eq!(tm.tabs()[0].id, id2);
    }

    #[test]
    fn test_tab_manager_dirty_tabs() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        tm.open_blank();
        tm.mark_dirty(&id);
        assert_eq!(tm.dirty_tabs().len(), 1);
    }

    #[test]
    fn test_tab_manager_has_dirty_tabs() {
        let mut tm = TabManager::new(10);
        let id = tm.open_blank();
        assert!(!tm.has_dirty_tabs());
        tm.mark_dirty(&id);
        assert!(tm.has_dirty_tabs());
    }

    #[test]
    fn test_tab_manager_save_and_restore_state() {
        let mut tm = TabManager::new(10);
        tm.open_blank();
        let id = tm.open_blank();
        tm.mark_dirty(&id);
        let saved = tm.save_state();
        assert_eq!(saved.len(), 2);

        let mut restored = TabManager::new(10);
        restored.restore_state(saved);
        assert_eq!(restored.len(), 2);
        assert!(restored.find_tab(&id).is_some());
    }

    #[test]
    fn test_serialized_tab_roundtrip() {
        let tab = Tab::new("test".to_string());
        let st: SerializedTab = (&tab).into();
        let restored: Tab = st.into();
        assert_eq!(restored.title, tab.title);
        assert_eq!(restored.id, tab.id);
    }

    #[test]
    fn test_tab_manager_active_tab_mut() {
        let mut tm = TabManager::new(10);
        tm.open_blank();
        let tab = tm.active_tab_mut().unwrap();
        tab.title = "modified".to_string();
        assert_eq!(tm.active_tab().unwrap().title, "modified");
    }

    #[test]
    fn test_tab_manager_active_tab_mut_empty() {
        let mut tm = TabManager::new(10);
        assert!(tm.active_tab_mut().is_none());
    }
}
