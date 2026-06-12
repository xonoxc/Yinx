use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

use yinx_core::collections::{Collection, CollectionItem};
use yinx_core::environments::Environment;
use yinx_core::request::request_to_curl;
use yinx_core::state::HistoryEntry;
use yinx_core::workspace_manager::WorkspaceSummary;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Workspaces,
    Collections,
    Environments,
    History,
}

#[derive(Debug, Clone)]
pub enum RenameTarget {
    Workspace { id: String, name: String },
    Collection { id: String, name: String },
    Request { collection_id: String, request_id: String, name: String },
    Folder { collection_id: String, name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SidebarItem {
    WorkspaceHeader {
        active_name: String,
    },
    WorkspaceEntry {
        id: String,
        name: String,
        active: bool,
    },
    CollectionHeader {
        id: String,
        name: String,
    },
    CollectionFolder {
        id: String,
        name: String,
        depth: usize,
    },
    CollectionRequest {
        id: String,
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
    history: Vec<HistoryEntry>,
    filter: String,
    collapsed_sections: Vec<SidebarSection>,
    collapsed_collections: Vec<String>,
    collapsed_folders: Vec<Vec<String>>,
    items: Vec<SidebarItem>,
    selected_index: usize,
    list_state: ListState,
    filter_active: bool,
    workspaces: Vec<WorkspaceSummary>,
    active_workspace_name: String,
    pub is_renaming: bool,
    pub rename_input: String,
    pub rename_target: Option<RenameTarget>,
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
            workspaces: Vec::new(),
            active_workspace_name: String::new(),
            is_renaming: false,
            rename_input: String::new(),
            rename_target: None,
        }
    }

    pub fn set_workspaces(&mut self, active: &str, all: &[WorkspaceSummary]) {
        self.active_workspace_name = active.to_string();
        self.workspaces = all.to_vec();
        self.rebuild_items();
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

    pub fn set_history(&mut self, history: Vec<HistoryEntry>) {
        self.history = history;
        self.rebuild_items();
    }

    pub fn get_history(&self) -> &[HistoryEntry] {
        &self.history
    }

    pub fn selected_history_entry(&self) -> Option<&HistoryEntry> {
        if let Some(SidebarItem::HistoryEntry { id, .. }) = self.items.get(self.selected_index) {
            self.history.iter().find(|e| &e.id == id)
        } else {
            None
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn is_filter_active(&self) -> bool {
        self.filter_active
    }

    pub fn start_filter(&mut self) {
        self.filter_active = true;
        self.filter.clear();
    }

    pub fn exit_filter(&mut self) {
        if self.filter_active {
            self.filter_active = false;
            self.filter.clear();
            self.rebuild_items();
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn selected_item(&self) -> Option<&SidebarItem> {
        self.items.get(self.selected_index)
    }

    fn rebuild_items(&mut self) {
        let mut items: Vec<SidebarItem> = Vec::new();

        // Workspaces section
        let ws_collapsed = self
            .collapsed_sections
            .contains(&SidebarSection::Workspaces);
        items.push(SidebarItem::SectionHeader {
            section: SidebarSection::Workspaces,
        });

        if !ws_collapsed {
            items.push(SidebarItem::WorkspaceHeader {
                active_name: self.active_workspace_name.clone(),
            });
            for ws in &self.workspaces {
                let is_active = ws.name == self.active_workspace_name;
                items.push(SidebarItem::WorkspaceEntry {
                    id: ws.id.clone(),
                    name: ws.name.clone(),
                    active: is_active,
                });
            }
        }

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
                    self.flatten_items(&collection.items, &collection.id, 1, &mut items);
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
                    id: entry.id.clone(),
                    method: entry.request.method.to_string(),
                    url: entry.request.url.as_str().to_string(),
                    status: entry.response.as_ref().map(|r| r.status.code()),
                    time_ago: relative_time(entry.timestamp),
                });
            }
        }

        self.items = items;
        self.selected_index = self.selected_index.min(self.items.len().saturating_sub(1));
        self.list_state.select(Some(self.selected_index));
    }

    fn flatten_items(&self, items: &[CollectionItem], collection_id: &str, depth: usize, result: &mut Vec<SidebarItem>) {
        for item in items {
            match item {
                CollectionItem::Request(req) => {
                    if !self.filter_applies(&req.name) {
                        continue;
                    }
                    result.push(SidebarItem::CollectionRequest {
                        id: req.id.clone(),
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
                        id: collection_id.to_string(),
                        name: name.clone(),
                        depth,
                    });
                    if !collapsed {
                        self.flatten_items(children, collection_id, depth + 1, result);
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

    pub fn is_rename_active(&self) -> bool {
        self.is_renaming
    }

    pub fn take_rename_target(&mut self) -> Option<RenameTarget> {
        if self.is_renaming {
            return None;
        }
        let target = self.rename_target.take();
        self.rename_input.clear();
        target
    }

    pub fn cancel_rename(&mut self) {
        self.is_renaming = false;
        self.rename_input.clear();
        self.rename_target = None;
    }

    pub fn handle_key(&mut self, key_code: KeyCode) -> bool {
        if self.filter_active {
            return self.handle_filter_key(key_code);
        }

        if self.is_renaming {
            return self.handle_rename_key(key_code);
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

    pub fn get_selected_history_action(&self) -> Option<HistoryAction> {
        let item = self.items.get(self.selected_index)?;
        match item {
            SidebarItem::HistoryEntry { id, .. } => {
                if id.is_empty() {
                    return None;
                }
                let entry = self.history.iter().find(|e| &e.id == id)?;
                Some(HistoryAction {
                    entry_id: id.clone(),
                    request: entry.request.clone(),
                    curl: request_to_curl(&entry.request),
                })
            }
            _ => None,
        }
    }

    pub fn clear_history_items(&mut self) {
        self.history.clear();
        self.rebuild_items();
    }
}

#[derive(Debug, Clone)]
pub struct HistoryAction {
    pub entry_id: String,
    pub request: yinx_core::request::Request,
    pub curl: String,
}

impl Sidebar {
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

    pub fn paste_filter_text(&mut self, text: &str) {
        if self.filter_active && !text.is_empty() {
            self.filter.push_str(text);
            self.rebuild_items();
        }
    }

    pub fn start_rename(&mut self) {
        let Some(item) = self.items.get(self.selected_index).cloned() else { return };
        let target = match item {
            SidebarItem::WorkspaceEntry { id, name, .. } => {
                RenameTarget::Workspace { id, name }
            }
            SidebarItem::CollectionHeader { id, name } => {
                RenameTarget::Collection { id, name }
            }
            SidebarItem::CollectionRequest { id, name, .. } => {
                let collection_id = self.find_collection_for_item(self.selected_index);
                RenameTarget::Request { collection_id, request_id: id, name }
            }
            SidebarItem::CollectionFolder { id, name, .. } => {
                RenameTarget::Folder { collection_id: id, name }
            }
            _ => return,
        };
        let current_name = match &target {
            RenameTarget::Workspace { name, .. } => name,
            RenameTarget::Collection { name, .. } => name,
            RenameTarget::Request { name, .. } => name,
            RenameTarget::Folder { name, .. } => name,
        }.clone();
        self.is_renaming = true;
        self.rename_input = current_name;
        self.rename_target = Some(target);
    }

    fn find_collection_for_item(&self, index: usize) -> String {
        // Walk backwards to find the nearest CollectionHeader
        for i in (0..=index).rev() {
            if let Some(SidebarItem::CollectionHeader { id, .. }) = self.items.get(i) {
                return id.clone();
            }
        }
        String::new()
    }

    fn handle_rename_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Esc => {
                self.cancel_rename();
                true
            }
            KeyCode::Enter => {
                self.is_renaming = false;
                true
            }
            KeyCode::Backspace => {
                self.rename_input.pop();
                true
            }
            KeyCode::Char(c) => {
                self.rename_input.push(c);
                true
            }
            _ => false,
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

    #[allow(dead_code)]
    fn can_select_up(&self) -> bool {
        self.selected_index > 0
    }

    #[allow(dead_code)]
    fn can_select_down(&self) -> bool {
        self.selected_index < self.items.len().saturating_sub(1)
    }

    fn activate_current(&mut self) {
        if let Some(item) = self.items.get(self.selected_index) {
            match item {
                SidebarItem::Environment { id, .. } => {
                    if self.active_env.as_deref() == Some(id) {
                        self.active_env = None;
                    } else {
                        self.active_env = Some(id.clone());
                    }
                    self.rebuild_items();
                }
                SidebarItem::WorkspaceEntry { .. } => {
                    // Workspace switching is handled in app.rs
                }
                _ => {}
            }
        }
    }

    pub fn get_selected_workspace_id(&self) -> Option<String> {
        self.items.get(self.selected_index).and_then(|item| {
            if let SidebarItem::WorkspaceEntry { id, .. } = item {
                Some(id.clone())
            } else {
                None
            }
        })
    }

    pub fn get_selected_environment_id(&self) -> Option<String> {
        self.items.get(self.selected_index).and_then(|item| {
            if let SidebarItem::Environment { id, .. } = item {
                Some(id.clone())
            } else {
                None
            }
        })
    }

    pub fn active_environment_id(&self) -> Option<String> {
        self.active_env.clone()
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        if area.width < 3 || area.height < 3 {
            return;
        }

        let inner = area;
        let line_width = inner.width.saturating_sub(4) as usize;

        // Background fill
        let bg = theme.pane_bg(is_active);
        frame.render_widget(
            Block::default().style(Style::default().bg(bg).fg(theme.foreground.as_color())),
            area,
        );

        let renaming_index = if self.is_renaming { Some(self.selected_index) } else { None };

        let rendered_items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                if renaming_index == Some(i) {
                    let display = if self.rename_input.is_empty() { " ".to_string() } else { self.rename_input.clone() };
                    return ListItem::new(Line::from(Span::styled(
                        format!(" {} ", display),
                        Style::default()
                            .fg(theme.semantic.info.as_color())
                            .bg(theme.highlight.selected_bg.as_color()),
                    )));
                }
                match item {
                SidebarItem::SectionHeader { section } => {
                    let (name, is_collapsed) = match section {
                        SidebarSection::Workspaces => (
                            "WORKSPACES",
                            self.collapsed_sections
                                .contains(&SidebarSection::Workspaces),
                        ),
                        SidebarSection::Collections => (
                            "COLLECTIONS",
                            self.collapsed_sections
                                .contains(&SidebarSection::Collections),
                        ),
                        SidebarSection::Environments => (
                            "ENVIRONMENTS",
                            self.collapsed_sections
                                .contains(&SidebarSection::Environments),
                        ),
                        SidebarSection::History => (
                            "HISTORY",
                            self.collapsed_sections.contains(&SidebarSection::History),
                        ),
                    };
                    let icon = if is_collapsed { "▸" } else { "▾" };
                    ListItem::new(Line::from(Span::styled(
                        format!(" {} {} ", icon, name),
                        Style::default()
                            .bg(theme.bg_element())
                            .fg(theme.section_title())
                            .add_modifier(Modifier::BOLD),
                    )))
                }
                SidebarItem::WorkspaceHeader { active_name } => {
                    ListItem::new(Line::from(Span::styled(
                        format!("  ▾ {} ", active_name),
                        Style::default()
                            .fg(theme.semantic.info.as_color())
                            .add_modifier(Modifier::BOLD),
                    )))
                }
                SidebarItem::WorkspaceEntry { name, active, .. } => {
                    if *active {
                        ListItem::new(Line::from(Span::styled(
                            format!("    ● {} ", name),
                            Style::default()
                                .fg(theme.semantic.success.as_color())
                                .add_modifier(Modifier::BOLD),
                        )))
                    } else {
                        ListItem::new(Line::from(Span::styled(
                            format!("    ○ {} ", name),
                            Style::default().fg(theme.foreground.as_color()),
                        )))
                    }
                }
                SidebarItem::CollectionHeader { id, name } => {
                    let icon = if self.collapsed_collections.contains(id) {
                        "▸"
                    } else {
                        "▾"
                    };
                    ListItem::new(Line::from(Span::styled(
                        format!(
                            "  {} {} ",
                            icon,
                            truncate_text(name, line_width.saturating_sub(4))
                        ),
                        Style::default().fg(theme.semantic.info.as_color()),
                    )))
                }
                SidebarItem::CollectionFolder { name, depth, .. } => {
                    let key = vec![name.clone()];
                    let icon = if self.collapsed_folders.contains(&key) {
                        "▸"
                    } else {
                        "▾"
                    };
                    let indent = "  ".repeat(*depth + 1);
                    ListItem::new(Line::from(Span::styled(
                        format!(
                            "{}{} {} ",
                            indent,
                            icon,
                            truncate_text(name, line_width.saturating_sub(indent.len() + 4))
                        ),
                        Style::default().fg(theme.semantic.warning.as_color()),
                    )))
                }
                SidebarItem::CollectionRequest { name, depth, .. } => {
                    let indent = "  ".repeat(*depth + 2);
                    ListItem::new(Line::from(Span::styled(
                        format!(
                            "{}▪ {} ",
                            indent,
                            truncate_text(name, line_width.saturating_sub(indent.len() + 4))
                        ),
                        Style::default().fg(theme.foreground.as_color()),
                    )))
                }
                SidebarItem::Environment { name, active, .. } => {
                    let indicator = if *active { "●" } else { "○" };
                    ListItem::new(Line::from(Span::styled(
                        format!(
                            "  {} {} ",
                            indicator,
                            truncate_text(name, line_width.saturating_sub(6))
                        ),
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
                    time_ago,
                    ..
                } => {
                    let method_color = match method.as_str() {
                        "GET" => theme.semantic.success.as_color(),
                        "POST" => theme.semantic.info.as_color(),
                        "PUT" | "PATCH" => theme.semantic.warning.as_color(),
                        "DELETE" => theme.semantic.error.as_color(),
                        _ => theme.foreground.as_color(),
                    };
                    let status_color = status
                        .map(|s| match s {
                            200..=299 => theme.semantic.success.as_color(),
                            300..=399 => theme.semantic.warning.as_color(),
                            400..=499 => theme.semantic.warning.as_color(),
                            500..=599 => theme.semantic.error.as_color(),
                            _ => theme.foreground.as_color(),
                        })
                        .unwrap_or(theme.foreground.as_color());
                    let status_str = status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "--".to_string());
                    let url_display = truncate_text(url, line_width.saturating_sub(12));
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(
                                format!(" {} ", method),
                                Style::default()
                                    .fg(method_color)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{} ", status_str),
                                Style::default()
                                    .fg(status_color)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(time_ago, Style::default().fg(theme.muted_color())),
                        ]),
                        Line::from(Span::styled(
                            format!("  {}", url_display),
                            Style::default().fg(theme.foreground.as_color()),
                        )),
                    ])
                }
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
            .highlight_symbol("▎");

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

fn truncate_text(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_width {
        return text.to_string();
    }

    if max_width == 1 {
        return "…".to_string();
    }

    let visible = max_width.saturating_sub(1);
    format!("{}…", chars.into_iter().take(visible).collect::<String>())
}

fn relative_time(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now - timestamp;

    let secs = duration.num_seconds().unsigned_abs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 604800 {
        format!("{}d ago", secs / 86400)
    } else if secs < 2592000 {
        format!("{}w ago", secs / 604800)
    } else {
        format!("{}mo ago", secs / 2592000)
    }
}
