use ratatui::layout::{Constraint, Direction, Layout as RatatuiLayout, Rect};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PaneConstraints {
    pub min_width: u16,
    pub min_height: u16,
    pub max_width: Option<u16>,
    pub max_height: Option<u16>,
    pub priority: u8,
}

impl Default for PaneConstraints {
    fn default() -> Self {
        Self {
            min_width: 20,
            min_height: 10,
            max_width: None,
            max_height: None,
            priority: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LayoutPreset {
    Default,
    Mixed,
    Wide,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LayoutConfig {
    pub request_pane_ratio: f32,
    pub response_pane_ratio: f32,
    pub workflow_pane_ratio: f32,
    pub logs_pane_ratio: f32,
    pub horizontal_split: bool,
    pub request_pane_width: u16,
    pub response_pane_height: u16,
    pub gutter: u16,
    pub preset: LayoutPreset,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            request_pane_ratio: 0.3,
            response_pane_ratio: 0.65,
            workflow_pane_ratio: 0.0,
            logs_pane_ratio: 0.35,
            horizontal_split: true,
            request_pane_width: 30,
            response_pane_height: 20,
            gutter: 0,
            preset: LayoutPreset::Wide,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneRects {
    pub request: Rect,
    pub response: Rect,
    pub logs: Rect,
    pub status_bar: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutContext {
    pub show_logs: bool,
    pub compact_logs: bool,
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self {
            show_logs: true,
            compact_logs: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutState {
    pub config: LayoutConfig,
    pub constraints: PaneConstraintsMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneConstraintsMap {
    pub request: PaneConstraints,
    pub response: PaneConstraints,
    pub logs: PaneConstraints,
    pub status_bar: PaneConstraints,
}

impl Default for PaneConstraintsMap {
    fn default() -> Self {
        Self {
            request: PaneConstraints {
                min_width: 30,
                min_height: 5,
                priority: 3,
                ..Default::default()
            },
            response: PaneConstraints {
                min_width: 40,
                min_height: 5,
                priority: 4,
                ..Default::default()
            },
            logs: PaneConstraints {
                min_width: 20,
                min_height: 5,
                priority: 1,
                ..Default::default()
            },
            status_bar: PaneConstraints {
                min_width: 10,
                min_height: 1,
                max_height: Some(3),
                priority: 0,
                ..Default::default()
            },
        }
    }
}

pub struct Layout {
    state: LayoutState,
    terminal_size: (u16, u16),
}

impl Layout {
    pub fn new() -> Self {
        Self {
            state: LayoutState::default(),
            terminal_size: (80, 24),
        }
    }

    pub fn with_config(config: LayoutConfig) -> Self {
        Self {
            state: LayoutState {
                config,
                ..Default::default()
            },
            terminal_size: (80, 24),
        }
    }

    pub fn with_preset(preset: LayoutPreset) -> Self {
        Self {
            state: LayoutState {
                config: LayoutConfig {
                    preset,
                    ..Default::default()
                },
                ..Default::default()
            },
            terminal_size: (80, 24),
        }
    }

    pub fn update_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_size = (width, height);
    }

    pub fn terminal_size(&self) -> (u16, u16) {
        self.terminal_size
    }

    pub fn config(&self) -> &LayoutConfig {
        &self.state.config
    }

    pub fn config_mut(&mut self) -> &mut LayoutConfig {
        &mut self.state.config
    }

    pub fn constraints(&self) -> &PaneConstraintsMap {
        &self.state.constraints
    }

    pub fn constraints_mut(&mut self) -> &mut PaneConstraintsMap {
        &mut self.state.constraints
    }

    pub fn calculate(&self) -> PaneRects {
        self.calculate_with_context(LayoutContext::default())
    }

    pub fn calculate_with_context(&self, context: LayoutContext) -> PaneRects {
        let (term_width, term_height) = self.terminal_size;
        let constraints = &self.state.constraints;

        let status_bar_height = constraints.status_bar.min_height.min(term_height);
        let available_height = term_height.saturating_sub(status_bar_height);

        match self.state.config.preset {
            LayoutPreset::Mixed => self.calculate_mixed(
                term_width,
                available_height,
                status_bar_height,
                constraints,
                context,
            ),
            LayoutPreset::Wide => self.calculate_wide(
                term_width,
                available_height,
                status_bar_height,
                constraints,
                context,
            ),
            LayoutPreset::Default => self.calculate_vertical(
                term_width,
                available_height,
                status_bar_height,
                constraints,
                context,
            ),
        }
    }

    fn calculate_vertical(
        &self,
        width: u16,
        available_height: u16,
        status_bar_height: u16,
        constraints: &PaneConstraintsMap,
        context: LayoutContext,
    ) -> PaneRects {
        let logs_height = self.logs_height(available_height, constraints, context);
        let body_height = available_height.saturating_sub(logs_height);

        let total_ratio =
            self.state.config.request_pane_ratio + self.state.config.response_pane_ratio;

        let req_height = Self::bounded_height(
            (body_height as f32 * self.state.config.request_pane_ratio / total_ratio) as u16,
            constraints.request.min_height,
            constraints.request.max_height,
        );
        let resp_height = body_height
            .saturating_sub(req_height)
            .max(constraints.response.min_height);

        let areas = RatatuiLayout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(req_height),
                Constraint::Length(resp_height),
                Constraint::Length(logs_height),
                Constraint::Length(status_bar_height),
            ])
            .split(Rect::new(0, 0, width, available_height + status_bar_height));

        PaneRects {
            request: areas[0],
            response: areas[1],
            logs: areas[2],
            status_bar: areas[3],
        }
    }

    fn calculate_mixed(
        &self,
        width: u16,
        available_height: u16,
        status_bar_height: u16,
        constraints: &PaneConstraintsMap,
        context: LayoutContext,
    ) -> PaneRects {
        let req_height = (available_height as f32 * 0.34) as u16;
        let req_height = req_height.max(constraints.request.min_height);

        let bottom_height = available_height.saturating_sub(req_height);
        let status_rect = Rect::new(0, req_height + bottom_height, width, status_bar_height);

        if !context.show_logs {
            return PaneRects {
                request: Rect::new(0, 0, width, req_height),
                response: Rect::new(0, req_height, width, bottom_height),
                logs: Rect::new(0, req_height + bottom_height, 0, 0),
                status_bar: status_rect,
            };
        }

        let total_ratio = self.state.config.response_pane_ratio + self.state.config.logs_pane_ratio;

        let resp_width =
            (width as f32 * self.state.config.response_pane_ratio / total_ratio) as u16;
        let logs_width = width
            .saturating_sub(resp_width)
            .max(constraints.logs.min_width);

        let request_rect = Rect::new(0, 0, width, req_height);
        let response_rect = Rect::new(0, req_height, resp_width, bottom_height);
        let logs_rect = Rect::new(resp_width, req_height, logs_width, bottom_height);

        PaneRects {
            request: request_rect,
            response: response_rect,
            logs: logs_rect,
            status_bar: status_rect,
        }
    }

    fn calculate_wide(
        &self,
        width: u16,
        available_height: u16,
        status_bar_height: u16,
        constraints: &PaneConstraintsMap,
        context: LayoutContext,
    ) -> PaneRects {
        let status_rect = Rect::new(0, available_height, width, status_bar_height);

        let req_width = Self::bounded_width(
            self.state.config.request_pane_width,
            constraints.request.min_width,
            constraints.request.max_width,
        );
        let right_width = width.saturating_sub(req_width);
        let logs_height = self.logs_height(available_height, constraints, context);
        let response_height = available_height
            .saturating_sub(logs_height)
            .max(constraints.response.min_height);
        let request_rect = Rect::new(0, 0, req_width, available_height);
        let response_rect = Rect::new(req_width, 0, right_width, response_height);
        let logs_rect = if logs_height == 0 {
            Rect::new(req_width, response_height, 0, 0)
        } else {
            Rect::new(req_width, response_height, right_width, logs_height)
        };

        PaneRects {
            request: request_rect,
            response: response_rect,
            logs: logs_rect,
            status_bar: status_rect,
        }
    }

    fn logs_height(
        &self,
        available_height: u16,
        constraints: &PaneConstraintsMap,
        context: LayoutContext,
    ) -> u16 {
        if !context.show_logs {
            return 0;
        }

        if context.compact_logs {
            return 4.min(available_height.saturating_sub(constraints.response.min_height));
        }

        let ratio_height = (available_height as f32 * self.state.config.logs_pane_ratio) as u16;
        let min_logs_height = constraints.logs.min_height.min(available_height);
        let max_logs_height = available_height.saturating_sub(constraints.response.min_height);

        if max_logs_height == 0 {
            0
        } else if max_logs_height < min_logs_height {
            max_logs_height
        } else {
            ratio_height.clamp(min_logs_height, max_logs_height)
        }
    }

    fn bounded_width(width: u16, min: u16, max: Option<u16>) -> u16 {
        let w = width.max(min);
        match max {
            Some(max) => w.min(max),
            None => w,
        }
    }

    fn bounded_height(height: u16, min: u16, max: Option<u16>) -> u16 {
        let h = height.max(min);
        match max {
            Some(max) => h.min(max),
            None => h,
        }
    }

    pub fn resize_request_pane(&mut self, delta: i16) {
        let new_width = (self.state.config.request_pane_width as i16 + delta)
            .max(self.state.constraints.request.min_width as i16) as u16;
        self.state.config.request_pane_width = new_width;
    }

    pub fn resize_response_pane(&mut self, delta: i16) {
        let new_height = (self.state.config.response_pane_height as i16 + delta)
            .max(self.state.constraints.response.min_height as i16) as u16;
        self.state.config.response_pane_height = new_height;
    }

    pub fn toggle_split_direction(&mut self) {
        self.state.config.preset = match self.state.config.preset {
            LayoutPreset::Default => LayoutPreset::Mixed,
            LayoutPreset::Mixed => LayoutPreset::Wide,
            LayoutPreset::Wide => LayoutPreset::Default,
        };
    }

    pub fn is_horizontal_split(&self) -> bool {
        self.state.config.horizontal_split
    }

    pub fn save(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.state)
    }

    pub fn load(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let state: LayoutState = serde_json::from_str(json)?;
        self.state = state;
        Ok(())
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let (width, height) = self.terminal_size;

        if width < 40 {
            errors.push("Terminal width too small (minimum 40)".to_string());
        }
        if height < 20 {
            errors.push("Terminal height too small (minimum 20)".to_string());
        }

        let constraints = &self.state.constraints;
        if constraints.request.min_width + constraints.response.min_width > width {
            errors.push("Wide layout: minimum widths exceed terminal width".to_string());
        }

        let minimum_stacked_height = constraints.request.min_height
            + constraints.response.min_height
            + constraints.status_bar.min_height;
        if minimum_stacked_height > height {
            errors.push("Layout minimum heights exceed terminal height".to_string());
        }

        errors
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_new() {
        let layout = Layout::new();
        let (w, h) = layout.terminal_size();
        assert_eq!(w, 80);
        assert_eq!(h, 24);
    }

    #[test]
    fn test_update_terminal_size() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let (w, h) = layout.terminal_size();
        assert_eq!(w, 120);
        assert_eq!(h, 40);
    }

    // Task 8.1
    #[test]
    fn test_layout_preset_variants() {
        let presets = vec![
            LayoutPreset::Default,
            LayoutPreset::Mixed,
            LayoutPreset::Wide,
        ];
        assert_eq!(presets.len(), 3);
    }

    // Task 8.2
    #[test]
    fn test_mixed_layout_request_on_top() {
        let mut layout = Layout::with_preset(LayoutPreset::Mixed);
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate();

        // Request should be at top, spanning full width
        assert_eq!(rects.request.y, 0);
        // Should be ~30% of 40 = 12, bounded by min_height
        assert!(rects.request.height >= 5); // min_height is 5
    }

    // Task 8.3
    #[test]
    fn test_f7_cycles_layout_presets() {
        let mut layout = Layout::new();
        let initial = layout.config().preset;

        layout.toggle_split_direction(); // This should cycle preset
        assert_ne!(layout.config().preset, initial);
    }

    #[test]
    fn test_pane_rects_all_panes_have_area() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate();

        let total_area = rects.request.area()
            + rects.response.area()
            + rects.logs.area()
            + rects.status_bar.area();

        assert!(total_area > 0);
    }

    #[test]
    fn test_pane_rects_dimensions() {
        let mut layout = Layout::new();
        layout.update_terminal_size(100, 30);
        let rects = layout.calculate();
        assert_eq!(rects.request.width, 30);
        assert_eq!(rects.response.width, 70);
        assert_eq!(rects.status_bar.y, 29); // Single row at bottom
    }

    #[test]
    fn test_calculate_horizontal_layout() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate();
        assert!(rects.request.width > 0);
        assert!(rects.response.width > 0);
        assert!(rects.logs.height > 0);
    }

    #[test]
    fn test_horizontal_layout_has_gutter() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate();

        let gutter = layout.config().gutter;
        assert_eq!(
            rects.request.x + rects.request.width + gutter,
            rects.response.x
        );
    }

    #[test]
    fn test_compact_logs_reduce_height() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate_with_context(LayoutContext {
            show_logs: true,
            compact_logs: true,
        });
        assert_eq!(rects.logs.height, 4);
    }

    #[test]
    fn test_hidden_logs_collapse_area() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate_with_context(LayoutContext {
            show_logs: false,
            compact_logs: false,
        });
        assert_eq!(rects.logs.area(), 0);
    }
}
