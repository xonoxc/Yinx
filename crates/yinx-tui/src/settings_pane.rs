use std::path::PathBuf;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use yinx_core::config::{discover_config, Config};
use yinx_core::events::AppEvent;

use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsMode {
    Viewing,
    Editing(usize),
}

#[derive(Debug, Clone)]
pub struct SettingsPane {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub mode: SettingsMode,
    pub selected_index: usize,
    pub edit_buffer: String,
    pub message: Option<String>,
    pub is_open: bool,
}

impl SettingsPane {
    pub fn new() -> Self {
        let config_path = discover_config();
        let config = if let Some(ref path) = config_path {
            Config::load_from_file(path).unwrap_or_else(|_| Config::default_config())
        } else {
            Config::default_config()
        };

        Self {
            config,
            config_path,
            mode: SettingsMode::Viewing,
            selected_index: 0,
            edit_buffer: String::new(),
            message: None,
            is_open: false,
        }
    }

    pub fn open(&mut self) {
        self.is_open = true;
        self.mode = SettingsMode::Viewing;
        self.selected_index = 0;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.message = None;
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn handle_event(&mut self, event: &AppEvent) -> Vec<AppEvent> {
        let mut events = Vec::new();

        if !self.is_open {
            return events;
        }

        match event {
            AppEvent::KeyPressed(key) => {
                self.handle_key_pressed(key, &mut events);
            }
            AppEvent::CursorMoved { lines, .. } => {
                if *lines < 0 {
                    self.select_prev();
                } else if *lines > 0 {
                    self.select_next();
                }
            }
            AppEvent::Scrolled(lines) => {
                if *lines < 0 {
                    for _ in 0..lines.abs() {
                        self.select_prev();
                    }
                } else {
                    for _ in 0..*lines {
                        self.select_next();
                    }
                }
            }
            _ => {}
        }

        events
    }

    fn handle_key_pressed(&mut self, key: &str, events: &mut Vec<AppEvent>) {
        match self.mode {
            SettingsMode::Viewing => match key {
                "e" => {
                    self.start_editing();
                }
                "s" => {
                    self.save();
                    events.push(AppEvent::SettingsSaved);
                }
                "q" | "Esc" => {
                    self.close();
                    events.push(AppEvent::SettingsClosed);
                }
                _ => {}
            },
            SettingsMode::Editing(idx) => match key {
                "Enter" => {
                    self.confirm_edit(idx);
                    events.push(AppEvent::SettingsChanged {
                        key: self.setting_key_at(idx),
                        value: self.edit_buffer.clone(),
                    });
                }
                "Esc" => {
                    self.mode = SettingsMode::Viewing;
                    self.edit_buffer.clear();
                }
                _ => {
                    if key.len() == 1 {
                        let c = key.chars().next().unwrap();
                        self.edit_buffer.push(c);
                    }
                }
            },
        }
    }

    fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    fn select_next(&mut self) {
        let max = self.setting_count() - 1;
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    fn setting_count(&self) -> usize {
        5 + self.config.keybindings.len()
    }

    fn setting_key_at(&self, index: usize) -> String {
        match index {
            0 => "theme".to_string(),
            1 => "default_timeout_secs".to_string(),
            2 => "follow_redirects".to_string(),
            3 => "verify_tls".to_string(),
            4 => "max_history_entries".to_string(),
            _ => {
                let keys: Vec<_> = self.config.keybindings.keys().collect();
                keys.get(index - 5)
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            }
        }
    }

    fn start_editing(&mut self) {
        let key = self.setting_key_at(self.selected_index);
        let value = match key.as_str() {
            "theme" => self.config.theme.clone(),
            "default_timeout_secs" => self.config.defaults.default_timeout_secs.to_string(),
            "follow_redirects" => self.config.defaults.follow_redirects.to_string(),
            "verify_tls" => self.config.defaults.verify_tls.to_string(),
            "max_history_entries" => self.config.defaults.max_history_entries.to_string(),
            _ => self
                .config
                .keybindings
                .get(&key)
                .cloned()
                .unwrap_or_default(),
        };
        self.edit_buffer = value;
        self.mode = SettingsMode::Editing(self.selected_index);
    }

    fn confirm_edit(&mut self, index: usize) -> Vec<AppEvent> {
        let key = self.setting_key_at(index);
        let value = self.edit_buffer.clone();
        let mut events = Vec::new();

        match key.as_str() {
            "theme" => {
                self.config.theme = value.clone();
            }
            "default_timeout_secs" => {
                if let Ok(v) = value.parse() {
                    self.config.defaults.default_timeout_secs = v;
                }
            }
            "follow_redirects" => {
                if let Ok(v) = value.parse() {
                    self.config.defaults.follow_redirects = v;
                }
            }
            "verify_tls" => {
                if let Ok(v) = value.parse() {
                    self.config.defaults.verify_tls = v;
                }
            }
            "max_history_entries" => {
                if let Ok(v) = value.parse() {
                    self.config.defaults.max_history_entries = v;
                }
            }
            _ => {
                self.config.keybindings.insert(key.clone(), value.clone());
            }
        }

        self.mode = SettingsMode::Viewing;
        self.edit_buffer.clear();
        self.message = Some("Setting updated".to_string());

        events.push(AppEvent::ConfigChanged {
            key: self.setting_key_at(index),
            value,
        });
        events
    }

    pub fn toggle_current_boolean(&mut self) {
        let key = self.setting_key_at(self.selected_index);
        match key.as_str() {
            "follow_redirects" => {
                self.config.defaults.follow_redirects = !self.config.defaults.follow_redirects;
            }
            "verify_tls" => {
                self.config.defaults.verify_tls = !self.config.defaults.verify_tls;
            }
            _ => {}
        }
        self.message = Some("Setting toggled".to_string());
    }

    pub fn get_theme_list(&self) -> Vec<String> {
        Theme::builtin_theme_names()
    }

    pub fn confirm_edit_and_get_events(&mut self, index: usize) -> Vec<AppEvent> {
        self.confirm_edit(index)
    }

    fn save(&mut self) {
        let path = self
            .config_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(".yinxrc.yaml"));

        match self.config.save_to_file(&path) {
            Ok(()) => {
                self.config_path = Some(path);
                self.message = Some("Settings saved".to_string());
            }
            Err(e) => {
                self.message = Some(format!("Save failed: {}", e));
            }
        }
    }

    pub fn render(&self, f: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if !self.is_open {
            return;
        }

        let block = Block::default()
            .title("SETTINGS")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border.active_color.as_color()))
            .style(Style::default().bg(theme.pane.bg_color()).fg(theme.foreground.as_color()));

        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);

        let footer_height = if self.selected_index == 0 { 2 } else { 1 };

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0), Constraint::Length(footer_height)])
            .split(inner);

        let list_widget = List::new(self.settings_items(theme))
            .style(Style::default().fg(theme.foreground.as_color()))
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color())
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(self.selected_index));

        ratatui::widgets::Widget::render(list_widget, layout[0], f.buffer_mut());

        let footer = if self.selected_index == 0 {
            let theme_list = self.get_theme_list().join("  ");
            let msg = self
                .message
                .as_deref()
                .unwrap_or("Enter a theme name, then press Enter.");
            Paragraph::new(vec![
                Line::from(Span::styled(
                    msg,
                    Style::default().fg(theme.semantic.warning.as_color()),
                )),
                Line::from(vec![
                    Span::styled("available: ", Style::default().fg(theme.muted_color())),
                    Span::styled(theme_list, Style::default().fg(theme.foreground.as_color())),
                ]),
            ])
            .style(Style::default().bg(theme.subtle_bg()))
        } else {
            Paragraph::new(self.message.as_deref().unwrap_or("")).style(
                Style::default()
                    .bg(theme.subtle_bg())
                    .fg(theme.semantic.warning.as_color()),
            )
        };
        f.render_widget(footer, layout[1]);

        if matches!(self.mode, SettingsMode::Editing(_)) {
            let edit_area = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(3)])
                .split(layout[0])[0];

            let edit_block = Block::default()
                .title("EDIT VALUE")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border.active_color.as_color()))
                .style(Style::default().bg(theme.pane.bg_color()).fg(theme.foreground.as_color()));

            let edit_para = Paragraph::new(self.edit_buffer.as_str())
                .block(edit_block)
                .style(Style::default().fg(theme.foreground.as_color()))
                .wrap(Wrap { trim: true });

            f.render_widget(edit_para, edit_area);
        }
    }

    fn settings_items(&self, theme: &Theme) -> Vec<ListItem<'_>> {
        let mut items = vec![
            ListItem::new(Line::from(vec![
                Span::styled("theme: ", Style::default().fg(theme.muted_color())),
                Span::raw(&self.config.theme),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("default_timeout_secs: ", Style::default().fg(theme.muted_color())),
                Span::raw(self.config.defaults.default_timeout_secs.to_string()),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("follow_redirects: ", Style::default().fg(theme.muted_color())),
                Span::raw(self.config.defaults.follow_redirects.to_string()),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("verify_tls: ", Style::default().fg(theme.muted_color())),
                Span::raw(self.config.defaults.verify_tls.to_string()),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("max_history_entries: ", Style::default().fg(theme.muted_color())),
                Span::raw(self.config.defaults.max_history_entries.to_string()),
            ])),
        ];

        for (key, value) in &self.config.keybindings {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{}: ", key), Style::default().fg(theme.muted_color())),
                Span::raw(value),
            ])));
        }

        items
    }
}

impl Default for SettingsPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yinx_core::config::Config;

    fn create_test_pane() -> SettingsPane {
        SettingsPane {
            config: Config::default_config(),
            config_path: None,
            mode: SettingsMode::Viewing,
            selected_index: 0,
            edit_buffer: String::new(),
            message: None,
            is_open: false,
        }
    }

    #[test]
    fn test_settings_pane_new() {
        let pane = create_test_pane();
        assert!(!pane.is_open());
        assert_eq!(pane.config.theme, "terminal");
    }

    #[test]
    fn test_settings_pane_open_close() {
        let mut pane = create_test_pane();
        pane.open();
        assert!(pane.is_open());

        pane.close();
        assert!(!pane.is_open());
    }

    #[test]
    fn test_settings_pane_select_navigation() {
        let mut pane = create_test_pane();
        pane.open();
        assert_eq!(pane.selected_index, 0);

        pane.select_next();
        assert_eq!(pane.selected_index, 1);

        pane.select_prev();
        assert_eq!(pane.selected_index, 0);
    }

    #[test]
    fn test_settings_pane_start_editing() {
        let mut pane = create_test_pane();
        pane.open();
        pane.selected_index = 0;

        pane.start_editing();
        assert!(matches!(pane.mode, SettingsMode::Editing(0)));
        assert_eq!(pane.edit_buffer, "terminal");
    }

    #[test]
    fn test_settings_pane_confirm_edit() {
        let mut pane = create_test_pane();
        pane.open();
        pane.selected_index = 0;

        pane.start_editing();
        pane.edit_buffer = "light".to_string();
        pane.confirm_edit(0);

        assert_eq!(pane.config.theme, "light");
        assert!(matches!(pane.mode, SettingsMode::Viewing));
    }

    #[test]
    fn test_settings_pane_edit_timeout() {
        let mut pane = create_test_pane();
        pane.open();
        pane.selected_index = 1;

        pane.start_editing();
        pane.edit_buffer = "60".to_string();
        pane.confirm_edit(1);

        assert_eq!(pane.config.defaults.default_timeout_secs, 60);
    }

    #[test]
    fn test_settings_pane_edit_boolean() {
        let mut pane = create_test_pane();
        pane.open();
        pane.selected_index = 2;

        pane.start_editing();
        pane.edit_buffer = "false".to_string();
        pane.confirm_edit(2);

        assert!(!pane.config.defaults.follow_redirects);
    }

    #[test]
    fn test_settings_pane_cancel_edit() {
        let mut pane = create_test_pane();
        pane.open();
        pane.selected_index = 0;

        pane.start_editing();
        pane.edit_buffer = "light".to_string();
        pane.mode = SettingsMode::Viewing;
        pane.edit_buffer.clear();

        assert_eq!(pane.config.theme, "terminal");
    }

    #[test]
    fn test_settings_pane_save() {
        let mut pane = create_test_pane();
        pane.open();

        pane.config.theme = "light".to_string();
        pane.save();

        assert!(pane.message.is_some());
    }

    #[test]
    fn test_settings_pane_setting_count() {
        let pane = create_test_pane();
        assert_eq!(pane.setting_count(), 5);
    }

    #[test]
    fn test_settings_pane_handle_event_close() {
        let mut pane = create_test_pane();
        pane.open();

        let events = pane.handle_event(&AppEvent::KeyPressed("q".to_string()));
        assert!(events.contains(&AppEvent::SettingsClosed));
        assert!(!pane.is_open());
    }

    #[test]
    fn test_settings_pane_handle_event_edit() {
        let mut pane = create_test_pane();
        pane.open();

        let events = pane.handle_event(&AppEvent::KeyPressed("e".to_string()));
        assert!(matches!(pane.mode, SettingsMode::Editing(_)));
        assert!(events.is_empty());
    }

    #[test]
    fn test_settings_pane_handle_event_save() {
        let mut pane = create_test_pane();
        pane.open();

        let events = pane.handle_event(&AppEvent::KeyPressed("s".to_string()));
        assert!(events.contains(&AppEvent::SettingsSaved));
    }

    #[test]
    fn test_settings_pane_handle_cursor_movement() {
        let mut pane = create_test_pane();
        pane.open();
        assert_eq!(pane.selected_index, 0);

        pane.handle_event(&AppEvent::CursorMoved { lines: 1, cols: 0 });
        assert_eq!(pane.selected_index, 1);
    }

    #[test]
    fn test_settings_pane_handle_scroll() {
        let mut pane = create_test_pane();
        pane.open();

        pane.handle_event(&AppEvent::Scrolled(2));
        assert_eq!(pane.selected_index, 2);
    }

    #[test]
    fn test_settings_pane_setting_key_at() {
        let pane = create_test_pane();
        assert_eq!(pane.setting_key_at(0), "theme");
        assert_eq!(pane.setting_key_at(1), "default_timeout_secs");
        assert_eq!(pane.setting_key_at(2), "follow_redirects");
        assert_eq!(pane.setting_key_at(3), "verify_tls");
        assert_eq!(pane.setting_key_at(4), "max_history_entries");
    }

    #[test]
    fn test_settings_pane_with_keybindings() {
        let mut pane = create_test_pane();
        pane.config
            .keybindings
            .insert("quit".to_string(), "Ctrl+c".to_string());
        pane.open();

        assert_eq!(pane.setting_count(), 6);
    }

    #[test]
    fn test_settings_pane_message_after_save() {
        let mut pane = create_test_pane();
        pane.open();
        pane.save();

        assert!(pane.message.is_some());
    }

    #[test]
    fn test_settings_pane_config_from_env() {
        let config = Config::default_config().apply_env_overrides();
        assert_eq!(config.theme, "terminal");
    }

    #[test]
    fn test_settings_pane_close_clears_message() {
        let mut pane = create_test_pane();
        pane.open();
        pane.message = Some("test".to_string());
        pane.close();
        assert!(pane.message.is_none());
    }

    // Issue 4: Settings Redesign - Task 4.2
    #[test]
    fn test_toggle_boolean_setting() {
        let mut pane = SettingsPane::new();
        pane.open();
        pane.selected_index = 2; // follow_redirects
        pane.start_editing();

        // Simulate Space to toggle
        pane.toggle_current_boolean();

        assert_ne!(
            pane.config.defaults.follow_redirects,
            Config::default_config().defaults.follow_redirects
        );
    }

    // Task 4.3
    #[test]
    fn test_theme_dropdown_lists_all_themes() {
        let mut pane = SettingsPane::new();
        pane.open();
        pane.selected_index = 0; // theme

        let themes = pane.get_theme_list();
        assert!(themes.contains(&"terminal".to_string()));
        assert!(themes.contains(&"dark".to_string()));
        assert!(themes.contains(&"light".to_string()));
        assert!(themes.contains(&"forest".to_string()));
    }

    // Task 4.4
    #[test]
    fn test_confirm_edit_emits_config_changed() {
        let mut pane = SettingsPane::new();
        pane.open();
        pane.selected_index = 0;
        pane.start_editing();
        pane.edit_buffer = "light".to_string();

        let events = pane.confirm_edit_and_get_events(0);
        assert!(events
            .iter()
            .any(|e| matches!(e, AppEvent::ConfigChanged { .. })));
    }
}
