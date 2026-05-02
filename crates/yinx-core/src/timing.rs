use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    pub dns_ms: Option<u64>,
    pub connect_ms: Option<u64>,
    pub tls_ms: Option<u64>,
    pub ttfb_ms: Option<u64>,
    pub total_ms: Option<u64>,
}

impl Timing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_dns(mut self, ms: u64) -> Self {
        self.dns_ms = Some(ms);
        self
    }

    pub fn with_connect(mut self, ms: u64) -> Self {
        self.connect_ms = Some(ms);
        self
    }

    pub fn with_tls(mut self, ms: u64) -> Self {
        self.tls_ms = Some(ms);
        self
    }

    pub fn with_ttfb(mut self, ms: u64) -> Self {
        self.ttfb_ms = Some(ms);
        self
    }

    pub fn with_total(mut self, ms: u64) -> Self {
        self.total_ms = Some(ms);
        self
    }

    pub fn set_dns(&mut self, ms: u64) {
        self.dns_ms = Some(ms);
    }

    pub fn set_connect(&mut self, ms: u64) {
        self.connect_ms = Some(ms);
    }

    pub fn set_tls(&mut self, ms: u64) {
        self.tls_ms = Some(ms);
    }

    pub fn set_ttfb(&mut self, ms: u64) {
        self.ttfb_ms = Some(ms);
    }

    pub fn set_total(&mut self, ms: u64) {
        self.total_ms = Some(ms);
    }

    pub fn is_complete(&self) -> bool {
        self.dns_ms.is_some()
            && self.connect_ms.is_some()
            && self.tls_ms.is_some()
            && self.ttfb_ms.is_some()
            && self.total_ms.is_some()
    }

    pub fn has_any(&self) -> bool {
        self.dns_ms.is_some()
            || self.connect_ms.is_some()
            || self.tls_ms.is_some()
            || self.ttfb_ms.is_some()
            || self.total_ms.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RequestMetrics {
    pub timing: Timing,
    pub status_code: Option<u16>,
    pub body_size: usize,
    pub retries: u32,
}

impl RequestMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timing(mut self, timing: Timing) -> Self {
        self.timing = timing;
        self
    }

    pub fn with_status_code(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }

    pub fn with_body_size(mut self, size: usize) -> Self {
        self.body_size = size;
        self
    }

    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    pub fn increment_retry(&mut self) {
        self.retries += 1;
    }

    pub fn is_success(&self) -> bool {
        self.status_code
            .map(|code| (200..=299).contains(&code))
            .unwrap_or(false)
    }

    pub fn total_duration(&self) -> Option<Duration> {
        self.timing.total_ms.map(Duration::from_millis)
    }
}

pub struct Stopwatch {
    start: Instant,
    phases: Vec<(&'static str, Duration)>,
}

impl Stopwatch {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
            phases: Vec::new(),
        }
    }

    pub fn lap(&mut self, label: &'static str) -> Duration {
        let elapsed = self.start.elapsed();
        self.phases.push((label, elapsed));
        elapsed
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub fn phases(&self) -> &[(&'static str, Duration)] {
        &self.phases
    }

    pub fn phase_duration(&self, label: &'static str) -> Option<Duration> {
        self.phases
            .iter()
            .find(|(l, _)| *l == label)
            .map(|(_, d)| *d)
    }

    pub fn phase_duration_ms(&self, label: &'static str) -> Option<u64> {
        self.phase_duration(label).map(|d| d.as_millis() as u64)
    }

    pub fn into_timing(self) -> Timing {
        let total = self.start.elapsed().as_millis() as u64;
        let mut timing = Timing::new().with_total(total);

        for (label, duration) in &self.phases {
            let ms = duration.as_millis() as u64;
            match *label {
                "dns" => timing.set_dns(ms),
                "connect" => timing.set_connect(ms),
                "tls" => timing.set_tls(ms),
                "ttfb" => timing.set_ttfb(ms),
                _ => {}
            }
        }

        timing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_default() {
        let timing = Timing::new();
        assert!(timing.dns_ms.is_none());
        assert!(timing.connect_ms.is_none());
        assert!(timing.tls_ms.is_none());
        assert!(timing.ttfb_ms.is_none());
        assert!(timing.total_ms.is_none());
    }

    #[test]
    fn test_timing_builder() {
        let timing = Timing::new()
            .with_dns(10)
            .with_connect(20)
            .with_tls(30)
            .with_ttfb(100)
            .with_total(150);
        assert_eq!(timing.dns_ms, Some(10));
        assert_eq!(timing.connect_ms, Some(20));
        assert_eq!(timing.tls_ms, Some(30));
        assert_eq!(timing.ttfb_ms, Some(100));
        assert_eq!(timing.total_ms, Some(150));
    }

    #[test]
    fn test_timing_setters() {
        let mut timing = Timing::new();
        timing.set_dns(5);
        timing.set_connect(15);
        timing.set_ttfb(50);
        assert_eq!(timing.dns_ms, Some(5));
        assert_eq!(timing.connect_ms, Some(15));
        assert_eq!(timing.ttfb_ms, Some(50));
    }

    #[test]
    fn test_timing_partial_updates() {
        let mut timing = Timing::new();
        assert!(!timing.is_complete());
        assert!(!timing.has_any());

        timing.set_ttfb(100);
        assert!(!timing.is_complete());
        assert!(timing.has_any());

        timing.set_dns(10);
        timing.set_connect(20);
        timing.set_tls(30);
        timing.set_total(200);
        assert!(timing.is_complete());
    }

    #[test]
    fn test_timing_serde_roundtrip() {
        let timing = Timing::new().with_dns(10).with_ttfb(100).with_total(150);
        let json = serde_json::to_string(&timing).unwrap();
        let decoded: Timing = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.dns_ms, timing.dns_ms);
        assert_eq!(decoded.ttfb_ms, timing.ttfb_ms);
    }

    #[test]
    fn test_request_metrics_default() {
        let metrics = RequestMetrics::new();
        assert_eq!(metrics.body_size, 0);
        assert_eq!(metrics.retries, 0);
        assert!(metrics.status_code.is_none());
    }

    #[test]
    fn test_request_metrics_builder() {
        let timing = Timing::new().with_total(100);
        let metrics = RequestMetrics::new()
            .with_timing(timing)
            .with_status_code(200)
            .with_body_size(1024)
            .with_retries(2);
        assert_eq!(metrics.timing.total_ms, Some(100));
        assert_eq!(metrics.status_code, Some(200));
        assert_eq!(metrics.body_size, 1024);
        assert_eq!(metrics.retries, 2);
    }

    #[test]
    fn test_request_metrics_increment_retry() {
        let mut metrics = RequestMetrics::new();
        assert_eq!(metrics.retries, 0);
        metrics.increment_retry();
        metrics.increment_retry();
        assert_eq!(metrics.retries, 2);
    }

    #[test]
    fn test_request_metrics_is_success() {
        let success = RequestMetrics::new().with_status_code(200);
        assert!(success.is_success());

        let client_error = RequestMetrics::new().with_status_code(404);
        assert!(!client_error.is_success());

        let server_error = RequestMetrics::new().with_status_code(500);
        assert!(!server_error.is_success());

        let no_status = RequestMetrics::new();
        assert!(!no_status.is_success());
    }

    #[test]
    fn test_request_metrics_total_duration() {
        let timing = Timing::new().with_total(150);
        let metrics = RequestMetrics::new().with_timing(timing);
        assert_eq!(metrics.total_duration(), Some(Duration::from_millis(150)));
    }

    #[test]
    fn test_request_metrics_total_duration_none() {
        let metrics = RequestMetrics::new();
        assert!(metrics.total_duration().is_none());
    }

    #[test]
    fn test_request_metrics_serde_roundtrip() {
        let metrics = RequestMetrics::new()
            .with_status_code(201)
            .with_body_size(512)
            .with_retries(1);
        let json = serde_json::to_string(&metrics).unwrap();
        let decoded: RequestMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.status_code, metrics.status_code);
        assert_eq!(decoded.body_size, metrics.body_size);
    }

    #[test]
    fn test_stopwatch_elapsed() {
        let sw = Stopwatch::start();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = sw.elapsed();
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn test_stopwatch_lap() {
        let mut sw = Stopwatch::start();
        std::thread::sleep(Duration::from_millis(5));
        let lap1 = sw.lap("phase1");
        std::thread::sleep(Duration::from_millis(5));
        let lap2 = sw.lap("phase2");
        assert!(lap2 > lap1);
    }

    #[test]
    fn test_stopwatch_phases() {
        let mut sw = Stopwatch::start();
        sw.lap("dns");
        sw.lap("connect");
        sw.lap("ttfb");
        assert_eq!(sw.phases().len(), 3);
        assert_eq!(sw.phases()[0].0, "dns");
        assert_eq!(sw.phases()[1].0, "connect");
    }

    #[test]
    fn test_stopwatch_phase_duration() {
        let mut sw = Stopwatch::start();
        sw.lap("dns");
        std::thread::sleep(Duration::from_millis(5));
        sw.lap("connect");

        let dns_dur = sw.phase_duration("dns");
        let connect_dur = sw.phase_duration("connect");
        assert!(dns_dur.is_some());
        assert!(connect_dur.is_some());
        assert!(connect_dur.unwrap() > dns_dur.unwrap());

        assert!(sw.phase_duration("nonexistent").is_none());
    }

    #[test]
    fn test_stopwatch_phase_duration_ms() {
        let mut sw = Stopwatch::start();
        sw.lap("dns");
        assert!(sw.phase_duration_ms("dns").is_some());
        assert!(sw.phase_duration_ms("missing").is_none());
    }

    #[test]
    fn test_stopwatch_elapsed_ms() {
        let sw = Stopwatch::start();
        std::thread::sleep(Duration::from_millis(15));
        assert!(sw.elapsed_ms() >= 15);
    }

    #[test]
    fn test_stopwatch_sub_millisecond_accuracy() {
        let sw = Stopwatch::start();
        let elapsed = sw.elapsed();
        assert!(elapsed.as_nanos() > 0);
    }

    #[test]
    fn test_stopwatch_into_timing() {
        let mut sw = Stopwatch::start();
        sw.lap("dns");
        sw.lap("connect");
        sw.lap("tls");
        sw.lap("ttfb");

        let timing = sw.into_timing();
        assert!(timing.dns_ms.is_some());
        assert!(timing.connect_ms.is_some());
        assert!(timing.tls_ms.is_some());
        assert!(timing.ttfb_ms.is_some());
        assert!(timing.total_ms.is_some());
        assert!(timing.is_complete());
    }

    #[test]
    fn test_stopwatch_into_timing_partial() {
        let mut sw = Stopwatch::start();
        sw.lap("ttfb");
        let timing = sw.into_timing();
        assert!(timing.ttfb_ms.is_some());
        assert!(timing.dns_ms.is_none());
        assert!(timing.total_ms.is_some());
    }
}
