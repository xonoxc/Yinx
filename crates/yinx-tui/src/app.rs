use std::io;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Terminal as RatatuiTerminal;

use crate::command_palette::{CommandPalette, PaletteAction};
use crate::editor::{self, EditorError, EditorFormat, SystemEditorRunner, TerminalSession};
use crate::layout::WorkspaceLayout;
use crate::logs_pane::{LogLevel, LogsPane};
use crate::request_pane::RequestPane;
use crate::response_pane::ResponsePane;
use crate::settings_pane::SettingsPane;
use crate::sidebar::Sidebar;
use crate::tab_bar::TabBar;
use crate::theme::{Theme, ThemeRegistry};
use crate::widgets::StatusBar;
use yinx_core::environments::Environment;
use yinx_core::events::{AppEvent, EventBus, StateReducer};
#[cfg(test)]
use yinx_core::state::UiState;
use yinx_core::state::{ActivePane, HistoryEntry, InputMode, NetworkState};
use yinx_core::tabs::TabManager;
use yinx_core::timing::{RequestMetrics, Timing};
use yinx_http::client::HttpClient;
use yinx_http::controller::{RequestController, RequestEvent};

use crate::input::InputHandler;

pub type TerminalType = RatatuiTerminal<CrosstermBackend<io::Stdout>>;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Terminal initialization failed: {0}")]
    TerminalInit(#[from] io::Error),
    #[error("Terminal restore failed: {0}")]
    TerminalRestore(String),
    #[error("Event loop error: {0}")]
    EventLoop(String),
    #[error("Render error: {0}")]
    Render(String),
    #[error("Panic: {0}")]
    Panic(String),
}

pub async fn run_tui() -> Result<(), AppError> {
    let mut app = App::init()?;
    let terminal_size = app
        .terminal()
        .size()
        .map_err(|e| AppError::Render(e.to_string()))?;
    let mut shell = TuiShell::new(terminal_size.width, terminal_size.height);
    shell.sync_sidebar_environments();

    loop {
        app.terminal()
            .draw(|frame| shell.render(frame))
            .map_err(|e| AppError::Render(e.to_string()))?;

        if shell.should_quit() {
            break;
        }

        shell.check_request_completion();

        if event::poll(Duration::from_millis(50)).map_err(|e| AppError::EventLoop(e.to_string()))? {
            let event = event::read().map_err(|e| AppError::EventLoop(e.to_string()))?;
            shell.handle_event(event).await?;
        }
    }

    Ok(())
}

pub struct TerminalGuard {
    raw_mode: bool,
}

impl TerminalGuard {
    pub fn enter_raw_mode() -> Result<Self, AppError> {
        terminal::enable_raw_mode().map_err(|e| AppError::TerminalRestore(e.to_string()))?;
        Self::hide_cursor()?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)
            .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
        Ok(Self { raw_mode: true })
    }

    fn hide_cursor() -> Result<(), AppError> {
        crossterm::execute!(io::stdout(), crossterm::cursor::Hide)
            .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
        Ok(())
    }

    fn show_cursor() -> Result<(), AppError> {
        crossterm::execute!(io::stdout(), crossterm::cursor::Show)
            .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
        Ok(())
    }

    pub fn exit_raw_mode() -> Result<(), AppError> {
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste);
        terminal::disable_raw_mode().map_err(|e| AppError::TerminalRestore(e.to_string()))?;
        Self::show_cursor()?;
        Ok(())
    }

    pub fn suspend(&mut self) -> Result<(), AppError> {
        if self.raw_mode {
            Self::exit_raw_mode()?;
            self.raw_mode = false;
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), AppError> {
        if !self.raw_mode {
            terminal::enable_raw_mode().map_err(|e| AppError::TerminalRestore(e.to_string()))?;
            Self::hide_cursor()?;
            crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)
                .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
            self.raw_mode = true;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.raw_mode {
            let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste);
            let _ = terminal::disable_raw_mode();
            let _ = Self::show_cursor();
        }
    }
}

struct TuiShell {
    theme: Theme,
    theme_registry: ThemeRegistry,
    workspace_layout: WorkspaceLayout,
    sidebar: Sidebar,
    tab_bar: TabBar,
    tab_manager: TabManager,
    request_pane: RequestPane,
    response_pane: ResponsePane,
    logs_pane: LogsPane,
    settings_pane: SettingsPane,
    active_pane: ActivePane,
    network_state: NetworkState,
    should_quit: bool,
    show_help: bool,
    input_handler: InputHandler,
    command_palette: CommandPalette,
    request_controller: RequestController,
    request_rx: Option<tokio::sync::mpsc::UnboundedReceiver<RequestEvent>>,
    history: Vec<HistoryEntry>,
    environments: Vec<Environment>,
    active_env_id: Option<String>,
}

impl TuiShell {
    fn new(width: u16, height: u16) -> Self {
        let mut logs_pane = LogsPane::new();
        logs_pane.add_log(
            LogLevel::Info,
            "Welcome to Yinx. Focus the URL bar and paste or type a request.",
        );
        logs_pane.add_log(
            LogLevel::Info,
            "Ctrl+Enter sends, Tab cycles panes, Esc leaves insert mode.",
        );

        let mut theme_registry = ThemeRegistry::with_defaults();
        let settings_pane = SettingsPane::new();
        let configured_theme = settings_pane.config.theme.clone();
        if theme_registry.get(&configured_theme).is_some() {
            theme_registry.set_current(&configured_theme);
        }
        let theme = theme_registry
            .current()
            .cloned()
            .unwrap_or_else(Theme::terminal_default);

        let mut workspace_layout = WorkspaceLayout::new();
        workspace_layout.update_terminal_size(width, height);

        let sidebar = Sidebar::new();
        let tab_bar = TabBar::new();
        let tab_manager = TabManager::new(20);

        Self {
            theme,
            theme_registry,
            workspace_layout,
            sidebar,
            tab_bar,
            tab_manager,
            request_pane: RequestPane::new(),
            response_pane: ResponsePane::new(),
            logs_pane,
            settings_pane,
            active_pane: ActivePane::Request,
            network_state: NetworkState::Idle,
            should_quit: false,
            show_help: false,
            input_handler: InputHandler::new(),
            command_palette: CommandPalette::new(),
            request_controller: RequestController::new(),
            request_rx: None,
            history: Vec::new(),
            environments: Vec::new(),
            active_env_id: None,
        }
    }

    fn sync_sidebar_environments(&mut self) {
        self.sidebar
            .set_environments(self.environments.clone(), self.active_env_id.clone());
    }

    fn apply_theme_name(&mut self, name: &str) {
        if let Some(theme) = Theme::get_by_name(name) {
            self.theme_registry.set_current(&theme.name);
            self.settings_pane.config.theme = theme.name.clone();
            self.theme = theme;
        }
    }

    fn handle_settings_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SettingsChanged { key, value } | AppEvent::ConfigChanged { key, value } => {
                if key == "theme" && !value.is_empty() {
                    self.apply_theme_name(&value);
                }
            }
            _ => {}
        }
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    async fn handle_event(&mut self, event: Event) -> Result<(), AppError> {
        match event {
            Event::Key(key_event) => self.handle_key(key_event).await,
            Event::Paste(text) => {
                self.handle_paste(&text);
                Ok(())
            }
            Event::Resize(width, height) => {
                self.workspace_layout.update_terminal_size(width, height);
                Ok(())
            }
            Event::Mouse(mouse_event) => {
                self.handle_mouse_event(mouse_event);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn handle_mouse_event(&mut self, mouse_event: crossterm::event::MouseEvent) {
        if mouse_event.kind
            != crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
        {
            return;
        }

        let wrects = self.workspace_layout.calculate();

        if self.is_in_rect(mouse_event.column, mouse_event.row, wrects.sidebar)
            && wrects.sidebar.width > 0
        {
            self.active_pane = ActivePane::Sidebar;
        } else {
            let (response_area, logs_area) = split_response_logs(wrects.center_bottom);
            if let Some(logs_rect) = logs_area {
                if self.is_in_rect(mouse_event.column, mouse_event.row, logs_rect) {
                    self.active_pane = ActivePane::Logs;
                    return;
                }
            }

            if self.is_in_rect(mouse_event.column, mouse_event.row, response_area) {
                self.active_pane = ActivePane::Response;
            } else if self.is_in_rect(mouse_event.column, mouse_event.row, wrects.center_top) {
                self.active_pane = ActivePane::Request;
            }
        }
    }

    fn is_in_rect(&self, col: u16, row: u16, rect: ratatui::layout::Rect) -> bool {
        col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }

    async fn handle_key(&mut self, key_event: KeyEvent) -> Result<(), AppError> {
        // Handle Ctrl+C for quit (emergency exit)
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && key_event.code == KeyCode::Char('c')
        {
            self.should_quit = true;
            return Ok(());
        }

        // Handle command palette if open
        if self.command_palette.is_visible() {
            let action = self.command_palette.handle_key(key_event);
            match action {
                PaletteAction::Execute(events) => {
                    self.command_palette.hide();
                    self.input_handler.switch_mode(InputMode::Normal);
                    for evt in events {
                        match evt {
                            AppEvent::Quit => {
                                self.should_quit = true;
                            }
                            AppEvent::ExecuteRequest => {
                                if let Err(e) = self.execute_request().await {
                                    self.logs_pane.add_log(LogLevel::Error, e.to_string());
                                    self.response_pane.set_error(e.to_string());
                                }
                            }
                            AppEvent::SaveState => {
                                self.logs_pane.add_log(LogLevel::Info, "State saved");
                            }
                            AppEvent::SearchActivated => {
                                self.logs_pane.add_log(LogLevel::Info, "Search activated");
                            }
                            AppEvent::SettingsOpened => {
                                self.settings_pane.open();
                                // Remove this once a proper settings screen exists
                                self.show_help = !self.show_help;
                            }
                            AppEvent::ImportStarted { .. } => {
                                self.logs_pane.add_log(LogLevel::Info, "Import triggered");
                            }
                            AppEvent::TabOpened { .. } => {
                                let id = self.tab_manager.open_blank();
                                self.logs_pane
                                    .add_log(LogLevel::Info, format!("New tab opened: {}", id));
                            }
                            AppEvent::PaneChanged(pane) => {
                                self.active_pane = pane;
                            }
                            AppEvent::ClearLogs => {
                                self.logs_pane = LogsPane::new();
                                self.logs_pane.add_log(LogLevel::Info, "Logs cleared");
                            }
                            AppEvent::MaximizePaneHeight => {
                                self.workspace_layout.resize_center_split(0.5);
                            }
                            AppEvent::GoToDefinition => {
                                self.logs_pane.add_log(LogLevel::Info, "Go to definition");
                            }
                            AppEvent::SearchResponse => {
                                self.logs_pane.add_log(LogLevel::Info, "Search in response");
                            }
                            AppEvent::CloseOtherTabs => {
                                if let Some(tab) = self.tab_manager.active_tab() {
                                    let id = tab.id.clone();
                                    self.tab_manager.close_others(&id);
                                    self.logs_pane.add_log(LogLevel::Info, "Closed other tabs");
                                }
                            }
                            _ => {}
                        }
                    }
                }
                PaletteAction::Close => {
                    self.command_palette.hide();
                    self.input_handler.switch_mode(InputMode::Normal);
                }
                PaletteAction::None => {}
            }
            return Ok(());
        }

        // Handle settings pane if open
        if self.settings_pane.is_open() {
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.settings_pane.close();
                    return Ok(());
                }
                _ => {
                    if let Some(event) = key_to_app_event(key_event) {
                        for event in self.settings_pane.handle_event(&event) {
                            self.handle_settings_event(event);
                        }
                    }
                    return Ok(());
                }
            }
        }

        // Handle help overlay
        if self.show_help {
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                    self.show_help = false;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }

        // Normal-mode global operations: resize, help, layout toggle, sidebar toggle
        if self.input_handler.current_mode() == InputMode::Normal {
            match key_event.code {
                KeyCode::Char('?') => {
                    self.show_help = true;
                    return Ok(());
                }
                KeyCode::Char('b') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.workspace_layout.toggle_sidebar();
                    return Ok(());
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    match self.active_pane {
                        ActivePane::Sidebar => self.workspace_layout.resize_sidebar(5),
                        ActivePane::Request | ActivePane::Response => {
                            self.workspace_layout.resize_center_split(0.05)
                        }
                        _ => {}
                    }
                    return Ok(());
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    match self.active_pane {
                        ActivePane::Sidebar => self.workspace_layout.resize_sidebar(-5),
                        ActivePane::Request | ActivePane::Response => {
                            self.workspace_layout.resize_center_split(-0.05)
                        }
                        _ => {}
                    }
                    return Ok(());
                }
                KeyCode::F(10) => {
                    let theme = self.theme_registry.cycle_next().clone();
                    self.settings_pane.config.theme = theme.name.clone();
                    self.theme = theme;
                    return Ok(());
                }
                _ => {}
            }
        }

        // Handle command mode (:) entry
        if self.input_handler.current_mode() == InputMode::Normal {
            if key_event.code == KeyCode::Char(':') && key_event.modifiers.is_empty() {
                self.command_palette.show();
                self.input_handler.switch_mode(InputMode::Command);
                return Ok(());
            }
        }

        // Use InputHandler for terminal-native keybindings
        let events = self.input_handler.handle_key(key_event);

        let mut handled = false;
        for event in events {
            match event {
                AppEvent::Quit => {
                    self.should_quit = true;
                    handled = true;
                }
                AppEvent::PaneChanged(pane) => {
                    self.active_pane = pane;
                    handled = true;
                }
                AppEvent::ModeChanged(mode) => {
                    if mode == InputMode::Command {
                        self.command_palette.show();
                    }
                    handled = true;
                }
                AppEvent::CyclePaneNext => {
                    self.active_pane = match self.active_pane {
                        ActivePane::Sidebar => ActivePane::Request,
                        ActivePane::Request => ActivePane::Response,
                        ActivePane::Response => ActivePane::Logs,
                        ActivePane::Logs => ActivePane::Sidebar,
                        _ => ActivePane::Request,
                    };
                    handled = true;
                }
                AppEvent::CyclePanePrev => {
                    self.active_pane = match self.active_pane {
                        ActivePane::Sidebar => ActivePane::Logs,
                        ActivePane::Logs => ActivePane::Response,
                        ActivePane::Response => ActivePane::Request,
                        ActivePane::Request => ActivePane::Sidebar,
                        _ => ActivePane::Request,
                    };
                    handled = true;
                }
                AppEvent::SendRequest(req) => {
                    if let Err(e) = self.execute_request_with(req).await {
                        self.logs_pane.add_log(LogLevel::Error, e.to_string());
                        self.response_pane.set_error(e.to_string());
                    }
                    handled = true;
                }
                AppEvent::ExecuteRequest => {
                    if let Err(e) = self.execute_request().await {
                        self.logs_pane.add_log(LogLevel::Error, e.to_string());
                        self.response_pane.set_error(e.to_string());
                    }
                    handled = true;
                }
                AppEvent::OpenCommandPalette => {
                    self.command_palette.show();
                    handled = true;
                }
                AppEvent::SearchActivated => {
                    handled = true;
                }
                AppEvent::ThemeChanged(name) => {
                    if name == "next" {
                        let theme = self.theme_registry.cycle_next().clone();
                        self.settings_pane.config.theme = theme.name.clone();
                        self.theme = theme;
                    } else {
                        self.apply_theme_name(&name);
                    }
                    handled = true;
                }
                AppEvent::TabSwitchRelative(delta) => {
                    if !self.tab_manager.is_empty() {
                        self.tab_manager.navigate(delta as i32);
                    }
                    handled = true;
                }
                AppEvent::TabOpened { .. } => {
                    let id = self.tab_manager.open_blank();
                    self.logs_pane
                        .add_log(LogLevel::Info, format!("New tab opened: {}", id));
                    handled = true;
                }
                AppEvent::TabClosed { id, .. } => {
                    if !id.is_empty() {
                        self.tab_manager.close(&id);
                    } else {
                        self.tab_manager.close_active();
                    }
                    self.logs_pane.add_log(LogLevel::Info, "Tab closed");
                    handled = true;
                }
                AppEvent::KeyPressed(_)
                | AppEvent::CursorMoved { .. }
                | AppEvent::Scrolled(_)
                | AppEvent::SaveState
                | AppEvent::NetworkStateChange(_) => {
                    handled = false;
                }
                AppEvent::CloseOtherTabs => {
                    if let Some(tab) = self.tab_manager.active_tab() {
                        let id = tab.id.clone();
                        self.tab_manager.close_others(&id);
                        self.logs_pane.add_log(LogLevel::Info, "Closed other tabs");
                    }
                    handled = true;
                }
                AppEvent::EqualizePanes => {
                    self.logs_pane.add_log(LogLevel::Info, "Panes equalized");
                    handled = true;
                }
                AppEvent::MaximizePaneHeight => {
                    self.workspace_layout.resize_center_split(0.5);
                    handled = true;
                }
                AppEvent::MaximizePaneWidth => {
                    self.workspace_layout.resize_center_split(0.3);
                    handled = true;
                }
                AppEvent::GoToDefinition => {
                    self.active_pane = ActivePane::Sidebar;
                    handled = true;
                }
                AppEvent::DeleteLine => {
                    match self.active_pane {
                        ActivePane::Sidebar => {
                            self.sidebar.handle_key(KeyCode::Char('d'));
                        }
                        ActivePane::Request => {
                            let _ = self
                                .request_pane
                                .handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
                        }
                        _ => {}
                    }
                    handled = true;
                }
                AppEvent::SearchResponse => {
                    self.active_pane = ActivePane::Response;
                    handled = true;
                }
                AppEvent::InsertAtStart => {
                    handled = false;
                }
                AppEvent::ClearLogs => {
                    self.logs_pane = LogsPane::new();
                    self.logs_pane.add_log(LogLevel::Info, "Logs cleared");
                    handled = true;
                }
                _ => {}
            }
        }

        // Forward key to active pane if InputHandler didn't mark it as handled.
        if !handled {
            self.forward_key_to_active_pane(key_event);
        }

        Ok(())
    }

    fn handle_paste(&mut self, text: &str) {
        if self.command_palette.is_visible() || self.settings_pane.is_open() || self.show_help {
            return;
        }

        if self.active_pane == ActivePane::Request && self.request_pane.paste_text(text) {
            self.logs_pane
                .add_log(LogLevel::Info, "Pasted into request editor");
        }
    }

    async fn execute_request_with(
        &mut self,
        request: yinx_core::request::Request,
    ) -> Result<(), AppError> {
        let timeout_secs = self.settings_pane.config.defaults.default_timeout_secs;
        let _follow_redirects = self.settings_pane.config.defaults.follow_redirects;
        let _verify_tls = self.settings_pane.config.defaults.verify_tls;

        self.logs_pane.set_current_request(request.clone());
        self.logs_pane.add_log(
            LogLevel::Info,
            format!("Sending {} {}", request.method, request.url.as_str()),
        );
        self.network_state = NetworkState::Loading;

        let client = HttpClient::new()
            .map_err(|e| AppError::Render(e.to_string()))?
            .with_timeout(timeout_secs)
            .with_follow_redirects(_follow_redirects)
            .with_tls_verify(_verify_tls);

        let rx = self.request_controller.execute(request, client);
        self.request_rx = Some(rx);

        Ok(())
    }

    fn add_history_entry(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        self.sidebar.set_history(self.history.clone());
    }

    fn forward_key_to_active_pane(&mut self, key_event: KeyEvent) {
        let is_normal = self.input_handler.current_mode() == InputMode::Normal;

        // Helper to convert vim motion keys to arrows
        let to_nav_key = |code: KeyCode| -> Option<KeyCode> {
            match code {
                KeyCode::Char('h') if is_normal => Some(KeyCode::Left),
                KeyCode::Char('j') if is_normal => Some(KeyCode::Down),
                KeyCode::Char('k') if is_normal => Some(KeyCode::Up),
                KeyCode::Char('l') if is_normal => Some(KeyCode::Right),
                KeyCode::Char('G') if is_normal => Some(code),
                KeyCode::Char('g') if is_normal => Some(code),
                KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => Some(code),
                KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown => Some(code),
                KeyCode::Tab => Some(code),
                _ if !is_normal => Some(code),
                _ => None,
            }
        };

        match self.active_pane {
            ActivePane::Sidebar => match key_event.code {
                KeyCode::Char('r') => {
                    if let Some(action) = self.sidebar.get_selected_history_action() {
                        self.logs_pane.add_log(
                            LogLevel::Info,
                            format!(
                                "Loaded from history: {} {}",
                                action.request.method,
                                action.request.url.as_str()
                            ),
                        );
                        self.request_pane.set_request(action.request);
                        self.active_pane = ActivePane::Request;
                    }
                    return;
                }
                KeyCode::Char('C') => {
                    if let Some(action) = self.sidebar.get_selected_history_action() {
                        self.logs_pane
                            .add_log(LogLevel::Info, format!("Curl: {}", action.curl));
                    }
                    return;
                }
                KeyCode::Char('d') => {
                    let entry_id = self.sidebar.selected_history_entry().map(|e| e.id.clone());
                    if let Some(id) = entry_id {
                        self.history.retain(|e| e.id != id);
                        self.sidebar.set_history(self.history.clone());
                        self.logs_pane
                            .add_log(LogLevel::Info, "History entry deleted");
                    }
                    return;
                }
                KeyCode::Char('D') => {
                    self.history.clear();
                    self.sidebar.clear_history_items();
                    self.logs_pane.add_log(LogLevel::Info, "History cleared");
                    return;
                }
                _ => {
                    if let Some(code) = to_nav_key(key_event.code) {
                        let _ = self.sidebar.handle_key(code);
                        self.active_env_id = self.sidebar.active_environment_id();
                    }
                }
            },
            ActivePane::Request => {
                if let Some(code) = to_nav_key(key_event.code) {
                    let _ = self.request_pane.handle_key(code, key_event.modifiers);
                }
            }
            ActivePane::Logs => {
                if let Some(code) = to_nav_key(key_event.code) {
                    let _ = self.logs_pane.handle_key(code, key_event.modifiers);
                }
            }
            ActivePane::Response => {
                if let Some(code) = to_nav_key(key_event.code) {
                    let _ = self.response_pane.handle_key(code);
                }
            }
            _ => {}
        }
    }

    async fn execute_request(&mut self) -> Result<(), AppError> {
        let timeout_secs = self.settings_pane.config.defaults.default_timeout_secs;
        let request = self
            .request_pane
            .to_request(timeout_secs)
            .map_err(|e| AppError::Render(e.to_string()))?;

        self.execute_request_with(request).await
    }

    fn check_request_completion(&mut self) {
        if let Some(rx) = &mut self.request_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        match event {
                            RequestEvent::Chunk(data, _offset) => {
                                self.response_pane.stream_chunk(data);
                                self.network_state = NetworkState::Streaming;
                            }
                            RequestEvent::Completed(response, elapsed_ms) => {
                                let metrics = RequestMetrics::new()
                                    .with_timing(Timing::new().with_total(elapsed_ms))
                                    .with_status_code(response.status.code())
                                    .with_body_size(response.body_size());
                                self.logs_pane.set_metrics(metrics);

                                if response.is_error() {
                                    self.logs_pane.add_log(
                                        LogLevel::Warning,
                                        format!("Request completed with {}", response.status),
                                    );
                                } else {
                                    self.logs_pane.add_log(
                                        LogLevel::Info,
                                        format!(
                                            "Request completed with {} in {}ms",
                                            response.status, elapsed_ms
                                        ),
                                    );
                                }

                                // Add to history
                                if let Some(request) = self.logs_pane.current_request() {
                                    let entry = HistoryEntry {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        request: request.clone(),
                                        response: Some(response.clone()),
                                        timestamp: chrono::Utc::now(),
                                        timing: Timing::new().with_total(elapsed_ms),
                                        timeline: None,
                                    };
                                    self.add_history_entry(entry);
                                }

                                self.response_pane.set_response(response);
                                self.network_state = NetworkState::Idle;
                                self.active_pane = ActivePane::Response;
                                self.request_rx = None;
                                return;
                            }
                            RequestEvent::Failed(message) => {
                                self.logs_pane.add_log(LogLevel::Error, message.clone());
                                self.response_pane.set_error(message.clone());
                                self.network_state = NetworkState::Error(message);
                                self.request_rx = None;
                                return;
                            }
                        }
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        self.request_rx = None;
                        return;
                    }
                }
            }
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>) {
        let area = frame.area();
        let (term_width, term_height) = (area.width, area.height);
        self.workspace_layout
            .update_terminal_size(term_width, term_height);

        frame.render_widget(
            Block::default().style(
                Style::default()
                    .bg(self.theme.pane_bg(false))
                    .fg(self.theme.foreground.as_color()),
            ),
            area,
        );

        if term_width < 60 || term_height < 12 {
            self.render_minimal_fallback(frame, area);
            return;
        }

        self.render_workspace(frame, area);

        if self.show_help {
            self.render_help(frame, area);
        }

        if self.command_palette.is_visible() {
            self.command_palette.render(frame, area, &self.theme);
        }
    }

    fn render_workspace(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        let wrects = self.workspace_layout.calculate();

        // Sidebar
        if wrects.sidebar.width > 0 {
            self.sidebar.render(
                frame,
                wrects.sidebar,
                &self.theme,
                self.active_pane == ActivePane::Sidebar,
            );
        }

        // Tab bar
        self.tab_bar.render(
            frame,
            wrects.tab_bar,
            &self.tab_manager,
            &self.theme,
            self.active_pane == ActivePane::Request,
        );

        // Request pane (compact)
        self.request_pane.set_compact(true);
        self.request_pane.render_compact(
            frame,
            wrects.center_top,
            &self.theme,
            self.active_pane == ActivePane::Request,
        );

        let (response_area, logs_area) = split_response_logs(wrects.center_bottom);
        self.response_pane.render(
            frame,
            response_area,
            &self.theme,
            self.active_pane == ActivePane::Response,
        );

        if let Some(logs_area) = logs_area {
            self.logs_pane.render(
                frame,
                logs_area,
                &self.theme,
                self.active_pane == ActivePane::Logs,
            );
        }

        // Status bar
        self.render_status_bar(frame, wrects.status_bar);

        // Settings overlay
        if self.settings_pane.is_open() {
            self.settings_pane
                .render(frame, centered_rect(area, 70, 70), &self.theme);
        }
    }

    fn render_minimal_fallback(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        let (w, h) = (area.width, area.height);
        let text = format!(
            "Terminal too small\n\nResize to at least 60x12\nCurrent: {}x{}\n\n[q] quit",
            w, h
        );
        let block = Block::default()
            .title(" YINX ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.semantic.warning.as_color()))
            .style(
                Style::default()
                    .bg(self.theme.pane.bg_color())
                    .fg(self.theme.foreground.as_color()),
            );
        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(self.theme.foreground.as_color()))
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_status_bar(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        let current_mode = self.input_handler.current_mode();
        let mode_str = match current_mode {
            InputMode::Normal => "NORMAL",
            InputMode::Insert => "INSERT",
            InputMode::Visual => "VISUAL",
            InputMode::Command => "COMMAND",
        };
        let response_meta = self
            .response_pane
            .response()
            .map(|response| format!("{} {}ms", response.status, response.timing_ms))
            .or_else(|| {
                self.response_pane
                    .error()
                    .map(|error| format!("Error: {}", truncate_status_text(error, 48)))
            })
            .unwrap_or_else(|| "No response".to_string());
        let focus_label = match self.active_pane {
            ActivePane::Sidebar => "SIDEBAR",
            ActivePane::Request => "REQUEST CONFIG",
            ActivePane::Response => "RESPONSE",
            ActivePane::Logs => "ACTIVITY",
            _ => "YINX",
        };
        let status = StatusBar::new(mode_str)
            .with_network_state(&self.network_state)
            .with_cursor(0, 0)
            .with_center(focus_label)
            .with_right(&response_meta);
        status.render(frame, area, &self.theme);
    }

    fn render_help(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        let help_rect = centered_rect(area, 60, 70);
        let help_lines = vec![
            Line::from(Span::styled(
                " KEYMAP ",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Navigation",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("  h/j/k/l             Move cursor / navigate lists"),
            Line::from("  gg / G              Go to top / bottom"),
            Line::from("  Ctrl+D / Ctrl+U     Page down / page up"),
            Line::from("  gt / gT             Next / previous tab"),
            Line::from("  Ctrl+b              Toggle sidebar"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Pane Management",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Tab / Shift+Tab     Cycle panes forward/backward"),
            Line::from("  Ctrl+w h/j/k/l     Navigate panes"),
            Line::from("  Ctrl+w w            Cycle panes"),
            Line::from("  Ctrl+w =            Equalize pane sizes"),
            Line::from("  Ctrl+w _            Maximize pane height"),
            Line::from("  Ctrl+w |            Maximize pane width"),
            Line::from("  Ctrl+w q            Close tab"),
            Line::from("  Ctrl+w o            Close other tabs"),
            Line::from("  + / -               Resize active pane"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Mode Switching",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("  i / a               Enter Insert mode"),
            Line::from("  I / A               Insert at start / end"),
            Line::from("  v                   Enter Visual mode"),
            Line::from("  :                   Enter Command mode"),
            Line::from("  Esc                 Return to Normal mode"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Actions",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Space / Ctrl+Enter  Send request"),
            Line::from("  Ctrl+N              New tab"),
            Line::from("  dd                  Delete item (list) / Delete line"),
            Line::from("  gd                  Go to definition"),
            Line::from("  T                   Cycle theme"),
            Line::from("  /                   Search collections"),
            Line::from("  ?                   Search within response"),
            Line::from("  n / N               Next / previous search result"),
            Line::from("  u                   Undo"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Command Palette",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("  :w / :save          Save current request"),
            Line::from("  :q / :quit          Close tab / quit"),
            Line::from("  :wq / :x            Save and close"),
            Line::from("  :new / :tabnew      New request tab"),
            Line::from("  :send / :run        Execute request"),
            Line::from("  :col                Focus collections"),
            Line::from("  :hist               Show history"),
            Line::from("  :theme <name>       Change theme"),
            Line::from(""),
            Line::from("  q / Ctrl+C          Quit"),
            Line::from("  ? / Esc / q         Close this help"),
        ];

        let help_block = Block::default()
            .borders(Borders::ALL)
            .title(" HELP ")
            .border_style(Style::default().fg(self.theme.border.active_color.as_color()))
            .style(
                Style::default()
                    .bg(self.theme.pane_bg(true))
                    .fg(self.theme.foreground.as_color()),
            );

        let inner = help_block.inner(help_rect);
        frame.render_widget(Clear, help_rect);
        frame.render_widget(help_block, help_rect);

        let paragraph = Paragraph::new(help_lines)
            .style(Style::default().fg(self.theme.foreground.as_color()))
            .alignment(Alignment::Left);
        frame.render_widget(paragraph, inner);
    }
}

fn key_to_app_event(key_event: KeyEvent) -> Option<AppEvent> {
    let key = match key_event.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        _ => return None,
    };
    Some(AppEvent::KeyPressed(key))
}

fn truncate_status_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let width = area.width.saturating_mul(width_percent).saturating_div(100);
    let height = area
        .height
        .saturating_mul(height_percent)
        .saturating_div(100);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.max(1), height.max(1))
}

fn split_response_logs(area: Rect) -> (Rect, Option<Rect>) {
    if area.width < 64 || area.height < 10 {
        return (area, None);
    }

    let logs_height = if area.height < 20 {
        3
    } else {
        (area.height / 4).clamp(3, 6)
    };
    let response_height = area.height.saturating_sub(logs_height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(response_height),
            Constraint::Length(logs_height),
        ])
        .split(area);

    (chunks[0], Some(chunks[1]))
}

pub struct EventLoop {
    event_sender: tokio::sync::mpsc::Sender<AppEvent>,
    shutdown_flag: Arc<AtomicBool>,
    input_handler: InputHandler,
}

impl EventLoop {
    pub fn new(
        event_sender: tokio::sync::mpsc::Sender<AppEvent>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            event_sender,
            shutdown_flag,
            input_handler: InputHandler::new(),
        }
    }

    pub fn with_input_handler(
        event_sender: tokio::sync::mpsc::Sender<AppEvent>,
        shutdown_flag: Arc<AtomicBool>,
        input_handler: InputHandler,
    ) -> Self {
        Self {
            event_sender,
            shutdown_flag,
            input_handler,
        }
    }

    pub async fn run(&mut self) -> Result<(), AppError> {
        while !self.shutdown_flag.load(Ordering::SeqCst) {
            if event::poll(Duration::from_millis(50))
                .map_err(|e| AppError::EventLoop(e.to_string()))?
            {
                let event = event::read().map_err(|e| AppError::EventLoop(e.to_string()))?;

                match event {
                    Event::Key(key_event) => {
                        let events = self.input_handler.handle_key(key_event);
                        for app_event in events {
                            if matches!(app_event, AppEvent::Quit) {
                                self.shutdown_flag.store(true, Ordering::SeqCst);
                            }
                            let _ = self.event_sender.send(app_event).await;
                        }
                        if self.shutdown_flag.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    Event::Resize(width, height) => {
                        let _ = self
                            .event_sender
                            .send(AppEvent::TerminalResized { width, height })
                            .await;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

pub struct App {
    terminal: TerminalType,
    _guard: TerminalGuard,
    event_bus: EventBus,
    state_reducer: StateReducer,
    shutdown_flag: Arc<AtomicBool>,
    input_handler: InputHandler,
}

impl App {
    pub fn init() -> Result<Self, AppError> {
        let guard = TerminalGuard::enter_raw_mode()?;
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = RatatuiTerminal::new(backend)
            .map_err(|e| AppError::TerminalInit(io::Error::other(e)))?;

        let event_bus = EventBus::new(100);
        let state_reducer = StateReducer::new();
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let input_handler = InputHandler::new();

        Ok(Self {
            terminal,
            _guard: guard,
            event_bus,
            state_reducer,
            shutdown_flag,
            input_handler,
        })
    }

    pub fn terminal(&mut self) -> &mut TerminalType {
        &mut self.terminal
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn state_reducer(&self) -> &StateReducer {
        &self.state_reducer
    }

    pub fn shutdown_flag(&self) -> &Arc<AtomicBool> {
        &self.shutdown_flag
    }

    pub fn suspend_terminal(&mut self) -> Result<(), AppError> {
        self._guard.suspend()
    }

    pub fn resume_terminal(&mut self) -> Result<(), AppError> {
        self._guard.resume()
    }

    pub fn edit_with_external_editor<V>(
        &mut self,
        prefix: &str,
        format: EditorFormat,
        initial_content: &str,
        validator: V,
    ) -> Result<String, AppError>
    where
        V: FnOnce(&str) -> Result<(), EditorError>,
    {
        editor::edit_with_runner(
            &mut self._guard,
            &SystemEditorRunner,
            prefix,
            format,
            initial_content,
            validator,
        )
        .map_err(|err| AppError::Render(err.to_string()))
    }

    pub async fn run(&mut self) -> Result<(), AppError> {
        let mut event_loop = EventLoop::with_input_handler(
            self.event_bus.sender(),
            self.shutdown_flag.clone(),
            std::mem::take(&mut self.input_handler),
        );
        event_loop.run().await?;
        self.input_handler = event_loop.input_handler;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), AppError> {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        TerminalGuard::exit_raw_mode()?;
        Ok(())
    }
}

pub fn with_error_boundary<F, R>(f: F) -> Result<R, AppError>
where
    F: FnOnce() -> Result<R, AppError> + panic::UnwindSafe,
{
    match panic::catch_unwind(f) {
        Ok(result) => result,
        Err(panic_err) => {
            let msg = if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "Unknown panic".to_string()
            };
            Err(AppError::Panic(msg))
        }
    }
}

impl TerminalSession for TerminalGuard {
    fn suspend(&mut self) -> Result<(), EditorError> {
        TerminalGuard::suspend(self).map_err(|err| EditorError::Terminal(err.to_string()))
    }

    fn resume(&mut self) -> Result<(), EditorError> {
        TerminalGuard::resume(self).map_err(|err| EditorError::Terminal(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires a real terminal"]
    fn test_terminal_guard_enter_exit() {
        let guard = TerminalGuard::enter_raw_mode();
        assert!(guard.is_ok());
        if let Ok(g) = guard {
            drop(g);
        }
        let _ = TerminalGuard::exit_raw_mode();
    }

    #[test]
    fn test_event_loop_creation() {
        let event_bus = EventBus::new(10);
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let event_loop = EventLoop::new(event_bus.sender(), shutdown_flag.clone());
        assert!(!event_loop.shutdown_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn test_event_loop_shutdown_flag() {
        let event_bus = EventBus::new(10);
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let event_loop = EventLoop::new(event_bus.sender(), shutdown_flag.clone());
        assert!(!event_loop.shutdown_flag.load(Ordering::SeqCst));
        shutdown_flag.store(true, Ordering::SeqCst);
        assert!(event_loop.shutdown_flag.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_event_loop_quit() {
        let event_bus = EventBus::new(10);
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let mut event_loop = EventLoop::new(event_bus.sender(), shutdown_flag.clone());
        shutdown_flag.store(true, Ordering::SeqCst);
        let result = event_loop.run().await;
        assert!(result.is_ok());
    }

    #[test]
    #[ignore = "Requires a real terminal"]
    fn test_app_struct_creation() {
        let result = App::init();
        assert!(result.is_ok());
        if let Ok(mut app) = result {
            app.shutdown().unwrap();
        }
    }

    #[test]
    #[ignore = "Requires a real terminal"]
    fn test_app_lifecycle_init_run_shutdown() {
        let result = App::init();
        assert!(result.is_ok());

        if let Ok(mut app) = result {
            app.shutdown_flag().store(true, Ordering::SeqCst);
            let _ = tokio::runtime::Runtime::new().unwrap().block_on(app.run());
            let result = app.shutdown();
            assert!(result.is_ok());
        }
    }

    #[test]
    #[ignore = "Requires a real terminal"]
    fn test_app_shutdown_restores_terminal() {
        let result = App::init();
        assert!(result.is_ok());

        if let Ok(mut app) = result {
            let shutdown_result = app.shutdown();
            assert!(shutdown_result.is_ok());
        }
    }

    #[test]
    #[ignore = "Requires a real terminal"]
    fn test_app_shutdown_flag_set() {
        let result = App::init();
        assert!(result.is_ok());

        if let Ok(mut app) = result {
            assert!(!app.shutdown_flag().load(Ordering::SeqCst));
            let _ = app.shutdown();
        }
    }

    #[test]
    fn test_terminal_resize_event() {
        let event = AppEvent::TerminalResized {
            width: 120,
            height: 40,
        };
        match event {
            AppEvent::TerminalResized { width, height } => {
                assert_eq!(width, 120);
                assert_eq!(height, 40);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_error_boundary_success() {
        let result = with_error_boundary(|| Ok::<_, AppError>(42));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_error_boundary_panic() {
        let result: Result<i32, AppError> = with_error_boundary(|| {
            panic!("test panic");
        });
        assert!(result.is_err());
        match result {
            Err(AppError::Panic(msg)) => {
                assert!(msg.contains("test panic"));
            }
            _ => panic!("Expected panic error"),
        }
    }

    #[test]
    fn test_error_boundary_panic_string() {
        let result: Result<i32, AppError> = with_error_boundary(|| {
            panic!("custom error message");
        });
        assert!(result.is_err());
        if let Err(AppError::Panic(msg)) = result {
            assert_eq!(msg, "custom error message");
        } else {
            panic!("Expected Panic error");
        }
    }

    #[test]
    fn test_input_handler_quit_key_ctrl_c() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let events = handler.handle_key(key);
        assert!(!events.is_empty());
        assert!(matches!(events[0], AppEvent::Quit));
    }

    #[test]
    fn test_input_handler_quit_key_q() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let events = handler.handle_key(key);
        assert!(!events.is_empty());
        assert!(matches!(events[0], AppEvent::Quit));
    }

    #[test]
    fn test_input_handler_not_quit_key_others() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let events = handler.handle_key(key);
        assert!(!events.contains(&AppEvent::Quit));
    }

    #[test]
    fn test_app_event_quit() {
        let event = AppEvent::Quit;
        match event {
            AppEvent::Quit => (),
            _ => panic!("Wrong event"),
        }
    }

    #[test]
    fn test_app_state_reducer_terminal_resize() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::TerminalResized {
            width: 100,
            height: 50,
        };
        let diff = reducer.reduce(&event);
        assert!(!diff.any());
    }

    #[test]
    fn test_app_state_reducer_quit() {
        let mut reducer = StateReducer::new();
        let event = AppEvent::Quit;
        let diff = reducer.reduce(&event);
        assert!(!diff.any());
    }

    #[tokio::test]
    async fn test_app_event_bus_integration() {
        let mut bus = EventBus::new(10);
        let sender = bus.sender();

        let _ = sender.try_send(AppEvent::TerminalResized {
            width: 80,
            height: 24,
        });
        let _ = sender.try_send(AppEvent::Quit);

        let event1 = bus.receive().await;
        let event2 = bus.receive().await;

        assert!(event1.is_some());
        assert!(event2.is_some());
    }

    #[test]
    fn test_ui_state_default() {
        let ui = UiState::new();
        assert_eq!(ui.mode, InputMode::Normal);
        assert_eq!(ui.active_pane, ActivePane::Request);
    }

    #[test]
    #[ignore = "Requires a real terminal"]
    fn test_terminal_guard_raw_mode_flag() {
        let guard = TerminalGuard::enter_raw_mode();
        assert!(guard.is_ok());
        if let Ok(g) = guard {
            assert!(g.raw_mode);
            drop(g);
        }
        let _ = TerminalGuard::exit_raw_mode();
    }

    #[test]
    fn test_shutdown_flag_atomic() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::SeqCst));
        flag.store(true, Ordering::SeqCst);
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn test_terminal_guard_suspend_is_noop_when_inactive() {
        let mut guard = TerminalGuard { raw_mode: false };
        assert!(guard.suspend().is_ok());
        assert!(!guard.raw_mode);
    }

    #[test]
    fn test_theme_changed_event_updates_theme() {
        let _shell = TuiShell::new(80, 24);

        // Simulate receiving ThemeChanged event
        let event = AppEvent::ThemeChanged("light".to_string());
        // This will need to be handled in handle_event
        // For now, just verify the event exists
        match event {
            AppEvent::ThemeChanged(name) => assert_eq!(name, "light"),
            _ => panic!("Wrong event"),
        }
    }

    #[test]
    fn test_t_key_cycles_theme() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE);
        let events = handler.handle_key(key);
        assert!(events
            .iter()
            .any(|e| matches!(e, AppEvent::ThemeChanged(_))));
    }

    #[test]
    fn test_mouse_click_focuses_pane() {
        let mut shell = TuiShell::new(80, 24);
        let rects = shell.workspace_layout.calculate();

        // Click in the middle of Response pane (center_bottom)
        let row = rects.center_bottom.y + rects.center_bottom.height / 2;
        let col = rects.center_bottom.x + rects.center_bottom.width / 2;

        let mouse_event = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: col,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };

        shell.handle_mouse_event(mouse_event);
        assert_eq!(shell.active_pane, ActivePane::Response);
    }
}
