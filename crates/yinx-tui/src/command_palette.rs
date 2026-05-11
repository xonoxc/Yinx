use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use yinx_core::commands::{CommandCategory, CommandRegistry};
use yinx_core::events::AppEvent;

use crate::input::InputBuffer;
use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum PaletteAction {
    Execute(Vec<AppEvent>),
    Close,
    None,
}

pub struct CommandPalette {
    pub input: InputBuffer,
    pub visible: bool,
    registry: CommandRegistry,
    matches: Vec<CommandMatch>,
    selected: usize,
}

#[derive(Debug, Clone)]
struct CommandMatch {
    command_name: &'static str,
    description: &'static str,
    category: CommandCategory,
    execute: fn() -> Vec<AppEvent>,
}

impl CommandPalette {
    pub fn new() -> Self {
        let registry = CommandRegistry::with_defaults();
        let matches = registry
            .search("")
            .into_iter()
            .map(|c| CommandMatch {
                command_name: c.name,
                description: c.description,
                category: c.category,
                execute: c.execute,
            })
            .collect();
        Self {
            input: InputBuffer::with_content(":"),
            visible: false,
            registry,
            matches,
            selected: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.input = InputBuffer::with_content(":");
            self.update_matches();
            self.selected = 0;
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.input = InputBuffer::with_content(":");
        self.update_matches();
        self.selected = 0;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn handle_key(&mut self, key_event: crossterm::event::KeyEvent) -> PaletteAction {
        if !self.visible {
            return PaletteAction::None;
        }

        use crossterm::event::{KeyCode, KeyModifiers};

        match key_event.code {
            KeyCode::Esc => {
                self.visible = false;
                return PaletteAction::Close;
            }
            KeyCode::Enter => {
                if !self.matches.is_empty() {
                    let idx = self.selected.min(self.matches.len() - 1);
                    let cmd_match = &self.matches[idx];
                    let events = (cmd_match.execute)();
                    self.visible = false;
                    return PaletteAction::Execute(events);
                }
                self.visible = false;
                return PaletteAction::Close;
            }
            KeyCode::Backspace => {
                self.input.delete_char();
                self.update_matches();
                self.selected = 0;
            }
            KeyCode::Up => {
                if !self.matches.is_empty() {
                    self.selected = self.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if !self.matches.is_empty() {
                    self.selected = (self.selected + 1).min(self.matches.len() - 1);
                }
            }
            KeyCode::Char(c) if c == 'k' && key_event.modifiers.is_empty() => {
                if !self.matches.is_empty() {
                    self.selected = self.selected.saturating_sub(1);
                }
            }
            KeyCode::Char(c) if c == 'j' && key_event.modifiers.is_empty() => {
                if !self.matches.is_empty() {
                    self.selected = (self.selected + 1).min(self.matches.len() - 1);
                }
            }
            KeyCode::Char(c) => {
                if key_event.modifiers == KeyModifiers::NONE || key_event.modifiers.is_empty() {
                    self.input.insert_char(c);
                    self.update_matches();
                    self.selected = 0;
                }
            }
            KeyCode::Tab => {
                if !self.matches.is_empty() {
                    self.selected = (self.selected + 1) % self.matches.len();
                }
            }
            KeyCode::PageUp => {
                let page_size = 10usize;
                self.selected = self.selected.saturating_sub(page_size);
            }
            KeyCode::PageDown => {
                let page_size = 10usize;
                if !self.matches.is_empty() {
                    self.selected =
                        (self.selected + page_size).min(self.matches.len().saturating_sub(1));
                }
            }
            KeyCode::Home => {
                self.selected = 0;
            }
            KeyCode::End => {
                if !self.matches.is_empty() {
                    self.selected = self.matches.len() - 1;
                }
            }
            _ => {}
        }

        PaletteAction::None
    }

    fn update_matches(&mut self) {
        let query = self.input.as_str().trim_start_matches(':');
        self.matches = self
            .registry
            .search(query)
            .into_iter()
            .map(|c| CommandMatch {
                command_name: c.name,
                description: c.description,
                category: c.category,
                execute: c.execute,
            })
            .collect();
    }

    pub fn selected_command(&self) -> Option<&'static str> {
        if self.matches.is_empty() {
            return None;
        }
        let idx = self.selected.min(self.matches.len() - 1);
        Some(self.matches[idx].command_name)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.visible {
            return;
        }

        let palette_height =
            (self.matches.len().min(10) as u16 + 3).min(area.height.saturating_sub(2));
        let palette_width = area.width.saturating_mul(60).saturating_div(100).max(40);

        let x = area.x + area.width.saturating_sub(palette_width) / 2;
        let y = area.y + 1;

        let palette_area = Rect::new(x, y, palette_width, palette_height);

        frame.render_widget(Clear, palette_area);

        let input_text = self.input.as_str();

        let block = Block::default()
            .title(" COMMAND ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border.active_color.as_color()))
            .style(
                Style::default()
                    .bg(theme.pane.bg_color())
                    .fg(theme.foreground.as_color()),
            );

        let inner = block.inner(palette_area);
        frame.render_widget(block, palette_area);

        let input_line = Paragraph::new(Line::from(Span::styled(
            input_text,
            Style::default().fg(theme.foreground.as_color()),
        )))
        .style(
            Style::default()
                .bg(theme.highlight.bg.as_color())
                .fg(theme.foreground.as_color()),
        );

        let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(input_line, input_area);

        let cursor_x = inner.x + 1 + (input_text.len() as u16).min(inner.width.saturating_sub(2));
        frame.set_cursor_position(ratatui::prelude::Position::new(cursor_x, inner.y));

        let list_start_y = inner.y + 1;
        let max_items = inner.height.saturating_sub(1).min(10) as usize;

        let visible_matches: Vec<_> = self.matches.iter().take(max_items).enumerate().collect();

        for (display_idx, cmd_match) in &visible_matches {
            let is_selected = *display_idx == self.selected;
            let y_pos = list_start_y + *display_idx as u16;

            let style = if is_selected {
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(theme.pane.bg_color())
                    .fg(theme.foreground.as_color())
            };

            let category_color = match cmd_match.category {
                CommandCategory::File => theme.semantic.info.as_color(),
                CommandCategory::Edit => theme.semantic.success.as_color(),
                CommandCategory::Navigation => theme.semantic.info.as_color(),
                CommandCategory::Request => theme.semantic.success.as_color(),
                CommandCategory::Collection => theme.semantic.warning.as_color(),
                CommandCategory::Environment => theme.semantic.warning.as_color(),
                CommandCategory::Settings => theme.muted_color(),
                CommandCategory::Help => theme.semantic.info.as_color(),
                CommandCategory::System => theme.semantic.error.as_color(),
            };

            let name_span = Span::styled(
                format!("  {}", cmd_match.command_name),
                Style::default()
                    .fg(category_color)
                    .add_modifier(Modifier::BOLD),
            );
            let desc_span = Span::styled(
                format!("  {}", cmd_match.description),
                Style::default().fg(if is_selected {
                    theme.highlight.selected_fg.as_color()
                } else {
                    theme.muted_color()
                }),
            );

            let line = Paragraph::new(Line::from(vec![name_span, desc_span]))
                .style(style)
                .alignment(Alignment::Left);

            let item_area = Rect::new(inner.x, y_pos, inner.width, 1);
            frame.render_widget(line, item_area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_command_palette_new() {
        let palette = CommandPalette::new();
        assert!(!palette.visible);
        assert_eq!(palette.input.as_str(), ":");
        assert!(!palette.matches.is_empty());
    }

    #[test]
    fn test_command_palette_toggle() {
        let mut palette = CommandPalette::new();
        palette.toggle();
        assert!(palette.visible);
        palette.toggle();
        assert!(!palette.visible);
    }

    #[test]
    fn test_command_palette_show_hide() {
        let mut palette = CommandPalette::new();
        palette.show();
        assert!(palette.visible);
        assert_eq!(palette.input.as_str(), ":");

        palette.hide();
        assert!(!palette.visible);
    }

    #[test]
    fn test_command_palette_esc_closes() {
        let mut palette = CommandPalette::new();
        palette.show();
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = palette.handle_key(event);
        assert_eq!(action, PaletteAction::Close);
        assert!(!palette.visible);
    }

    #[test]
    fn test_command_palette_enter_executes() {
        let mut palette = CommandPalette::new();
        palette.show();
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = palette.handle_key(event);
        assert!(matches!(action, PaletteAction::Execute(_)));
        assert!(!palette.visible);
    }

    #[test]
    fn test_command_palette_typing_filters() {
        let mut palette = CommandPalette::new();
        palette.show();

        let initial_count = palette.matches.len();

        // type 'send' which should narrow to very few matches
        for c in "send".chars() {
            let e = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            palette.handle_key(e);
        }
        assert!(palette.matches.len() < initial_count);
        assert!(palette.matches.iter().any(|m| m.command_name == "send"));
    }

    #[test]
    fn test_command_palette_navigation() {
        let mut palette = CommandPalette::new();
        palette.show();
        assert_eq!(palette.selected, 0);

        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        palette.handle_key(down);
        assert_eq!(palette.selected, 1);

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        palette.handle_key(up);
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn test_command_palette_vim_keys_navigation() {
        let mut palette = CommandPalette::new();
        palette.show();
        assert_eq!(palette.selected, 0);

        let j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        palette.handle_key(j);
        assert_eq!(palette.selected, 1);

        let k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        palette.handle_key(k);
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn test_command_palette_backspace() {
        let mut palette = CommandPalette::new();
        palette.show();
        palette.input.insert_str("se");

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        palette.handle_key(event);
        assert_eq!(palette.input.as_str(), ":s");
    }

    #[test]
    fn test_command_palette_selected_command() {
        let mut palette = CommandPalette::new();
        palette.show();
        assert!(palette.selected_command().is_some());
    }

    #[test]
    fn test_command_palette_home_end() {
        let mut palette = CommandPalette::new();
        palette.show();

        let end = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        palette.handle_key(end);
        assert_eq!(palette.selected, palette.matches.len() - 1);

        let home = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        palette.handle_key(home);
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn test_command_palette_page_up_down() {
        let mut palette = CommandPalette::new();
        palette.show();
        palette.selected = 15;

        let pgup = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        palette.handle_key(pgup);
        assert_eq!(palette.selected, 5);

        let pgdn = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        palette.handle_key(pgdn);
        assert_eq!(palette.selected, 15);
    }

    #[test]
    fn test_command_palette_handle_key_when_hidden() {
        let mut palette = CommandPalette::new();
        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = palette.handle_key(event);
        assert_eq!(action, PaletteAction::None);
    }

    #[test]
    fn test_command_palette_tab_cycles() {
        let mut palette = CommandPalette::new();
        palette.show();
        let first_selected = palette.selected;

        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        palette.handle_key(tab);
        assert_ne!(palette.selected, first_selected);
        assert!(palette.selected < palette.matches.len());
    }

    #[test]
    fn test_command_palette_input_buffer_starts_with_colon() {
        let palette = CommandPalette::new();
        assert!(palette.input.as_str().starts_with(':'));
    }

    #[test]
    fn test_command_palette_execute_send_command() {
        let mut palette = CommandPalette::new();
        palette.show();

        palette.input = InputBuffer::with_content(":send");
        palette.update_matches();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = palette.handle_key(event);
        match action {
            PaletteAction::Execute(events) => {
                assert!(events.iter().any(|e| matches!(e, AppEvent::ExecuteRequest)));
            }
            _ => panic!("Expected Execute action"),
        }
    }

    #[test]
    fn test_command_palette_execute_save_command() {
        let mut palette = CommandPalette::new();
        palette.show();

        palette.input = InputBuffer::with_content(":w");
        palette.update_matches();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = palette.handle_key(event);
        match action {
            PaletteAction::Execute(events) => {
                assert!(events.iter().any(|e| matches!(e, AppEvent::SaveState)));
            }
            _ => panic!("Expected Execute action"),
        }
    }

    #[test]
    fn test_update_matches_empty_query() {
        let mut palette = CommandPalette::new();
        palette.input = InputBuffer::with_content(":");
        palette.update_matches();
        assert_eq!(palette.matches.len(), palette.registry.all().len());
    }

    #[test]
    fn test_is_visible() {
        let mut palette = CommandPalette::new();
        assert!(!palette.is_visible());
        palette.show();
        assert!(palette.is_visible());
    }
}
