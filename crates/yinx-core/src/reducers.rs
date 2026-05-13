use crate::events::{AppEvent, StateDiff};
use crate::state::{InputMode, NetworkState, UiState};
use crate::tabs::TabManager;

pub trait DomainReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Default)]
pub struct UiReducer {
    pub state: UiState,
}

impl UiReducer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DomainReducer for UiReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();
        match event {
            AppEvent::PaneChanged(pane) => {
                self.state.set_pane(*pane);
                diff.ui_state_changed = true;
            }
            AppEvent::ModeChanged(mode) => {
                self.state.set_mode(*mode);
                diff.ui_state_changed = true;
            }
            AppEvent::CursorMoved { lines, cols } => {
                self.state.move_cursor(*lines, *cols);
                diff.ui_state_changed = true;
            }
            AppEvent::Scrolled(lines) => {
                self.state.scroll(*lines);
                diff.ui_state_changed = true;
            }
            AppEvent::TabSwitched { .. } | AppEvent::TabSwitchRelative(_) => {
                diff.ui_state_changed = true;
            }
            AppEvent::OpenCommandPalette => {
                self.state.set_mode(InputMode::Command);
                diff.ui_state_changed = true;
            }
            AppEvent::SearchActivated => {
                diff.ui_state_changed = true;
            }
            _ => {}
        }
        diff
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetworkReducer {
    pub state: NetworkState,
}

impl NetworkReducer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DomainReducer for NetworkReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();
        match event {
            AppEvent::SendRequest(_) | AppEvent::ExecuteRequest | AppEvent::RequestStarted => {
                self.state = NetworkState::Loading;
                diff.network_state_changed = true;
            }
            AppEvent::RequestCompleted(_) => {
                self.state = NetworkState::Idle;
                diff.network_state_changed = true;
            }
            AppEvent::RequestFailed(msg) => {
                self.state = NetworkState::Error(msg.clone());
                diff.network_state_changed = true;
            }
            AppEvent::StreamChunk(_) => {
                self.state = NetworkState::Streaming;
                diff.network_state_changed = true;
            }
            AppEvent::StreamEnded => {
                self.state = NetworkState::Idle;
                diff.network_state_changed = true;
            }
            AppEvent::NetworkStateChange(ns) => {
                self.state = ns.clone();
                diff.network_state_changed = true;
            }
            AppEvent::WorkflowStarted { .. } => {
                self.state = NetworkState::Loading;
                diff.network_state_changed = true;
            }
            AppEvent::WorkflowCompleted { .. } => {
                self.state = NetworkState::Idle;
                diff.network_state_changed = true;
            }
            AppEvent::WorkflowFailed { error, .. } => {
                self.state = NetworkState::Error(error.clone());
                diff.network_state_changed = true;
            }
            _ => {}
        }
        diff
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct CollectionsReducer;

impl CollectionsReducer {
    pub fn new() -> Self {
        Self
    }
}

impl DomainReducer for CollectionsReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();
        match event {
            AppEvent::CollectionCreated(_)
            | AppEvent::CollectionUpdated { .. }
            | AppEvent::CollectionDeleted { .. }
            | AppEvent::CollectionItemAdded { .. }
            | AppEvent::CollectionItemRemoved { .. }
            | AppEvent::CollectionItemMoved { .. } => {
                diff.app_state_changed = true;
            }
            _ => {}
        }
        diff
    }

    fn reset(&mut self) {}
}

#[derive(Debug, Clone, Default)]
pub struct EnvironmentsReducer;

impl EnvironmentsReducer {
    pub fn new() -> Self {
        Self
    }
}

impl DomainReducer for EnvironmentsReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();
        match event {
            AppEvent::EnvironmentCreated(_)
            | AppEvent::EnvironmentUpdated { .. }
            | AppEvent::EnvironmentDeleted { .. }
            | AppEvent::EnvironmentActivated { .. }
            | AppEvent::EnvironmentVariableAdded { .. }
            | AppEvent::EnvironmentVariableRemoved { .. } => {
                diff.app_state_changed = true;
            }
            _ => {}
        }
        diff
    }

    fn reset(&mut self) {}
}

#[derive(Debug, Clone)]
pub struct TabsReducer {
    pub tab_manager: TabManager,
}

impl TabsReducer {
    pub fn new(max_tabs: usize) -> Self {
        Self {
            tab_manager: TabManager::new(max_tabs),
        }
    }

    pub fn tab_manager(&self) -> &TabManager {
        &self.tab_manager
    }

    pub fn tab_manager_mut(&mut self) -> &mut TabManager {
        &mut self.tab_manager
    }
}

impl Default for TabsReducer {
    fn default() -> Self {
        Self::new(20)
    }
}

impl DomainReducer for TabsReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();
        match event {
            AppEvent::TabOpened { .. } | AppEvent::TabClosed { .. } => {
                diff.ui_state_changed = true;
                diff.app_state_changed = true;
            }
            AppEvent::TabSwitched { .. } => {
                diff.ui_state_changed = true;
            }
            AppEvent::TabDirtyChanged { .. } => {
                diff.app_state_changed = true;
            }
            _ => {}
        }
        diff
    }

    fn reset(&mut self) {
        self.tab_manager = TabManager::new(20);
    }
}

#[derive(Debug, Clone, Default)]
pub struct HistoryReducer;

impl HistoryReducer {
    pub fn new() -> Self {
        Self
    }
}

impl DomainReducer for HistoryReducer {
    fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();
        if let AppEvent::HistoryEntryAdded { .. } = event {
            diff.app_state_changed = true;
        }
        diff
    }

    fn reset(&mut self) {}
}

#[derive(Debug, Clone)]
pub struct AppReducer {
    pub ui: UiReducer,
    pub network: NetworkReducer,
    pub collections: CollectionsReducer,
    pub environments: EnvironmentsReducer,
    pub tabs: TabsReducer,
    pub history: HistoryReducer,
}

impl AppReducer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut combined = StateDiff::new();

        let ui_diff = self.ui.reduce(event);
        combined.ui_state_changed |= ui_diff.ui_state_changed;

        let network_diff = self.network.reduce(event);
        combined.network_state_changed |= network_diff.network_state_changed;

        let coll_diff = self.collections.reduce(event);
        combined.app_state_changed |= coll_diff.app_state_changed;

        let env_diff = self.environments.reduce(event);
        combined.app_state_changed |= env_diff.app_state_changed;

        let tabs_diff = self.tabs.reduce(event);
        combined.ui_state_changed |= tabs_diff.ui_state_changed;
        combined.app_state_changed |= tabs_diff.app_state_changed;

        let history_diff = self.history.reduce(event);
        combined.app_state_changed |= history_diff.app_state_changed;

        combined
    }

    pub fn reset(&mut self) {
        self.ui.reset();
        self.network.reset();
        self.collections.reset();
        self.environments.reset();
        self.tabs.reset();
        self.history.reset();
    }
}

impl Default for AppReducer {
    fn default() -> Self {
        Self {
            ui: UiReducer::new(),
            network: NetworkReducer::new(),
            collections: CollectionsReducer::new(),
            environments: EnvironmentsReducer::new(),
            tabs: TabsReducer::new(20),
            history: HistoryReducer::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::{Collection, CollectionItem};
    use crate::environments::Environment;
    use crate::events::AppEvent;
    use crate::state::ActivePane;

    #[test]
    fn test_ui_reducer_pane_change() {
        let mut reducer = UiReducer::new();
        let diff = reducer.reduce(&AppEvent::PaneChanged(ActivePane::Response));
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.state.active_pane, ActivePane::Response);
    }

    #[test]
    fn test_ui_reducer_mode_change() {
        let mut reducer = UiReducer::new();
        let diff = reducer.reduce(&AppEvent::ModeChanged(InputMode::Insert));
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.state.mode, InputMode::Insert);
    }

    #[test]
    fn test_ui_reducer_cursor_movement() {
        let mut reducer = UiReducer::new();
        let diff = reducer.reduce(&AppEvent::CursorMoved { lines: 5, cols: 3 });
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.state.cursor_line, 5);
        assert_eq!(reducer.state.cursor_col, 3);
    }

    #[test]
    fn test_ui_reducer_scroll() {
        let mut reducer = UiReducer::new();
        let diff = reducer.reduce(&AppEvent::Scrolled(10));
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.state.scroll_offset, 10);
    }

    #[test]
    fn test_ui_reducer_open_command_palette() {
        let mut reducer = UiReducer::new();
        let diff = reducer.reduce(&AppEvent::OpenCommandPalette);
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.state.mode, InputMode::Command);
    }

    #[test]
    fn test_ui_reducer_ignores_network_events() {
        let mut reducer = UiReducer::new();
        let diff = reducer.reduce(&AppEvent::RequestStarted);
        assert!(!diff.any());
    }

    #[test]
    fn test_ui_reducer_reset() {
        let mut reducer = UiReducer::new();
        reducer.reduce(&AppEvent::ModeChanged(InputMode::Insert));
        reducer.reset();
        assert_eq!(reducer.state.mode, InputMode::Normal);
    }

    #[test]
    fn test_network_reducer_send_request() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::SendRequest(
            crate::request::RequestBuilder::new()
                .url("https://example.com")
                .build()
                .unwrap(),
        ));
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_loading());
    }

    #[test]
    fn test_network_reducer_execute_request() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::ExecuteRequest);
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_loading());
    }

    #[test]
    fn test_network_reducer_request_completed() {
        let mut reducer = NetworkReducer::new();
        reducer.state = NetworkState::Loading;
        let diff = reducer.reduce(&AppEvent::RequestCompleted(
            crate::response::Response::builder()
                .status(200)
                .body(crate::response::ResponseBody::None)
                .build(),
        ));
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_idle());
    }

    #[test]
    fn test_network_reducer_request_failed() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::RequestFailed("timeout".to_string()));
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_error());
    }

    #[test]
    fn test_network_reducer_stream_chunk() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::StreamChunk(vec![1, 2, 3]));
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_streaming());
    }

    #[test]
    fn test_network_reducer_stream_ended() {
        let mut reducer = NetworkReducer::new();
        reducer.state = NetworkState::Streaming;
        let diff = reducer.reduce(&AppEvent::StreamEnded);
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_idle());
    }

    #[test]
    fn test_network_reducer_network_state_change() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::NetworkStateChange(NetworkState::Streaming));
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_streaming());
    }

    #[test]
    fn test_network_reducer_workflow_started() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::WorkflowStarted {
            id: "wf1".to_string(),
        });
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_loading());
    }

    #[test]
    fn test_network_reducer_workflow_completed() {
        let mut reducer = NetworkReducer::new();
        reducer.state = NetworkState::Loading;
        let diff = reducer.reduce(&AppEvent::WorkflowCompleted {
            id: "wf1".to_string(),
        });
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_idle());
    }

    #[test]
    fn test_network_reducer_workflow_failed() {
        let mut reducer = NetworkReducer::new();
        let diff = reducer.reduce(&AppEvent::WorkflowFailed {
            id: "wf1".to_string(),
            error: "err".to_string(),
        });
        assert!(diff.network_state_changed);
        assert!(reducer.state.is_error());
    }

    #[test]
    fn test_network_reducer_reset() {
        let mut reducer = NetworkReducer::new();
        reducer.reduce(&AppEvent::RequestStarted);
        reducer.reset();
        assert!(reducer.state.is_idle());
    }

    #[test]
    fn test_collections_reducer_created() {
        let mut reducer = CollectionsReducer::new();
        let coll = Collection::new("Test".to_string());
        let diff = reducer.reduce(&AppEvent::CollectionCreated(coll));
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_collections_reducer_updated() {
        let mut reducer = CollectionsReducer::new();
        let diff = reducer.reduce(&AppEvent::CollectionUpdated {
            id: "c1".to_string(),
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_collections_reducer_deleted() {
        let mut reducer = CollectionsReducer::new();
        let diff = reducer.reduce(&AppEvent::CollectionDeleted {
            id: "c1".to_string(),
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_collections_reducer_item_added() {
        let mut reducer = CollectionsReducer::new();
        let diff = reducer.reduce(&AppEvent::CollectionItemAdded {
            collection_id: "c1".to_string(),
            item: CollectionItem::Folder {
                name: "F".to_string(),
                children: Vec::new(),
            },
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_collections_reducer_item_removed() {
        let mut reducer = CollectionsReducer::new();
        let diff = reducer.reduce(&AppEvent::CollectionItemRemoved {
            collection_id: "c1".to_string(),
            index: 0,
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_collections_reducer_item_moved() {
        let mut reducer = CollectionsReducer::new();
        let diff = reducer.reduce(&AppEvent::CollectionItemMoved {
            collection_id: "c1".to_string(),
            from: 0,
            to: 2,
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_collections_reducer_ignores_ui_events() {
        let mut reducer = CollectionsReducer::new();
        let diff = reducer.reduce(&AppEvent::PaneChanged(ActivePane::Request));
        assert!(!diff.any());
    }

    #[test]
    fn test_environments_reducer_created() {
        let mut reducer = EnvironmentsReducer::new();
        let env = Environment::new("Staging".to_string());
        let diff = reducer.reduce(&AppEvent::EnvironmentCreated(env));
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_environments_reducer_activated() {
        let mut reducer = EnvironmentsReducer::new();
        let diff = reducer.reduce(&AppEvent::EnvironmentActivated {
            id: Some("e1".to_string()),
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_environments_reducer_deactivated() {
        let mut reducer = EnvironmentsReducer::new();
        let diff = reducer.reduce(&AppEvent::EnvironmentActivated { id: None });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_environments_reducer_ignores_network_events() {
        let mut reducer = EnvironmentsReducer::new();
        let diff = reducer.reduce(&AppEvent::RequestStarted);
        assert!(!diff.any());
    }

    #[test]
    fn test_tabs_reducer_tab_opened() {
        let mut reducer = TabsReducer::new(10);
        let id = reducer.tab_manager.open_blank();
        let diff = reducer.reduce(&AppEvent::TabOpened { id: id.clone() });
        assert!(diff.ui_state_changed);
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_tabs_reducer_tab_closed() {
        let mut reducer = TabsReducer::new(10);
        let diff = reducer.reduce(&AppEvent::TabClosed {
            id: "tab1".to_string(),
        });
        assert!(diff.ui_state_changed);
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_tabs_reducer_tab_switched() {
        let mut reducer = TabsReducer::new(10);
        let diff = reducer.reduce(&AppEvent::TabSwitched {
            from: "tab1".to_string(),
            to: "tab2".to_string(),
        });
        assert!(diff.ui_state_changed);
        assert!(!diff.app_state_changed);
    }

    #[test]
    fn test_tabs_reducer_tab_dirty() {
        let mut reducer = TabsReducer::new(10);
        let diff = reducer.reduce(&AppEvent::TabDirtyChanged {
            id: "tab1".to_string(),
            dirty: true,
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_tabs_reducer_reset() {
        let mut reducer = TabsReducer::new(10);
        reducer.tab_manager.open_blank();
        assert_eq!(reducer.tab_manager.len(), 1);
        reducer.reset();
        assert!(reducer.tab_manager.is_empty());
    }

    #[test]
    fn test_history_reducer_entry_added() {
        let mut reducer = HistoryReducer::new();
        let diff = reducer.reduce(&AppEvent::HistoryEntryAdded {
            id: "h1".to_string(),
        });
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_history_reducer_ignores_ui_events() {
        let mut reducer = HistoryReducer::new();
        let diff = reducer.reduce(&AppEvent::PaneChanged(ActivePane::Request));
        assert!(!diff.any());
    }

    #[test]
    fn test_app_reducer_combines_diffs() {
        let mut app = AppReducer::new();
        let diff = app.reduce(&AppEvent::RequestStarted);
        assert!(diff.network_state_changed);
        assert!(!diff.ui_state_changed);
        assert!(!diff.app_state_changed);
    }

    #[test]
    fn test_app_reducer_multiple_domains() {
        let mut app = AppReducer::new();
        let diff = app.reduce(&AppEvent::CollectionCreated(Collection::new(
            "Test".to_string(),
        )));
        assert!(!diff.ui_state_changed);
        assert!(!diff.network_state_changed);
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_app_reducer_empty_diff() {
        let mut app = AppReducer::new();
        let diff = app.reduce(&AppEvent::Quit);
        assert!(!diff.any());
    }

    #[test]
    fn test_app_reducer_reset() {
        let mut app = AppReducer::new();
        app.reduce(&AppEvent::ModeChanged(InputMode::Insert));
        app.reduce(&AppEvent::RequestStarted);
        app.reset();
        assert_eq!(app.ui.state.mode, InputMode::Normal);
        assert!(app.network.state.is_idle());
        assert!(app.tabs.tab_manager.is_empty());
    }

    #[test]
    fn test_app_reducer_tab_event_triggers_ui_and_app() {
        let mut app = AppReducer::new();
        let diff = app.reduce(&AppEvent::TabOpened {
            id: "t1".to_string(),
        });
        assert!(diff.ui_state_changed);
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_app_reducer_tab_switched_only_ui() {
        let mut app = AppReducer::new();
        let diff = app.reduce(&AppEvent::TabSwitched {
            from: "t1".to_string(),
            to: "t2".to_string(),
        });
        assert!(diff.ui_state_changed);
        assert!(!diff.app_state_changed);
    }
}
