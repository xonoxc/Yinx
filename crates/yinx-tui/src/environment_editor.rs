use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use yinx_core::environments::{Environment, EnvironmentVariable};
use yinx_core::events::AppEvent;

use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct EnvironmentEditor {
    pub visible: bool,
    pub environment: Option<Environment>,
    pub selected_index: usize,
    pub editing_key: bool,
    pub editing_value: bool,
    pub edit_buffer: String,
    pub focused_field: usize,
}

impl EnvironmentEditor {
    pub fn new() -> Self {
        Self {
            visible: false,
            environment: None,
            selected_index: 0,
            editing_key: false,
            editing_value: false,
            edit_buffer: String::new(),
            focused_field: 0,
        }
    }

    pub fn open(&mut self, env: Environment) {
        self.visible = true;
        self.environment = Some(env);
        self.selected_index = 0;
        self.editing_key = false;
        self.editing_value = false;
        self.edit_buffer.clear();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.environment = None;
        self.editing_key = false;
        self.editing_value = false;
        self.edit_buffer.clear();
    }

    pub fn is_open(&self) -> bool {
        self.visible
    }

    pub fn handle_key(&mut self, key: &str) -> Vec<AppEvent> {
        let mut events = Vec::new();

        if !self.visible {
            return events;
        }

        match key {
            "Esc" | "q" => {
                self.close();
                events.push(AppEvent::EnvironmentEditOpened { id: String::new() });
            }
            "j" | "Down" => {
                if let Some(ref env) = self.environment {
                    if self.selected_index + 1 < env.variables.len() {
                        self.selected_index += 1;
                    }
                }
            }
            "k" | "Up" => {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            "i" | "a" => {
                self.editing_key = false;
                self.editing_value = false;
                if let Some(ref env) = self.environment {
                    if let Some(var) = env.variables.get(self.selected_index) {
                        self.edit_buffer = var.key.clone();
                        self.editing_key = true;
                    }
                }
            }
            "v" => {
                self.editing_key = false;
                self.editing_value = false;
                if let Some(ref env) = self.environment {
                    if let Some(var) = env.variables.get(self.selected_index) {
                        self.edit_buffer = var.value.clone();
                        self.editing_value = true;
                    }
                }
            }
            "o" => {
                if let Some(ref mut env) = self.environment {
                    let new_var = EnvironmentVariable::new(
                        format!("var_{}", env.variables.len() + 1),
                        String::new(),
                    );
                    env.add_variable(new_var);
                    self.selected_index = env.variables.len().saturating_sub(1);
                    self.edit_buffer = env.variables[self.selected_index].key.clone();
                    self.editing_key = true;
                }
            }
            "d" => {
                if let Some(ref mut env) = self.environment {
                    if !env.variables.is_empty() {
                        let key = env.variables[self.selected_index].key.clone();
                        env.remove_variable(&key);
                        self.selected_index = self.selected_index.min(env.variables.len().saturating_sub(1));
                    }
                }
            }
            "Enter" => {
                if let Some(ref mut env) = self.environment {
                    if self.editing_key {
                        if let Some(var) = env.variables.get_mut(self.selected_index) {
                            var.key = self.edit_buffer.clone();
                        }
                        self.editing_key = false;
                        self.edit_buffer.clear();
                    } else if self.editing_value {
                        if let Some(var) = env.variables.get_mut(self.selected_index) {
                            var.value = self.edit_buffer.clone();
                        }
                        self.editing_value = false;
                        self.edit_buffer.clear();
                    } else {
                        // Toggle enabled/disabled
                        if let Some(var) = env.variables.get_mut(self.selected_index) {
                            var.enabled = !var.enabled;
                        }
                    }
                }
            }
            "Ctrl+s" => {
                if let Some(ref env) = self.environment {
                    events.push(AppEvent::EnvironmentUpdated { id: env.id.clone() });
                }
                self.close();
            }
            _ => {
                if self.editing_key || self.editing_value {
                    if key.len() == 1 {
                        self.edit_buffer.push_str(key);
                    } else if key == "Backspace" {
                        self.edit_buffer.pop();
                    } else if key == "Enter" {
                        if let Some(ref mut env) = self.environment {
                            if self.editing_key {
                                if let Some(var) = env.variables.get_mut(self.selected_index) {
                                    var.key = self.edit_buffer.clone();
                                }
                                self.editing_key = false;
                                self.edit_buffer.clear();
                            } else if self.editing_value {
                                if let Some(var) = env.variables.get_mut(self.selected_index) {
                                    var.value = self.edit_buffer.clone();
                                }
                                self.editing_value = false;
                                self.edit_buffer.clear();
                            }
                        }
                    }
                }
            }
        }

        events
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.visible {
            return;
        }

        let editor_area = centered_rect(area, 60, 60);

        frame.render_widget(Clear, editor_area);

        let block = Block::default()
            .title(" ENVIRONMENT EDITOR ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border.active_color.as_color()))
            .style(
                Style::default()
                    .bg(theme.pane.bg_color())
                    .fg(theme.foreground.as_color()),
            );

        let inner = block.inner(editor_area);
        frame.render_widget(block, editor_area);

        if let Some(ref env) = self.environment {
            // Title showing environment name
            let title = Paragraph::new(Line::from(Span::styled(
                format!(" Environment: {} ", env.name),
                Style::default()
                    .fg(theme.section_title())
                    .add_modifier(Modifier::BOLD),
            )))
            .style(Style::default().bg(theme.subtle_bg()));
            let title_area = Rect::new(inner.x, inner.y, inner.width, 1);
            frame.render_widget(title, title_area);

            // Instructions
            let help = Paragraph::new(Line::from(Span::styled(
                " j/k navigate | i=edit key | v=edit value | o=new | d=delete | Enter=toggle | Ctrl+s=save | Esc=close ",
                Style::default().fg(theme.muted_color()),
            )))
            .style(Style::default().bg(theme.subtle_bg()));
            let help_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
            frame.render_widget(help, help_area);

            // Variables list
            let list_start_y = inner.y + 2;
            let max_items = inner.height.saturating_sub(4) as usize;

            let rendered_items: Vec<ListItem> = env
                .variables
                .iter()
                .enumerate()
                .take(max_items)
                .map(|(idx, var)| {
                    let is_selected = idx == self.selected_index;
                    let indicator = if var.enabled { "✓" } else { "✗" };
                    let key_text = if self.editing_key && is_selected {
                        &self.edit_buffer
                    } else {
                        &var.key
                    };
                    let value_text = if self.editing_value && is_selected {
                        &self.edit_buffer
                    } else {
                        &var.value
                    };

                    let line = if is_selected && self.editing_key {
                        Line::from(vec![
                            Span::styled(
                                format!(" {} ", indicator),
                                Style::default().fg(if var.enabled {
                                    theme.semantic.success.as_color()
                                } else {
                                    theme.semantic.error.as_color()
                                }),
                            ),
                            Span::styled(
                                format!(" {} ", key_text),
                                Style::default()
                                    .fg(theme.semantic.warning.as_color())
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" = ", Style::default().fg(theme.muted_color())),
                            Span::styled(
                                value_text.to_string(),
                                Style::default().fg(theme.foreground.as_color()),
                            ),
                        ])
                    } else if is_selected && self.editing_value {
                        Line::from(vec![
                            Span::styled(
                                format!(" {} ", indicator),
                                Style::default().fg(if var.enabled {
                                    theme.semantic.success.as_color()
                                } else {
                                    theme.semantic.error.as_color()
                                }),
                            ),
                            Span::styled(
                                format!(" {} ", key_text),
                                Style::default()
                                    .fg(theme.semantic.warning.as_color())
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" = ", Style::default().fg(theme.muted_color())),
                            Span::styled(
                                self.edit_buffer.clone(),
                                Style::default()
                                    .fg(theme.semantic.info.as_color())
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled(
                                format!(" {} ", indicator),
                                Style::default().fg(if var.enabled {
                                    theme.semantic.success.as_color()
                                } else {
                                    theme.semantic.error.as_color()
                                }),
                            ),
                            Span::styled(
                                format!(" {} ", key_text),
                                Style::default()
                                    .fg(theme.semantic.warning.as_color())
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" = ", Style::default().fg(theme.muted_color())),
                            Span::styled(
                                value_text.to_string(),
                                Style::default().fg(theme.foreground.as_color()),
                            ),
                        ])
                    };

                    let style = if is_selected {
                        Style::default()
                            .bg(theme.highlight.selected_bg.as_color())
                            .fg(theme.highlight.selected_fg.as_color())
                    } else {
                        Style::default()
                            .bg(theme.pane.bg_color())
                            .fg(theme.foreground.as_color())
                    };

                    ListItem::new(line).style(style)
                })
                .collect();

            let list = List::new(rendered_items)
                .style(Style::default().fg(theme.foreground.as_color()))
                .highlight_style(
                    Style::default()
                        .bg(theme.highlight.selected_bg.as_color())
                        .fg(theme.highlight.selected_fg.as_color()),
                );

            let list_area = Rect::new(inner.x, list_start_y, inner.width, max_items as u16);
            let mut state = ratatui::widgets::ListState::default();
            state.select(Some(self.selected_index));
            frame.render_stateful_widget(list, list_area, &mut state);
        }
    }

    pub fn take_environment(&mut self) -> Option<Environment> {
        self.environment.take()
    }
}

impl Default for EnvironmentEditor {
    fn default() -> Self {
        Self::new()
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
