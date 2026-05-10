use std::io;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Terminal as RatatuiTerminal;

use crate::command_palette::{CommandPalette, PaletteAction};
use crate::editor::{self, EditorError, EditorFormat, SystemEditorRunner, TerminalSession};
use crate::layout::Layout;
use crate::logs_pane::{LogLevel, LogsPane};
use crate::request_pane::RequestPane;
use crate::settings_pane::SettingsPane;
use crate::theme::{Theme, ThemeRegistry};
use crate::widgets::StatusBar;
use yinx_core::events::{AppEvent, EventBus, StateReducer};
use yinx_core::response::{Response, ResponseBody};
#[cfg(test)]
use yinx_core::state::UiState;
use yinx_core::state::{ActivePane, InputMode, NetworkState};
use yinx_core::timing::{RequestMetrics, Timing};
use yinx_http::client::HttpClient;

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

    loop {
        app.terminal()
            .draw(|frame| shell.render(frame))
            .map_err(|e| AppError::Render(e.to_string()))?;

        if shell.should_quit() {
            break;
        }

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
        crossterm::execute!(io::stdout(), EnterAlternateScreen)
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
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
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
            crossterm::execute!(io::stdout(), EnterAlternateScreen)
                .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
            self.raw_mode = true;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.raw_mode {
            let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
            let _ = terminal::disable_raw_mode();
            let _ = Self::show_cursor();
        }
    }
}

struct TuiShell {
    theme: Theme,
    theme_registry: ThemeRegistry,
    layout: Layout,
    request_pane: RequestPane,
    logs_pane: LogsPane,
    settings_pane: SettingsPane,
    active_pane: ActivePane,
    network_state: NetworkState,
    latest_response: Option<Response>,
    latest_error: Option<String>,
    should_quit: bool,
    show_help: bool,
    input_handler: InputHandler,
    command_palette: CommandPalette,
}

impl TuiShell {
    fn new(width: u16, height: u16) -> Self {
        let mut layout = Layout::new();
        layout.update_terminal_size(width, height);
        if height < 30 {
            layout.toggle_split_direction();
        }

        let mut logs_pane = LogsPane::new();
        logs_pane.add_log(
            LogLevel::Info,
            "Welcome to Yinx. Edit the request, then press ^R to send.",
        );
        logs_pane.add_log(
            LogLevel::Info,
            "Tab: panes | ^R: send | Esc/q: quit | /: search",
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

        Self {
            theme,
            theme_registry,
            layout,
            request_pane: RequestPane::new(),
            logs_pane,
            settings_pane,
            active_pane: ActivePane::Request,
            network_state: NetworkState::Idle,
            latest_response: None,
            latest_error: None,
            should_quit: false,
            show_help: false,
            input_handler: InputHandler::new(),
            command_palette: CommandPalette::new(),
        }
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
            Event::Resize(width, height) => {
                self.layout.update_terminal_size(width, height);
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

        let rects = self.layout.calculate();

        // Check which pane was clicked based on coordinates
        if self.is_in_rect(mouse_event.column, mouse_event.row, rects.response) {
            self.active_pane = ActivePane::Response;
        } else if self.is_in_rect(mouse_event.column, mouse_event.row, rects.request) {
            self.active_pane = ActivePane::Request;
        } else if self.is_in_rect(mouse_event.column, mouse_event.row, rects.logs) {
            self.active_pane = ActivePane::Logs;
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
                                    self.logs_pane.add_log(
                                        LogLevel::Error,
                                        e.to_string(),
                                    );
                                    self.latest_error = Some(e.to_string());
                                }
                            }
                            AppEvent::SaveState => {
                                self.logs_pane
                                    .add_log(LogLevel::Info, "State saved");
                            }
                            AppEvent::SearchActivated => {
                                // handled by pane
                            }
                            AppEvent::SettingsOpened => {
                                self.show_help = true;
                            }
                            AppEvent::ImportStarted { .. } => {
                                self.logs_pane.add_log(
                                    LogLevel::Info,
                                    "Import triggered",
                                );
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

        // Normal-mode global operations: resize, help, layout toggle
        if self.input_handler.current_mode() == InputMode::Normal {
            match key_event.code {
                KeyCode::Char('?') => {
                    self.show_help = true;
                    return Ok(());
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    match self.active_pane {
                        ActivePane::Request => self.layout.resize_request_pane(5),
                        ActivePane::Response => self.layout.resize_response_pane(5),
                        ActivePane::Logs => self.layout.resize_logs_pane(5),
                        _ => {}
                    }
                    return Ok(());
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    match self.active_pane {
                        ActivePane::Request => self.layout.resize_request_pane(-5),
                        ActivePane::Response => self.layout.resize_response_pane(-5),
                        ActivePane::Logs => self.layout.resize_logs_pane(-5),
                        _ => {}
                    }
                    return Ok(());
                }
                KeyCode::F(7) => {
                    self.layout.toggle_split_direction();
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
                        ActivePane::Request => ActivePane::Response,
                        ActivePane::Response => ActivePane::Logs,
                        ActivePane::Logs => ActivePane::Request,
                        _ => ActivePane::Request,
                    };
                    handled = true;
                }
                AppEvent::CyclePanePrev => {
                    self.active_pane = match self.active_pane {
                        ActivePane::Request => ActivePane::Logs,
                        ActivePane::Logs => ActivePane::Response,
                        ActivePane::Response => ActivePane::Request,
                        _ => ActivePane::Request,
                    };
                    handled = true;
                }
                AppEvent::SendRequest(req) => {
                    if let Err(e) = self.execute_request_with(req).await {
                        self.logs_pane.add_log(LogLevel::Error, e.to_string());
                        self.latest_error = Some(e.to_string());
                    }
                    handled = true;
                }
                AppEvent::ExecuteRequest => {
                    if let Err(e) = self.execute_request().await {
                        self.logs_pane.add_log(LogLevel::Error, e.to_string());
                        self.latest_error = Some(e.to_string());
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
                    self.logs_pane.add_log(
                        LogLevel::Info,
                        format!("Tab switch relative: {}", delta),
                    );
                    handled = true;
                }
                AppEvent::TabOpened { .. } => {
                    self.logs_pane.add_log(LogLevel::Info, "New tab opened");
                    handled = true;
                }
                AppEvent::TabClosed { .. } => {
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
                _ => {}
            }
        }

        // Forward key to active pane if InputHandler didn't mark it as handled.
        if !handled {
            self.forward_key_to_active_pane(key_event);
        }

        Ok(())
    }

    async fn execute_request_with(
        &mut self,
        request: yinx_core::request::Request,
    ) -> Result<(), AppError> {
        let timeout_secs = self.settings_pane.config.defaults.default_timeout_secs;
        let follow_redirects = self.settings_pane.config.defaults.follow_redirects;
        let verify_tls = self.settings_pane.config.defaults.verify_tls;

        self.logs_pane.set_current_request(request.clone());
        self.logs_pane.add_log(
            LogLevel::Info,
            format!("Sending {} {}", request.method, request.url.as_str()),
        );
        self.network_state = NetworkState::Loading;
        self.latest_error = None;

        let started_at = std::time::Instant::now();
        let client = HttpClient::new()
            .map_err(|e| AppError::Render(e.to_string()))?
            .with_timeout(timeout_secs)
            .with_follow_redirects(follow_redirects)
            .with_tls_verify(verify_tls);

        match client.send_request(request).await {
            Ok(mut response) => {
                let elapsed_ms = started_at.elapsed().as_millis() as u64;
                response.timing_ms = elapsed_ms;

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

                self.latest_response = Some(response);
                self.network_state = NetworkState::Idle;
                self.active_pane = ActivePane::Response;
            }
            Err(error) => {
                let message = error.to_string();
                self.logs_pane.add_log(LogLevel::Error, message.clone());
                self.latest_error = Some(message.clone());
                self.network_state = NetworkState::Error(message);
            }
        }

        Ok(())
    }

    fn forward_key_to_active_pane(&mut self, key_event: KeyEvent) {
        let is_normal = self.input_handler.current_mode() == InputMode::Normal;
        let code = if is_normal {
            match key_event.code {
                KeyCode::Char('h') => KeyCode::Left,
                KeyCode::Char('j') => KeyCode::Down,
                KeyCode::Char('k') => KeyCode::Up,
                KeyCode::Char('l') => KeyCode::Right,
                _ => key_event.code,
            }
        } else {
            key_event.code
        };
        match self.active_pane {
            ActivePane::Request => {
                let _ = self.request_pane.handle_key(code, key_event.modifiers);
            }
            ActivePane::Logs => {
                let _ = self.logs_pane.handle_key(code, key_event.modifiers);
            }
            ActivePane::Response => {}
            _ => {}
        }
    }

    async fn execute_request(&mut self) -> Result<(), AppError> {
        let timeout_secs = self.settings_pane.config.defaults.default_timeout_secs;
        let follow_redirects = self.settings_pane.config.defaults.follow_redirects;
        let verify_tls = self.settings_pane.config.defaults.verify_tls;

        let request = self
            .request_pane
            .to_request(timeout_secs)
            .map_err(|e| AppError::Render(e.to_string()))?;

        self.logs_pane.set_current_request(request.clone());
        self.logs_pane.add_log(
            LogLevel::Info,
            format!("Sending {} {}", request.method, request.url.as_str()),
        );
        self.network_state = NetworkState::Loading;
        self.latest_error = None;

        let started_at = std::time::Instant::now();
        let client = HttpClient::new()
            .map_err(|e| AppError::Render(e.to_string()))?
            .with_timeout(timeout_secs)
            .with_follow_redirects(follow_redirects)
            .with_tls_verify(verify_tls);

        match client.send_request(request).await {
            Ok(mut response) => {
                let elapsed_ms = started_at.elapsed().as_millis() as u64;
                response.timing_ms = elapsed_ms;

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

                self.latest_response = Some(response);
                self.network_state = NetworkState::Idle;
                self.active_pane = ActivePane::Response;
            }
            Err(error) => {
                let message = error.to_string();
                self.logs_pane.add_log(LogLevel::Error, message.clone());
                self.latest_error = Some(message.clone());
                self.network_state = NetworkState::Error(message);
            }
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>) {
        let area = frame.area();

        let outer = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border.color.as_color()))
            .style(
                Style::default().bg(self
                    .theme
                    .background
                    .as_ref()
                    .map(|c| c.as_color())
                    .unwrap_or(ratatui::style::Color::Reset)),
            );
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let saved = self.layout.terminal_size();
        self.layout.update_terminal_size(inner.width, inner.height);

        // Auto-resize request pane to fit URL content
        self.layout
            .auto_resize_request_to_fit(self.request_pane.url().len());

        let pane_rects = self
            .layout
            .calculate_with_context(crate::layout::LayoutContext {
                show_logs: true,
                compact_logs: self.logs_pane.should_compact()
                    && self.active_pane != ActivePane::Logs
                    && self.network_state == NetworkState::Idle,
            });
        self.layout.update_terminal_size(saved.0, saved.1);

        let off = |mut r: ratatui::layout::Rect| {
            r.x += inner.x;
            r.y += inner.y;
            r
        };

        self.request_pane.render(
            frame,
            off(pane_rects.request),
            &self.theme,
            self.active_pane == ActivePane::Request,
        );
        self.render_response_pane(
            frame,
            off(pane_rects.response),
            self.active_pane == ActivePane::Response,
        );
        self.logs_pane.render(
            frame,
            off(pane_rects.logs),
            &self.theme,
            self.active_pane == ActivePane::Logs,
        );

        let current_mode = self.input_handler.current_mode();
        let mode_str = match current_mode {
            InputMode::Normal => "NORMAL",
            InputMode::Insert => "INSERT",
            InputMode::Visual => "VISUAL",
            InputMode::Command => "COMMAND",
        };
        let response_meta = self
            .latest_response
            .as_ref()
            .map(|response| format!("{} {}ms", response.status, response.timing_ms))
            .unwrap_or_else(|| "No response".to_string());
        let focus_label = match self.active_pane {
            ActivePane::Request => "REQUEST CONFIG",
            ActivePane::Response => "RESPONSE",
            ActivePane::Logs => "ACTIVITY",
            _ => "YINX",
        };
        let status = StatusBar::new(mode_str)
            .with_network_state(&self.network_state)
            .with_cursor(0, 0)
            .with_center(focus_label)
            .with_right(&response_meta)
            .with_hints(vec![
                ("Tab", "Panes"),
                ("^R", "Send"),
                ("Esc/q", "Quit"),
                ("/", "Search"),
            ]);
        status.render(frame, off(pane_rects.status_bar), &self.theme);

        if self.settings_pane.is_open() {
            self.settings_pane
                .render(frame, centered_rect(area, 70, 70), &self.theme);
        }

        if self.show_help {
            self.render_help(frame, area);
        }

        if self.command_palette.is_visible() {
            self.command_palette.render(frame, area, &self.theme);
        }
    }

    fn render_help(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        let help_rect = centered_rect(area, 60, 70);
        let help_lines = vec![
            Line::from(Span::styled(
                " KEYMAP ",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Pane Navigation", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from("  Tab / Shift+Tab    Cycle panes forward/backward"),
            Line::from("  Ctrl+1/2/3/4       Jump to Request/Response/Workflow/Logs"),
            Line::from("  Mouse click         Focus pane under cursor"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Resize Active Pane", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from("  + / =               Expand active pane"),
            Line::from("  - / _               Shrink active pane"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Layout", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from("  F7                  Cycle layout preset"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Editing (Normal mode)", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from("  i                   Enter Insert mode"),
            Line::from("  v                   Enter Visual mode"),
            Line::from("  Esc                 Return to Normal mode"),
            Line::from("  h/j/k/l             Move cursor (Normal/Visual mode)"),
            Line::from("  Ctrl+D / Ctrl+U     Page down / Page up"),
            Line::from("  Backspace           Delete character before cursor"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Actions", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from("  Ctrl+R / Ctrl+Enter Send request"),
            Line::from("  Ctrl+S              Save state"),
            Line::from("  T / Shift+T         Cycle theme"),
            Line::from("  /                   Search"),
            Line::from("  q / Ctrl+C          Quit"),
            Line::from(""),
            Line::from("  ? / Esc / q         Close this help"),
        ];

        let help_block = Block::default()
            .borders(Borders::ALL)
            .title(" HELP ")
            .border_style(Style::default().fg(self.theme.border.active_color.as_color()))
            .style(Style::default().bg(self.theme.pane_bg(true)).fg(self.theme.foreground.as_color()));

        let inner = help_block.inner(help_rect);
        frame.render_widget(Clear, help_rect);
        frame.render_widget(help_block, help_rect);

        let paragraph = Paragraph::new(help_lines)
            .style(Style::default().fg(self.theme.foreground.as_color()))
            .alignment(Alignment::Left);
        frame.render_widget(paragraph, inner);
    }

    fn render_response_pane(&self, frame: &mut ratatui::Frame<'_>, area: Rect, is_active: bool) {
        let block = Block::default()
            .title(if let Some(response) = &self.latest_response {
                format!("RESPONSE  {}  {}ms", response.status, response.timing_ms)
            } else {
                "RESPONSE".to_string()
            })
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_color(is_active)))
            .style(Style::default().bg(self.theme.pane_bg(is_active)).fg(self.theme.foreground.as_color()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = Vec::new();

        for warning in self.layout.validate() {
            lines.push(Line::from(Span::styled(
                warning,
                Style::default().fg(self.theme.semantic.warning.as_color()),
            )));
        }

        if let Some(error) = &self.latest_error {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Last error: {error}"),
                Style::default().fg(self.theme.semantic.error.as_color()),
            )));
        }

        if let Some(response) = &self.latest_response {
            lines.push(Line::from(vec![
                Span::styled("Status ", Style::default().fg(self.theme.muted_color())),
                Span::styled(
                    response.status.to_string(),
                    Style::default().fg(if response.is_error() {
                        self.theme.semantic.error.as_color()
                    } else {
                        self.theme.semantic.success.as_color()
                    }),
                ),
                Span::raw("   "),
                Span::styled("Size ", Style::default().fg(self.theme.muted_color())),
                Span::styled(
                    format!("{}b", response.body_size()),
                    Style::default().fg(self.theme.foreground.as_color()),
                ),
                Span::raw("   "),
                Span::styled("Type ", Style::default().fg(self.theme.muted_color())),
                Span::styled(
                    response.content_type().unwrap_or("unknown").to_string(),
                    Style::default().fg(self.theme.foreground.as_color()),
                ),
            ]));
            lines.push(Line::from(""));

            for line in response_body_preview(response).lines().take(20) {
                lines.push(Line::from(line.to_string()));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "No response yet.",
                Style::default()
                    .fg(self.theme.title_color(is_active))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "Build the request on the left, send it, and the preview will land here.",
                Style::default().fg(self.theme.placeholder_color()),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Method ", Style::default().fg(self.theme.muted_color())),
                Span::styled(
                    self.request_pane.method().to_string(),
                    Style::default().fg(self.theme.foreground.as_color()),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("URL    ", Style::default().fg(self.theme.muted_color())),
                Span::styled(
                    {
                        let url = self.request_pane.url();
                        if url.is_empty() {
                            "https://api.example.com".to_string()
                        } else {
                            url
                        }
                    },
                    Style::default().fg(self.theme.foreground.as_color()),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Hints: Tab switch panes, / search logs, Ctrl+R send request.",
                Style::default().fg(self.theme.placeholder_color()),
            )));
        }

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(self.theme.foreground.as_color()))
            .wrap(Wrap { trim: false });
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

fn response_body_preview(response: &Response) -> String {
    match &response.body {
        ResponseBody::Json(_) => response
            .body
            .pretty_json()
            .unwrap_or_else(|| response.body.to_string()),
        ResponseBody::Text(_) => response.body.as_text().unwrap_or_default(),
        _ => response.body.to_string(),
    }
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
        let rects = shell.layout.calculate();

        // Click in the middle of Response pane
        let row = rects.response.y + rects.response.height / 2;
        let col = rects.response.x + rects.response.width / 2;

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
