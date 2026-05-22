use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{BarChart, Block, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

use std::collections::VecDeque;

use yinx_core::metrics::MetricsCollector;
use yinx_core::request::{request_to_curl, Request};
use yinx_core::timing::RequestMetrics;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogsTab {
    Logs,
    Metrics,
    Histogram,
    StatusCodes,
    Errors,
    Curl,
}

impl LogsTab {
    pub fn all() -> Vec<LogsTab> {
        vec![
            LogsTab::Logs,
            LogsTab::Metrics,
            LogsTab::Histogram,
            LogsTab::StatusCodes,
            LogsTab::Errors,
            LogsTab::Curl,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LogsTab::Logs => "Logs",
            LogsTab::Metrics => "Metrics",
            LogsTab::Histogram => "Histogram",
            LogsTab::StatusCodes => "Status",
            LogsTab::Errors => "Errors",
            LogsTab::Curl => "Curl",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn all() -> Vec<LogLevel> {
        vec![
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warning,
            LogLevel::Error,
        ]
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub context: Option<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            message: message.into(),
            context: None,
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn format_timestamp(&self) -> String {
        self.timestamp.format("%H:%M:%S%.3f").to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedField {
    Tabs,
    TabContent,
}

pub struct LogsPane {
    logs: VecDeque<LogEntry>,
    max_logs: usize,
    selected_tab: usize,
    selected_log: usize,
    focused_field: FocusedField,
    metrics: Option<RequestMetrics>,
    metrics_collector: MetricsCollector,
    chunk_intervals: VecDeque<u64>,
    max_intervals: usize,
    errors: Vec<LogEntry>,
    selected_error: usize,
    error_rate_threshold: f64,
    current_request: Option<Request>,
}

impl Default for LogsPane {
    fn default() -> Self {
        Self::new()
    }
}

impl LogsPane {
    pub fn new() -> Self {
        Self {
            logs: VecDeque::new(),
            max_logs: 1000,
            selected_tab: 0,
            selected_log: 0,
            focused_field: FocusedField::TabContent,
            metrics: None,
            metrics_collector: MetricsCollector::new(),
            chunk_intervals: VecDeque::new(),
            max_intervals: 50,
            errors: Vec::new(),
            selected_error: 0,
            error_rate_threshold: 10.0,
            current_request: None,
        }
    }

    pub fn set_current_request(&mut self, request: Request) {
        self.current_request = Some(request);
    }

    pub fn current_request(&self) -> Option<&Request> {
        self.current_request.as_ref()
    }

    pub fn clear_current_request(&mut self) {
        self.current_request = None;
    }

    pub fn with_max_logs(mut self, max: usize) -> Self {
        self.max_logs = max;
        self
    }

    pub fn add_log(&mut self, level: LogLevel, message: impl Into<String>) {
        let entry = LogEntry::new(level, message);
        self.logs.push_back(entry.clone());

        if level == LogLevel::Error {
            self.errors.push(entry);
        }

        while self.logs.len() > self.max_logs {
            if let Some(removed) = self.logs.pop_front() {
                if removed.level == LogLevel::Error {
                    self.errors.retain(|e| e.timestamp != removed.timestamp);
                }
            }
        }
    }

    pub fn add_log_with_context(
        &mut self,
        level: LogLevel,
        message: impl Into<String>,
        context: impl Into<String>,
    ) {
        let entry = LogEntry::new(level, message).with_context(context);
        self.logs.push_back(entry.clone());

        if level == LogLevel::Error {
            self.errors.push(entry);
        }

        while self.logs.len() > self.max_logs {
            if let Some(removed) = self.logs.pop_front() {
                if removed.level == LogLevel::Error {
                    self.errors.retain(|e| e.timestamp != removed.timestamp);
                }
            }
        }
    }

    pub fn set_metrics(&mut self, metrics: RequestMetrics) {
        self.metrics = Some(metrics.clone());
        self.metrics_collector.record(&metrics);
    }

    pub fn clear_metrics(&mut self) {
        self.metrics = None;
    }

    pub fn record_metrics(&mut self, metrics: &RequestMetrics) {
        self.metrics_collector.record(metrics);
    }

    pub fn metrics_collector(&self) -> &MetricsCollector {
        &self.metrics_collector
    }

    pub fn metrics_collector_mut(&mut self) -> &mut MetricsCollector {
        &mut self.metrics_collector
    }

    pub fn set_error_rate_threshold(&mut self, threshold: f64) {
        self.error_rate_threshold = threshold;
    }

    pub fn add_chunk_interval(&mut self, interval_ms: u64) {
        self.chunk_intervals.push_back(interval_ms);
        while self.chunk_intervals.len() > self.max_intervals {
            self.chunk_intervals.pop_front();
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.errors.clear();
        self.selected_log = 0;
        self.selected_error = 0;
    }

    pub fn log_count(&self) -> usize {
        self.logs.len()
    }

    pub fn latest_entry(&self) -> Option<&LogEntry> {
        self.logs.back()
    }

    pub fn should_compact(&self) -> bool {
        self.logs.len() <= 2
            && self.metrics.is_none()
            && self.errors.is_empty()
            && self.current_request.is_none()
    }

    pub fn handle_key(&mut self, key_code: KeyCode, _modifiers: KeyModifiers) -> bool {
        match self.focused_field {
            FocusedField::Tabs => self.handle_tabs_key(key_code),
            FocusedField::TabContent => self.handle_content_key(key_code),
        }
    }

    fn handle_tabs_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Left => {
                self.selected_tab = self.selected_tab.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                self.selected_tab = (self.selected_tab + 1).min(LogsTab::all().len() - 1);
                true
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                self.focused_field = FocusedField::TabContent;
                true
            }
            _ => false,
        }
    }

    fn handle_content_key(&mut self, key_code: KeyCode) -> bool {
        let tab = LogsTab::all()[self.selected_tab];
        match tab {
            LogsTab::Logs => self.handle_logs_key(key_code),
            LogsTab::Metrics => self.handle_metrics_key(key_code),
            LogsTab::Histogram => self.handle_histogram_key(key_code),
            LogsTab::StatusCodes => self.handle_status_codes_key(key_code),
            LogsTab::Errors => self.handle_errors_key(key_code),
            LogsTab::Curl => self.handle_curl_key(key_code),
        }
    }

    fn handle_curl_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_log = self.selected_log.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_log = (self.selected_log + 1).min(self.logs.len().saturating_sub(1));
                true
            }
            _ => false,
        }
    }

    fn handle_logs_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_log = self.selected_log.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_log = (self.selected_log + 1).min(self.logs.len().saturating_sub(1));
                true
            }
            KeyCode::Char('g') => {
                self.selected_log = 0;
                true
            }
            KeyCode::Char('G') => {
                self.selected_log = self.logs.len().saturating_sub(1);
                true
            }
            KeyCode::Tab => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            _ => false,
        }
    }

    fn handle_metrics_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Tab => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            _ => false,
        }
    }

    fn handle_histogram_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Tab => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            _ => false,
        }
    }

    fn handle_status_codes_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Tab => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            _ => false,
        }
    }

    fn handle_errors_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_error = self.selected_error.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_error =
                    (self.selected_error + 1).min(self.errors.len().saturating_sub(1));
                true
            }
            KeyCode::Char('g') => {
                self.selected_error = 0;
                true
            }
            KeyCode::Char('G') => {
                self.selected_error = self.errors.len().saturating_sub(1);
                true
            }
            KeyCode::Tab => {
                self.focused_field = FocusedField::Tabs;
                true
            }
            _ => false,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let bg = theme.pane_bg(is_active);

        // Background fill
        frame.render_widget(
            Block::default().style(Style::default().bg(bg).fg(theme.foreground.as_color())),
            area,
        );

        let inner = area;

        if area.height <= 4 || (!is_active && self.should_compact()) {
            self.render_compact(frame, inner, theme);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(1), Constraint::Min(0)])
            .split(inner);

        self.render_tabs(frame, chunks[0], theme);
        self.render_content(frame, chunks[1], theme, is_active);
    }

    fn render_compact(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let latest = self
            .latest_entry()
            .map(|entry| format!("{} {}", entry.level.as_str(), entry.message))
            .unwrap_or_else(|| "No activity yet".to_string());
        let summary = Line::from(vec![
            Span::styled(
                "ACTIVITY",
                Style::default()
                    .fg(theme.section_title())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ ", Style::default().fg(theme.muted_color())),
            Span::styled(
                format!("{} logs", self.log_count()),
                Style::default()
                    .fg(theme.typography_level(1).0)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ ", Style::default().fg(theme.muted_color())),
            Span::styled(latest, Style::default().fg(theme.typography_level(2).0)),
        ]);

        let paragraph = Paragraph::new(summary)
            .style(
                Style::default()
                    .bg(theme.subtle_bg())
                    .fg(theme.foreground.as_color()),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let titles: Vec<Line> = LogsTab::all()
            .iter()
            .map(|t| Line::from(t.as_str().to_uppercase()))
            .collect();

        let tabs = Tabs::new(titles)
            .select(self.selected_tab)
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

    fn render_content(&self, frame: &mut Frame, area: Rect, theme: &Theme, _is_active: bool) {
        let tab = LogsTab::all()[self.selected_tab];
        match tab {
            LogsTab::Logs => self.render_logs(frame, area, theme),
            LogsTab::Metrics => self.render_metrics(frame, area, theme),
            LogsTab::Histogram => self.render_histogram(frame, area, theme),
            LogsTab::StatusCodes => self.render_status_codes(frame, area, theme),
            LogsTab::Errors => self.render_errors(frame, area, theme),
            LogsTab::Curl => self.render_curl(frame, area, theme),
        }
    }

    fn render_logs(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let logs: Vec<ListItem> = self
            .logs
            .iter()
            .map(|entry| {
                let color = match entry.level {
                    LogLevel::Debug => theme.semantic.info.as_color(),
                    LogLevel::Info => theme.foreground.as_color(),
                    LogLevel::Warning => theme.semantic.warning.as_color(),
                    LogLevel::Error => theme.semantic.error.as_color(),
                };

                let mut spans = vec![
                    Span::styled(
                        entry.format_timestamp(),
                        Style::default().fg(theme.muted_color()),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!(" {} ", entry.level.as_str()),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(&entry.message, Style::default().fg(color)),
                ];

                if let Some(ref context) = entry.context {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("[{}]", context),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(logs)
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color())
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▎");

        let mut state = ListState::default();
        if !self.logs.is_empty() {
            state.select(Some(self.selected_log));
        }

        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_metrics(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = Vec::new();

        // Aggregated metrics from collector
        let collector = &self.metrics_collector;
        let total_requests = collector.total_requests();

        if total_requests > 0 {
            // Title
            lines.push(Line::from(vec![Span::styled(
                "=== Performance Summary ===",
                Style::default()
                    .fg(theme.foreground.as_color())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
            lines.push(Line::from(""));

            // Total requests
            lines.push(Line::from(vec![
                Span::styled(
                    "Total Requests: ",
                    Style::default().fg(theme.foreground.as_color()),
                ),
                Span::styled(
                    total_requests.to_string(),
                    Style::default().fg(theme.semantic.info.as_color()),
                ),
            ]));

            // Error rate with alert
            let error_rate = collector.error_rate();
            let error_color = if collector.error_rate_exceeds(self.error_rate_threshold) {
                theme.semantic.error.as_color()
            } else {
                theme.semantic.info.as_color()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    "Error Rate: ",
                    Style::default().fg(theme.foreground.as_color()),
                ),
                Span::styled(
                    format!("{:.1}%", error_rate),
                    Style::default()
                        .fg(error_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            // Total errors
            lines.push(Line::from(vec![
                Span::styled(
                    "Total Errors: ",
                    Style::default().fg(theme.foreground.as_color()),
                ),
                Span::styled(
                    collector.total_errors().to_string(),
                    Style::default().fg(theme.semantic.error.as_color()),
                ),
            ]));

            // Average TTFB
            if let Some(avg_ttfb) = collector.avg_ttfb() {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Avg TTFB: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        format!("{:.1}ms", avg_ttfb),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            // Average Duration
            if let Some(avg_dur) = collector.avg_duration() {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Avg Duration: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        format!("{:.1}ms", avg_dur),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            // P95
            if let Some(p95) = collector.p95() {
                lines.push(Line::from(vec![
                    Span::styled("P95: ", Style::default().fg(theme.foreground.as_color())),
                    Span::styled(
                        format!("{}ms", p95),
                        Style::default().fg(theme.semantic.warning.as_color()),
                    ),
                ]));
            }

            // P99
            if let Some(p99) = collector.p99() {
                lines.push(Line::from(vec![
                    Span::styled("P99: ", Style::default().fg(theme.foreground.as_color())),
                    Span::styled(
                        format!("{}ms", p99),
                        Style::default().fg(theme.semantic.error.as_color()),
                    ),
                ]));
            }

            // Total Retries
            let (total_retries, _) = collector.retry_summary();
            if total_retries > 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Total Retries: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        total_retries.to_string(),
                        Style::default().fg(theme.semantic.warning.as_color()),
                    ),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "=== Current Request ===",
                Style::default()
                    .fg(theme.foreground.as_color())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
        }

        // Current request metrics
        if let Some(ref metrics) = self.metrics {
            // Status code
            if let Some(status) = metrics.status_code {
                let status_color = if (200..300).contains(&status) {
                    theme.semantic.success.as_color()
                } else if (300..400).contains(&status) {
                    theme.semantic.info.as_color()
                } else if (400..500).contains(&status) {
                    theme.semantic.warning.as_color()
                } else {
                    theme.semantic.error.as_color()
                };

                lines.push(Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(theme.foreground.as_color())),
                    Span::styled(
                        status.to_string(),
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }

            // Timing
            let timing = &metrics.timing;
            if let Some(ttfb) = timing.ttfb_ms {
                lines.push(Line::from(vec![
                    Span::styled("TTFB: ", Style::default().fg(theme.foreground.as_color())),
                    Span::styled(
                        format!("{}ms", ttfb),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            if let Some(total) = timing.total_ms {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Duration: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        format!("{}ms", total),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            if let Some(dns) = timing.dns_ms {
                lines.push(Line::from(vec![
                    Span::styled("DNS: ", Style::default().fg(theme.foreground.as_color())),
                    Span::styled(
                        format!("{}ms", dns),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            if let Some(connect) = timing.connect_ms {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Connect: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        format!("{}ms", connect),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            if let Some(tls) = timing.tls_ms {
                lines.push(Line::from(vec![
                    Span::styled("TLS: ", Style::default().fg(theme.foreground.as_color())),
                    Span::styled(
                        format!("{}ms", tls),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            // Body size
            if metrics.body_size > 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Body Size: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        format_bytes(metrics.body_size),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                ]));
            }

            // Retries
            if metrics.retries > 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Retries: ",
                        Style::default().fg(theme.foreground.as_color()),
                    ),
                    Span::styled(
                        metrics.retries.to_string(),
                        Style::default().fg(theme.semantic.warning.as_color()),
                    ),
                ]));
            }
        } else if total_requests == 0 {
            lines.push(Line::from(
                "No metrics available. Send a request to see metrics.",
            ));
        }

        let paragraph = Paragraph::new(lines)
            .style(
                Style::default()
                    .bg(theme.pane_bg(false))
                    .fg(theme.foreground.as_color()),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_histogram(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Show request duration histogram if available, otherwise chunk intervals
        let durations: Vec<u64> = self.metrics_collector.duration_history().to_vec();

        if !durations.is_empty() {
            // Create histogram buckets for request durations
            let max_duration = durations.iter().max().copied().unwrap_or(1);
            let bucket_count = 10.min(durations.len());
            let bucket_size = (max_duration as f64 / bucket_count as f64).ceil() as u64 + 1;

            let mut buckets = vec![0u64; bucket_count];
            for &duration in &durations {
                let bucket_idx =
                    ((duration as f64 / bucket_size as f64).floor() as usize).min(bucket_count - 1);
                buckets[bucket_idx] += 1;
            }

            let bar_labels: Vec<String> = buckets
                .iter()
                .enumerate()
                .map(|(i, _)| format!("{}", i * bucket_size as usize))
                .collect();

            let bar_data: Vec<(&str, u64)> = bar_labels
                .iter()
                .zip(buckets.iter())
                .map(|(label, &count)| (label.as_str(), count))
                .collect();

            let barchart = BarChart::default()
                .bar_width(8)
                .bar_gap(1)
                .bar_style(Style::default().fg(theme.semantic.info.as_color()))
                .value_style(
                    Style::default()
                        .fg(theme.pane_bg(false))
                        .bg(theme.semantic.info.as_color()),
                )
                .data(&bar_data);

            frame.render_widget(barchart, area);
        } else if !self.chunk_intervals.is_empty() {
            // Fall back to chunk interval histogram
            let intervals: Vec<u64> = self.chunk_intervals.iter().copied().collect();
            let max_interval = intervals.iter().max().copied().unwrap_or(1);
            let bucket_count = 10.min(intervals.len());
            let bucket_size = (max_interval as f64 / bucket_count as f64).ceil() as u64 + 1;

            let mut buckets = vec![0u64; bucket_count];
            for &interval in &intervals {
                let bucket_idx =
                    ((interval as f64 / bucket_size as f64).floor() as usize).min(bucket_count - 1);
                buckets[bucket_idx] += 1;
            }

            let bar_labels: Vec<String> = buckets
                .iter()
                .enumerate()
                .map(|(i, _)| format!("{}", i * bucket_size as usize))
                .collect();

            let bar_data: Vec<(&str, u64)> = bar_labels
                .iter()
                .zip(buckets.iter())
                .map(|(label, &count)| (label.as_str(), count))
                .collect();

            let barchart = BarChart::default()
                .bar_width(8)
                .bar_gap(1)
                .bar_style(Style::default().fg(theme.semantic.info.as_color()))
                .value_style(
                    Style::default()
                        .fg(theme.pane_bg(false))
                        .bg(theme.semantic.info.as_color()),
                )
                .data(&bar_data);

            frame.render_widget(barchart, area);
        } else {
            let paragraph =
                Paragraph::new("No data available. Send requests to see the histogram.")
                    .style(
                        Style::default()
                            .fg(theme.text_muted())
                            .bg(theme.pane_bg(false)),
                    )
                    .alignment(Alignment::Center);

            frame.render_widget(paragraph, area);
        }
    }

    fn render_status_codes(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let collector = &self.metrics_collector;

        if collector.total_requests() == 0 {
            let paragraph = Paragraph::new(
                "No request data available. Send requests to see status code distribution.",
            )
            .style(
                Style::default()
                    .fg(theme.text_muted())
                    .bg(theme.pane_bg(false)),
            )
            .alignment(Alignment::Center);

            frame.render_widget(paragraph, area);
            return;
        }

        // Get status code distribution and render as bar chart
        let distribution = collector.status_distribution();

        // Create bar chart data from grouped distribution
        let bar_data: Vec<(&str, u64)> = distribution
            .iter()
            .map(|(label, count)| (label.as_str(), u64::from(*count)))
            .collect();

        if !bar_data.is_empty() {
            let barchart = BarChart::default()
                .bar_width(8)
                .bar_gap(1)
                .bar_style(Style::default().fg(theme.semantic.info.as_color()))
                .value_style(
                    Style::default()
                        .fg(theme.pane_bg(false))
                        .bg(theme.semantic.info.as_color()),
                )
                .data(&bar_data);

            frame.render_widget(barchart, area);
        }

        // Also show detailed breakdown below the chart (if space permits)
        // This would require splitting the area, but for simplicity, we just show the chart
    }

    fn render_errors(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.errors.is_empty() {
            let paragraph = Paragraph::new("No errors logged.")
                .style(
                    Style::default()
                        .fg(theme.text_muted())
                        .bg(theme.pane_bg(false)),
                )
                .alignment(Alignment::Center);

            frame.render_widget(paragraph, area);
            return;
        }

        let error_items: Vec<ListItem> = self
            .errors
            .iter()
            .map(|entry| {
                let mut spans = vec![
                    Span::styled(
                        entry.format_timestamp(),
                        Style::default().fg(theme.semantic.info.as_color()),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        "ERROR",
                        Style::default()
                            .fg(theme.semantic.error.as_color())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        &entry.message,
                        Style::default().fg(theme.semantic.error.as_color()),
                    ),
                ];

                if let Some(ref context) = entry.context {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("\n  Context: {}", context),
                        Style::default().fg(theme.foreground.as_color()),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(error_items)
            .highlight_style(
                Style::default()
                    .bg(theme.highlight.selected_bg.as_color())
                    .fg(theme.highlight.selected_fg.as_color()),
            )
            .highlight_symbol(">> ");

        let mut state = ListState::default();
        state.select(Some(self.selected_error));

        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_curl(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if let Some(ref request) = self.current_request {
            let curl_cmd = request_to_curl(request);
            let lines: Vec<Line> = curl_cmd
                .lines()
                .map(|line| {
                    Line::from(vec![Span::styled(
                        line,
                        Style::default().fg(theme.foreground.as_color()),
                    )])
                })
                .collect();

            let paragraph = Paragraph::new(lines)
                .style(
                    Style::default()
                        .bg(theme.pane_bg(false))
                        .fg(theme.foreground.as_color()),
                )
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new(
                "No request available. Create a request to see the curl equivalent.",
            )
            .style(
                Style::default()
                    .fg(theme.text_muted())
                    .bg(theme.pane_bg(false)),
            )
            .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        }
    }
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use yinx_core::timing::{RequestMetrics, Timing};

    #[test]
    fn test_logs_tab_all() {
        let tabs = LogsTab::all();
        assert_eq!(tabs.len(), 6);
    }

    #[test]
    fn test_logs_tab_as_str() {
        assert_eq!(LogsTab::Logs.as_str(), "Logs");
        assert_eq!(LogsTab::Metrics.as_str(), "Metrics");
        assert_eq!(LogsTab::Histogram.as_str(), "Histogram");
        assert_eq!(LogsTab::StatusCodes.as_str(), "Status");
        assert_eq!(LogsTab::Errors.as_str(), "Errors");
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Warning.as_str(), "WARN");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test]
    fn test_log_level_all() {
        let levels = LogLevel::all();
        assert_eq!(levels.len(), 4);
    }

    #[test]
    fn test_log_entry_new() {
        let entry = LogEntry::new(LogLevel::Info, "test message");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "test message");
        assert!(entry.context.is_none());
    }

    #[test]
    fn test_log_entry_with_context() {
        let entry =
            LogEntry::new(LogLevel::Error, "error occurred").with_context("stack trace here");
        assert_eq!(entry.context, Some("stack trace here".to_string()));
    }

    #[test]
    fn test_logs_pane_new() {
        let pane = LogsPane::new();
        assert_eq!(pane.selected_tab, 0);
        assert!(pane.logs.is_empty());
        assert!(pane.metrics.is_none());
        assert_eq!(pane.metrics_collector.total_requests(), 0);
    }

    #[test]
    fn test_logs_pane_record_metrics() {
        let mut pane = LogsPane::new();
        let timing = Timing::new().with_ttfb(100).with_total(200);
        let metrics = RequestMetrics::new()
            .with_timing(timing)
            .with_status_code(200);
        pane.record_metrics(&metrics);
        assert_eq!(pane.metrics_collector.total_requests(), 1);
    }

    #[test]
    fn test_logs_pane_set_metrics_records_to_collector() {
        let mut pane = LogsPane::new();
        let timing = Timing::new().with_ttfb(100).with_total(200);
        let metrics = RequestMetrics::new()
            .with_timing(timing)
            .with_status_code(200);
        pane.set_metrics(metrics);
        assert_eq!(pane.metrics_collector.total_requests(), 1);
        assert!(pane.metrics.is_some());
    }

    #[test]
    fn test_logs_pane_error_rate_threshold() {
        let mut pane = LogsPane::new();
        pane.set_error_rate_threshold(5.0);
        assert_eq!(pane.error_rate_threshold, 5.0);
    }

    #[test]
    fn test_logs_pane_add_log() {
        let mut pane = LogsPane::new();
        pane.add_log(LogLevel::Info, "test message");
        assert_eq!(pane.logs.len(), 1);
        assert_eq!(pane.logs[0].level, LogLevel::Info);
    }

    #[test]
    fn test_logs_pane_add_error_log() {
        let mut pane = LogsPane::new();
        pane.add_log(LogLevel::Error, "error message");
        assert_eq!(pane.errors.len(), 1);
        assert_eq!(pane.errors[0].message, "error message");
    }

    #[test]
    fn test_logs_pane_add_log_with_context() {
        let mut pane = LogsPane::new();
        pane.add_log_with_context(LogLevel::Error, "error", "context info");
        assert_eq!(pane.logs.len(), 1);
        assert_eq!(pane.errors.len(), 1);
    }

    #[test]
    fn test_logs_pane_max_logs() {
        let mut pane = LogsPane::new().with_max_logs(3);
        pane.add_log(LogLevel::Info, "msg1");
        pane.add_log(LogLevel::Info, "msg2");
        pane.add_log(LogLevel::Info, "msg3");
        pane.add_log(LogLevel::Info, "msg4");
        assert_eq!(pane.logs.len(), 3);
    }

    #[test]
    fn test_logs_pane_set_metrics() {
        let mut pane = LogsPane::new();
        let timing = Timing::new().with_ttfb(100).with_total(200);
        let metrics = RequestMetrics::new()
            .with_timing(timing)
            .with_status_code(200);
        pane.set_metrics(metrics);
        assert!(pane.metrics.is_some());
        assert_eq!(pane.metrics.unwrap().status_code, Some(200));
    }

    #[test]
    fn test_logs_pane_clear_metrics() {
        let mut pane = LogsPane::new();
        let timing = Timing::new().with_total(100);
        let metrics = RequestMetrics::new().with_timing(timing);
        pane.set_metrics(metrics);
        pane.clear_metrics();
        assert!(pane.metrics.is_none());
    }

    #[test]
    fn test_logs_pane_add_chunk_interval() {
        let mut pane = LogsPane::new();
        pane.add_chunk_interval(50);
        pane.add_chunk_interval(75);
        assert_eq!(pane.chunk_intervals.len(), 2);
    }

    #[test]
    fn test_logs_pane_clear_logs() {
        let mut pane = LogsPane::new();
        pane.add_log(LogLevel::Error, "error");
        pane.add_log(LogLevel::Info, "info");
        pane.clear_logs();
        assert!(pane.logs.is_empty());
        assert!(pane.errors.is_empty());
    }

    #[test]
    fn test_logs_pane_handle_tabs_key_left() {
        let mut pane = LogsPane::new();
        pane.selected_tab = 2;
        pane.focused_field = FocusedField::Tabs;
        assert!(pane.handle_key(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(pane.selected_tab, 1);
    }

    #[test]
    fn test_logs_pane_handle_tabs_key_right() {
        let mut pane = LogsPane::new();
        pane.focused_field = FocusedField::Tabs;
        assert!(pane.handle_key(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(pane.selected_tab, 1);
    }

    #[test]
    fn test_logs_pane_handle_tabs_key_enter() {
        let mut pane = LogsPane::new();
        pane.focused_field = FocusedField::Tabs;
        assert!(pane.handle_key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(pane.focused_field, FocusedField::TabContent);
    }

    #[test]
    fn test_logs_pane_handle_logs_key_up() {
        let mut pane = LogsPane::new();
        pane.add_log(LogLevel::Info, "msg1");
        pane.add_log(LogLevel::Info, "msg2");
        pane.selected_log = 1;
        assert!(pane.handle_key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(pane.selected_log, 0);
    }

    #[test]
    fn test_logs_pane_handle_logs_key_down() {
        let mut pane = LogsPane::new();
        pane.add_log(LogLevel::Info, "msg1");
        pane.add_log(LogLevel::Info, "msg2");
        assert!(pane.handle_key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(pane.selected_log, 1);
    }

    #[test]
    fn test_logs_pane_handle_errors_key_up() {
        let mut pane = LogsPane::new();
        pane.add_log(LogLevel::Error, "err1");
        pane.add_log(LogLevel::Error, "err2");
        pane.selected_error = 1;
        pane.selected_tab = LogsTab::all()
            .iter()
            .position(|&t| t == LogsTab::Errors)
            .unwrap();
        assert!(pane.handle_key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(pane.selected_error, 0);
    }

    #[test]
    fn test_format_bytes_b() {
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(format_bytes(2048), "2.00 KB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(1048576), "1.00 MB");
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_log_entry_timestamp_format() {
        let entry = LogEntry::new(LogLevel::Info, "test");
        let ts = entry.format_timestamp();
        assert!(ts.contains(':'));
    }

    #[test]
    fn test_logs_pane_with_max_logs() {
        let pane = LogsPane::new().with_max_logs(500);
        assert_eq!(pane.max_logs, 500);
    }
}
