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
        let mut spans: Vec<Span> = Vec::new();

        if tabs.is_empty() {
            spans.push(Span::styled(
                " No open requests ",
                Style::default()
                    .fg(theme.typography_level(3).0),
            ));
        } else {
            let reserved = 8usize;
            let tab_width =
                ((area.width as usize).saturating_sub(reserved) / tabs.len().max(1)).clamp(12, 28);

            for (i, tab) in tabs.iter().enumerate() {
                let dirty_indicator = if tab.dirty { "● " } else { "" };
                let display_name = if tab.title.len() > tab_width.saturating_sub(4) {
                    format!("{}…", &tab.title[..tab_width.saturating_sub(5)])
                } else {
                    tab.title.clone()
                };

                let tab_text = format!(" {} ", dirty_indicator);

                if i == active_idx {
                    spans.push(Span::styled(
                        format!("{}{}", tab_text, display_name),
                        Style::default()
                            .fg(theme.foreground.as_color())
                            .bg(theme.pane_bg(true))
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::styled(
                        format!(" {}{} ", dirty_indicator, display_name),
                        Style::default()
                            .fg(theme.muted_color())
                            .bg(theme.pane_bg(false)),
                    ));
                }

                if i < tabs.len() - 1 {
                    spans.push(Span::styled(
                        "│",
                        Style::default().fg(theme.muted_color()),
                    ));
                }
            }

            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "+New",
                Style::default()
                    .fg(theme.muted_color())
                    .bg(theme.pane_bg(false)),
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

        let border_color = if is_active {
            theme.border.active_color.as_color()
        } else {
            theme.dim_border_color()
        };
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(border_color))
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
