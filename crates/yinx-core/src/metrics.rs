//! Observability metrics collection for aggregated request statistics.

use crate::timing::RequestMetrics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Collects and aggregates request metrics across multiple requests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsCollector {
    /// History of request durations in milliseconds.
    durations: Vec<u64>,
    /// History of TTFB values in milliseconds.
    ttfb_history: Vec<u64>,
    /// Status code to count mapping.
    status_counts: HashMap<u16, u32>,
    /// Total requests tracked.
    total_requests: u32,
    /// Total errors (status >= 400).
    error_count: u32,
    /// Retry count with reasons: (count, reason).
    retry_reasons: Vec<(u32, String)>,
    /// Total retries across all requests.
    total_retries: u32,
    /// Maximum history size to prevent unbounded growth.
    #[serde(skip)]
    max_history: usize,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            durations: Vec::new(),
            ttfb_history: Vec::new(),
            status_counts: HashMap::new(),
            total_requests: 0,
            error_count: 0,
            retry_reasons: Vec::new(),
            total_retries: 0,
            max_history: 1000,
        }
    }

    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    /// Record a completed request's metrics.
    pub fn record(&mut self, metrics: &RequestMetrics) {
        self.total_requests += 1;

        // Record duration
        if let Some(total_ms) = metrics.timing.total_ms {
            self.durations.push(total_ms);
            self.truncate_history();
        }

        // Record TTFB
        if let Some(ttfb) = metrics.timing.ttfb_ms {
            self.ttfb_history.push(ttfb);
            self.truncate_history();
        }

        // Record status code
        if let Some(status) = metrics.status_code {
            *self.status_counts.entry(status).or_insert(0) += 1;
            if status >= 400 {
                self.error_count += 1;
            }
        }

        // Record retries
        if metrics.retries > 0 {
            self.total_retries += metrics.retries;
            // Find or add retry reason entry
            let reason = "request_retry".to_string(); // Default reason
            match self.retry_reasons.iter_mut().find(|(_, r)| r == &reason) {
                Some((count, _)) => *count += metrics.retries,
                None => self.retry_reasons.push((metrics.retries, reason)),
            }
        }
    }

    /// Record retry with a specific reason.
    pub fn record_retry(&mut self, reason: impl Into<String>) {
        self.total_retries += 1;
        let reason = reason.into();
        match self.retry_reasons.iter_mut().find(|(_, r)| r == &reason) {
            Some((count, _)) => *count += 1,
            None => self.retry_reasons.push((1, reason)),
        }
    }

    fn truncate_history(&mut self) {
        if self.durations.len() > self.max_history {
            let excess = self.durations.len() - self.max_history;
            self.durations.drain(0..excess);
        }
        if self.ttfb_history.len() > self.max_history {
            let excess = self.ttfb_history.len() - self.max_history;
            self.ttfb_history.drain(0..excess);
        }
    }

    /// Calculate average TTFB in milliseconds.
    pub fn avg_ttfb(&self) -> Option<f64> {
        if self.ttfb_history.is_empty() {
            return None;
        }
        let sum: u64 = self.ttfb_history.iter().sum();
        Some(sum as f64 / self.ttfb_history.len() as f64)
    }

    /// Calculate average request duration in milliseconds.
    pub fn avg_duration(&self) -> Option<f64> {
        if self.durations.is_empty() {
            return None;
        }
        let sum: u64 = self.durations.iter().sum();
        Some(sum as f64 / self.durations.len() as f64)
    }

    /// Calculate percentile (0.0 to 100.0) for request durations.
    pub fn percentile(&self, p: f64) -> Option<u64> {
        if self.durations.is_empty() {
            return None;
        }
        let mut sorted = self.durations.clone();
        sorted.sort_unstable();
        let index = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
        Some(sorted[index.min(sorted.len() - 1)])
    }

    /// Get p95 request duration.
    pub fn p95(&self) -> Option<u64> {
        self.percentile(95.0)
    }

    /// Get p99 request duration.
    pub fn p99(&self) -> Option<u64> {
        self.percentile(99.0)
    }

    /// Calculate error rate as a percentage (0.0 to 100.0).
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        (self.error_count as f64 / self.total_requests as f64) * 100.0
    }

    /// Check if error rate exceeds threshold.
    pub fn error_rate_exceeds(&self, threshold_pct: f64) -> bool {
        self.error_rate() > threshold_pct
    }

    /// Get status code distribution as (label, count) pairs.
    pub fn status_distribution(&self) -> Vec<(String, u32)> {
        let mut distribution = Vec::new();

        let mut by_group: HashMap<&str, u32> = HashMap::new();
        for (&status, &count) in &self.status_counts {
            let group = match status {
                200..=299 => "2xx",
                300..=399 => "3xx",
                400..=499 => "4xx",
                500..=599 => "5xx",
                _ => "other",
            };
            *by_group.entry(group).or_insert(0) += count;
        }

        for (label, count) in by_group {
            distribution.push((label.to_string(), count));
        }

        distribution.sort_by(|a, b| a.0.cmp(&b.0));
        distribution
    }

    /// Get detailed status code breakdown.
    pub fn status_breakdown(&self) -> Vec<(u16, u32)> {
        let mut breakdown: Vec<_> = self.status_counts.iter().map(|(&s, &c)| (s, c)).collect();
        breakdown.sort_by_key(|&(s, _)| s);
        breakdown
    }

    /// Get retry summary as (total_retries, reasons).
    pub fn retry_summary(&self) -> (u32, Vec<(u32, String)>) {
        (self.total_retries, self.retry_reasons.clone())
    }

    /// Get total requests tracked.
    pub fn total_requests(&self) -> u32 {
        self.total_requests
    }

    /// Get total errors.
    pub fn total_errors(&self) -> u32 {
        self.error_count
    }

    /// Get duration history for histogram.
    pub fn duration_history(&self) -> &[u64] {
        &self.durations
    }

    /// Get TTFB history.
    pub fn ttfb_history(&self) -> &[u64] {
        &self.ttfb_history
    }

    /// Reset all collected metrics.
    pub fn reset(&mut self) {
        self.durations.clear();
        self.ttfb_history.clear();
        self.status_counts.clear();
        self.total_requests = 0;
        self.error_count = 0;
        self.retry_reasons.clear();
        self.total_retries = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::{RequestMetrics, Timing};

    fn make_metrics(status: u16, total_ms: u64, ttfb_ms: u64, retries: u32) -> RequestMetrics {
        RequestMetrics {
            timing: Timing {
                dns_ms: None,
                connect_ms: None,
                tls_ms: None,
                ttfb_ms: Some(ttfb_ms),
                total_ms: Some(total_ms),
            },
            status_code: Some(status),
            body_size: 0,
            retries,
        }
    }

    #[test]
    fn test_metrics_collector_new() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.total_requests(), 0);
        assert_eq!(collector.total_errors(), 0);
        assert_eq!(collector.total_retries, 0);
    }

    #[test]
    fn test_metrics_collector_record() {
        let mut collector = MetricsCollector::new();
        let metrics = make_metrics(200, 150, 50, 0);
        collector.record(&metrics);

        assert_eq!(collector.total_requests(), 1);
        assert_eq!(collector.total_errors(), 0);
    }

    #[test]
    fn test_metrics_collector_record_error() {
        let mut collector = MetricsCollector::new();
        let metrics = make_metrics(500, 200, 60, 0);
        collector.record(&metrics);

        assert_eq!(collector.total_requests(), 1);
        assert_eq!(collector.total_errors(), 1);
    }

    #[test]
    fn test_metrics_collector_record_retries() {
        let mut collector = MetricsCollector::new();
        let metrics = make_metrics(200, 150, 50, 3);
        collector.record(&metrics);

        assert_eq!(collector.total_retries, 3);
        let (total, reasons) = collector.retry_summary();
        assert_eq!(total, 3);
        assert_eq!(reasons.len(), 1);
    }

    #[test]
    fn test_metrics_collector_avg_ttfb() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 30, 0));
        collector.record(&make_metrics(200, 200, 50, 0));
        collector.record(&make_metrics(200, 300, 70, 0));

        let avg = collector.avg_ttfb().unwrap();
        assert!((avg - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_avg_ttfb_empty() {
        let collector = MetricsCollector::new();
        assert!(collector.avg_ttfb().is_none());
    }

    #[test]
    fn test_metrics_collector_avg_duration() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 30, 0));
        collector.record(&make_metrics(200, 200, 50, 0));
        collector.record(&make_metrics(200, 300, 70, 0));

        let avg = collector.avg_duration().unwrap();
        assert!((avg - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_percentile() {
        let mut collector = MetricsCollector::new();
        for i in 1..=100u64 {
            collector.record(&make_metrics(200, i * 10, 50, 0));
        }

        let p50 = collector.percentile(50.0).unwrap();
        let p95 = collector.p95().unwrap();
        let p99 = collector.p99().unwrap();

        assert!(p50 > 0);
        assert!(p95 > p50);
        assert!(p99 >= p95);
    }

    #[test]
    fn test_metrics_collector_percentile_empty() {
        let collector = MetricsCollector::new();
        assert!(collector.percentile(95.0).is_none());
    }

    #[test]
    fn test_metrics_collector_error_rate() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 50, 0));
        collector.record(&make_metrics(200, 200, 50, 0));
        collector.record(&make_metrics(500, 300, 50, 0));

        let rate = collector.error_rate();
        assert!((rate - 33.333).abs() < 1.0);
    }

    #[test]
    fn test_metrics_collector_error_rate_zero() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.error_rate(), 0.0);
    }

    #[test]
    fn test_metrics_collector_error_rate_exceeds() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 50, 0));
        collector.record(&make_metrics(500, 200, 50, 0));
        collector.record(&make_metrics(500, 300, 50, 0));

        assert!(collector.error_rate_exceeds(50.0));
        assert!(!collector.error_rate_exceeds(70.0));
    }

    #[test]
    fn test_metrics_collector_status_distribution() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 50, 0));
        collector.record(&make_metrics(201, 100, 50, 0));
        collector.record(&make_metrics(301, 100, 50, 0));
        collector.record(&make_metrics(404, 100, 50, 0));
        collector.record(&make_metrics(500, 100, 50, 0));

        let dist = collector.status_distribution();
        assert!(dist.iter().any(|(label, _)| label == "2xx"));
        assert!(dist.iter().any(|(label, _)| label == "3xx"));
        assert!(dist.iter().any(|(label, _)| label == "4xx"));
        assert!(dist.iter().any(|(label, _)| label == "5xx"));
    }

    #[test]
    fn test_metrics_collector_status_breakdown() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 50, 0));
        collector.record(&make_metrics(404, 100, 50, 0));

        let breakdown = collector.status_breakdown();
        assert_eq!(breakdown.len(), 2);
        assert!(breakdown.iter().any(|&(s, _)| s == 200));
        assert!(breakdown.iter().any(|&(s, _)| s == 404));
    }

    #[test]
    fn test_metrics_collector_record_retry_with_reason() {
        let mut collector = MetricsCollector::new();
        collector.record_retry("timeout");
        collector.record_retry("timeout");
        collector.record_retry("connection refused");

        let (total, reasons) = collector.retry_summary();
        assert_eq!(total, 3);
        assert_eq!(reasons.len(), 2);
    }

    #[test]
    fn test_metrics_collector_reset() {
        let mut collector = MetricsCollector::new();
        collector.record(&make_metrics(200, 100, 50, 0));
        collector.record(&make_metrics(500, 200, 50, 0));

        collector.reset();
        assert_eq!(collector.total_requests(), 0);
        assert_eq!(collector.total_errors(), 0);
        assert_eq!(collector.total_retries, 0);
    }

    #[test]
    fn test_metrics_collector_truncate_history() {
        let mut collector = MetricsCollector::new().with_max_history(3);
        collector.record(&make_metrics(200, 100, 50, 0));
        collector.record(&make_metrics(200, 200, 50, 0));
        collector.record(&make_metrics(200, 300, 50, 0));
        collector.record(&make_metrics(200, 400, 50, 0));

        assert!(collector.duration_history().len() <= 3);
    }

    #[test]
    fn test_metrics_collector_serde_roundtrip() {
        let collector = MetricsCollector::new();
        // Note: we can't easily test serde for collector since record takes &mut
        // Just verify it serializes without error
        let json = serde_json::to_string(&collector).unwrap();
        let decoded: MetricsCollector = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_requests(), 0);
    }
}
