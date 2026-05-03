use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Tabs,
        Wrap,
    },
    Frame,
};

use yinx_workflow::engine::WorkflowState;
use yinx_workflow::graph::Workflow;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowTab {
    Graph,
    Nodes,
    Variables,
}

impl WorkflowTab {
    pub fn all() -> Vec<WorkflowTab> {
        vec![
            WorkflowTab::Graph,
            WorkflowTab::Nodes,
            WorkflowTab::Variables,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowTab::Graph => "Graph",
            WorkflowTab::Nodes => "Nodes",
            WorkflowTab::Variables => "Variables",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedField {
    Sidebar,
    Tabs,
    TabContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Pending,
    Running,
    Success,
    Error,
}

impl NodeStatus {
    pub fn to_workflow_state(&self) -> WorkflowState {
        match self {
            NodeStatus::Pending => WorkflowState::Pending,
            NodeStatus::Running => WorkflowState::Running,
            NodeStatus::Success => WorkflowState::Done,
            NodeStatus::Error => WorkflowState::Failed,
        }
    }

    pub fn color(&self, theme: &Theme) -> ratatui::style::Color {
        match self {
            NodeStatus::Pending => theme.foreground.as_color(),
            NodeStatus::Running => theme.semantic.warning.as_color(),
            NodeStatus::Success => theme.semantic.success.as_color(),
            NodeStatus::Error => theme.semantic.error.as_color(),
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            NodeStatus::Pending => "○",
            NodeStatus::Running => "◉",
            NodeStatus::Success => "●",
            NodeStatus::Error => "✕",
        }
    }
}

pub struct WorkflowPane {
    workflows: Vec<Workflow>,
    selected_workflow: usize,
    selected_tab: usize,
    focused_field: FocusedField,
    sidebar_visible: bool,
    sidebar_list_state: ListState,
    node_list_state: ListState,
    selected_node: Option<String>,
    node_statuses: std::collections::HashMap<String, NodeStatus>,
    workflow_state: WorkflowState,
    execution_result: Option<String>,
    graph_offset_x: u16,
    variable_list_state: ListState,
    variables: Vec<(String, String)>,
}

impl WorkflowPane {
    pub fn new() -> Self {
        let mut sidebar_list_state = ListState::default();
        sidebar_list_state.select(Some(0));

        let mut node_list_state = ListState::default();
        node_list_state.select(Some(0));

        let mut variable_list_state = ListState::default();
        variable_list_state.select(Some(0));

        Self {
            workflows: Vec::new(),
            selected_workflow: 0,
            selected_tab: 0,
            focused_field: FocusedField::Sidebar,
            sidebar_visible: true,
            sidebar_list_state,
            node_list_state,
            selected_node: None,
            node_statuses: std::collections::HashMap::new(),
            workflow_state: WorkflowState::Pending,
            execution_result: None,
            graph_offset_x: 0,
            variable_list_state,
            variables: Vec::new(),
        }
    }

    pub fn with_workflows(mut self, workflows: Vec<Workflow>) -> Self {
        self.workflows = workflows;
        self.update_variables();
        if !self.workflows.is_empty() {
            self.select_workflow(0);
        }
        self
    }

    pub fn with_workflow(mut self, workflow: Workflow) -> Self {
        self.workflows.push(workflow);
        self.update_variables();
        if self.workflows.len() == 1 {
            self.select_workflow(0);
        }
        self
    }

    pub fn add_workflow(&mut self, workflow: Workflow) {
        self.workflows.push(workflow);
        self.update_variables();
        if self.workflows.len() == 1 {
            self.select_workflow(0);
        }
    }

    fn select_workflow(&mut self, index: usize) {
        if index < self.workflows.len() {
            self.selected_workflow = index;
            self.sidebar_list_state.select(Some(index));
            self.selected_node = None;
            self.node_statuses.clear();
            self.workflow_state = WorkflowState::Pending;
            self.execution_result = None;
            self.update_variables();

            if let Some(workflow) = self.workflows.get(index) {
                if !workflow.nodes.is_empty() {
                    let first_node_id = workflow.nodes.keys().next().unwrap().clone();
                    self.selected_node = Some(first_node_id);
                    self.node_list_state.select(Some(0));
                }
            }
        }
    }

    fn update_variables(&mut self) {
        self.variables.clear();
        if let Some(workflow) = self.workflows.get(self.selected_workflow) {
            for (key, value) in &workflow.variables {
                self.variables.push((key.clone(), format!("{}", value)));
            }
        }
        if !self.variables.is_empty() {
            self.variable_list_state.select(Some(0));
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let border_style = if is_active {
            Style::default().fg(theme.border.active_color.as_color())
        } else {
            Style::default().fg(theme.border.color.as_color())
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title("Workflow");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.workflows.is_empty() {
            let empty_msg = Paragraph::new("No workflows loaded. Import or create a workflow.")
                .style(Style::default().fg(theme.foreground.as_color()))
                .alignment(Alignment::Center);
            let centered = centered_rect(60, 20, inner);
            frame.render_widget(empty_msg, centered);
            return;
        }

        let main_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if self.sidebar_visible {
                vec![Constraint::Percentage(20), Constraint::Percentage(80)]
            } else {
                vec![Constraint::Percentage(100)]
            })
            .split(inner);

        if self.sidebar_visible {
            self.render_sidebar(frame, main_layout[0], theme);
            self.render_main_content(frame, main_layout[1], theme);
        } else {
            self.render_main_content(frame, main_layout[0], theme);
        }
    }

    fn render_sidebar(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if matches!(self.focused_field, FocusedField::Sidebar) {
                Style::default().fg(theme.border.active_color.as_color())
            } else {
                Style::default().fg(theme.border.color.as_color())
            })
            .title("Workflows");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = self
            .workflows
            .iter()
            .enumerate()
            .map(|(idx, w)| {
                let style = if idx == self.selected_workflow {
                    Style::default()
                        .fg(theme.semantic.info.as_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(w.name.clone()).style(style)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color()),
            )
            .highlight_symbol("> ");

        let mut state = self.sidebar_list_state.clone();
        frame.render_stateful_widget(list, inner, &mut state);
    }

    fn render_main_content(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        self.render_tabs(frame, layout[0], theme);
        self.render_tab_content(frame, layout[1], theme);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let tabs = WorkflowTab::all();
        let titles: Vec<Line> = tabs.iter().map(|t| Line::from(t.as_str())).collect();

        let tab_widget = Tabs::new(titles)
            .select(self.selected_tab)
            .style(Style::default().fg(theme.foreground.as_color()))
            .highlight_style(
                Style::default()
                    .fg(theme.semantic.info.as_color())
                    .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
            )
            .divider(" | ");

        let block = Block::default().borders(Borders::BOTTOM).border_style(
            if matches!(self.focused_field, FocusedField::Tabs) {
                Style::default().fg(theme.border.active_color.as_color())
            } else {
                Style::default().fg(theme.border.color.as_color())
            },
        );

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(tab_widget, inner);
    }

    fn render_tab_content(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        match WorkflowTab::all()[self.selected_tab] {
            WorkflowTab::Graph => self.render_graph_tab(frame, area, theme),
            WorkflowTab::Nodes => self.render_nodes_tab(frame, area, theme),
            WorkflowTab::Variables => self.render_variables_tab(frame, area, theme),
        }
    }

    fn render_graph_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let workflow = &self.workflows[self.selected_workflow];

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        self.render_graph_visualization(frame, layout[0], theme, workflow);
        self.render_execution_controls(frame, layout[1], theme);
    }

    fn render_nodes_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let workflow = &self.workflows[self.selected_workflow];

        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        self.render_node_list(frame, layout[0], theme, workflow);
        self.render_node_detail(frame, layout[1], theme, workflow);
    }

    fn render_graph_visualization(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        workflow: &Workflow,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if matches!(self.focused_field, FocusedField::TabContent) {
                Style::default().fg(theme.border.active_color.as_color())
            } else {
                Style::default().fg(theme.border.color.as_color())
            })
            .title("Graph Visualization");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if workflow.nodes.is_empty() {
            let msg = Paragraph::new("No nodes in workflow")
                .style(Style::default().fg(theme.foreground.as_color()))
                .alignment(Alignment::Center);
            frame.render_widget(msg, inner);
            return;
        }

        let mut lines = Vec::new();

        for (node_id, node) in &workflow.nodes {
            let status = self
                .node_statuses
                .get(node_id)
                .unwrap_or(&NodeStatus::Pending);
            let status_symbol = status.symbol();
            let method = node.request.method.to_string();
            let url_short = node
                .request
                .url
                .as_str()
                .split('/')
                .last()
                .unwrap_or("")
                .to_string();

            let line = format!("{} [{}] {} - {}", status_symbol, method, node_id, url_short);
            let style = Style::default().fg(status.color(theme));

            lines.push(Line::from(vec![Span::styled(line, style)]));
        }

        lines.push(Line::from(""));

        for edge in &workflow.edges {
            let line = format!(
                "  {} --({})--> {}",
                edge.from,
                edge.condition.as_deref().unwrap_or(""),
                edge.to
            );
            lines.push(Line::from(vec![Span::styled(
                line,
                Style::default().fg(theme.foreground.as_color()),
            )]));
        }

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.foreground.as_color()))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, inner);
    }

    fn render_execution_controls(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border.color.as_color()))
            .title("Controls");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let state_symbol = match self.workflow_state {
            WorkflowState::Pending => "○",
            WorkflowState::Running => "◉",
            WorkflowState::Done => "●",
            WorkflowState::Failed => "✕",
            WorkflowState::Cancelled => "⊘",
        };

        let state_color = match self.workflow_state {
            WorkflowState::Pending => theme.foreground.as_color(),
            WorkflowState::Running => theme.semantic.warning.as_color(),
            WorkflowState::Done => theme.semantic.success.as_color(),
            WorkflowState::Failed => theme.semantic.error.as_color(),
            WorkflowState::Cancelled => theme.semantic.warning.as_color(),
        };

        let controls = vec![
            Span::styled(
                "Run All (r)",
                Style::default().fg(theme.semantic.info.as_color()),
            ),
            Span::raw(" | "),
            Span::styled(
                "Run Selected (Enter)",
                Style::default().fg(theme.semantic.info.as_color()),
            ),
            Span::raw(" | "),
            Span::styled("Stop", Style::default().fg(theme.semantic.error.as_color())),
            Span::raw(" | "),
            Span::styled("State: ", Style::default().fg(theme.foreground.as_color())),
            Span::styled(state_symbol, Style::default().fg(state_color)),
            Span::raw(format!(" {:?}", self.workflow_state)),
        ];

        let line = Line::from(controls);
        let paragraph = Paragraph::new(line)
            .style(Style::default().fg(theme.foreground.as_color()))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, inner);
    }

    fn render_node_list(&self, frame: &mut Frame, area: Rect, theme: &Theme, workflow: &Workflow) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border.color.as_color()))
            .title("Nodes");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = workflow
            .nodes
            .values()
            .map(|node| {
                let status = self
                    .node_statuses
                    .get(&node.id)
                    .unwrap_or(&NodeStatus::Pending);
                let symbol = status.symbol();
                let style = Style::default().fg(status.color(theme));
                let content = format!("{} {} [{}]", symbol, node.id, node.request.method);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color()),
            )
            .highlight_symbol("> ");

        let mut state = self.node_list_state.clone();
        frame.render_stateful_widget(list, inner, &mut state);
    }

    fn render_node_detail(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        workflow: &Workflow,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border.color.as_color()))
            .title("Node Detail");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(node_id) = &self.selected_node {
            if let Some(node) = workflow.nodes.get(node_id) {
                let status = self
                    .node_statuses
                    .get(node_id)
                    .unwrap_or(&NodeStatus::Pending);

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("ID: ", Style::default().fg(theme.foreground.as_color())),
                        Span::styled(
                            &node.id,
                            Style::default()
                                .fg(theme.foreground.as_color())
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Status: ", Style::default().fg(theme.foreground.as_color())),
                        Span::styled(status.symbol(), Style::default().fg(status.color(theme))),
                        Span::styled(
                            format!(" {:?}", status),
                            Style::default().fg(status.color(theme)),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Method: ", Style::default().fg(theme.foreground.as_color())),
                        Span::styled(
                            node.request.method.to_string(),
                            Style::default().fg(theme.semantic.info.as_color()),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("URL: ", Style::default().fg(theme.foreground.as_color())),
                        Span::styled(
                            node.request.url.as_str(),
                            Style::default().fg(theme.foreground.as_color()),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "Headers:",
                        Style::default()
                            .fg(theme.foreground.as_color())
                            .add_modifier(Modifier::BOLD),
                    )]),
                ];

                for header in node.request.headers.iter() {
                    lines.push(Line::from(format!("  {}: {}", header.name, header.value)));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Metadata:",
                    Style::default()
                        .fg(theme.foreground.as_color())
                        .add_modifier(Modifier::BOLD),
                )]));

                for (key, value) in &node.metadata {
                    lines.push(Line::from(format!("  {}: {}", key, value)));
                }

                let paragraph = Paragraph::new(lines)
                    .style(Style::default().fg(theme.foreground.as_color()))
                    .wrap(Wrap { trim: true });

                frame.render_widget(paragraph, inner);
            }
        } else {
            let msg = Paragraph::new("No node selected")
                .style(Style::default().fg(theme.foreground.as_color()))
                .alignment(Alignment::Center);
            frame.render_widget(msg, inner);
        }
    }

    fn render_variables_tab(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border.color.as_color()))
            .title("Variables");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.variables.is_empty() {
            let msg = Paragraph::new("No variables defined")
                .style(Style::default().fg(theme.foreground.as_color()))
                .alignment(Alignment::Center);
            frame.render_widget(msg, inner);
            return;
        }

        let rows: Vec<Row> = self
            .variables
            .iter()
            .map(|(key, value)| {
                Row::new(vec![Cell::from(key.as_str()), Cell::from(value.as_str())])
            })
            .collect();

        let table = Table::new(
            rows,
            &[Constraint::Percentage(30), Constraint::Percentage(70)],
        )
        .header(
            Row::new(vec!["Key", "Value"]).style(
                Style::default()
                    .fg(theme.semantic.info.as_color())
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .style(Style::default().fg(theme.foreground.as_color()));

        let mut state = ratatui::widgets::TableState::default();
        frame.render_stateful_widget(table, inner, &mut state);
    }

    pub fn handle_key(&mut self, key_code: KeyCode, _modifiers: KeyModifiers) -> bool {
        if self.workflows.is_empty() {
            return false;
        }

        match self.focused_field {
            FocusedField::Sidebar => self.handle_sidebar_key(key_code),
            FocusedField::Tabs => self.handle_tabs_key(key_code),
            FocusedField::TabContent => self.handle_tab_content_key(key_code),
        }
    }

    fn handle_sidebar_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                if self.selected_workflow > 0 {
                    self.select_workflow(self.selected_workflow - 1);
                }
                true
            }
            KeyCode::Down => {
                if self.selected_workflow < self.workflows.len() - 1 {
                    self.select_workflow(self.selected_workflow + 1);
                }
                true
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Tab => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Char('s') => {
                self.sidebar_visible = !self.sidebar_visible;
                true
            }
            _ => false,
        }
    }

    fn handle_tabs_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Left => {
                if self.selected_tab == 0 {
                    self.focused_field = FocusedField::Sidebar;
                } else {
                    self.selected_tab -= 1;
                }
                true
            }
            KeyCode::Right => {
                let tabs = WorkflowTab::all();
                if self.selected_tab >= tabs.len() - 1 {
                    self.focused_field = FocusedField::TabContent;
                } else {
                    self.selected_tab += 1;
                }
                true
            }
            KeyCode::Down | KeyCode::Enter => {
                self.focused_field = FocusedField::TabContent;
                true
            }
            KeyCode::Up => {
                self.focused_field = FocusedField::Sidebar;
                true
            }
            KeyCode::Char('s') => {
                self.sidebar_visible = !self.sidebar_visible;
                true
            }
            _ => false,
        }
    }

    fn handle_tab_content_key(&mut self, key_code: KeyCode) -> bool {
        match WorkflowTab::all()[self.selected_tab] {
            WorkflowTab::Graph => self.handle_graph_key(key_code),
            WorkflowTab::Nodes => self.handle_nodes_key(key_code),
            WorkflowTab::Variables => self.handle_variables_key(key_code),
        }
    }

    fn handle_graph_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Left => {
                self.graph_offset_x = self.graph_offset_x.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                self.graph_offset_x += 1;
                true
            }
            KeyCode::Char('r') => {
                self.run_all();
                true
            }
            KeyCode::Char('s') => {
                self.sidebar_visible = !self.sidebar_visible;
                true
            }
            _ => false,
        }
    }

    fn handle_nodes_key(&mut self, key_code: KeyCode) -> bool {
        let workflow = &self.workflows[self.selected_workflow];
        let node_count = workflow.nodes.len();
        let node_ids: Vec<String> = workflow.nodes.keys().cloned().collect();

        match key_code {
            KeyCode::Up => {
                if let Some(current_idx) = self.node_list_state.selected() {
                    if current_idx > 0 {
                        self.node_list_state.select(Some(current_idx - 1));
                        self.selected_node = node_ids.get(current_idx - 1).cloned();
                    } else {
                        self.focused_field = FocusedField::Tabs;
                    }
                }
                true
            }
            KeyCode::Down => {
                if let Some(current_idx) = self.node_list_state.selected() {
                    if current_idx < node_count - 1 {
                        self.node_list_state.select(Some(current_idx + 1));
                        self.selected_node = node_ids.get(current_idx + 1).cloned();
                    }
                }
                true
            }
            KeyCode::Left => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Enter => {
                if let Some(node_id) = self.selected_node.clone() {
                    self.run_node(&node_id);
                }
                true
            }
            KeyCode::Char('s') => {
                self.sidebar_visible = !self.sidebar_visible;
                true
            }
            _ => false,
        }
    }

    fn handle_variables_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up => {
                if let Some(current_idx) = self.variable_list_state.selected() {
                    if current_idx > 0 {
                        self.variable_list_state.select(Some(current_idx - 1));
                    } else {
                        self.focused_field = FocusedField::Tabs;
                    }
                }
                true
            }
            KeyCode::Down => {
                if let Some(current_idx) = self.variable_list_state.selected() {
                    if current_idx < self.variables.len() - 1 {
                        self.variable_list_state.select(Some(current_idx + 1));
                    }
                }
                true
            }
            KeyCode::Left => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            KeyCode::Char('s') => {
                self.sidebar_visible = !self.sidebar_visible;
                true
            }
            _ => false,
        }
    }

    fn run_all(&mut self) {
        self.workflow_state = WorkflowState::Running;
        for node_id in self.workflows[self.selected_workflow].nodes.keys() {
            self.node_statuses
                .insert(node_id.clone(), NodeStatus::Running);
        }
    }

    fn run_node(&mut self, node_id: &str) {
        self.workflow_state = WorkflowState::Running;
        self.node_statuses
            .insert(node_id.to_string(), NodeStatus::Running);
    }

    pub fn set_node_status(&mut self, node_id: &str, status: NodeStatus) {
        self.node_statuses.insert(node_id.to_string(), status);
        if self
            .node_statuses
            .values()
            .all(|s| matches!(s, NodeStatus::Success))
        {
            self.workflow_state = WorkflowState::Done;
        } else if self
            .node_statuses
            .values()
            .any(|s| matches!(s, NodeStatus::Error))
        {
            self.workflow_state = WorkflowState::Failed;
        }
    }

    pub fn set_workflow_state(&mut self, state: WorkflowState) {
        self.workflow_state = state;
    }

    pub fn focused_field(&self) -> FocusedField {
        self.focused_field
    }

    pub fn set_focused_field(&mut self, field: FocusedField) {
        self.focused_field = field;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yinx_core::request::{Method, Request, RequestBody, RequestBuilder};
    use yinx_workflow::graph::{Workflow, WorkflowEdge, WorkflowNode};

    fn create_test_request(method: Method, url: &str) -> Request {
        RequestBuilder::new()
            .method(method)
            .url(url)
            .body(RequestBody::None)
            .build()
            .unwrap()
    }

    fn create_test_workflow(name: &str) -> Workflow {
        let mut workflow = Workflow::new(name);
        let request = create_test_request(Method::Get, "https://api.example.com/users");
        let node = WorkflowNode::new(request).with_id("get-users");
        workflow.add_node(node).unwrap();

        let request2 = create_test_request(Method::Post, "https://api.example.com/users");
        let node2 = WorkflowNode::new(request2).with_id("create-user");
        workflow.add_node(node2).unwrap();

        workflow
            .add_edge(WorkflowEdge::new("get-users", "create-user"))
            .unwrap();
        workflow
    }

    #[test]
    fn test_workflow_pane_new() {
        let pane = WorkflowPane::new();
        assert_eq!(pane.focused_field(), FocusedField::Sidebar);
    }

    #[test]
    fn test_workflow_pane_with_workflow() {
        let workflow = create_test_workflow("Test Workflow");
        let pane = WorkflowPane::new().with_workflow(workflow);
        assert_eq!(pane.workflows.len(), 1);
    }

    #[test]
    fn test_workflow_tab_all() {
        let tabs = WorkflowTab::all();
        assert_eq!(tabs.len(), 3);
        assert!(matches!(tabs[0], WorkflowTab::Graph));
        assert!(matches!(tabs[1], WorkflowTab::Nodes));
        assert!(matches!(tabs[2], WorkflowTab::Variables));
    }

    #[test]
    fn test_workflow_tab_as_str() {
        assert_eq!(WorkflowTab::Graph.as_str(), "Graph");
        assert_eq!(WorkflowTab::Nodes.as_str(), "Nodes");
        assert_eq!(WorkflowTab::Variables.as_str(), "Variables");
    }

    #[test]
    fn test_node_status_symbol() {
        assert_eq!(NodeStatus::Pending.symbol(), "○");
        assert_eq!(NodeStatus::Running.symbol(), "◉");
        assert_eq!(NodeStatus::Success.symbol(), "●");
        assert_eq!(NodeStatus::Error.symbol(), "✕");
    }

    #[test]
    fn test_node_status_to_workflow_state() {
        assert!(matches!(
            NodeStatus::Pending.to_workflow_state(),
            WorkflowState::Pending
        ));
        assert!(matches!(
            NodeStatus::Running.to_workflow_state(),
            WorkflowState::Running
        ));
        assert!(matches!(
            NodeStatus::Success.to_workflow_state(),
            WorkflowState::Done
        ));
        assert!(matches!(
            NodeStatus::Error.to_workflow_state(),
            WorkflowState::Failed
        ));
    }

    #[test]
    fn test_focused_field_transitions() {
        let mut pane = WorkflowPane::new();
        assert_eq!(pane.focused_field(), FocusedField::Sidebar);

        pane.set_focused_field(FocusedField::Tabs);
        assert_eq!(pane.focused_field(), FocusedField::Tabs);

        pane.set_focused_field(FocusedField::TabContent);
        assert_eq!(pane.focused_field(), FocusedField::TabContent);
    }

    #[test]
    fn test_sidebar_key_navigation() {
        let workflow1 = create_test_workflow("Workflow 1");
        let workflow2 = create_test_workflow("Workflow 2");
        let mut pane = WorkflowPane::new()
            .with_workflow(workflow1)
            .with_workflow(workflow2);

        assert_eq!(pane.selected_workflow, 0);

        pane.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(pane.selected_workflow, 1);

        pane.handle_key(KeyCode::Up, KeyModifiers::empty());
        assert_eq!(pane.selected_workflow, 0);
    }

    #[test]
    fn test_tab_navigation() {
        let workflow = create_test_workflow("Test");
        let mut pane = WorkflowPane::new().with_workflow(workflow);
        pane.set_focused_field(FocusedField::Tabs);

        assert_eq!(pane.selected_tab, 0);

        pane.handle_key(KeyCode::Right, KeyModifiers::empty());
        assert_eq!(pane.selected_tab, 1);

        pane.handle_key(KeyCode::Left, KeyModifiers::empty());
        assert_eq!(pane.selected_tab, 0);
    }

    #[test]
    fn test_run_all() {
        let workflow = create_test_workflow("Test");
        let mut pane = WorkflowPane::new().with_workflow(workflow);
        pane.set_focused_field(FocusedField::TabContent);
        pane.selected_tab = 0; // Graph tab
        pane.handle_key(KeyCode::Char('r'), KeyModifiers::empty());

        assert!(matches!(pane.workflow_state, WorkflowState::Running));
    }

    #[test]
    fn test_set_node_status() {
        let workflow = create_test_workflow("Test");
        let mut pane = WorkflowPane::new().with_workflow(workflow);

        pane.set_node_status("get-users", NodeStatus::Success);
        assert!(matches!(
            pane.node_statuses.get("get-users"),
            Some(NodeStatus::Success)
        ));

        pane.set_node_status("create-user", NodeStatus::Error);
        assert!(matches!(pane.workflow_state, WorkflowState::Failed));
    }

    #[test]
    fn test_workflow_state_transitions() {
        let workflow = create_test_workflow("Test");
        let mut pane = WorkflowPane::new().with_workflow(workflow);

        pane.set_workflow_state(WorkflowState::Running);
        assert!(matches!(pane.workflow_state, WorkflowState::Running));

        pane.set_workflow_state(WorkflowState::Done);
        assert!(matches!(pane.workflow_state, WorkflowState::Done));

        pane.set_workflow_state(WorkflowState::Failed);
        assert!(matches!(pane.workflow_state, WorkflowState::Failed));
    }

    #[test]
    fn test_sidebar_toggle() {
        let workflow = create_test_workflow("Test");
        let mut pane = WorkflowPane::new().with_workflow(workflow);
        assert!(pane.sidebar_visible);

        pane.handle_key(KeyCode::Char('s'), KeyModifiers::empty());
        assert!(!pane.sidebar_visible);

        pane.handle_key(KeyCode::Char('s'), KeyModifiers::empty());
        assert!(pane.sidebar_visible);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
