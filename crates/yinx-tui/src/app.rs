use std::io;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{self, Event},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal as RatatuiTerminal;

use yinx_core::events::{AppEvent, EventBus, StateReducer};
#[allow(unused_imports)]
use yinx_core::state::{ActivePane, InputMode, UiState};

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

pub struct TerminalGuard {
    raw_mode: bool,
}

impl TerminalGuard {
    pub fn enter_raw_mode() -> Result<Self, AppError> {
        terminal::enable_raw_mode()
            .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
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
        terminal::disable_raw_mode()
            .map_err(|e| AppError::TerminalRestore(e.to_string()))?;
        Self::show_cursor()?;
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
        let mut event_loop =
            EventLoop::new(event_bus.sender(), shutdown_flag.clone());
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
            let _ = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(app.run());
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
}
