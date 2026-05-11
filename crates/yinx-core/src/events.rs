use serde::{Deserialize, Serialize};

use crate::request::Request;
use crate::response::Response;
use crate::collections::{Collection, CollectionItem};
use crate::environments::Environment;
use crate::state::{ActivePane, AppState, InputMode, NetworkState, UiState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AppEvent {
    // UI events
    KeyPressed(String),
    PaneChanged(ActivePane),
    ModeChanged(InputMode),
    CursorMoved { lines: i64, cols: i64 },
    Scrolled(i64),
    TerminalResized { width: u16, height: u16 },
    Quit,
    CyclePaneNext,
    CyclePanePrev,
    OpenCommandPalette,
    SearchActivated,

    // Network events
    SendRequest(Request),
    ExecuteRequest,
    RequestStarted,
    RequestCompleted(Response),
    RequestFailed(String),
    StreamChunk(Vec<u8>),
    StreamEnded,
    NetworkStateChange(NetworkState),

    // State events
    AppStateUpdated,
    HistoryEntryAdded { id: String },
    RequestSaved { id: String },
    RequestDeleted { id: String },

    // Collection events
    CollectionCreated(Collection),
    CollectionUpdated { id: String },
    CollectionDeleted { id: String },
    CollectionItemAdded { collection_id: String, item: CollectionItem },
    CollectionItemRemoved { collection_id: String, index: usize },
    CollectionItemMoved { collection_id: String, from: usize, to: usize },

    // Environment events
    EnvironmentCreated(Environment),
    EnvironmentUpdated { id: String },
    EnvironmentDeleted { id: String },
    EnvironmentActivated { id: Option<String> },
    EnvironmentVariableAdded { env_id: String },
    EnvironmentVariableRemoved { env_id: String, key: String },

    // Tab events
    TabOpened { id: String },
    TabClosed { id: String },
    TabSwitched { from: String, to: String },
    TabSwitchRelative(i64),
    TabDirtyChanged { id: String, dirty: bool },

    // Workspace events
    WorkspaceOpened { id: String },
    WorkspaceSaved,
    WorkspaceSettingsChanged,

    // Workflow events
    WorkflowStarted { id: String },
    WorkflowNodeCompleted { node_id: String },
    WorkflowCompleted { id: String },
    WorkflowFailed { id: String, error: String },

    // Storage events
    SaveState,
    StateLoaded,

    // Import events
    ImportStarted { source: String },
    ImportCompleted { count: usize },
    ImportFailed { error: String },

    // Settings events
    SettingsOpened,
    SettingsChanged { key: String, value: String },
    SettingsSaved,
    SettingsClosed,

    // Config events
    ConfigChanged { key: String, value: String },

    // Theme events
    ThemeChanged(String),

    // Command & keybinding events
    ClearLogs,
    CloseOtherTabs,
    EqualizePanes,
    MaximizePaneHeight,
    MaximizePaneWidth,
    GoToDefinition,
    SearchResponse,
    DeleteLine,
    InsertAtStart,
}

pub struct EventBus {
    sender: tokio::sync::mpsc::Sender<AppEvent>,
    receiver: Option<tokio::sync::mpsc::Receiver<AppEvent>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
        Self {
            sender,
            receiver: Some(receiver),
        }
    }

    pub fn sender(&self) -> tokio::sync::mpsc::Sender<AppEvent> {
        self.sender.clone()
    }

    pub fn take_receiver(&mut self) -> Option<tokio::sync::mpsc::Receiver<AppEvent>> {
        self.receiver.take()
    }

    pub async fn send(
        &self,
        event: AppEvent,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<AppEvent>> {
        self.sender.send(event).await
    }

    #[allow(clippy::result_large_err)]
    pub fn try_send(
        &self,
        event: AppEvent,
    ) -> Result<(), tokio::sync::mpsc::error::TrySendError<AppEvent>> {
        self.sender.try_send(event)
    }

    pub async fn receive(&mut self) -> Option<AppEvent> {
        if let Some(ref mut rx) = self.receiver {
            rx.recv().await
        } else {
            None
        }
    }

    pub fn channel_size(&self) -> usize {
        self.sender.max_capacity() - self.sender.capacity()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateDiff {
    pub ui_state_changed: bool,
    pub network_state_changed: bool,
    pub app_state_changed: bool,
}

impl StateDiff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        !self.ui_state_changed && !self.network_state_changed && !self.app_state_changed
    }

    pub fn any(&self) -> bool {
        !self.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct StateReducer {
    pub app_state: AppState,
    pub ui_state: UiState,
    pub network_state: NetworkState,
}

impl StateReducer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reduce(&mut self, event: &AppEvent) -> StateDiff {
        let mut diff = StateDiff::new();

        match event {
            AppEvent::KeyPressed(_) => {}
            AppEvent::PaneChanged(pane) => {
                self.ui_state.set_pane(*pane);
                diff.ui_state_changed = true;
            }
            AppEvent::ModeChanged(mode) => {
                self.ui_state.set_mode(*mode);
                diff.ui_state_changed = true;
            }
            AppEvent::CursorMoved { lines, cols } => {
                self.ui_state.move_cursor(*lines, *cols);
                diff.ui_state_changed = true;
            }
            AppEvent::Scrolled(lines) => {
                self.ui_state.scroll(*lines);
                diff.ui_state_changed = true;
            }
            AppEvent::TerminalResized { .. } => {}
            AppEvent::Quit => {}
            AppEvent::CyclePaneNext => {}
            AppEvent::CyclePanePrev => {}
            AppEvent::SendRequest(_) => {
                self.network_state = NetworkState::Loading;
                diff.network_state_changed = true;
            }
            AppEvent::ExecuteRequest => {
                self.network_state = NetworkState::Loading;
                diff.network_state_changed = true;
            }
            AppEvent::RequestStarted => {
                self.network_state = NetworkState::Loading;
                diff.network_state_changed = true;
            }
            AppEvent::RequestCompleted(_) => {
                self.network_state = NetworkState::Idle;
                diff.network_state_changed = true;
                diff.app_state_changed = true;
            }
            AppEvent::RequestFailed(_) => {
                self.network_state = NetworkState::Error("Request failed".to_string());
                diff.network_state_changed = true;
            }
            AppEvent::StreamChunk(_) => {
                self.network_state = NetworkState::Streaming;
                diff.network_state_changed = true;
            }
            AppEvent::StreamEnded => {
                self.network_state = NetworkState::Idle;
                diff.network_state_changed = true;
            }
            AppEvent::NetworkStateChange(state) => {
                self.network_state = state.clone();
                diff.network_state_changed = true;
            }
            AppEvent::AppStateUpdated => {
                diff.app_state_changed = true;
            }
            AppEvent::HistoryEntryAdded { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::RequestSaved { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::RequestDeleted { id } => {
                let _ = self.app_state.remove_request(id);
                diff.app_state_changed = true;
            }
            AppEvent::WorkflowStarted { .. } => {
                self.network_state = NetworkState::Loading;
                diff.network_state_changed = true;
            }
            AppEvent::WorkflowNodeCompleted { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::WorkflowCompleted { .. } => {
                self.network_state = NetworkState::Idle;
                diff.network_state_changed = true;
            }
            AppEvent::WorkflowFailed { .. } => {
                self.network_state = NetworkState::Error("Workflow failed".to_string());
                diff.network_state_changed = true;
            }
            AppEvent::SaveState => {}
            AppEvent::StateLoaded => {}
            AppEvent::ImportStarted { .. } => {}
            AppEvent::ImportCompleted { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::ImportFailed { .. } => {}
            AppEvent::SettingsOpened => {}
            AppEvent::SettingsChanged { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::SettingsSaved => {}
            AppEvent::SettingsClosed => {}
            AppEvent::ThemeChanged(_) => {
                diff.app_state_changed = true;
            }
            AppEvent::OpenCommandPalette => {
                self.ui_state.set_mode(InputMode::Command);
                diff.ui_state_changed = true;
            }
            AppEvent::SearchActivated => {
                diff.ui_state_changed = true;
            }
            AppEvent::ConfigChanged { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::ClearLogs
            | AppEvent::CloseOtherTabs
            | AppEvent::EqualizePanes
            | AppEvent::MaximizePaneHeight
            | AppEvent::MaximizePaneWidth
            | AppEvent::GoToDefinition
            | AppEvent::SearchResponse
            | AppEvent::DeleteLine
            | AppEvent::InsertAtStart => {
                diff.ui_state_changed = true;
            }

            // Collection events
            AppEvent::CollectionCreated(_) => {
                diff.app_state_changed = true;
            }
            AppEvent::CollectionUpdated { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::CollectionDeleted { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::CollectionItemAdded { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::CollectionItemRemoved { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::CollectionItemMoved { .. } => {
                diff.app_state_changed = true;
            }

            // Environment events
            AppEvent::EnvironmentCreated(_) => {
                diff.app_state_changed = true;
            }
            AppEvent::EnvironmentUpdated { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::EnvironmentDeleted { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::EnvironmentActivated { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::EnvironmentVariableAdded { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::EnvironmentVariableRemoved { .. } => {
                diff.app_state_changed = true;
            }

            // Tab events
            AppEvent::TabOpened { .. } => {
                diff.ui_state_changed = true;
                diff.app_state_changed = true;
            }
            AppEvent::TabClosed { .. } => {
                diff.ui_state_changed = true;
                diff.app_state_changed = true;
            }
            AppEvent::TabSwitched { .. } | AppEvent::TabSwitchRelative(_) => {
                diff.ui_state_changed = true;
            }
            AppEvent::TabDirtyChanged { .. } => {
                diff.app_state_changed = true;
            }

            // Workspace events
            AppEvent::WorkspaceOpened { .. } => {
                diff.app_state_changed = true;
            }
            AppEvent::WorkspaceSaved => {}
            AppEvent::WorkspaceSettingsChanged => {
                diff.app_state_changed = true;
            }
        }

        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::RequestBuilder;
    use crate::response::{Response, ResponseBody};

    #[test]
    fn test_app_state_default() {
        let state = AppState::new();
        assert!(state.requests.is_empty());
        assert!(state.history.is_empty());
        assert!(state.workflows.is_empty());
    }

    #[test]
    fn test_app_state_add_request() {
        let mut state = AppState::new();
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let saved = crate::state::SavedRequest {
            id: "1".to_string(),
            name: "Test".to_string(),
            request,
            tags: vec!["test".to_string()],
        };
        state.add_request(saved);
        assert_eq!(state.requests.len(), 1);
        assert!(state.get_request("1").is_some());
    }

    #[test]
    fn test_app_state_remove_request() {
        let mut state = AppState::new();
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        state.add_request(crate::state::SavedRequest {
            id: "1".to_string(),
            name: "Test".to_string(),
            request,
            tags: vec![],
        });
        let removed = state.remove_request("1");
        assert!(removed.is_some());
        assert!(state.requests.is_empty());
    }

    #[test]
    fn test_app_state_history() {
        let mut state = AppState::new();
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let entry = crate::state::HistoryEntry {
            id: "1".to_string(),
            request,
            response: None,
            timestamp: chrono::Utc::now(),
            timing: crate::timing::Timing::new(),
            timeline: None,
        };
        state.add_history_entry(entry);
        assert_eq!(state.history_len(), 1);
        assert_eq!(state.history_iter().count(), 1);
    }

    #[test]
    fn test_timeline_record_state_management() {
        let mut record = crate::state::TimelineRecord::new();
        assert!(record.is_empty());

        record.snapshots.push(crate::state::TimelineSnapshotRecord {
            kind: crate::state::TimelineSnapshotKind::Ttfb,
            offset: 3,
            timestamp: chrono::Utc::now(),
            body: b"hey".to_vec(),
        });
        record.current_index = Some(0);

        assert_eq!(record.len(), 1);
        assert!(!record.is_empty());
        assert_eq!(record.current_index, Some(0));
    }

    #[test]
    fn test_app_state_workflows() {
        let mut state = AppState::new();
        let workflow = crate::state::WorkflowDefinition {
            id: "wf1".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec!["node1".to_string()],
            edges: vec![],
        };
        state.add_workflow(workflow);
        assert!(state.get_workflow("wf1").is_some());
        assert!(state.get_workflow("nonexistent").is_none());
    }

    #[test]
    fn test_app_state_settings() {
        let state = AppState::new();
        assert_eq!(state.settings.default_timeout_secs, 30);
        assert!(state.settings.follow_redirects);
        assert!(state.settings.verify_tls);
    }

    #[test]
    fn test_ui_state_default() {
        let ui = UiState::new();
        assert_eq!(ui.mode, InputMode::Normal);
        assert_eq!(ui.active_pane, ActivePane::Request);
        assert_eq!(ui.cursor_line, 0);
        assert_eq!(ui.cursor_col, 0);
    }

    #[test]
    fn test_ui_state_mode_transitions() {
        let mut ui = UiState::new();
        assert!(ui.is_normal_mode());

        ui.set_mode(InputMode::Insert);
        assert!(ui.is_insert_mode());

        ui.set_mode(InputMode::Normal);
        assert!(ui.is_normal_mode());
    }

    #[test]
    fn test_ui_state_pane_change() {
        let mut ui = UiState::new();
        assert_eq!(ui.active_pane, ActivePane::Request);

        ui.set_pane(ActivePane::Response);
        assert_eq!(ui.active_pane, ActivePane::Response);

        ui.set_pane(ActivePane::Workflow);
        assert_eq!(ui.active_pane, ActivePane::Workflow);

        ui.set_pane(ActivePane::Logs);
        assert_eq!(ui.active_pane, ActivePane::Logs);
    }

    #[test]
    fn test_ui_state_cursor_movement() {
        let mut ui = UiState::new();
        ui.move_cursor(5, 10);
        assert_eq!(ui.cursor_line, 5);
        assert_eq!(ui.cursor_col, 10);
    }

    #[test]
    fn test_ui_state_cursor_negative_clamp() {
        let mut ui = UiState::new();
        ui.move_cursor(-5, -10);
        assert_eq!(ui.cursor_line, 0);
        assert_eq!(ui.cursor_col, 0);
    }

    #[test]
    fn test_ui_state_scroll() {
        let mut ui = UiState::new();
        ui.scroll(10);
        assert_eq!(ui.scroll_offset, 10);
        ui.scroll(-5);
        assert_eq!(ui.scroll_offset, 5);
    }

    #[test]
    fn test_ui_state_scroll_negative_clamp() {
        let mut ui = UiState::new();
        ui.scroll(-100);
        assert_eq!(ui.scroll_offset, 0);
    }

    #[test]
    fn test_ui_state_serde_roundtrip() {
        let ui = UiState::new();
        let json = serde_json::to_string(&ui).unwrap();
        let decoded: UiState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.mode, ui.mode);
        assert_eq!(decoded.active_pane, ui.active_pane);
    }

    #[test]
    fn test_network_state_default() {
        let state = NetworkState::default();
        assert!(state.is_idle());
    }

    #[test]
    fn test_network_state_transitions() {
        let mut state = NetworkState::Idle;
        assert!(state.is_idle());

        state.to_loading();
        assert!(state.is_loading());

        state.to_streaming();
        assert!(state.is_streaming());

        state.to_error("timeout".to_string());
        assert!(state.is_error());

        state.to_idle();
        assert!(state.is_idle());
    }

    #[test]
    fn test_network_state_serde() {
        let state = NetworkState::Idle;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: NetworkState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, state);

        let state = NetworkState::Error("fail".to_string());
        let json = serde_json::to_string(&state).unwrap();
        let decoded: NetworkState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, state);
    }

    #[test]
    fn test_app_event_variants() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();

        let events = vec![
            AppEvent::KeyPressed("j".to_string()),
            AppEvent::PaneChanged(ActivePane::Response),
            AppEvent::ModeChanged(InputMode::Insert),
            AppEvent::SendRequest(request),
            AppEvent::RequestCompleted(
                Response::builder()
                    .status(200)
                    .body(ResponseBody::None)
                    .build(),
            ),
            AppEvent::RequestFailed("timeout".to_string()),
            AppEvent::NetworkStateChange(NetworkState::Loading),
            AppEvent::Quit,
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: AppEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, event);
        }
    }

    #[tokio::test]
    async fn test_event_bus_send_receive() {
        let mut bus = EventBus::new(10);
        let sender = bus.sender();

        sender
            .send(AppEvent::KeyPressed("a".to_string()))
            .await
            .unwrap();

        let received = bus.receive().await;
        assert!(matches!(received, Some(AppEvent::KeyPressed(k)) if k == "a"));
    }

    #[tokio::test]
    async fn test_event_bus_try_send() {
        let bus = EventBus::new(1);
        assert!(bus.try_send(AppEvent::KeyPressed("a".to_string())).is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_bounded_overflow() {
        let bus = EventBus::new(1);
        assert!(bus.try_send(AppEvent::KeyPressed("a".to_string())).is_ok());
        assert!(bus.try_send(AppEvent::KeyPressed("b".to_string())).is_err());
    }

    #[test]
    fn test_state_diff_default() {
        let diff = StateDiff::new();
        assert!(diff.is_empty());
        assert!(!diff.any());
    }

    #[test]
    fn test_state_diff_any() {
        let mut diff = StateDiff::new();
        diff.ui_state_changed = true;
        assert!(diff.any());
    }

    #[test]
    fn test_state_reducer_pane_change() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::PaneChanged(ActivePane::Response);
        let diff = reducer.reduce(&event);
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.ui_state.active_pane, ActivePane::Response);
    }

    #[test]
    fn test_state_reducer_mode_change() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::ModeChanged(InputMode::Insert);
        let diff = reducer.reduce(&event);
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.ui_state.mode, InputMode::Insert);
    }

    #[test]
    fn test_state_reducer_send_request() {
        let mut reducer = StateReducer::new();
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let event = AppEvent::SendRequest(request);
        let diff = reducer.reduce(&event);
        assert!(diff.network_state_changed);
        assert!(reducer.network_state.is_loading());
    }

    #[test]
    fn test_state_reducer_request_completed() {
        let mut reducer = StateReducer::new();
        reducer.network_state = NetworkState::Loading;

        let event = AppEvent::RequestCompleted(
            Response::builder()
                .status(200)
                .body(ResponseBody::None)
                .build(),
        );
        let diff = reducer.reduce(&event);
        assert!(diff.network_state_changed);
        assert!(reducer.network_state.is_idle());
    }

    #[test]
    fn test_state_reducer_request_failed() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::RequestFailed("error".to_string());
        let diff = reducer.reduce(&event);
        assert!(diff.network_state_changed);
        assert!(reducer.network_state.is_error());
    }

    #[test]
    fn test_state_reducer_stream_chunk() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::StreamChunk(vec![1, 2, 3]);
        let diff = reducer.reduce(&event);
        assert!(diff.network_state_changed);
        assert!(reducer.network_state.is_streaming());
    }

    #[test]
    fn test_state_reducer_stream_ended() {
        let mut reducer = StateReducer::new();
        reducer.network_state = NetworkState::Streaming;
        let event = AppEvent::StreamEnded;
        let diff = reducer.reduce(&event);
        assert!(diff.network_state_changed);
        assert!(reducer.network_state.is_idle());
    }

    #[test]
    fn test_state_reducer_request_deleted() {
        let mut reducer = StateReducer::new();
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        reducer.app_state.add_request(crate::state::SavedRequest {
            id: "1".to_string(),
            name: "Test".to_string(),
            request,
            tags: vec![],
        });

        let event = AppEvent::RequestDeleted {
            id: "1".to_string(),
        };
        let diff = reducer.reduce(&event);
        assert!(diff.app_state_changed);
        assert!(reducer.app_state.get_request("1").is_none());
    }

    #[test]
    fn test_state_reducer_cursor_movement() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::CursorMoved { lines: 5, cols: 10 };
        let diff = reducer.reduce(&event);
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.ui_state.cursor_line, 5);
        assert_eq!(reducer.ui_state.cursor_col, 10);
    }

    #[test]
    fn test_state_reducer_scroll() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::Scrolled(20);
        let diff = reducer.reduce(&event);
        assert!(diff.ui_state_changed);
        assert_eq!(reducer.ui_state.scroll_offset, 20);
    }

    #[test]
    fn test_state_reducer_network_state_change() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::NetworkStateChange(NetworkState::Streaming);
        let diff = reducer.reduce(&event);
        assert!(diff.network_state_changed);
        assert!(reducer.network_state.is_streaming());
    }

    #[test]
    fn test_state_reducer_import_completed() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::ImportCompleted { count: 5 };
        let diff = reducer.reduce(&event);
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_state_reducer_settings_opened() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::SettingsOpened;
        let diff = reducer.reduce(&event);
        assert!(!diff.any());
    }

    #[test]
    fn test_state_reducer_settings_changed() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::SettingsChanged {
            key: "theme".to_string(),
            value: "light".to_string(),
        };
        let diff = reducer.reduce(&event);
        assert!(diff.app_state_changed);
    }

    #[test]
    fn test_state_reducer_settings_saved() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::SettingsSaved;
        let diff = reducer.reduce(&event);
        assert!(!diff.any());
    }

    #[test]
    fn test_state_reducer_settings_closed() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::SettingsClosed;
        let diff = reducer.reduce(&event);
        assert!(!diff.any());
    }

    #[tokio::test]
    async fn test_event_bus_channel_size() {
        let bus = EventBus::new(10);
        assert_eq!(bus.channel_size(), 0);
    }

    #[test]
    fn test_theme_changed_event_has_name() {
        let event = AppEvent::ThemeChanged("dark".to_string());
        match event {
            AppEvent::ThemeChanged(name) => assert_eq!(name, "dark"),
            _ => panic!("Wrong event type"),
        }
    }
}
