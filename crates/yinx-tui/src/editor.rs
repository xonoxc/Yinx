use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFormat {
    Text,
    Json,
    Yaml,
}

impl EditorFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Text => ".txt",
            Self::Json => ".json",
            Self::Yaml => ".yaml",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorRunResult {
    pub success: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Failed to launch editor: {0}")]
    Spawn(String),
    #[error("Editor exited without saving changes (code: {0:?})")]
    Aborted(Option<i32>),
    #[error("Edited content is invalid: {0}")]
    Validation(String),
    #[error("Terminal state error: {0}")]
    Terminal(String),
}

pub trait EditorRunner {
    fn run(&self, editor: &str, path: &Path) -> Result<EditorRunResult, EditorError>;
}

pub trait TerminalSession {
    fn suspend(&mut self) -> Result<(), EditorError>;
    fn resume(&mut self) -> Result<(), EditorError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEditorRunner;

impl EditorRunner for SystemEditorRunner {
    fn run(&self, editor: &str, path: &Path) -> Result<EditorRunResult, EditorError> {
        let escaped_path = shell_escape(path);
        let command = format!("{editor} {escaped_path}");
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()
            .map_err(|err| EditorError::Spawn(err.to_string()))?;

        Ok(EditorRunResult {
            success: status.success(),
            exit_code: status.code(),
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopTerminalSession;

impl TerminalSession for NoopTerminalSession {
    fn suspend(&mut self) -> Result<(), EditorError> {
        Ok(())
    }

    fn resume(&mut self) -> Result<(), EditorError> {
        Ok(())
    }
}

pub fn detect_editor() -> String {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|key| env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "vim".to_string())
}

pub fn create_temp_edit_file(
    prefix: &str,
    format: EditorFormat,
    content: &str,
) -> Result<PathBuf, EditorError> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sanitized_prefix = prefix
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let file_name = format!("yinx-{sanitized_prefix}-{unique}{}", format.extension());
    let path = env::temp_dir().join(file_name);
    fs::write(&path, content)?;
    Ok(path)
}

pub fn edit_with_runner<T, R, V>(
    terminal: &mut T,
    runner: &R,
    prefix: &str,
    format: EditorFormat,
    initial_content: &str,
    validator: V,
) -> Result<String, EditorError>
where
    T: TerminalSession,
    R: EditorRunner,
    V: FnOnce(&str) -> Result<(), EditorError>,
{
    let editor = detect_editor();
    let path = create_temp_edit_file(prefix, format, initial_content)?;

    terminal.suspend()?;
    let run_result = runner.run(&editor, &path);
    let resume_result = terminal.resume();

    if let Err(err) = resume_result {
        let _ = fs::remove_file(&path);
        return Err(err);
    }

    let run_result = run_result?;
    if !run_result.success {
        let _ = fs::remove_file(&path);
        return Err(EditorError::Aborted(run_result.exit_code));
    }

    let updated = fs::read_to_string(&path)?;
    let _ = fs::remove_file(&path);
    validator(&updated)?;
    Ok(updated)
}

fn shell_escape(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\'', r"'\''");
    format!("'{path}'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_editor_env<T>(visual: Option<&str>, editor: Option<&str>, f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().unwrap();
        let original_visual = env::var("VISUAL").ok();
        let original_editor = env::var("EDITOR").ok();

        unsafe {
            match visual {
                Some(value) => env::set_var("VISUAL", value),
                None => env::remove_var("VISUAL"),
            }
            match editor {
                Some(value) => env::set_var("EDITOR", value),
                None => env::remove_var("EDITOR"),
            }
        }

        let result = f();

        unsafe {
            match original_visual {
                Some(value) => env::set_var("VISUAL", value),
                None => env::remove_var("VISUAL"),
            }
            match original_editor {
                Some(value) => env::set_var("EDITOR", value),
                None => env::remove_var("EDITOR"),
            }
        }

        result
    }

    struct RecordingTerminal {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_resume: bool,
    }

    impl RecordingTerminal {
        fn new(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self {
                events,
                fail_resume: false,
            }
        }

        fn with_resume_failure(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self {
                events,
                fail_resume: true,
            }
        }
    }

    impl TerminalSession for RecordingTerminal {
        fn suspend(&mut self) -> Result<(), EditorError> {
            self.events.lock().unwrap().push("suspend");
            Ok(())
        }

        fn resume(&mut self) -> Result<(), EditorError> {
            self.events.lock().unwrap().push("resume");
            if self.fail_resume {
                Err(EditorError::Terminal("resume failed".to_string()))
            } else {
                Ok(())
            }
        }
    }

    struct WriteRunner {
        replacement: String,
    }

    impl EditorRunner for WriteRunner {
        fn run(&self, _editor: &str, path: &Path) -> Result<EditorRunResult, EditorError> {
            fs::write(path, &self.replacement)?;
            Ok(EditorRunResult {
                success: true,
                exit_code: Some(0),
            })
        }
    }

    struct AbortRunner;

    impl EditorRunner for AbortRunner {
        fn run(&self, _editor: &str, _path: &Path) -> Result<EditorRunResult, EditorError> {
            Ok(EditorRunResult {
                success: false,
                exit_code: Some(7),
            })
        }
    }

    #[test]
    fn test_detect_editor_prefers_visual() {
        with_editor_env(Some("nano"), Some("vim"), || {
            assert_eq!(detect_editor(), "nano");
        });
    }

    #[test]
    fn test_detect_editor_falls_back_to_vim() {
        with_editor_env(None, None, || {
            assert_eq!(detect_editor(), "vim");
        });
    }

    #[test]
    fn test_temp_file_creation_uses_requested_extension() {
        let text_path = create_temp_edit_file("url", EditorFormat::Text, "hello").unwrap();
        let json_path = create_temp_edit_file("body", EditorFormat::Json, "{}").unwrap();
        let yaml_path = create_temp_edit_file("headers", EditorFormat::Yaml, "[]").unwrap();

        assert_eq!(
            text_path.extension().and_then(|ext| ext.to_str()),
            Some("txt")
        );
        assert_eq!(
            json_path.extension().and_then(|ext| ext.to_str()),
            Some("json")
        );
        assert_eq!(
            yaml_path.extension().and_then(|ext| ext.to_str()),
            Some("yaml")
        );

        fs::remove_file(text_path).unwrap();
        fs::remove_file(json_path).unwrap();
        fs::remove_file(yaml_path).unwrap();
    }

    #[test]
    fn test_edit_with_runner_suspends_and_resumes_terminal() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = RecordingTerminal::new(events.clone());
        let runner = WriteRunner {
            replacement: "https://example.com\n".to_string(),
        };

        let edited = edit_with_runner(
            &mut terminal,
            &runner,
            "url",
            EditorFormat::Text,
            "https://before.example.com",
            |_| Ok(()),
        )
        .unwrap();

        assert_eq!(edited, "https://example.com\n");
        assert_eq!(&*events.lock().unwrap(), &["suspend", "resume"]);
    }

    #[test]
    fn test_edit_with_runner_returns_validation_errors() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = RecordingTerminal::new(events);
        let runner = WriteRunner {
            replacement: "{invalid json}".to_string(),
        };

        let result = edit_with_runner(
            &mut terminal,
            &runner,
            "body",
            EditorFormat::Json,
            "{}",
            |content| {
                serde_json::from_str::<serde_json::Value>(content)
                    .map(|_| ())
                    .map_err(|err| EditorError::Validation(err.to_string()))
            },
        );

        assert!(matches!(result, Err(EditorError::Validation(_))));
    }

    #[test]
    fn test_edit_with_runner_discards_changes_on_abort() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = RecordingTerminal::new(events);
        let result = edit_with_runner(
            &mut terminal,
            &AbortRunner,
            "url",
            EditorFormat::Text,
            "https://example.com",
            |_| Ok(()),
        );

        assert!(matches!(result, Err(EditorError::Aborted(Some(7)))));
    }

    #[test]
    fn test_edit_with_runner_surfaces_resume_failures() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = RecordingTerminal::with_resume_failure(events);
        let runner = WriteRunner {
            replacement: "new".to_string(),
        };

        let result = edit_with_runner(
            &mut terminal,
            &runner,
            "body",
            EditorFormat::Text,
            "old",
            |_| Ok(()),
        );

        assert!(matches!(result, Err(EditorError::Terminal(msg)) if msg == "resume failed"));
    }
}
