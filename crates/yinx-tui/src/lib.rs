pub mod app;

pub use app::{App, AppError, EventLoop, TerminalGuard, with_error_boundary};

pub mod theme;
pub mod widgets;
