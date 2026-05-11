use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use yinx_core::collections::{Collection, CollectionItem};
use yinx_core::environments::Environment;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Collections,
    Environments,
    History,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SidebarItem {
    CollectionHeader {
        id: String,
        name: String,
    },
    CollectionFolder {
        name: String,
        depth: usize,
    },
    CollectionRequest {
        name: String,
        depth: usize,
    },
    Environment {
        id: String,
        name: String,
        active: bool,
    },
    HistoryEntry {
        id: String,
        method: String,
        url: String,
        status: Option<u16>,
        time_ago: String,
    },
    SectionHeader {
        section: SidebarSection,
    },
}

pub struct Sidebar {
    collections: Vec<Collection>,
    environments: Vec<Environment>,
    active_env: Option<String>,
    history: Vec<String>,
    filter: String,
    collapsed_sections: Vec<SidebarSection>,
    collapsed_collections: Vec<String>,
    collapsed_folders: Vec<Vec<String>>,
    items: Vec<SidebarItem>,
    selected_index: usize,
    list_state: ListState,
    filter_active: bool,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            collections: Vec::new(),
            environments: Vec::new(),
            active_env: None,
            history: Vec::new(),
            filter: String::new(),
            collapsed_sections: Vec::new(),
            collapsed_collections: Vec::new(),
            collapsed_folders: Vec::new(),
            items: Vec::new(),
            selected_index: 0,
            list_state: ListState::default(),
            filter_active: false,
        }
    }

    pub fn set_collections(&mut self, collections: Vec<Collection>) {
        self.collections = collections;
        self.rebuild_items();
    }

    pub fn set_environments(&mut self, environments: Vec<Environment>, active_env: Option<String>) {
        self.environments = environments;
        self.active_env = active_env;
        self.rebuild_items();
    }

    pub fn set_history(&mut self, history: Vec<String>) {
        self.history = history;
        self.rebuild_items();
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn selected_item(&self) -> Option<&SidebarItem> {
        self.items.get(self.selected_index)
    }

    fn rebuild_items(&mut self) {
        let mut items: Vec<SidebarItem> = Vec::new();

        // Collections section
        let collections_collapsed = self
            .collapsed_sections
            .contains(&SidebarSection::Collections);
        items.push(SidebarItem::SectionHeader {
            section: SidebarSection::Collections,
        });

        if !collections_collapsed {
            for collection in &self.collections {
                let collection_collapsed = self.collapsed_collections.contains(&collection.id);
                items.push(SidebarItem::CollectionHeader {
                    id: collection.id.clone(),
                    name: collection.name.clone(),
                });

                if !collection_collapsed {
                    self.flatten_items(&collection.items, 1, &mut items);
                }
            }
        }

        // Environments section
        let env_collapsed = self
            .collapsed_sections
            .contains(&SidebarSection::Environments);
        items.push(SidebarItem::SectionHeader {
            section: SidebarSection::Environments,
        });

        if !env_collapsed {
            for env in &self.environments {
                let is_active = self.active_env.as_deref() == Some(&env.id);
                items.push(SidebarItem::Environment {
                    id: env.id.clone(),
                    name: env.name.clone(),
                    active: is_active,
                });
            }
        }

        // History section
        let history_collapsed = self.collapsed_sections.contains(&SidebarSection::History);
        items.push(SidebarItem::SectionHeader {
            section: SidebarSection::History,
        });

        if !history_collapsed {
            for entry in &self.history {
                items.push(SidebarItem::HistoryEntry {
                    id: String::new(),
                    method: "GET".to_string(),
                    url: entry.clone(),
                    status: None,
                    time_ago: String::new(),
                });
            }
        }

        self.items = items;
        self.selected_index = self.selected_index.min(self.items.len().saturating_sub(1));
        self.list_state.select(Some(self.selected_index));
    }

    fn flatten_items(&self, items: &[CollectionItem], depth: usize, result: &mut Vec<SidebarItem>) {
        for item in items {
            match item {
                CollectionItem::Request(req) => {
                    if !self.filter_applies(&req.name) {
                        continue;
                    }
                    result.push(SidebarItem::CollectionRequest {
                        name: req.name.clone(),
                        depth,
                    });
                }
                CollectionItem::Folder { name, children } => {
                    if !self.filter_applies(name) {
                        continue;
                    }
                    let folder_key = vec![name.clone()];
                    let collapsed = self.collapsed_folders.contains(&folder_key);
                    result.push(SidebarItem::CollectionFolder {
                        name: name.clone(),
                        depth,
                    });
                    if !collapsed {
                        self.flatten_items(children, depth + 1, result);
                    }
                }
            }
        }
    }

    fn filter_applies(&self, text: &str) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        text.to_lowercase().contains(&self.filter.to_lowercase())
    }

    pub fn handle_key(&mut self, key_code: KeyCode) -> bool {
        if self.filter_active {
            return self.handle_filter_key(key_code);
        }

        match key_code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected_index =
                    (self.selected_index + 1).min(self.items.len().saturating_sub(1));
                self.list_state.select(Some(self.selected_index));
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
                self.list_state.select(Some(self.selected_index));
                true
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.collapse_current();
                true
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.expand_current();
                true
            }
            KeyCode::Enter => {
                self.activate_current();
                true
            }
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter.clear();
                true
            }
            KeyCode::Char('g') => {
                self.selected_index = 0;
                self.list_state.select(Some(0));
                true
            }
            KeyCode::Char('G') => {
                self.selected_index = self.items.len().saturating_sub(1);
                self.list_state.select(Some(self.selected_index));
                true
            }
            _ => false,
        }
    }

    fn handle_filter_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Esc => {
                self.filter_active = false;
                self.filter.clear();
                self.rebuild_items();
                true
            }
            KeyCode::Enter => {
                self.filter_active = false;
                self.rebuild_items();
                true
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_items();
                true
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.rebuild_items();
                true
            }
            _ => true,
        }
    }

    fn collapse_current(&mut self) {
        if let Some(item) = self.items.get(self.selected_index) {
            match item {
                SidebarItem::SectionHeader { section } => {
                    if !self.collapsed_sections.contains(section) {
                        self.collapsed_sections.push(*section);
                        self.rebuild_items();
                    }
                }
                SidebarItem::CollectionHeader { id, .. } => {
                    if !self.collapsed_collections.contains(id) {
                        self.collapsed_collections.push(id.clone());
                        self.rebuild_items();
                    }
                }
                SidebarItem::CollectionFolder { name, .. } => {
                    let key = vec![name.clone()];
                    if !self.collapsed_folders.contains(&key) {
                        self.collapsed_folders.push(key);
                        self.rebuild_items();
                    }
                }
                _ => {}
            }
        }
    }

    fn expand_current(&mut self) {
        if let Some(item) = self.items.get(self.selected_index) {
            match item {
                SidebarItem::SectionHeader { section } => {
                    self.collapsed_sections.retain(|s| s != section);
                    self.rebuild_items();
                }
                SidebarItem::CollectionHeader { id, .. } => {
                    self.collapsed_collections.retain(|c| c != id);
                    self.rebuild_items();
                }
                SidebarItem::CollectionFolder { name, .. } => {
                    let key = vec![name.clone()];
                    self.collapsed_folders.retain(|k| k != &key);
                    self.rebuild_items();
                }
                _ => {}
            }
        }
    }

    fn activate_current(&mut self) {
        // Handled by TuiShell based on selected_item()
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        if area.width < 3 || area.height < 3 {
            return;
        }

        let block = Block::default()
            .title(" SIDEBAR ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_color(is_active)))
            .style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let rendered_items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(_i, item)| match item {
                SidebarItem::SectionHeader { section } => {
                    let name = match section {
                        SidebarSection::Collections => "COLLECTIONS",
                        SidebarSection::Environments => "ENVIRONMENTS",
                        SidebarSection::History => "HISTORY",
                    };
                    ListItem::new(Line::from(Span::styled(
                        format!(" {} ", name),
                        Style::default()
                            .fg(theme.pane.title.as_color())
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    )))
                }
                SidebarItem::CollectionHeader { name, .. } => {
                    ListItem::new(Line::from(Span::styled(
                        format!("  {} ", name),
                        Style::default().fg(theme.semantic.info.as_color()),
                    )))
                }
                SidebarItem::CollectionFolder { name, depth } => {
                    let indent = "  ".repeat(*depth + 1);
                    ListItem::new(Line::from(Span::styled(
                        format!("{} {} ", indent, name),
                        Style::default().fg(theme.semantic.warning.as_color()),
                    )))
                }
                SidebarItem::CollectionRequest { name, depth } => {
                    let indent = "  ".repeat(*depth + 2);
                    ListItem::new(Line::from(Span::styled(
                        format!("{}● {} ", indent, name),
                        Style::default().fg(theme.foreground.as_color()),
                    )))
                }
                SidebarItem::Environment { name, active, .. } => {
                    let indicator = if *active { "●" } else { "○" };
                    ListItem::new(Line::from(Span::styled(
                        format!("  {} {} ", indicator, name),
                        Style::default().fg(if *active {
                            theme.semantic.success.as_color()
                        } else {
                            theme.foreground.as_color()
                        }),
                    )))
                }
                SidebarItem::HistoryEntry {
                    method,
                    url,
                    status,
                    ..
                } => {
                    let status_str = status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "--".to_string());
                    ListItem::new(Line::from(Span::styled(
                        format!("  {} {} {} ", method, url, status_str),
                        Style::default().fg(theme.foreground.as_color()),
                    )))
                }
            })
            .collect();

        let list = List::new(rendered_items)
            .style(Style::default().fg(theme.foreground.as_color()))
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color())
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸");

        let mut state = ListState::default();
        state.select(Some(
            self.selected_index.min(self.items.len().saturating_sub(1)),
        ));
        frame.render_stateful_widget(list, inner, &mut state);

        if self.filter_active {
            let filter_area = Rect::new(inner.x, inner.y, inner.width.min(30), 1);
            let filter_text = format!("/{}", self.filter);
            let filter_widget = Paragraph::new(Line::from(Span::styled(
                filter_text,
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color()),
            )));
            frame.render_widget(filter_widget, filter_area);
            let cursor_x = filter_area.x + 1 + self.filter.len() as u16;
            frame.set_cursor_position(ratatui::prelude::Position::new(
                cursor_x.min(filter_area.x + filter_area.width - 1),
                filter_area.y,
            ));
        }
    }

    pub fn toggle_section(&mut self, section: SidebarSection) {
        if self.collapsed_sections.contains(&section) {
            self.collapsed_sections.retain(|s| s != &section);
        } else {
            self.collapsed_sections.push(section);
        }
        self.rebuild_items();
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
