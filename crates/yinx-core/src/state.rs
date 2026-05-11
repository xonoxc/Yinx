use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::request::Request;
use crate::response::Response;
use crate::tabs::TabManager;
use crate::timing::Timing;
use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: String,
    pub name: String,
    pub request: Request,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimelineSnapshotKind {
    ChunkBoundary,
    Ttfb,
    Error,
    LastChunk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineSnapshotRecord {
    pub kind: TimelineSnapshotKind,
    pub offset: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TimelineRecord {
    pub snapshots: Vec<TimelineSnapshotRecord>,
    pub current_index: Option<usize>,
}

impl TimelineRecord {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub request: Request,
    pub response: Option<Response>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub timing: Timing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<TimelineRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub default_timeout_secs: u64,
    pub follow_redirects: bool,
    pub verify_tls: bool,
    pub max_history_entries: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            default_timeout_secs: 30,
            follow_redirects: true,
            verify_tls: true,
            max_history_entries: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppState {
    pub requests: HashMap<String, SavedRequest>,
    pub history: Vec<HistoryEntry>,
    pub workflows: HashMap<String, WorkflowDefinition>,
    pub settings: AppSettings,
    pub workspace: Workspace,
    pub tab_manager: TabManager,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_request(&mut self, req: SavedRequest) {
        self.requests.insert(req.id.clone(), req);
    }

    pub fn get_request(&self, id: &str) -> Option<&SavedRequest> {
        self.requests.get(id)
    }

    pub fn remove_request(&mut self, id: &str) -> Option<SavedRequest> {
        self.requests.remove(id)
    }

    pub fn add_history_entry(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
    }

    pub fn history_iter(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.history.iter()
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn add_workflow(&mut self, workflow: WorkflowDefinition) {
        self.workflows.insert(workflow.id.clone(), workflow);
    }

    pub fn get_workflow(&self, id: &str) -> Option<&WorkflowDefinition> {
        self.workflows.get(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
    Visual,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActivePane {
    #[default]
    Request,
    Response,
    Workflow,
    Logs,
    Sidebar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UiState {
    pub active_pane: ActivePane,
    pub mode: InputMode,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
}

impl UiState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }

    pub fn set_pane(&mut self, pane: ActivePane) {
        self.active_pane = pane;
    }

    pub fn move_cursor(&mut self, lines: i64, cols: i64) {
        self.cursor_line = (self.cursor_line as i64 + lines).max(0) as usize;
        self.cursor_col = (self.cursor_col as i64 + cols).max(0) as usize;
    }

    pub fn scroll(&mut self, lines: i64) {
        self.scroll_offset = (self.scroll_offset as i64 + lines).max(0) as usize;
    }

    pub fn is_normal_mode(&self) -> bool {
        self.mode == InputMode::Normal
    }

    pub fn is_insert_mode(&self) -> bool {
        self.mode == InputMode::Insert
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum NetworkState {
    #[default]
    Idle,
    Loading,
    Streaming,
    Error(String),
}

impl NetworkState {
    pub fn is_idle(&self) -> bool {
        matches!(self, NetworkState::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, NetworkState::Loading)
    }

    pub fn is_streaming(&self) -> bool {
        matches!(self, NetworkState::Streaming)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, NetworkState::Error(_))
    }

    pub fn to_idle(&mut self) {
        *self = NetworkState::Idle;
    }

    pub fn to_loading(&mut self) {
        *self = NetworkState::Loading;
    }

    pub fn to_streaming(&mut self) {
        *self = NetworkState::Streaming;
    }

    pub fn to_error(&mut self, msg: String) {
        *self = NetworkState::Error(msg);
    }
}
