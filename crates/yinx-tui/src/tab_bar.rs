use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
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
        let band_bg = theme.bg_element();

        if tabs.is_empty() {
            spans.push(Span::styled(
                " No open requests ",
                Style::default()
                    .fg(theme.typography_level(3).0)
                    .add_modifier(Modifier::BOLD),
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
                            .bg(band_bg)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::styled(
                        format!(" {}{} ", dirty_indicator, display_name),
                        Style::default().fg(theme.typography_level(3).0).bg(band_bg),
                    ));
                }

                if i < tabs.len() - 1 {
                    spans.push(Span::styled("│", Style::default().fg(theme.muted_color())));
                }
            }

            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "+New",
                Style::default().fg(theme.typography_level(3).0).bg(band_bg),
            ));
        }

        let line_area = Rect::new(area.x, area.y, area.width, 1);
        let paragraph = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(band_bg).fg(theme.foreground.as_color()));
        frame.render_widget(paragraph, line_area);

        if area.height > 1 {
            let divider_area = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
            let divider = Paragraph::new("─".repeat(area.width as usize)).style(
                Style::default()
                    .bg(theme.pane_bg(is_active))
                    .fg(theme.dim_border_color()),
            );
            frame.render_widget(divider, divider_area);
        }
    }
}
