use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::HashSet;
use yinx_core::response::{Response, ResponseBody, StatusCategory};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseViewMode {
    Pretty,
    Raw,
    Headers,
    Preview,
}

impl ResponseViewMode {
    pub fn next(&self) -> Self {
        match self {
            Self::Pretty => Self::Raw,
            Self::Raw => Self::Headers,
            Self::Headers => Self::Preview,
            Self::Preview => Self::Pretty,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pretty => "Pretty",
            Self::Raw => "Raw",
            Self::Headers => "Headers",
            Self::Preview => "Preview",
        }
    }
}

use crate::virtual_scroll::VirtualScroll;

pub struct ResponsePane {
    response: Option<Response>,
    error: Option<String>,
    scroll_offset: usize,
    search_term: String,
    search_visible: bool,
    search_matches: Vec<usize>,
    search_match_set: HashSet<usize>,
    search_selected: usize,
    view_mode: ResponseViewMode,
    lines_cache: Vec<String>,
    max_visible_lines: usize,
    follow_stream: bool,
    stream_bytes: Vec<u8>,
    truncation_warned: bool,
    collapsed_large_arrays: bool,
    virtual_scroll: VirtualScroll<String>,
}

impl ResponsePane {
    pub fn new() -> Self {
        Self {
            response: None,
            error: None,
            scroll_offset: 0,
            search_term: String::new(),
            search_visible: false,
            search_matches: Vec::new(),
            search_match_set: HashSet::new(),
            search_selected: 0,
            view_mode: ResponseViewMode::Pretty,
            lines_cache: Vec::new(),
            max_visible_lines: 1000,
            follow_stream: false,
            stream_bytes: Vec::new(),
            truncation_warned: false,
            collapsed_large_arrays: false,
            virtual_scroll: VirtualScroll::new(Vec::new()),
        }
    }

    pub fn set_response(&mut self, response: Response) {
        self.response = Some(response);
        self.error = None;
        self.scroll_offset = 0;
        self.search_term.clear();
        self.search_visible = false;
        self.search_matches.clear();
        self.search_match_set.clear();
        self.search_selected = 0;
        self.follow_stream = false;
        self.stream_bytes.clear();
        self.truncation_warned = false;
        self.collapsed_large_arrays = false;
        self.rebuild_lines_cache();
        self.sync_virtual_scroll();
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error.clone());
        self.response = None;
        self.scroll_offset = 0;
        self.search_term.clear();
        self.search_visible = false;
        self.search_matches.clear();
        self.search_match_set.clear();
        self.search_selected = 0;
        self.follow_stream = false;
        self.stream_bytes.clear();
        self.truncation_warned = false;
        self.collapsed_large_arrays = false;
        self.lines_cache = build_error_lines(&error);
        self.sync_virtual_scroll();
    }

    pub fn response(&self) -> Option<&Response> {
        self.response.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn view_mode(&self) -> ResponseViewMode {
        self.view_mode
    }

    pub fn is_search_visible(&self) -> bool {
        self.search_visible
    }

    pub fn search_term(&self) -> &str {
        &self.search_term
    }

    pub fn search_match_count(&self) -> usize {
        self.search_matches.len()
    }

    pub fn search_selected_index(&self) -> usize {
        self.search_selected
    }

    pub fn has_content(&self) -> bool {
        self.response.is_some() || self.error.is_some()
    }

    pub fn total_lines(&self) -> usize {
        self.lines_cache.len()
    }

    pub fn follow_stream(&self) -> bool {
        self.follow_stream
    }

    pub fn set_follow_stream(&mut self, follow: bool) {
        self.follow_stream = follow;
    }

    pub fn stream_chunk(&mut self, chunk: Vec<u8>) {
        self.stream_bytes.extend_from_slice(&chunk);
        let text = String::from_utf8_lossy(&self.stream_bytes);
        self.lines_cache = text.lines().map(|l| l.to_string()).collect();
        self.lines_cache.truncate(self.max_visible_lines * 10);
        if self.follow_stream {
            self.scroll_offset = self
                .lines_cache
                .len()
                .saturating_sub(self.max_visible_lines);
        }
        self.sync_virtual_scroll();
    }

    fn sync_virtual_scroll(&mut self) {
        self.virtual_scroll = VirtualScroll::new(self.lines_cache.clone());
        self.virtual_scroll.set_scroll_offset(self.scroll_offset);
    }

    fn rebuild_lines_cache(&mut self) {
        self.lines_cache.clear();
        let Some(response) = &self.response else {
            if self.error.is_some() {
                self.lines_cache.push(self.error.clone().unwrap());
            }
            return;
        };

        match self.view_mode {
            ResponseViewMode::Pretty => match &response.body {
                ResponseBody::Json(v) => {
                    let collapsed = self.collapse_large_arrays(v.clone());
                    let pretty = serde_json::to_string_pretty(&collapsed).unwrap_or_default();
                    for line in pretty.lines() {
                        self.lines_cache.push(line.to_string());
                    }
                }
                ResponseBody::Text(t) => {
                    for line in t.lines() {
                        self.lines_cache.push(line.to_string());
                    }
                }
                ResponseBody::Binary(b) => {
                    self.lines_cache.push(format!("<binary {} bytes>", b.len()));
                }
                ResponseBody::Stream(b) => {
                    self.lines_cache.push(format!("<stream {} bytes>", b.len()));
                }
                ResponseBody::None => {
                    self.lines_cache.push("(empty body)".to_string());
                }
            },
            ResponseViewMode::Raw => {
                let text = match &response.body {
                    ResponseBody::Json(v) => serde_json::to_string(v).unwrap_or_default(),
                    ResponseBody::Text(t) => t.clone(),
                    ResponseBody::Binary(b) => format!("<binary {} bytes>", b.len()),
                    ResponseBody::Stream(b) => format!("<stream {} bytes>", b.len()),
                    ResponseBody::None => String::new(),
                };
                for line in text.lines() {
                    self.lines_cache.push(line.to_string());
                }
                if self.lines_cache.is_empty() {
                    self.lines_cache.push("(empty body)".to_string());
                }
            }
            ResponseViewMode::Headers => {
                for (name, value) in response.headers.to_pairs() {
                    self.lines_cache.push(format!("{}: {}", name, value));
                }
                if self.lines_cache.is_empty() {
                    self.lines_cache.push("(no headers)".to_string());
                }
            }
            ResponseViewMode::Preview => {
                let text = match &response.body {
                    ResponseBody::Json(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
                    ResponseBody::Text(t) => t.clone(),
                    ResponseBody::Binary(b) => format!("<binary {} bytes>", b.len()),
                    ResponseBody::Stream(b) => format!("<stream {} bytes>", b.len()),
                    ResponseBody::None => String::new(),
                };
                for line in text.lines() {
                    self.lines_cache.push(line.to_string());
                }
                if self.lines_cache.is_empty() {
                    self.lines_cache.push("(empty body)".to_string());
                }
            }
        }

        if self.lines_cache.len() > self.max_visible_lines * 10 && !self.truncation_warned {
            self.lines_cache.truncate(self.max_visible_lines * 10);
            let warn = format!(
                "[Response truncated: showing first {} lines]",
                self.max_visible_lines * 10
            );
            self.lines_cache.push(warn);
            self.truncation_warned = true;
        }
    }

    fn collapse_large_arrays(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(items) if items.len() > 50 => {
                let mut collapsed = Vec::with_capacity(52);
                for item in items.iter().take(50).cloned() {
                    collapsed.push(self.collapse_large_arrays(item));
                }
                collapsed.push(serde_json::Value::String(format!(
                    "[+ {} more items]",
                    items.len() - 50
                )));
                serde_json::Value::Array(collapsed)
            }
            serde_json::Value::Array(items) => serde_json::Value::Array(
                items
                    .into_iter()
                    .map(|i| self.collapse_large_arrays(i))
                    .collect(),
            ),
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, self.collapse_large_arrays(v)))
                    .collect(),
            ),
            other => other,
        }
    }

    fn update_search_matches(&mut self) {
        self.search_matches.clear();
        self.search_match_set.clear();
        if self.search_term.is_empty() || self.lines_cache.is_empty() {
            return;
        }
        let lower_term = self.search_term.to_lowercase();
        for (i, line) in self.lines_cache.iter().enumerate() {
            if line.to_lowercase().contains(&lower_term) {
                self.search_matches.push(i);
                self.search_match_set.insert(i);
            }
        }
        self.search_selected = self
            .search_matches
            .iter()
            .position(|&m| m >= self.scroll_offset)
            .unwrap_or(0);
    }

    fn scroll_to_match(&mut self) {
        if let Some(&match_line) = self.search_matches.get(self.search_selected) {
            if match_line < self.scroll_offset
                || match_line >= self.scroll_offset + self.max_visible_lines
            {
                self.scroll_offset = match_line.saturating_sub(10);
            }
        }
    }

    pub fn handle_key(&mut self, key_code: KeyCode) -> bool {
        if self.search_visible {
            return self.handle_search_key(key_code);
        }

        match key_code {
            KeyCode::Char('j') | KeyCode::Down => {
                let max = self.lines_cache.len().saturating_sub(1);
                if self.scroll_offset < max {
                    self.scroll_offset += 1;
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self
                    .scroll_offset
                    .saturating_add(self.max_visible_lines)
                    .min(self.lines_cache.len().saturating_sub(1));
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(self.max_visible_lines);
                true
            }
            KeyCode::Home | KeyCode::Char('g') => {
                if key_code == KeyCode::Char('g') && self.scroll_offset > 0 {
                    self.scroll_offset = 0;
                    true
                } else {
                    false
                }
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.scroll_offset = self
                    .lines_cache
                    .len()
                    .saturating_sub(self.max_visible_lines);
                true
            }
            KeyCode::Char('t') => {
                self.view_mode = self.view_mode.next();
                self.scroll_offset = 0;
                self.rebuild_lines_cache();
                self.update_search_matches();
                true
            }
            KeyCode::Char('/') => {
                self.search_visible = true;
                self.search_term.clear();
                true
            }
            KeyCode::Char('n') => {
                if !self.search_matches.is_empty() {
                    self.search_selected = (self.search_selected + 1) % self.search_matches.len();
                    self.scroll_to_match();
                }
                true
            }
            KeyCode::Char('N') => {
                if !self.search_matches.is_empty() {
                    self.search_selected = if self.search_selected == 0 {
                        self.search_matches.len() - 1
                    } else {
                        self.search_selected - 1
                    };
                    self.scroll_to_match();
                }
                true
            }
            KeyCode::Char('d') => {
                let half = self.max_visible_lines.saturating_div(2).max(1);
                self.scroll_offset = self
                    .scroll_offset
                    .saturating_add(half)
                    .min(self.lines_cache.len().saturating_sub(1));
                true
            }
            KeyCode::Char('u') => {
                let half = self.max_visible_lines.saturating_div(2).max(1);
                self.scroll_offset = self.scroll_offset.saturating_sub(half);
                true
            }
            _ => false,
        }
    }

    fn handle_search_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Esc => {
                self.search_visible = false;
                self.search_term.clear();
                true
            }
            KeyCode::Enter => {
                self.update_search_matches();
                if !self.search_matches.is_empty() {
                    self.scroll_to_match();
                }
                self.search_visible = false;
                true
            }
            KeyCode::Backspace => {
                self.search_term.pop();
                self.update_search_matches();
                true
            }
            KeyCode::Char(c) => {
                self.search_term.push(c);
                self.update_search_matches();
                true
            }
            _ => true,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        let title = self.build_title(theme);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(theme.tui_border_type())
            .border_style(Style::default().fg(theme.border_color(is_active)))
            .style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let inner_height = inner.height as usize;

        if self.lines_cache.is_empty() && !self.has_content() {
            let placeholder = Paragraph::new(Line::from(vec![Span::styled(
                " No response yet. Send a request to see results. ",
                Style::default()
                    .fg(theme.placeholder_color())
                    .add_modifier(Modifier::ITALIC),
            )]))
            .style(Style::default().fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: false });
            frame.render_widget(placeholder, inner);
            return;
        }

        let total_lines = self.lines_cache.len();
        let viewport = inner_height.saturating_sub(1);
        self.max_visible_lines = viewport.max(1);
        let scroll = self.scroll_offset.min(total_lines.saturating_sub(viewport));

        self.virtual_scroll.set_viewport_height(viewport);
        self.virtual_scroll.set_scroll_offset(scroll);

        let visible_lines: Vec<Line> = self
            .lines_cache
            .iter()
            .skip(scroll)
            .take(viewport)
            .enumerate()
            .map(|(i, line)| {
                let global_idx = scroll + i;
                let is_match = self.search_match_set.contains(&global_idx);
                let is_selected =
                    is_match && self.search_matches.get(self.search_selected) == Some(&global_idx);

                if is_selected {
                    Line::from(Span::styled(
                        line.clone(),
                        Style::default()
                            .bg(theme.semantic.warning.as_color())
                            .fg(ratatui::style::Color::Black),
                    ))
                } else if is_match {
                    Line::from(Span::styled(
                        line.clone(),
                        Style::default()
                            .bg(theme.semantic.info.as_color())
                            .fg(ratatui::style::Color::Black),
                    ))
                } else if self.view_mode == ResponseViewMode::Pretty {
                    self.syntax_highlight_line(line, theme)
                } else {
                    Line::from(Span::styled(
                        line.clone(),
                        Style::default().fg(theme.foreground.as_color()),
                    ))
                }
            })
            .collect();

        let paragraph = Paragraph::new(visible_lines)
            .style(Style::default().fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);

        if viewport > 0 && total_lines > viewport {
            self.render_scrollbar(frame, inner, scroll, total_lines, viewport, theme);
        }

        if self.search_visible {
            self.render_search_overlay(frame, inner, theme);
        }

        if self.follow_stream {
            let follow_area = ratatui::layout::Rect::new(
                inner.x + inner.width.saturating_sub(12),
                inner.y,
                12,
                1,
            );
            let follow_text = Paragraph::new(Line::from(Span::styled(
                " [FOLLOWING] ",
                Style::default()
                    .bg(theme.semantic.success.as_color())
                    .fg(Color::Black),
            )));
            frame.render_widget(follow_text, follow_area);
        }

        if self.truncation_warned && self.view_mode == ResponseViewMode::Pretty {
            let warn_area = ratatui::layout::Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(1),
                inner.width.min(40),
                1,
            );
            let warn_text = Paragraph::new(Line::from(Span::styled(
                " [Response truncated, press 't' for Raw view] ",
                Style::default().fg(theme.semantic.warning.as_color()),
            )));
            frame.render_widget(warn_text, warn_area);
        }
    }

    fn build_title(&self, theme: &Theme) -> Line<'static> {
        let mut spans = Vec::new();
        spans.push(Span::styled(
            " RESPONSE ",
            Style::default()
                .fg(theme.title_color(true))
                .add_modifier(Modifier::BOLD),
        ));

        if let Some(response) = &self.response {
            let status_color = match response.status.category() {
                StatusCategory::Success => theme.semantic.success.as_color(),
                StatusCategory::Redirection => theme.semantic.warning.as_color(),
                StatusCategory::ClientError => Color::Indexed(208),
                StatusCategory::ServerError => theme.semantic.error.as_color(),
                _ => theme.foreground.as_color(),
            };
            let status_str = format!("  {} ", response.status);
            spans.push(Span::styled(
                status_str,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ));

            let timing_color = if response.timing_ms < 100 {
                theme.semantic.success.as_color()
            } else if response.timing_ms < 500 {
                theme.semantic.warning.as_color()
            } else {
                theme.semantic.error.as_color()
            };
            let timing_str = format!("  {}ms ", response.timing_ms);
            spans.push(Span::styled(timing_str, Style::default().fg(timing_color)));

            spans.push(Span::raw(format!(
                "  {}  {}",
                human_size(response.body_size()),
                response.content_type().unwrap_or("unknown"),
            )));
        } else if self.error.is_some() {
            spans.push(Span::styled(
                "  ERROR ",
                Style::default()
                    .fg(theme.semantic.error.as_color())
                    .add_modifier(Modifier::BOLD),
            ));
            if let Some(error) = &self.error {
                spans.push(Span::styled(
                    format!("  {} ", truncate_inline(error, 64)),
                    Style::default().fg(theme.muted_color()),
                ));
            }
        }

        Line::from(spans)
    }

    fn render_scrollbar(
        &self,
        frame: &mut Frame,
        inner: Rect,
        scroll: usize,
        total: usize,
        viewport: usize,
        theme: &Theme,
    ) {
        if total <= viewport {
            return;
        }

        let scrollbar_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y,
            1,
            inner.height,
        );

        let thumb_height = ((viewport as f64 / total as f64) * inner.height as f64).max(1.0) as u16;
        let thumb_pos = if total > viewport {
            ((scroll as f64 / (total - viewport) as f64) * (inner.height - thumb_height) as f64)
                as u16
        } else {
            0
        };

        let mut scrollbar_chars = Vec::new();
        for y in 0..inner.height {
            let is_thumb = y >= thumb_pos && y < thumb_pos + thumb_height;
            let style = if is_thumb {
                Style::default().fg(theme.semantic.info.as_color()).bg(theme.pane_bg(true))
            } else {
                Style::default().fg(theme.muted_color()).bg(theme.pane_bg(false))
            };
            let ch = if is_thumb { "▐" } else { "·" };
            scrollbar_chars.push(Line::from(Span::styled(ch, style)));
        }

        let scrollbar = Paragraph::new(scrollbar_chars).style(Style::default());
        frame.render_widget(scrollbar, scrollbar_area);
    }

    fn render_search_overlay(&self, frame: &mut Frame, inner: Rect, theme: &Theme) {
        let search_area = Rect::new(inner.x, inner.y, inner.width.min(40), 1);
        let search_text = format!("/{}", self.search_term);
        let search_widget = Paragraph::new(Line::from(Span::styled(
            search_text,
            Style::default()
                .bg(theme.highlight.selected_bg.as_color())
                .fg(theme.highlight.selected_fg.as_color()),
        )));
        frame.render_widget(search_widget, search_area);
        let cursor_x = search_area.x + 1 + self.search_term.len() as u16;
        frame.set_cursor_position(ratatui::prelude::Position::new(
            cursor_x.min(search_area.x + search_area.width - 1),
            search_area.y,
        ));
    }

    fn syntax_highlight_line(&self, line: &str, theme: &Theme) -> Line<'_> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Line::from("");
        }

        let mut spans: Vec<Span> = Vec::new();
        let s = line.to_string();
        let bytes = s.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            let ch = bytes[i] as char;

            match ch {
                '"' => {
                    let start = i;
                    i += 1;
                    while i < len {
                        if bytes[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'"' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    let slice = &s[start..i];
                    let after = s[i..].trim_start();

                    if after.starts_with(':') {
                        spans.push(Span::styled(
                            slice.to_string(),
                            Style::default().fg(theme.semantic.info.as_color()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            slice.to_string(),
                            Style::default().fg(theme.semantic.success.as_color()),
                        ));
                    }
                }
                c if c.is_ascii_digit()
                    || (c == '-' && i + 1 < len && (bytes[i + 1] as char).is_ascii_digit()) =>
                {
                    let start = i;
                    if bytes[i] == b'-' {
                        i += 1;
                    }
                    while i < len {
                        let c = bytes[i] as char;
                        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    let slice = &s[start..i];
                    spans.push(Span::styled(
                        slice.to_string(),
                        Style::default().fg(ratatui::style::Color::Cyan),
                    ));
                }
                't' => {
                    if s[i..].starts_with("true") {
                        spans.push(Span::styled(
                            "true".to_string(),
                            Style::default().fg(theme.semantic.warning.as_color()),
                        ));
                        i += 4;
                    } else {
                        spans.push(Span::raw("t"));
                        i += 1;
                    }
                }
                'f' => {
                    if s[i..].starts_with("false") {
                        spans.push(Span::styled(
                            "false".to_string(),
                            Style::default().fg(theme.semantic.warning.as_color()),
                        ));
                        i += 5;
                    } else {
                        spans.push(Span::raw("f"));
                        i += 1;
                    }
                }
                'n' => {
                    if s[i..].starts_with("null") {
                        spans.push(Span::styled(
                            "null".to_string(),
                            Style::default().fg(theme.muted_color()),
                        ));
                        i += 4;
                    } else {
                        spans.push(Span::raw("n"));
                        i += 1;
                    }
                }
                ',' => {
                    spans.push(Span::styled(
                        ",",
                        Style::default().fg(theme.muted_color()),
                    ));
                    i += 1;
                }
                ':' => {
                    spans.push(Span::styled(
                        ": ",
                        Style::default().fg(theme.muted_color()),
                    ));
                    i += 1;
                }
                '{' | '}' | '[' | ']' => {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::default().fg(theme.pane.title.as_color()),
                    ));
                    i += 1;
                }
                _ => {
                    spans.push(Span::raw(ch.to_string()));
                    i += 1;
                }
            }
        }

        Line::from(spans)
    }
}

fn build_error_lines(error: &str) -> Vec<String> {
    let mut lines = vec![
        "Request failed.".to_string(),
        String::new(),
        "Details:".to_string(),
    ];

    for line in error.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(format!("  {trimmed}"));
        }
    }

    if lines.len() == 3 {
        lines.push("  Unknown transport error".to_string());
    }

    lines.push(String::new());
    lines.push("Checks:".to_string());
    lines.push("  - Verify the URL, DNS, and network connectivity".to_string());
    lines.push("  - Check TLS, proxy, or firewall settings".to_string());
    lines.push("  - Inspect the activity log for request lifecycle details".to_string());
    lines
}

fn truncate_inline(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

impl Default for ResponsePane {
    fn default() -> Self {
        Self::new()
    }
}

fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}b", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yinx_core::response::{Response, ResponseBody};

    #[test]
    fn test_response_pane_new() {
        let pane = ResponsePane::new();
        assert!(!pane.has_content());
        assert_eq!(pane.view_mode(), ResponseViewMode::Pretty);
    }

    #[test]
    fn test_set_response() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Json(serde_json::json!({"key": "value"})))
            .timing_ms(100)
            .build();
        pane.set_response(response);
        assert!(pane.has_content());
        assert!(pane.response().is_some());
    }

    #[test]
    fn test_set_error() {
        let mut pane = ResponsePane::new();
        pane.set_error("timeout".to_string());
        assert!(pane.has_content());
        assert_eq!(pane.error(), Some("timeout"));
        assert!(pane.total_lines() >= 4);
    }

    #[test]
    fn test_view_mode_cycle() {
        let mode = ResponseViewMode::Pretty;
        assert_eq!(mode.next(), ResponseViewMode::Raw);
        assert_eq!(mode.next().next(), ResponseViewMode::Headers);
        assert_eq!(mode.next().next().next(), ResponseViewMode::Preview);
        assert_eq!(mode.next().next().next().next(), ResponseViewMode::Pretty);
    }

    #[test]
    fn test_view_mode_as_str() {
        assert_eq!(ResponseViewMode::Pretty.as_str(), "Pretty");
        assert_eq!(ResponseViewMode::Raw.as_str(), "Raw");
        assert_eq!(ResponseViewMode::Headers.as_str(), "Headers");
        assert_eq!(ResponseViewMode::Preview.as_str(), "Preview");
    }

    #[test]
    fn test_search_no_results() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text("hello world".to_string()))
            .build();
        pane.set_response(response);
        pane.handle_key(KeyCode::Char('/'));
        assert!(pane.is_search_visible());
        pane.handle_key(KeyCode::Char('x'));
        pane.handle_key(KeyCode::Enter);
        assert_eq!(pane.search_match_count(), 0);
    }

    #[test]
    fn test_search_with_results() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text(
                "hello world\nfoo bar\nhello again".to_string(),
            ))
            .build();
        pane.set_response(response);
        pane.handle_key(KeyCode::Char('/'));
        pane.handle_key(KeyCode::Char('h'));
        pane.handle_key(KeyCode::Enter);
        assert!(pane.search_match_count() > 0);
    }

    #[test]
    fn test_scroll_down() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text("line1\nline2\nline3".to_string()))
            .build();
        pane.set_response(response);
        pane.handle_key(KeyCode::Char('j'));
        assert_eq!(pane.scroll_offset(), 1);
    }

    #[test]
    fn test_scroll_up() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text("line1\nline2\nline3".to_string()))
            .build();
        pane.set_response(response);
        pane.handle_key(KeyCode::Char('j'));
        pane.handle_key(KeyCode::Char('j'));
        pane.handle_key(KeyCode::Char('k'));
        assert_eq!(pane.scroll_offset(), 1);
    }

    #[test]
    fn test_human_size() {
        assert_eq!(human_size(500), "500b");
        assert_eq!(human_size(2048), "2.0KB");
        assert_eq!(human_size(1048576), "1.0MB");
    }

    #[test]
    fn test_json_highlighting() {
        let pane = ResponsePane::new();
        let line = pane.syntax_highlight_line(r#"  "key": "value","#, &Theme::dark());
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_empty_response_pane_render_no_panic() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::None)
            .build();
        pane.set_response(response);
        assert!(pane.has_content());
    }

    #[test]
    fn test_follow_stream_default_false() {
        let pane = ResponsePane::new();
        assert!(!pane.follow_stream());
    }

    #[test]
    fn test_set_follow_stream() {
        let mut pane = ResponsePane::new();
        pane.set_follow_stream(true);
        assert!(pane.follow_stream());
        pane.set_follow_stream(false);
        assert!(!pane.follow_stream());
    }

    #[test]
    fn test_stream_chunk_appends() {
        let mut pane = ResponsePane::new();
        pane.stream_chunk(b"hello ".to_vec());
        assert!(pane.total_lines() > 0);
        let first = pane.total_lines();
        pane.stream_chunk(b"world".to_vec());
        assert!(pane.total_lines() >= first);
    }

    #[test]
    fn test_follow_stream_scrolls_to_bottom() {
        let mut pane = ResponsePane::new();
        pane.set_follow_stream(true);
        pane.max_visible_lines = 2;
        for i in 0..10 {
            pane.stream_chunk(format!("line {}\n", i).into_bytes());
        }
        // follow_stream should have scrolled to near the bottom
        assert!(pane.scroll_offset() > 5);
    }

    #[test]
    fn test_scroll_half_page_down() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text(
                (0..100).map(|i| format!("line{}\n", i)).collect::<String>(),
            ))
            .build();
        pane.set_response(response);
        pane.max_visible_lines = 20;
        // Ctrl+d goes down by half viewport
        pane.handle_key(KeyCode::Char('d'));
        assert!(pane.scroll_offset() >= 10);
    }

    #[test]
    fn test_scroll_half_page_up() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text(
                (0..100).map(|i| format!("line{}\n", i)).collect::<String>(),
            ))
            .build();
        pane.set_response(response);
        pane.max_visible_lines = 20;
        pane.scroll_offset = 50;
        // Ctrl+u goes up by half viewport
        pane.handle_key(KeyCode::Char('u'));
        assert!(pane.scroll_offset <= 40);
    }

    #[test]
    fn test_collapse_large_arrays() {
        let pane = ResponsePane::new();
        let large: Vec<serde_json::Value> = (0..100).map(|i| serde_json::json!(i)).collect();
        let value = serde_json::Value::Array(large);
        let collapsed = pane.collapse_large_arrays(value);
        if let serde_json::Value::Array(items) = collapsed {
            assert_eq!(items.len(), 51); // 50 items + 1 placeholder
            assert!(items[50].is_string());
            assert!(items[50].as_str().unwrap().contains("50 more items"));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_small_arrays_not_collapsed() {
        let pane = ResponsePane::new();
        let small: Vec<serde_json::Value> = (0..10).map(|i| serde_json::json!(i)).collect();
        let value = serde_json::Value::Array(small);
        let collapsed = pane.collapse_large_arrays(value);
        if let serde_json::Value::Array(items) = collapsed {
            assert_eq!(items.len(), 10);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_preview_view_mode() {
        let mut pane = ResponsePane::new();
        let response = Response::builder()
            .status(200)
            .body(ResponseBody::Text(
                "<html><body>Hello</body></html>".to_string(),
            ))
            .build();
        pane.view_mode = ResponseViewMode::Preview;
        pane.set_response(response);
        let has_html = pane.lines_cache.iter().any(|l| l.contains("<html>"));
        assert!(has_html);
    }

    #[test]
    fn test_stream_chunk_empty() {
        let mut pane = ResponsePane::new();
        pane.stream_chunk(Vec::new());
        assert_eq!(pane.total_lines(), 0);
    }
}
