use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use yinx_core::tabs::TabManager;

use crate::theme::Theme;

pub struct TabBar {}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

impl TabBar {
    pub fn new() -> Self {
        Self {}
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        tab_manager: &TabManager,
        theme: &Theme,
        is_active: bool,
    ) {
        if area.width < 5 || area.height == 0 {
            return;
        }

        let tabs = tab_manager.tabs();
        let active_idx = tab_manager.active_idx();
        let mut spans: Vec<Span> = vec![Span::styled(
            " YINX ",
            Style::default()
                .fg(theme.pane.status_bar_bg.as_color())
                .bg(theme.border.active_color.as_color())
                .add_modifier(Modifier::BOLD),
        )];

        if tabs.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "No open requests",
                Style::default()
                    .fg(theme.muted_color()),
            ));
        } else {
            let reserved = 14usize;
            let tab_width =
                ((area.width as usize).saturating_sub(reserved) / tabs.len().max(1)).clamp(12, 28);

            for (i, tab) in tabs.iter().enumerate() {
                spans.push(Span::raw(" "));

                let dirty_indicator = if tab.dirty { "● " } else { "" };
                let display_name = if tab.title.len() > tab_width.saturating_sub(4) {
                    format!("{}…", &tab.title[..tab_width.saturating_sub(5)])
                } else {
                    tab.title.clone()
                };

                let tab_text = format!(" {}{} ", dirty_indicator, display_name);

                if i == active_idx {
                    spans.push(Span::styled(
                        tab_text,
                        Style::default()
                            .fg(theme.highlight.selected_fg.as_color())
                            .bg(theme.highlight.selected_bg.as_color())
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::styled(
                        tab_text,
                        Style::default()
                            .fg(theme.title_color(false))
                            .bg(theme.pane.inactive_background.as_ref().map(|c| c.as_color()).unwrap_or(theme.subtle_bg())),
                    ));
                }
            }

            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                " + New ",
                Style::default()
                    .fg(theme.semantic.success.as_color())
                    .bg(theme.pane.inactive_background.as_ref().map(|c| c.as_color()).unwrap_or(theme.subtle_bg()))
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).style(
            Style::default()
                .bg(theme.pane_bg(is_active))
                .fg(theme.foreground.as_color()),
        );

        if area.height == 1 {
            frame.render_widget(paragraph, area);
            return;
        }

        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border_color(is_active)))
            .style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.foreground.as_color()),
            );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(paragraph, inner);
    }
}
