use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
        Tabs, Wrap,
    },
    Frame,
};

use yinx_core::state::NetworkState;
use yinx_http::streaming::{SnapshotKind, TimelineJumpTarget, TimelineState};

use crate::theme::Theme;

/// Renders a panel with lazygit/k9s-style title embedded in the top border.
/// Active pane gets bright border + accent title; inactive gets dim border + muted title.
pub fn render_panel(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    title: &str,
    is_active: bool,
    level: u8,
) {
    if area.width < 4 || area.height < 2 {
        return;
    }

    let (title_color, title_mod) = theme.typography_level(level);
    let border_color = if is_active {
        theme.border.active_color.as_color()
    } else {
        theme.dim_border_color()
    };
    let bg = theme.pane_bg(is_active);

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title,
                Style::default().fg(title_color).add_modifier(title_mod),
            ),
        ]))
        .borders(Borders::ALL)
        .border_type(theme.tui_border_type())
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg).fg(theme.foreground.as_color()));

    frame.render_widget(block, area);
}

pub struct Panel<'a> {
    title: &'a str,
    is_active: bool,
    border_style: Option<BorderType>,
}

impl<'a> Panel<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            is_active: false,
            border_style: None,
        }
    }

    pub fn active(mut self, is_active: bool) -> Self {
        self.is_active = is_active;
        self
    }

    pub fn border_type(mut self, border_type: BorderType) -> Self {
        self.border_style = Some(border_type);
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        render_panel(frame, area, theme, self.title, self.is_active, 0);
    }
}

pub struct ScrollableList<'a> {
    items: Vec<String>,
    selected: Option<usize>,
    title: Option<&'a str>,
}

impl<'a> ScrollableList<'a> {
    pub fn new(items: Vec<String>) -> Self {
        Self {
            items,
            selected: None,
            title: None,
        }
    }

    pub fn with_selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|i| ListItem::new(i.as_str()))
            .collect();

        let list = List::new(items)
            .style(Style::default().fg(theme.foreground.as_color()))
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color())
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        let mut state = ListState::default();
        if let Some(sel) = self.selected {
            state.select(Some(sel));
        }

        let block = if let Some(title) = self.title {
            Block::default()
                .title(Line::from(Span::styled(
                    format!(" {} ", title),
                    Style::default().fg(theme.section_title()),
                )))
                .style(
                    Style::default()
                        .bg(theme.pane_bg(false))
                        .fg(theme.foreground.as_color()),
                )
        } else {
            Block::default().style(
                Style::default()
                    .bg(theme.pane_bg(false))
                    .fg(theme.foreground.as_color()),
            )
        };

        let list = list.block(block);
        frame.render_stateful_widget(list, area, &mut state);
    }
}

pub struct TableWidget<'a> {
    headers: Vec<&'a str>,
    rows: Vec<Vec<String>>,
    selected_row: Option<usize>,
    sort_col: Option<usize>,
    sort_asc: bool,
}

impl<'a> TableWidget<'a> {
    pub fn new(headers: Vec<&'a str>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
            selected_row: None,
            sort_col: None,
            sort_asc: true,
        }
    }

    pub fn with_rows(mut self, rows: Vec<Vec<String>>) -> Self {
        self.rows = rows;
        self
    }

    pub fn with_selected(mut self, selected: Option<usize>) -> Self {
        self.selected_row = selected;
        self
    }

    pub fn with_sort(mut self, col: Option<usize>, asc: bool) -> Self {
        self.sort_col = col;
        self.sort_asc = asc;
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let header_cells: Vec<Cell> = self
            .headers
            .iter()
            .map(|h| {
                let mut text = h.to_string();
                if let Some(col) = self.sort_col {
                    if col < self.headers.len() && self.headers[col] == *h {
                        text.push_str(if self.sort_asc { " ▲" } else { " ▼" });
                    }
                }
                Cell::from(text).style(
                    Style::default()
                        .fg(theme.pane.title.as_color())
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect();

        let header = Row::new(header_cells).height(1);

        let rows: Vec<Row> = self
            .rows
            .iter()
            .map(|row| {
                let cells: Vec<Cell> = row.iter().map(|c| Cell::from(c.as_str())).collect();
                Row::new(cells).height(1)
            })
            .collect();

        let constraints: Vec<Constraint> = self
            .headers
            .iter()
            .map(|_| Constraint::Percentage(100 / self.headers.len().max(1) as u16))
            .collect();

        let table = Table::new(rows, &constraints)
            .header(header)
            .block(
                Block::default().style(
                    Style::default()
                        .bg(theme.pane_bg(false))
                        .fg(theme.foreground.as_color()),
                ),
            )
            .row_highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color()),
            )
            .column_spacing(1);

        let mut state = ratatui::widgets::TableState::default();
        if let Some(sel) = self.selected_row {
            state.select(Some(sel));
        }

        frame.render_stateful_widget(table, area, &mut state);
    }
}

pub struct TabsWidget<'a> {
    titles: Vec<&'a str>,
    selected: usize,
}

impl<'a> TabsWidget<'a> {
    pub fn new(titles: Vec<&'a str>) -> Self {
        Self {
            titles,
            selected: 0,
        }
    }

    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected.min(self.titles.len().saturating_sub(1));
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let titles: Vec<Line> = self
            .titles
            .iter()
            .map(|t| Line::from(t.to_string()))
            .collect();

        let tabs = Tabs::new(titles)
            .select(self.selected)
            .style(
                Style::default()
                    .bg(theme.pane_bg(false))
                    .fg(theme.foreground.as_color()),
            )
            .highlight_style(
                Style::default()
                    .fg(theme.highlight.selected_fg.as_color())
                    .bg(theme.highlight.selected_bg.as_color())
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(tabs, area);
    }
}

pub struct InputField<'a> {
    content: &'a str,
    cursor_pos: usize,
    title: Option<&'a str>,
    is_focused: bool,
}

impl<'a> InputField<'a> {
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            cursor_pos: content.len(),
            title: None,
            is_focused: false,
        }
    }

    pub fn with_cursor(mut self, cursor_pos: usize) -> Self {
        self.cursor_pos = cursor_pos.min(self.content.len());
        self
    }

    pub fn with_title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    pub fn focused(mut self, is_focused: bool) -> Self {
        self.is_focused = is_focused;
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let title_style = if self.is_focused {
            Style::default()
                .fg(theme.section_title())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted())
        };

        let block = if let Some(title) = self.title {
            Block::default()
                .title(Line::from(Span::styled(
                    format!(" {} ", title),
                    title_style,
                )))
                .style(
                    Style::default()
                        .bg(theme.bg_element())
                        .fg(theme.foreground.as_color()),
                )
        } else {
            Block::default().style(
                Style::default()
                    .bg(theme.bg_element())
                    .fg(theme.foreground.as_color()),
            )
        };

        let paragraph = Paragraph::new(self.content)
            .block(block)
            .style(Style::default().fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);

        if self.cursor_pos <= self.content.len() {
            let x_offset = self.content[..self.cursor_pos].chars().count() as u16;
            frame.set_cursor_position(ratatui::prelude::Position::new(
                area.x + 1 + x_offset,
                area.y + 1,
            ));
        }
    }
}

pub struct StatusBar<'a> {
    hints: Vec<(&'a str, &'a str)>,
    mode: &'a str,
    network_state: Option<&'a yinx_core::state::NetworkState>,
    cursor_line: usize,
    cursor_col: usize,
    left: &'a str,
    center: &'a str,
    right: &'a str,
    status_code: Option<u16>,
    response_time_ms: Option<u128>,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: &'a str) -> Self {
        Self {
            hints: Vec::new(),
            mode,
            network_state: None,
            cursor_line: 0,
            cursor_col: 0,
            left: "",
            center: "",
            right: "",
            status_code: None,
            response_time_ms: None,
        }
    }

    pub fn with_hints(mut self, hints: Vec<(&'a str, &'a str)>) -> Self {
        self.hints = hints;
        self
    }

    pub fn with_network_state(mut self, state: &'a yinx_core::state::NetworkState) -> Self {
        self.network_state = Some(state);
        self
    }

    pub fn with_cursor(mut self, line: usize, col: usize) -> Self {
        self.cursor_line = line;
        self.cursor_col = col;
        self
    }

    pub fn with_left(mut self, left: &'a str) -> Self {
        self.left = left;
        self
    }

    pub fn with_center(mut self, center: &'a str) -> Self {
        self.center = center;
        self
    }

    pub fn with_right(mut self, right: &'a str) -> Self {
        self.right = right;
        self
    }

    pub fn set_response_info(&mut self, status: u16, time_ms: u128) {
        self.status_code = Some(status);
        self.response_time_ms = Some(time_ms);
    }

    fn mode_color(&self, theme: &Theme) -> ratatui::style::Color {
        match self.mode {
            "NORMAL" => theme.semantic.info.as_color(),
            "INSERT" => theme.semantic.success.as_color(),
            "VISUAL" => theme.semantic.warning.as_color(),
            _ => theme.foreground.as_color(),
        }
    }

    fn network_status_text(&self) -> &'static str {
        match self.network_state {
            Some(NetworkState::Idle) => "IDLE",
            Some(NetworkState::Loading) => "LOADING",
            Some(NetworkState::Streaming) => "STREAMING",
            Some(NetworkState::Error(_)) => "ERROR",
            None => "IDLE",
        }
    }

    fn network_status_color(&self, theme: &Theme) -> ratatui::style::Color {
        match self.network_state {
            Some(NetworkState::Idle) => theme.semantic.success.as_color(),
            Some(NetworkState::Loading) => theme.semantic.warning.as_color(),
            Some(NetworkState::Streaming) => theme.semantic.info.as_color(),
            Some(NetworkState::Error(_)) => theme.semantic.error.as_color(),
            None => theme.foreground.as_color(),
        }
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.width < 10 || area.height == 0 {
            return;
        }

        let mode_span = Span::styled(
            format!(" {} ", self.mode),
            Style::default()
                .fg(theme.pane.status_bar_bg.as_color())
                .bg(self.mode_color(theme))
                .add_modifier(Modifier::BOLD),
        );

        let network_span = Span::styled(
            format!("{} ", self.network_status_text()),
            Style::default()
                .fg(self.network_status_color(theme))
                .add_modifier(Modifier::BOLD),
        );

        let hint_spans: Vec<Span> = self
            .hints
            .iter()
            .flat_map(|(key, desc)| {
                vec![
                    Span::styled(*key, Style::default().fg(theme.muted_color())),
                    Span::raw(" "),
                    Span::styled(*desc, Style::default().fg(theme.foreground.as_color())),
                    Span::raw("  "),
                ]
            })
            .collect();

        let mut line = vec![Span::raw(" "), mode_span, Span::raw("  "), network_span];

        if !self.left.is_empty() {
            line.push(Span::styled(
                format!(" {}  ", self.left),
                Style::default().fg(theme.semantic.info.as_color()),
            ));
        }

        if !self.center.is_empty() {
            line.push(Span::styled(
                format!(" {}  ", self.center),
                Style::default().fg(theme.title_color(true)),
            ));
        }

        if !self.right.is_empty() {
            line.push(Span::styled(
                format!(" {}  ", self.right),
                Style::default().fg(theme.foreground.as_color()),
            ));
        }

        line.push(Span::styled(
            format!(
                " Ln {}, Col {}  ",
                self.cursor_line + 1,
                self.cursor_col + 1
            ),
            Style::default().fg(theme.muted_color()),
        ));
        line.extend(hint_spans);

        let border_color = theme.border.color.as_color();
        let bg_color = theme.pane.status_bar_bg.as_color();
        let fg_color = theme.pane.status_bar_fg.as_color();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme.tui_border_type())
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(bg_color).fg(fg_color));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let paragraph = Paragraph::new(Line::from(line))
            .style(Style::default().bg(bg_color).fg(fg_color))
            .alignment(Alignment::Left);
        frame.render_widget(paragraph, inner);
    }
}

pub struct TimelineWidget {
    timeline: TimelineState,
    title: String,
}

impl TimelineWidget {
    pub fn new(timeline: TimelineState) -> Self {
        Self {
            timeline,
            title: "Timeline".to_string(),
        }
    }

    pub fn timeline(&self) -> &TimelineState {
        &self.timeline
    }

    pub fn timeline_mut(&mut self) -> &mut TimelineState {
        &mut self.timeline
    }

    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Left => self.timeline.move_prev(),
            KeyCode::Right => self.timeline.move_next(),
            KeyCode::Home | KeyCode::Char('t') => {
                self.timeline.jump_to(TimelineJumpTarget::Ttfb).is_some()
            }
            KeyCode::Char('e') => self
                .timeline
                .jump_to(TimelineJumpTarget::FirstError)
                .is_some(),
            KeyCode::End | KeyCode::Char('f') => self
                .timeline
                .jump_to(TimelineJumpTarget::LastChunk)
                .is_some(),
            _ => false,
        }
    }

    pub fn progress_line(&self, width: u16) -> String {
        let width = width.max(3) as usize;
        let bar_width = width.saturating_sub(2);

        if self.timeline.is_empty() {
            return format!("[{}] 0/0", "-".repeat(bar_width));
        }

        let mut chars = vec!['-'; bar_width];
        let len = self.timeline.len();
        let current = self.timeline.current_index().unwrap_or(0);
        let current_pos = if len <= 1 {
            0
        } else {
            current * bar_width.saturating_sub(1) / (len - 1)
        };

        for ch in chars.iter_mut().take(current_pos) {
            *ch = '=';
        }
        if current_pos < chars.len() {
            chars[current_pos] = '|';
        }

        format!(
            "[{}] {}/{}",
            chars.into_iter().collect::<String>(),
            current + 1,
            len
        )
    }

    pub fn marker_line(&self, width: u16) -> String {
        let width = width.max(1) as usize;
        let mut chars = vec![' '; width];
        let len = self.timeline.len();

        if len == 0 {
            return chars.into_iter().collect();
        }

        for index in 0..len {
            if let Some(snapshot) = self.timeline.snapshot(index) {
                let pos = if len <= 1 {
                    0
                } else {
                    index * width.saturating_sub(1) / (len - 1)
                };

                chars[pos] = match snapshot.kind {
                    SnapshotKind::Ttfb => 'T',
                    SnapshotKind::Error => 'E',
                    SnapshotKind::LastChunk => 'F',
                    SnapshotKind::ChunkBoundary => '.',
                };
            }
        }

        chars.into_iter().collect()
    }

    pub fn summary_line(&self) -> String {
        match self.timeline.current_snapshot() {
            Some(snapshot) => format!(
                "{} @ {} bytes",
                match snapshot.kind {
                    SnapshotKind::Ttfb => "TTFB",
                    SnapshotKind::Error => "ERROR",
                    SnapshotKind::LastChunk => "FINAL",
                    SnapshotKind::ChunkBoundary => "CHUNK",
                },
                snapshot.offset
            ),
            None => "No snapshots recorded".to_string(),
        }
    }

    pub fn diff_line(&self) -> String {
        let Some(current) = self.timeline.current_index() else {
            return "Diff: n/a".to_string();
        };

        if current == 0 {
            return "Diff: start of timeline".to_string();
        }

        match self.timeline.diff(current - 1, current) {
            Some(diff) => {
                let rendered = diff.render().replace('\n', " ");
                format!("Diff: {rendered}")
            }
            None => "Diff: binary or unavailable".to_string(),
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let lines = vec![
            Line::from(self.progress_line(area.width.saturating_sub(2))),
            Line::from(self.marker_line(area.width.saturating_sub(2))),
            Line::from(self.summary_line()),
            Line::from(self.diff_line()),
        ];

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Line::from(Span::styled(
                        format!(" {} ", self.title),
                        Style::default().fg(theme.section_title()),
                    )))
                    .style(
                        Style::default()
                            .bg(theme.pane_bg(false))
                            .fg(theme.foreground.as_color()),
                    ),
            )
            .style(Style::default().fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}

pub struct Modal<'a> {
    title: &'a str,
    content: Vec<Line<'a>>,
    actions: Vec<(&'a str, &'a str)>,
    selected_action: usize,
}

impl<'a> Modal<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            content: Vec::new(),
            actions: Vec::new(),
            selected_action: 0,
        }
    }

    pub fn with_content(mut self, content: Vec<Line<'a>>) -> Self {
        self.content = content;
        self
    }

    pub fn with_actions(mut self, actions: Vec<(&'a str, &'a str)>) -> Self {
        self.actions = actions;
        self
    }

    pub fn with_selected_action(mut self, selected: usize) -> Self {
        self.selected_action = selected.min(self.actions.len().saturating_sub(1));
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let popup_area = centered_rect(60, 40, area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(self.title)
            .borders(Borders::ALL)
            .border_type(theme.tui_border_type())
            .border_style(Style::default().fg(theme.border.active_color.as_color()))
            .style(
                Style::default()
                    .bg(theme.pane.bg_color())
                    .fg(theme.foreground.as_color()),
            );

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut lines = self.content.clone();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::raw("Actions: ")]));

        for (i, (key, label)) in self.actions.iter().enumerate() {
            let style = if i == self.selected_action {
                Style::default()
                    .fg(theme.highlight.selected_fg.as_color())
                    .bg(theme.highlight.selected_bg.as_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.foreground.as_color())
            };
            lines.push(Line::from(vec![Span::styled(
                format!("[{}] {}", key, label),
                style,
            )]));
        }

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, inner);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::theme::Theme;
    use yinx_http::streaming::{SnapshotKind, TimelineSnapshot, TimelineState};

    #[test]
    fn test_panel_new() {
        let panel = Panel::new("Test");
        assert_eq!(panel.title, "Test");
        assert!(!panel.is_active);
    }

    #[test]
    fn test_panel_active() {
        let panel = Panel::new("Test").active(true);
        assert!(panel.is_active);
    }

    #[test]
    fn test_panel_border_type() {
        let panel = Panel::new("Test").border_type(BorderType::Double);
        assert!(panel.border_style.is_some());
    }

    #[test]
    fn test_scrollable_list_new() {
        let items = vec!["a".to_string(), "b".to_string()];
        let list = ScrollableList::new(items);
        assert_eq!(list.items.len(), 2);
    }

    #[test]
    fn test_scrollable_list_with_selected() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let list = ScrollableList::new(items).with_selected(Some(1));
        assert_eq!(list.selected, Some(1));
    }

    #[test]
    fn test_scrollable_list_with_title() {
        let items = vec!["a".to_string()];
        let list = ScrollableList::new(items).with_title("My List");
        assert_eq!(list.title, Some("My List"));
    }

    #[test]
    fn test_table_widget_new() {
        let headers = vec!["Name", "Value"];
        let table = TableWidget::new(headers.clone());
        assert_eq!(table.headers.len(), 2);
    }

    #[test]
    fn test_table_widget_with_rows() {
        let headers = vec!["Name", "Value"];
        let rows = vec![
            vec!["foo".to_string(), "1".to_string()],
            vec!["bar".to_string(), "2".to_string()],
        ];
        let table = TableWidget::new(headers).with_rows(rows.clone());
        assert_eq!(table.rows.len(), 2);
    }

    #[test]
    fn test_table_widget_with_sort() {
        let headers = vec!["Name"];
        let table = TableWidget::new(headers).with_sort(Some(0), false);
        assert_eq!(table.sort_col, Some(0));
        assert!(!table.sort_asc);
    }

    #[test]
    fn test_tabs_widget_new() {
        let titles = vec!["Tab1", "Tab2", "Tab3"];
        let tabs = TabsWidget::new(titles.clone());
        assert_eq!(tabs.titles.len(), 3);
    }

    #[test]
    fn test_tabs_widget_with_selected() {
        let titles = vec!["Tab1", "Tab2", "Tab3"];
        let tabs = TabsWidget::new(titles).with_selected(2);
        assert_eq!(tabs.selected, 2);
    }

    #[test]
    fn test_tabs_widget_selected_out_of_bounds() {
        let titles = vec!["Tab1", "Tab2"];
        let tabs = TabsWidget::new(titles).with_selected(10);
        assert_eq!(tabs.selected, 1);
    }

    #[test]
    fn test_input_field_new() {
        let field = InputField::new("hello");
        assert_eq!(field.content, "hello");
        assert_eq!(field.cursor_pos, 5);
    }

    #[test]
    fn test_input_field_with_cursor() {
        let field = InputField::new("hello").with_cursor(2);
        assert_eq!(field.cursor_pos, 2);
    }

    #[test]
    fn test_input_field_cursor_out_of_bounds() {
        let field = InputField::new("hi").with_cursor(100);
        assert_eq!(field.cursor_pos, 2);
    }

    #[test]
    fn test_status_bar_new() {
        let bar = StatusBar::new("NORMAL");
        assert_eq!(bar.mode, "NORMAL");
        assert!(bar.hints.is_empty());
        assert!(bar.network_state.is_none());
        assert_eq!(bar.cursor_line, 0);
        assert_eq!(bar.cursor_col, 0);
    }

    #[test]
    fn test_status_bar_with_hints() {
        let hints = vec![("q", "quit"), ("i", "insert")];
        let bar = StatusBar::new("NORMAL").with_hints(hints);
        assert_eq!(bar.hints.len(), 2);
    }

    #[test]
    fn test_status_bar_with_network_state() {
        let state = NetworkState::Loading;
        let bar = StatusBar::new("NORMAL").with_network_state(&state);
        assert!(bar.network_state.is_some());
    }

    #[test]
    fn test_status_bar_with_cursor() {
        let bar = StatusBar::new("NORMAL").with_cursor(10, 5);
        assert_eq!(bar.cursor_line, 10);
        assert_eq!(bar.cursor_col, 5);
    }

    #[test]
    fn test_status_bar_mode_colors() {
        let bar_normal = StatusBar::new("NORMAL");
        let bar_insert = StatusBar::new("INSERT");
        let bar_visual = StatusBar::new("VISUAL");
        assert_eq!(bar_normal.mode, "NORMAL");
        assert_eq!(bar_insert.mode, "INSERT");
        assert_eq!(bar_visual.mode, "VISUAL");
    }

    #[test]
    fn test_modal_new() {
        let modal = Modal::new("Confirm");
        assert_eq!(modal.title, "Confirm");
        assert!(modal.content.is_empty());
    }

    #[test]
    fn test_modal_with_actions() {
        let actions = vec![("y", "Yes"), ("n", "No")];
        let modal = Modal::new("Confirm").with_actions(actions);
        assert_eq!(modal.actions.len(), 2);
    }

    #[test]
    fn test_modal_with_selected_action() {
        let actions = vec![("y", "Yes"), ("n", "No")];
        let modal = Modal::new("Confirm")
            .with_actions(actions)
            .with_selected_action(1);
        assert_eq!(modal.selected_action, 1);
    }

    #[test]
    fn test_modal_selected_action_out_of_bounds() {
        let actions = vec![("y", "Yes")];
        let modal = Modal::new("Confirm")
            .with_actions(actions)
            .with_selected_action(10);
        assert_eq!(modal.selected_action, 0);
    }

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(50, 30, area);
        assert!(result.width > 0);
        assert!(result.height > 0);
    }

    #[test]
    fn test_theme_dark_has_colors() {
        let theme = Theme::dark();
        let _ = theme.background.map(|c| c.as_color());
        let _ = theme.foreground.as_color();
        let _ = theme.border.color.as_color();
        let _ = theme.highlight.selected_bg.as_color();
        let _ = theme.semantic.success.as_color();
    }

    fn sample_timeline() -> TimelineState {
        let mut timeline = TimelineState::new();
        timeline.push_snapshot(TimelineSnapshot::from_text("hello").with_kind(SnapshotKind::Ttfb));
        timeline.push_snapshot(TimelineSnapshot::from_text("hello!"));
        timeline.push_snapshot(
            TimelineSnapshot::from_text("hello! done").with_kind(SnapshotKind::LastChunk),
        );
        timeline
    }

    #[test]
    fn test_timeline_widget_handle_key_left_right() {
        let mut widget = TimelineWidget::new(sample_timeline());
        assert_eq!(widget.timeline().current_index(), Some(2));

        assert!(widget.handle_key(crossterm::event::KeyCode::Left));
        assert_eq!(widget.timeline().current_index(), Some(1));
        assert!(widget.handle_key(crossterm::event::KeyCode::Right));
        assert_eq!(widget.timeline().current_index(), Some(2));
    }

    #[test]
    fn test_timeline_widget_handle_key_jump_targets() {
        let mut widget = TimelineWidget::new(sample_timeline());

        assert!(widget.handle_key(crossterm::event::KeyCode::Char('t')));
        assert_eq!(widget.timeline().current_index(), Some(0));
        assert!(!widget.handle_key(crossterm::event::KeyCode::Char('e')));
        assert_eq!(widget.timeline().current_index(), Some(0));
        assert!(widget.handle_key(crossterm::event::KeyCode::Char('f')));
        assert_eq!(widget.timeline().current_index(), Some(2));
    }

    #[test]
    fn test_timeline_widget_render_lines_include_progress_and_markers() {
        let widget = TimelineWidget::new(sample_timeline());
        let progress = widget.progress_line(12);
        let markers = widget.marker_line(12);
        let summary = widget.summary_line();
        let diff = widget.diff_line();

        assert!(progress.contains('['));
        assert!(progress.contains('|'));
        assert!(progress.contains("3/3"));
        assert!(markers.contains('T'));
        assert!(markers.contains('F'));
        assert!(summary.contains("FINAL"));
        assert!(diff.contains('+'));
    }

    #[test]
    fn test_status_bar_has_three_sections() {
        let bar = StatusBar::new("NORMAL")
            .with_left("Yinx")
            .with_center("GET https://example.com")
            .with_right("100ms");

        assert_eq!(bar.left, "Yinx");
        assert_eq!(bar.center, "GET https://example.com");
        assert_eq!(bar.right, "100ms");
    }

    #[test]
    fn test_mode_pill_color_matches_mode() {
        let bar = StatusBar::new("NORMAL");
        let color = bar.mode_color(&Theme::dark());
        assert_eq!(color, Theme::dark().semantic.info.as_color());
    }

    #[test]
    fn test_statusline_shows_response_info() {
        let mut bar = StatusBar::new("NORMAL");
        bar.set_response_info(200, 150);

        assert_eq!(bar.status_code, Some(200));
        assert_eq!(bar.response_time_ms, Some(150));
    }
}
