use ratatui::{layout::Rect, style::Style, widgets::Paragraph, Frame};

use crate::theme::Theme;

pub struct VirtualScroll<T> {
    items: Vec<T>,
    scroll_offset: usize,
    viewport_height: usize,
}

impl<T> VirtualScroll<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            scroll_offset: 0,
            viewport_height: 0,
        }
    }

    pub fn with_items(mut self, items: Vec<T>) -> Self {
        self.items = items;
        self.scroll_offset = 0;
        self
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        let max_offset = self.items.len().saturating_sub(self.viewport_height);
        self.scroll_offset = offset.min(max_offset);
    }

    pub fn scroll_by(&mut self, delta: i64) {
        let max_offset = self.items.len().saturating_sub(self.viewport_height);
        let new_offset = (self.scroll_offset as i64 + delta).max(0) as usize;
        self.scroll_offset = new_offset.min(max_offset);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.items.len().saturating_sub(self.viewport_height);
    }

    pub fn total_items(&self) -> usize {
        self.items.len()
    }

    pub fn visible_range(&self) -> (usize, usize) {
        let start = self.scroll_offset;
        let end = (start + self.viewport_height).min(self.items.len());
        (start, end)
    }

    pub fn visible_items(&self) -> &[T] {
        let (start, end) = self.visible_range();
        &self.items[start..end]
    }

    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
        let max_offset = self.items.len().saturating_sub(height);
        if self.scroll_offset > max_offset {
            self.scroll_offset = max_offset;
        }
    }
}

pub struct Scrollbar {
    position: f64,
    thumb_size: f64,
    visible: bool,
}

impl Scrollbar {
    pub fn new() -> Self {
        Self {
            position: 0.0,
            thumb_size: 1.0,
            visible: false,
        }
    }

    pub fn update(&mut self, scroll_offset: usize, total: usize, viewport: usize) {
        if total <= viewport || viewport == 0 {
            self.visible = false;
            return;
        }
        self.visible = true;
        let max_scroll = total.saturating_sub(viewport);
        self.position = if max_scroll == 0 {
            0.0
        } else {
            scroll_offset as f64 / max_scroll as f64
        };
        self.thumb_size = viewport as f64 / total as f64;
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.visible || area.width < 1 {
            return;
        }

        let height = area.height as usize;
        if height == 0 {
            return;
        }

        let thumb_height = (self.thumb_size * height as f64).max(1.0) as usize;
        let thumb_pos = (self.position * (height - thumb_height) as f64) as usize;

        let mut scrollbar_chars = Vec::new();
        for y in 0..height {
            let in_thumb = y >= thumb_pos && y < thumb_pos + thumb_height;
            let ch = if in_thumb { "▐" } else { "│" };
            let style = if in_thumb {
                Style::default().fg(theme.semantic.info.as_color())
            } else {
                Style::default().fg(theme.muted_color())
            };
            scrollbar_chars.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                ch, style,
            )));
        }

        let paragraph = Paragraph::new(scrollbar_chars).style(Style::default());
        frame.render_widget(paragraph, area);
    }
}

impl Default for Scrollbar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_scroll_new() {
        let items = vec!["a", "b", "c"];
        let vs = VirtualScroll::new(items);
        assert_eq!(vs.total_items(), 3);
        assert_eq!(vs.scroll_offset(), 0);
    }

    #[test]
    fn test_virtual_scroll_scroll_by() {
        let items = vec!["a", "b", "c", "d", "e"];
        let mut vs = VirtualScroll::new(items);
        vs.set_viewport_height(2);
        vs.scroll_by(1);
        assert_eq!(vs.scroll_offset(), 1);
    }

    #[test]
    fn test_virtual_scroll_scroll_by_negative() {
        let items = vec!["a", "b", "c", "d", "e"];
        let mut vs = VirtualScroll::new(items);
        vs.set_viewport_height(2);
        vs.scroll_by(-1);
        assert_eq!(vs.scroll_offset(), 0);
    }

    #[test]
    fn test_virtual_scroll_scroll_to_bottom() {
        let items = vec!["a", "b", "c", "d", "e"];
        let mut vs = VirtualScroll::new(items);
        vs.set_viewport_height(2);
        vs.scroll_to_bottom();
        assert_eq!(vs.scroll_offset(), 3);
    }

    #[test]
    fn test_virtual_scroll_visible_range() {
        let items = vec!["a", "b", "c", "d", "e"];
        let mut vs = VirtualScroll::new(items);
        vs.set_viewport_height(2);
        vs.set_scroll_offset(1);
        let (start, end) = vs.visible_range();
        assert_eq!(start, 1);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_virtual_scroll_visible_items() {
        let items = vec!["a", "b", "c", "d", "e"];
        let mut vs = VirtualScroll::new(items);
        vs.set_viewport_height(2);
        vs.set_scroll_offset(2);
        let visible = vs.visible_items();
        assert_eq!(visible, &["c", "d"]);
    }

    #[test]
    fn test_scrollbar_new() {
        let sb = Scrollbar::new();
        assert!(!sb.visible);
    }

    #[test]
    fn test_scrollbar_update_hidden_when_fits() {
        let mut sb = Scrollbar::new();
        sb.update(0, 5, 10);
        assert!(!sb.visible);
    }

    #[test]
    fn test_scrollbar_update_visible_when_overflow() {
        let mut sb = Scrollbar::new();
        sb.update(0, 10, 5);
        assert!(sb.visible);
    }

    #[test]
    fn test_scrollbar_update_position() {
        let mut sb = Scrollbar::new();
        sb.update(5, 10, 5);
        assert!(sb.visible);
        assert!((sb.position - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_virtual_scroll_set_viewport_height_truncates_offset() {
        let items = vec!["a", "b", "c"];
        let mut vs = VirtualScroll::new(items);
        vs.set_scroll_offset(10);
        vs.set_viewport_height(1);
        assert_eq!(vs.scroll_offset(), 2);
    }

    #[test]
    fn test_virtual_scroll_empty() {
        let items: Vec<String> = Vec::new();
        let mut vs = VirtualScroll::new(items);
        vs.set_viewport_height(10);
        assert_eq!(vs.total_items(), 0);
        assert!(vs.visible_items().is_empty());
    }

    #[test]
    fn test_scrollbar_zero_height() {
        let mut sb = Scrollbar::new();
        sb.update(0, 10, 0);
        assert!(!sb.visible);
    }
}
