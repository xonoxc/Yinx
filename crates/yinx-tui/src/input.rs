use std::collections::HashMap;
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

use yinx_core::events::AppEvent;
use yinx_core::state::InputMode;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct KeyBindingKey {
    key: String,
    modifiers: Vec<String>,
}

impl From<&KeyBinding> for KeyBindingKey {
    fn from(kb: &KeyBinding) -> Self {
        Self {
            key: kb.key.clone(),
            modifiers: kb.modifiers.clone(),
        }
    }
}

impl From<KeyBindingKey> for KeyBinding {
    fn from(kbk: KeyBindingKey) -> Self {
        Self {
            key: kbk.key,
            modifiers: kbk.modifiers,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAction {
    Quit,
    SwitchPaneRequest,
    SwitchPaneResponse,
    SwitchPaneWorkflow,
    SwitchPaneLogs,
    ModeInsert,
    ModeNormal,
    ModeVisual,
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    CursorTop,
    CursorBottom,
    PageUp,
    PageDown,
    ScrollUp,
    ScrollDown,
    DeleteChar,
    DeleteWord,
    SendRequest,
    Save,
    Cancel,
    Unknown,
}

impl fmt::Display for KeyAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            KeyAction::Quit => "quit",
            KeyAction::SwitchPaneRequest => "switch_pane_request",
            KeyAction::SwitchPaneResponse => "switch_pane_response",
            KeyAction::SwitchPaneWorkflow => "switch_pane_workflow",
            KeyAction::SwitchPaneLogs => "switch_pane_logs",
            KeyAction::ModeInsert => "mode_insert",
            KeyAction::ModeNormal => "mode_normal",
            KeyAction::ModeVisual => "mode_visual",
            KeyAction::CursorUp => "cursor_up",
            KeyAction::CursorDown => "cursor_down",
            KeyAction::CursorLeft => "cursor_left",
            KeyAction::CursorRight => "cursor_right",
            KeyAction::CursorTop => "cursor_top",
            KeyAction::CursorBottom => "cursor_bottom",
            KeyAction::PageUp => "page_up",
            KeyAction::PageDown => "page_down",
            KeyAction::ScrollUp => "scroll_up",
            KeyAction::ScrollDown => "scroll_down",
            KeyAction::DeleteChar => "delete_char",
            KeyAction::DeleteWord => "delete_word",
            KeyAction::SendRequest => "send_request",
            KeyAction::Save => "save",
            KeyAction::Cancel => "cancel",
            KeyAction::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub modifiers: Vec<String>,
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.is_empty() {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}+{}", self.modifiers.join("+"), self.key)
        }
    }
}

impl KeyBinding {
    pub fn new(key: &str, modifiers: &[&str]) -> Self {
        Self {
            key: key.to_string(),
            modifiers: modifiers.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn from_key_event(event: &KeyEvent) -> Self {
        let mut modifiers = Vec::new();

        if event.modifiers.contains(KeyModifiers::CONTROL) {
            modifiers.push("Ctrl".to_string());
        }
        if event.modifiers.contains(KeyModifiers::ALT) {
            modifiers.push("Alt".to_string());
        }
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            modifiers.push("Shift".to_string());
        }

        let key = match event.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::F(n) => format!("F{}", n),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            _ => format!("{:?}", event.code),
        };

        Self { key, modifiers }
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        let event_binding = Self::from_key_event(event);
        self == &event_binding
    }
}

#[derive(Debug, Clone, Default)]
pub struct KeyBindingConfig {
    pub bindings: HashMap<KeyBinding, KeyAction>,
}

#[derive(Serialize, Deserialize)]
struct SerializableBinding {
    key: String,
    modifiers: Vec<String>,
    action: KeyAction,
}

impl Serialize for KeyBindingConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.bindings.len()))?;
        for (kb, action) in &self.bindings {
            let binding = SerializableBinding {
                key: kb.key.clone(),
                modifiers: kb.modifiers.clone(),
                action: action.clone(),
            };
            seq.serialize_element(&binding)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for KeyBindingConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<SerializableBinding> = Vec::deserialize(deserializer)?;
        let bindings = vec
            .into_iter()
            .map(|sb| {
                (
                    KeyBinding {
                        key: sb.key,
                        modifiers: sb.modifiers,
                    },
                    sb.action,
                )
            })
            .collect();
        Ok(Self { bindings })
    }
}

impl KeyBindingConfig {
    pub fn default_bindings() -> Self {
        let mut bindings = HashMap::new();

        // Quit
        bindings.insert(KeyBinding::new("c", &["Ctrl"]), KeyAction::Quit);
        bindings.insert(KeyBinding::new("q", &[]), KeyAction::Quit);

        // Pane switching
        bindings.insert(KeyBinding::new("1", &[]), KeyAction::SwitchPaneRequest);
        bindings.insert(KeyBinding::new("2", &[]), KeyAction::SwitchPaneResponse);
        bindings.insert(KeyBinding::new("3", &[]), KeyAction::SwitchPaneWorkflow);
        bindings.insert(KeyBinding::new("4", &[]), KeyAction::SwitchPaneLogs);

        // Mode switching
        bindings.insert(KeyBinding::new("i", &[]), KeyAction::ModeInsert);
        bindings.insert(KeyBinding::new("v", &[]), KeyAction::ModeVisual);
        bindings.insert(KeyBinding::new("Esc", &[]), KeyAction::ModeNormal);

        // Vim-style navigation
        bindings.insert(KeyBinding::new("h", &[]), KeyAction::CursorLeft);
        bindings.insert(KeyBinding::new("j", &[]), KeyAction::CursorDown);
        bindings.insert(KeyBinding::new("k", &[]), KeyAction::CursorUp);
        bindings.insert(KeyBinding::new("l", &[]), KeyAction::CursorRight);
        // 'g' is handled specially for 'gg' (go to top)
        bindings.insert(KeyBinding::new("G", &[]), KeyAction::CursorBottom);
        bindings.insert(KeyBinding::new("d", &["Ctrl"]), KeyAction::PageDown);
        bindings.insert(KeyBinding::new("u", &["Ctrl"]), KeyAction::PageUp);

        // Scrolling
        bindings.insert(KeyBinding::new("Up", &[]), KeyAction::ScrollUp);
        bindings.insert(KeyBinding::new("Down", &[]), KeyAction::ScrollDown);
        bindings.insert(KeyBinding::new("PageUp", &[]), KeyAction::PageUp);
        bindings.insert(KeyBinding::new("PageDown", &[]), KeyAction::PageDown);

        // Editing
        bindings.insert(KeyBinding::new("Backspace", &[]), KeyAction::DeleteChar);
        bindings.insert(KeyBinding::new("w", &["Ctrl"]), KeyAction::DeleteWord);

        // Actions
        bindings.insert(KeyBinding::new("s", &["Ctrl"]), KeyAction::Save);
        bindings.insert(KeyBinding::new("Enter", &[]), KeyAction::SendRequest);
        bindings.insert(KeyBinding::new("c", &["Ctrl"]), KeyAction::Quit);

        Self { bindings }
    }

    pub fn get_action(&self, event: &KeyEvent) -> KeyAction {
        let binding = KeyBinding::from_key_event(event);
        self.bindings
            .get(&binding)
            .cloned()
            .unwrap_or(KeyAction::Unknown)
    }

    pub fn set_binding(&mut self, key: &str, modifiers: &[&str], action: KeyAction) {
        self.bindings
            .insert(KeyBinding::new(key, modifiers), action);
    }

    pub fn remove_binding(&mut self, key: &str, modifiers: &[&str]) {
        self.bindings.remove(&KeyBinding::new(key, modifiers));
    }
}

pub struct InputHandler {
    pub config: KeyBindingConfig,
    pub mode: InputMode,
    pub pending_key: Option<char>,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            config: KeyBindingConfig::default_bindings(),
            mode: InputMode::Normal,
            pending_key: None,
        }
    }

    pub fn with_config(config: KeyBindingConfig) -> Self {
        Self {
            config,
            mode: InputMode::Normal,
            pending_key: None,
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) -> Vec<AppEvent> {
        match self.mode {
            InputMode::Normal => self.handle_normal_mode(event),
            InputMode::Insert => self.handle_insert_mode(event),
            InputMode::Visual => self.handle_visual_mode(event),
        }
    }

    fn handle_normal_mode(&mut self, event: KeyEvent) -> Vec<AppEvent> {
        let mut events = Vec::new();
        let action = self.config.get_action(&event);

        match action {
            KeyAction::Quit => {
                events.push(AppEvent::Quit);
            }
            KeyAction::ModeInsert => {
                self.mode = InputMode::Insert;
                events.push(AppEvent::ModeChanged(InputMode::Insert));
            }
            KeyAction::ModeVisual => {
                self.mode = InputMode::Visual;
                events.push(AppEvent::ModeChanged(InputMode::Visual));
            }
            KeyAction::CursorUp => {
                events.push(AppEvent::CursorMoved { lines: -1, cols: 0 });
            }
            KeyAction::CursorDown => {
                events.push(AppEvent::CursorMoved { lines: 1, cols: 0 });
            }
            KeyAction::CursorLeft => {
                events.push(AppEvent::CursorMoved { lines: 0, cols: -1 });
            }
            KeyAction::CursorRight => {
                events.push(AppEvent::CursorMoved { lines: 0, cols: 1 });
            }
            KeyAction::CursorTop => {
                events.push(AppEvent::CursorMoved {
                    lines: i64::MIN,
                    cols: 0,
                });
            }
            KeyAction::CursorBottom => {
                events.push(AppEvent::CursorMoved {
                    lines: i64::MAX,
                    cols: 0,
                });
            }
            KeyAction::PageUp => {
                events.push(AppEvent::Scrolled(-20));
            }
            KeyAction::PageDown => {
                events.push(AppEvent::Scrolled(20));
            }
            KeyAction::ScrollUp => {
                events.push(AppEvent::Scrolled(-1));
            }
            KeyAction::ScrollDown => {
                events.push(AppEvent::Scrolled(1));
            }
            KeyAction::SwitchPaneRequest => {
                events.push(AppEvent::PaneChanged(yinx_core::state::ActivePane::Request));
            }
            KeyAction::SwitchPaneResponse => {
                events.push(AppEvent::PaneChanged(
                    yinx_core::state::ActivePane::Response,
                ));
            }
            KeyAction::SwitchPaneWorkflow => {
                events.push(AppEvent::PaneChanged(
                    yinx_core::state::ActivePane::Workflow,
                ));
            }
            KeyAction::SwitchPaneLogs => {
                events.push(AppEvent::PaneChanged(yinx_core::state::ActivePane::Logs));
            }
            KeyAction::SendRequest => {
                events.push(AppEvent::SendRequest(
                    yinx_core::request::RequestBuilder::new()
                        .url("https://example.com")
                        .build()
                        .unwrap(),
                ));
            }
            KeyAction::Save => {
                events.push(AppEvent::SaveState);
            }
            _ => {
                if let KeyCode::Char('g') = event.code {
                    if self.pending_key == Some('g') {
                        events.push(AppEvent::CursorMoved {
                            lines: i64::MIN,
                            cols: 0,
                        });
                        self.pending_key = None;
                    } else {
                        self.pending_key = Some('g');
                    }
                }
            }
        }

        events
    }

    fn handle_insert_mode(&mut self, event: KeyEvent) -> Vec<AppEvent> {
        let mut events = Vec::new();

        match event.code {
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                events.push(AppEvent::ModeChanged(InputMode::Normal));
            }
            KeyCode::Backspace => {
                events.push(AppEvent::KeyPressed("Backspace".to_string()));
            }
            KeyCode::Enter => {
                events.push(AppEvent::KeyPressed("Enter".to_string()));
            }
            KeyCode::Char(c) => {
                events.push(AppEvent::KeyPressed(c.to_string()));
            }
            _ => {}
        }

        events
    }

    fn handle_visual_mode(&mut self, event: KeyEvent) -> Vec<AppEvent> {
        let mut events = Vec::new();

        match event.code {
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                events.push(AppEvent::ModeChanged(InputMode::Normal));
            }
            KeyCode::Char('h') => {
                events.push(AppEvent::CursorMoved { lines: 0, cols: -1 });
            }
            KeyCode::Char('j') => {
                events.push(AppEvent::CursorMoved { lines: 1, cols: 0 });
            }
            KeyCode::Char('k') => {
                events.push(AppEvent::CursorMoved { lines: -1, cols: 0 });
            }
            KeyCode::Char('l') => {
                events.push(AppEvent::CursorMoved { lines: 0, cols: 1 });
            }
            _ => {}
        }

        events
    }

    pub fn current_mode(&self) -> InputMode {
        self.mode
    }

    pub fn switch_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputBuffer {
    pub content: String,
    pub cursor_pos: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn with_content(content: &str) -> Self {
        Self {
            content: content.to_string(),
            cursor_pos: content.len(),
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.content.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.content.insert_str(self.cursor_pos, s);
        self.cursor_pos += s.len();
    }

    pub fn delete_char(&mut self) -> Option<char> {
        if self.cursor_pos == 0 {
            return None;
        }
        self.cursor_pos -= 1;
        Some(self.content.remove(self.cursor_pos))
    }

    pub fn delete_char_forward(&mut self) -> Option<char> {
        if self.cursor_pos >= self.content.len() {
            return None;
        }
        Some(self.content.remove(self.cursor_pos))
    }

    pub fn delete_word(&mut self) -> String {
        let mut deleted = String::new();
        while self.cursor_pos > 0 {
            let c = self.content.chars().nth(self.cursor_pos - 1).unwrap();
            if c.is_whitespace() {
                break;
            }
            deleted.push(self.delete_char().unwrap());
        }
        deleted.chars().rev().collect()
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.content.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn move_cursor_to_start(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor_pos = self.content.len();
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor_pos = 0;
    }

    pub fn as_str(&self) -> &str {
        &self.content
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_key_action_display() {
        assert_eq!(KeyAction::Quit.to_string(), "quit");
        assert_eq!(KeyAction::CursorUp.to_string(), "cursor_up");
        assert_eq!(KeyAction::ModeInsert.to_string(), "mode_insert");
    }

    #[test]
    fn test_key_binding_new() {
        let binding = KeyBinding::new("a", &["Ctrl"]);
        assert_eq!(binding.key, "a");
        assert_eq!(binding.modifiers.len(), 1);
        assert_eq!(binding.modifiers[0], "Ctrl");
    }

    #[test]
    fn test_key_binding_display() {
        let binding = KeyBinding::new("s", &["Ctrl"]);
        assert_eq!(binding.to_string(), "Ctrl+s");

        let binding = KeyBinding::new("a", &[]);
        assert_eq!(binding.to_string(), "a");
    }

    #[test]
    fn test_key_binding_from_key_event() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let binding = KeyBinding::from_key_event(&event);
        assert_eq!(binding.key, "a");
        assert!(binding.modifiers.is_empty());

        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        let binding = KeyBinding::from_key_event(&event);
        assert_eq!(binding.key, "s");
        assert_eq!(binding.modifiers.len(), 1);
        assert_eq!(binding.modifiers[0], "Ctrl");

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let binding = KeyBinding::from_key_event(&event);
        assert_eq!(binding.key, "Up");
    }

    #[test]
    fn test_key_binding_matches() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let binding = KeyBinding::new("c", &["Ctrl"]);
        assert!(binding.matches(&event));

        let event2 = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(!binding.matches(&event2));
    }

    #[test]
    fn test_key_binding_config_default() {
        let config = KeyBindingConfig::default_bindings();
        assert!(!config.bindings.is_empty());

        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = config.get_action(&event);
        assert_eq!(action, KeyAction::Quit);
    }

    #[test]
    fn test_key_binding_config_get_action() {
        let config = KeyBindingConfig::default_bindings();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = config.get_action(&event);
        assert_eq!(action, KeyAction::CursorDown);

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = config.get_action(&event);
        assert_eq!(action, KeyAction::CursorUp);
    }

    #[test]
    fn test_key_binding_config_set_remove() {
        let mut config = KeyBindingConfig::default_bindings();

        config.set_binding("x", &[], KeyAction::Quit);
        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(config.get_action(&event), KeyAction::Quit);

        config.remove_binding("x", &[]);
        assert_eq!(config.get_action(&event), KeyAction::Unknown);
    }

    #[test]
    fn test_input_handler_new() {
        let handler = InputHandler::new();
        assert_eq!(handler.current_mode(), InputMode::Normal);
    }

    #[test]
    fn test_input_handler_normal_mode_quit() {
        let mut handler = InputHandler::new();

        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(!events.is_empty());
        assert!(matches!(events[0], AppEvent::Quit));
    }

    #[test]
    fn test_input_handler_normal_mode_switch_to_insert() {
        let mut handler = InputHandler::new();

        let event = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert_eq!(handler.current_mode(), InputMode::Insert);
        assert!(matches!(
            events[0],
            AppEvent::ModeChanged(InputMode::Insert)
        ));
    }

    #[test]
    fn test_input_handler_normal_mode_navigation() {
        let mut handler = InputHandler::new();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(
            events[0],
            AppEvent::CursorMoved { lines: 1, cols: 0 }
        ));

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(
            events[0],
            AppEvent::CursorMoved { lines: -1, cols: 0 }
        ));
    }

    #[test]
    fn test_input_handler_insert_mode_escape() {
        let mut handler = InputHandler::new();
        handler.switch_mode(InputMode::Insert);

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert_eq!(handler.current_mode(), InputMode::Normal);
        assert!(matches!(
            events[0],
            AppEvent::ModeChanged(InputMode::Normal)
        ));
    }

    #[test]
    fn test_input_handler_insert_mode_typing() {
        let mut handler = InputHandler::new();
        handler.switch_mode(InputMode::Insert);

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(events[0], AppEvent::KeyPressed(ref s) if s == "a"));
    }

    #[test]
    fn test_input_handler_vim_top_bottom() {
        let mut handler = InputHandler::new();

        // Test 'gg' to go to top
        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(events.is_empty());
        assert_eq!(handler.pending_key, Some('g'));

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(
            events[0],
            AppEvent::CursorMoved {
                lines: i64::MIN,
                ..
            }
        ));
        assert_eq!(handler.pending_key, None);

        // Test 'G' to go to bottom
        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(
            events[0],
            AppEvent::CursorMoved {
                lines: i64::MAX,
                ..
            }
        ));
    }

    #[test]
    fn test_input_handler_page_up_down() {
        let mut handler = InputHandler::new();

        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let events = handler.handle_key(event);
        assert!(matches!(events[0], AppEvent::Scrolled(20)));

        let event = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let events = handler.handle_key(event);
        assert!(matches!(events[0], AppEvent::Scrolled(-20)));
    }

    #[test]
    fn test_input_handler_pane_switching() {
        let mut handler = InputHandler::new();

        let event = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(
            events[0],
            AppEvent::PaneChanged(yinx_core::state::ActivePane::Request)
        ));

        let event = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
        let events = handler.handle_key(event);
        assert!(matches!(
            events[0],
            AppEvent::PaneChanged(yinx_core::state::ActivePane::Response)
        ));
    }

    #[test]
    fn test_input_buffer_new() {
        let buffer = InputBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.cursor_pos, 0);
    }

    #[test]
    fn test_input_buffer_with_content() {
        let buffer = InputBuffer::with_content("hello");
        assert_eq!(buffer.as_str(), "hello");
        assert_eq!(buffer.cursor_pos, 5);
    }

    #[test]
    fn test_input_buffer_insert_char() {
        let mut buffer = InputBuffer::new();
        buffer.insert_char('a');
        assert_eq!(buffer.as_str(), "a");
        assert_eq!(buffer.cursor_pos, 1);

        buffer.insert_char('b');
        assert_eq!(buffer.as_str(), "ab");
        assert_eq!(buffer.cursor_pos, 2);
    }

    #[test]
    fn test_input_buffer_insert_str() {
        let mut buffer = InputBuffer::new();
        buffer.insert_str("hello");
        assert_eq!(buffer.as_str(), "hello");
        assert_eq!(buffer.cursor_pos, 5);
    }

    #[test]
    fn test_input_buffer_delete_char() {
        // Cursor at end (pos 3), delete_char removes char at pos 2 ('c')
        let mut buffer = InputBuffer::with_content("abc");
        let deleted = buffer.delete_char();
        assert_eq!(deleted, Some('c'));
        assert_eq!(buffer.as_str(), "ab");
        assert_eq!(buffer.cursor_pos, 2); // After decrement: 3-1=2

        // Move to position 1, delete char before cursor (at pos 0 = 'a')
        let mut buffer = InputBuffer::with_content("abc");
        buffer.move_cursor_to_end();
        buffer.move_cursor_left(); // pos 2
        buffer.move_cursor_left(); // pos 1
        let deleted = buffer.delete_char(); // decrement to 0, remove 'a'
        assert_eq!(deleted, Some('a'));
        assert_eq!(buffer.as_str(), "bc");
        assert_eq!(buffer.cursor_pos, 0);
    }

    #[test]
    fn test_input_buffer_delete_char_at_start() {
        let mut buffer = InputBuffer::with_content("a");
        buffer.move_cursor_to_start();
        let deleted = buffer.delete_char();
        assert_eq!(deleted, None);
        assert_eq!(buffer.as_str(), "a");
        assert_eq!(buffer.cursor_pos, 0);
    }

    #[test]
    fn test_input_buffer_delete_word() {
        let mut buffer = InputBuffer::with_content("hello world");
        buffer.move_cursor_to_end();
        let deleted = buffer.delete_word();
        assert_eq!(deleted, "world");
        assert_eq!(buffer.as_str(), "hello ");
    }

    #[test]
    fn test_input_buffer_cursor_movement() {
        let mut buffer = InputBuffer::with_content("abc");
        assert_eq!(buffer.cursor_pos, 3);

        buffer.move_cursor_left();
        assert_eq!(buffer.cursor_pos, 2);

        buffer.move_cursor_left();
        assert_eq!(buffer.cursor_pos, 1);

        buffer.move_cursor_right();
        assert_eq!(buffer.cursor_pos, 2);
    }

    #[test]
    fn test_input_buffer_clear() {
        let mut buffer = InputBuffer::with_content("hello");
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.cursor_pos, 0);
    }

    #[test]
    fn test_input_buffer_delete_char_forward() {
        // Cursor at end, nothing to delete forward
        let mut buffer = InputBuffer::with_content("abc");
        let deleted = buffer.delete_char_forward();
        assert_eq!(deleted, None);
        assert_eq!(buffer.as_str(), "abc");

        // Move to start and delete forward
        let mut buffer = InputBuffer::with_content("abc");
        buffer.move_cursor_to_start();
        let deleted = buffer.delete_char_forward();
        assert_eq!(deleted, Some('a'));
        assert_eq!(buffer.as_str(), "bc");
        assert_eq!(buffer.cursor_pos, 0);
    }

    #[test]
    fn test_key_binding_serde_roundtrip() {
        let binding = KeyBinding::new("s", &["Ctrl"]);
        let json = serde_json::to_string(&binding).unwrap();
        let decoded: KeyBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.key, "s");
        assert_eq!(decoded.modifiers[0], "Ctrl");
    }

    #[test]
    fn test_key_binding_config_serde() {
        let config = KeyBindingConfig::default_bindings();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: KeyBindingConfig = serde_json::from_str(&json).unwrap();
        assert!(!decoded.bindings.is_empty());
    }
}
