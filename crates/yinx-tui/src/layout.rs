use serde::{Deserialize, Serialize};
use ratatui::layout::{Constraint, Direction, Layout as RatatuiLayout, Rect};

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
pub struct LayoutConfig {
    pub request_pane_ratio: f32,
    pub response_pane_ratio: f32,
    pub workflow_pane_ratio: f32,
    pub logs_pane_ratio: f32,
    pub horizontal_split: bool,
    pub request_pane_width: u16,
    pub response_pane_height: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            request_pane_ratio: 0.3,
            response_pane_ratio: 0.4,
            workflow_pane_ratio: 0.2,
            logs_pane_ratio: 0.1,
            horizontal_split: false,
            request_pane_width: 40,
            response_pane_height: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneRects {
    pub request: Rect,
    pub response: Rect,
    pub workflow: Rect,
    pub logs: Rect,
    pub status_bar: Rect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutState {
    pub config: LayoutConfig,
    pub constraints: PaneConstraintsMap,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            config: LayoutConfig::default(),
            constraints: PaneConstraintsMap::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneConstraintsMap {
    pub request: PaneConstraints,
    pub response: PaneConstraints,
    pub workflow: PaneConstraints,
    pub logs: PaneConstraints,
    pub status_bar: PaneConstraints,
}

impl Default for PaneConstraintsMap {
    fn default() -> Self {
        Self {
            request: PaneConstraints {
                min_width: 30,
                min_height: 15,
                priority: 3,
                ..Default::default()
            },
            response: PaneConstraints {
                min_width: 40,
                min_height: 20,
                priority: 4,
                ..Default::default()
            },
            workflow: PaneConstraints {
                min_width: 25,
                min_height: 10,
                priority: 2,
                ..Default::default()
            },
            logs: PaneConstraints {
                min_width: 20,
                min_height: 8,
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
        let (term_width, term_height) = self.terminal_size;
        let constraints = &self.state.constraints;

        let status_bar_height = constraints.status_bar.min_height.min(term_height);
        let available_height = term_height.saturating_sub(status_bar_height);

        if self.state.config.horizontal_split {
            self.calculate_horizontal(term_width, available_height, status_bar_height, constraints)
        } else {
            self.calculate_vertical(term_width, available_height, status_bar_height, constraints)
        }
    }

    fn calculate_vertical(
        &self,
        width: u16,
        available_height: u16,
        status_bar_height: u16,
        constraints: &PaneConstraintsMap,
    ) -> PaneRects {
        let total_ratio = self.state.config.request_pane_ratio
            + self.state.config.response_pane_ratio
            + self.state.config.workflow_pane_ratio
            + self.state.config.logs_pane_ratio;

        let req_height = Self::bounded_height(
            (available_height as f32 * self.state.config.request_pane_ratio / total_ratio) as u16,
            constraints.request.min_height,
            constraints.request.max_height,
        );
        let resp_height = Self::bounded_height(
            (available_height as f32 * self.state.config.response_pane_ratio / total_ratio) as u16,
            constraints.response.min_height,
            constraints.response.max_height,
        );
        let wf_height = Self::bounded_height(
            (available_height as f32 * self.state.config.workflow_pane_ratio / total_ratio) as u16,
            constraints.workflow.min_height,
            constraints.workflow.max_height,
        );
        let logs_height = available_height
            .saturating_sub(req_height)
            .saturating_sub(resp_height)
            .saturating_sub(wf_height)
            .max(constraints.logs.min_height);

        let areas = RatatuiLayout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(req_height),
                Constraint::Length(resp_height),
                Constraint::Length(wf_height),
                Constraint::Length(logs_height),
                Constraint::Length(status_bar_height),
            ])
            .split(Rect::new(0, 0, width, available_height + status_bar_height));

        PaneRects {
            request: areas[0],
            response: areas[1],
            workflow: areas[2],
            logs: areas[3],
            status_bar: areas[4],
        }
    }

    fn calculate_horizontal(
        &self,
        width: u16,
        available_height: u16,
        status_bar_height: u16,
        constraints: &PaneConstraintsMap,
    ) -> PaneRects {
        let status_rect = Rect::new(0, available_height, width, status_bar_height);

        let req_width = Self::bounded_width(
            self.state.config.request_pane_width,
            constraints.request.min_width,
            constraints.request.max_width,
        );
        let remaining_width = width.saturating_sub(req_width);

        let resp_width = (remaining_width as f32 * self.state.config.response_pane_ratio
            / (self.state.config.response_pane_ratio + self.state.config.workflow_pane_ratio + self.state.config.logs_pane_ratio))
            as u16;
        let wf_width = (remaining_width as f32 * self.state.config.workflow_pane_ratio
            / (self.state.config.response_pane_ratio + self.state.config.workflow_pane_ratio + self.state.config.logs_pane_ratio))
            as u16;
        let logs_width = remaining_width
            .saturating_sub(resp_width)
            .saturating_sub(wf_width)
            .max(constraints.logs.min_width);

        let request_rect = Rect::new(0, 0, req_width, available_height);
        let response_rect = Rect::new(req_width, 0, resp_width, available_height);
        let workflow_rect = Rect::new(req_width + resp_width, 0, wf_width, available_height);
        let logs_rect = Rect::new(
            req_width + resp_width + wf_width,
            0,
            logs_width,
            available_height,
        );

        PaneRects {
            request: request_rect,
            response: response_rect,
            workflow: workflow_rect,
            logs: logs_rect,
            status_bar: status_rect,
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
        let new_width = (self.state.config.request_pane_width as i16 + delta).max(
            self.state.constraints.request.min_width as i16,
        ) as u16;
        self.state.config.request_pane_width = new_width;
    }

    pub fn resize_response_pane(&mut self, delta: i16) {
        let new_height = (self.state.config.response_pane_height as i16 + delta).max(
            self.state.constraints.response.min_height as i16,
        ) as u16;
        self.state.config.response_pane_height = new_height;
    }

    pub fn toggle_split_direction(&mut self) {
        self.state.config.horizontal_split = !self.state.config.horizontal_split;
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
        if constraints.request.min_width + constraints.response.min_width > width
            && self.state.config.horizontal_split
        {
            errors.push("Horizontal layout: minimum widths exceed terminal width".to_string());
        }

        if constraints.request.min_height + constraints.response.min_height
            + constraints.workflow.min_height + constraints.logs.min_height
            + constraints.status_bar.min_height
            > height
        {
            errors.push("Vertical layout: minimum heights exceed terminal height".to_string());
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

    #[test]
    fn test_calculate_vertical_layout() {
        let mut layout = Layout::new();
        layout.update_terminal_size(80, 24);
        let rects = layout.calculate();
        assert!(rects.request.width > 0);
        assert!(rects.response.width > 0);
        assert!(rects.workflow.width > 0);
        assert!(rects.logs.width > 0);
        assert!(rects.status_bar.width > 0);
    }

    #[test]
    fn test_calculate_horizontal_layout() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        layout.toggle_split_direction();
        let rects = layout.calculate();
        assert!(rects.request.width > 0);
        assert!(rects.response.width > 0);
        assert!(rects.workflow.width > 0);
        assert!(rects.logs.width > 0);
    }

    #[test]
    fn test_pane_rects_dimensions() {
        let mut layout = Layout::new();
        layout.update_terminal_size(100, 30);
        let rects = layout.calculate();
        let total_height = rects.request.height
            + rects.response.height
            + rects.workflow.height
            + rects.logs.height
            + rects.status_bar.height;
        assert_eq!(total_height, 30);
    }

    #[test]
    fn test_resize_request_pane() {
        let mut layout = Layout::new();
        let initial = layout.config().request_pane_width;
        layout.resize_request_pane(10);
        assert_eq!(layout.config().request_pane_width, initial + 10);
    }

    #[test]
    fn test_resize_request_pane_minimum() {
        let mut layout = Layout::new();
        layout.resize_request_pane(-1000);
        assert!(layout.config().request_pane_width >= layout.constraints().request.min_width);
    }

    #[test]
    fn test_toggle_split_direction() {
        let mut layout = Layout::new();
        assert!(!layout.is_horizontal_split());
        layout.toggle_split_direction();
        assert!(layout.is_horizontal_split());
        layout.toggle_split_direction();
        assert!(!layout.is_horizontal_split());
    }

    #[test]
    fn test_save_and_load() {
        let mut layout = Layout::new();
        layout.update_terminal_size(100, 30);
        layout.resize_request_pane(10);

        let json = layout.save().unwrap();
        let mut new_layout = Layout::new();
        new_layout.load(&json).unwrap();

        assert_eq!(
            layout.config().request_pane_width,
            new_layout.config().request_pane_width
        );
    }

    #[test]
    fn test_validate_no_errors() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 60);
        let errors = layout.validate();
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_validate_small_terminal() {
        let mut layout = Layout::new();
        layout.update_terminal_size(30, 15);
        let errors = layout.validate();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_bounded_width() {
        assert_eq!(Layout::bounded_width(10, 20, None), 20);
        assert_eq!(Layout::bounded_width(30, 20, Some(25)), 25);
        assert_eq!(Layout::bounded_width(20, 10, Some(30)), 20);
    }

    #[test]
    fn test_bounded_height() {
        assert_eq!(Layout::bounded_height(5, 10, None), 10);
        assert_eq!(Layout::bounded_height(15, 10, Some(12)), 12);
        assert_eq!(Layout::bounded_height(10, 5, Some(20)), 10);
    }

    #[test]
    fn test_constraints_default() {
        let constraints = PaneConstraints::default();
        assert_eq!(constraints.min_width, 20);
        assert_eq!(constraints.min_height, 10);
        assert_eq!(constraints.priority, 1);
    }

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert_eq!(config.request_pane_ratio, 0.3);
        assert!(!config.horizontal_split);
    }

    #[test]
    fn test_pane_rects_all_panes_have_area() {
        let mut layout = Layout::new();
        layout.update_terminal_size(120, 40);
        let rects = layout.calculate();

        let total_area = rects.request.area()
            + rects.response.area()
            + rects.workflow.area()
            + rects.logs.area()
            + rects.status_bar.area();

        assert!(total_area > 0);
    }
}
