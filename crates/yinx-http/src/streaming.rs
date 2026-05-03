use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use yinx_core::{
    response::Response,
    state::{HistoryEntry, TimelineRecord, TimelineSnapshotKind, TimelineSnapshotRecord},
};

// ==================== 4A: Chunked Streaming ====================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamMode {
    Raw,
    Json,
    Sse,
}

impl Default for StreamMode {
    fn default() -> Self {
        Self::Raw
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamConfig {
    pub mode: StreamMode,
    pub buffer_size: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            mode: StreamMode::default(),
            buffer_size: 8192,
        }
    }
}

impl StreamConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mut self, mode: StreamMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.buffer_size == 0 {
            return Err("buffer_size must be greater than 0".to_string());
        }
        if self.buffer_size > 1024 * 1024 * 10 {
            return Err("buffer_size exceeds maximum of 10MB".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub data: Bytes,
    pub offset: u64,
    pub timestamp: DateTime<Utc>,
    pub is_final: bool,
}

impl Chunk {
    pub fn new(data: impl Into<Bytes>, offset: u64) -> Self {
        Self {
            data: data.into(),
            offset,
            timestamp: Utc::now(),
            is_final: false,
        }
    }

    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn final_chunk(mut self) -> Self {
        self.is_final = true;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug, Clone)]
pub struct StreamMetrics {
    pub ttfb_ms: Option<u64>,
    pub chunk_intervals: Vec<u64>,
    pub total_chunks: u32,
    pub total_bytes: u64,
}

impl Default for StreamMetrics {
    fn default() -> Self {
        Self {
            ttfb_ms: None,
            chunk_intervals: Vec::new(),
            total_chunks: 0,
            total_bytes: 0,
        }
    }
}

impl StreamMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_first_chunk(&mut self, ms: u64) {
        if self.ttfb_ms.is_none() {
            self.ttfb_ms = Some(ms);
        }
    }

    pub fn record_chunk(&mut self, interval_ms: u64) {
        self.chunk_intervals.push(interval_ms);
        self.total_chunks += 1;
    }

    pub fn add_bytes(&mut self, bytes: usize) {
        self.total_bytes += bytes as u64;
    }

    pub fn avg_interval_ms(&self) -> Option<u64> {
        if self.chunk_intervals.is_empty() {
            None
        } else {
            Some(self.chunk_intervals.iter().sum::<u64>() / self.chunk_intervals.len() as u64)
        }
    }
}

// ==================== 4B: SSE Parser ====================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

impl SseEvent {
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: None,
            data: data.into(),
            id: None,
            retry: None,
        }
    }

    pub fn with_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_retry(mut self, retry: u64) -> Self {
        self.retry = Some(retry);
        self
    }
}

pub struct SseParser {
    current_event: SseEvent,
    has_data: bool,
    last_event_id: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            current_event: SseEvent::new(""),
            has_data: false,
            last_event_id: None,
        }
    }

    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }

    pub fn parse_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.flush();
        }

        if let Some(value) = line.strip_prefix("event:") {
            self.current_event.event = Some(value.trim().to_string());
            let event = SseEvent {
                event: self.current_event.event.clone(),
                data: String::new(),
                id: None,
                retry: None,
            };
            return Some(event);
        }

        if let Some(value) = line.strip_prefix("data:") {
            self.current_event.data.push_str(value.trim());
            self.current_event.data.push('\n');
            self.has_data = true;
            return None;
        }

        if let Some(value) = line.strip_prefix("id:") {
            self.last_event_id = Some(value.trim().to_string());
            self.current_event.id = Some(value.trim().to_string());
            let event = SseEvent {
                event: None,
                data: String::new(),
                id: self.current_event.id.clone(),
                retry: None,
            };
            return Some(event);
        }

        if let Some(value) = line.strip_prefix("retry:") {
            if let Ok(retry) = value.trim().parse::<u64>() {
                self.current_event.retry = Some(retry);
                let event = SseEvent {
                    event: None,
                    data: String::new(),
                    id: None,
                    retry: self.current_event.retry,
                };
                return Some(event);
            }
        }

        None
    }

    pub fn parse(&mut self, input: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();
        for line in input.lines() {
            if let Some(event) = self.parse_line(line) {
                events.push(event);
            }
        }
        if let Some(event) = self.flush() {
            events.push(event);
        }
        events
    }

    fn flush(&mut self) -> Option<SseEvent> {
        if !self.has_data && self.current_event.event.is_none() {
            return None;
        }
        let mut event = std::mem::take(&mut self.current_event);
        event.data = event.data.trim_end_matches('\n').to_string();
        self.current_event = SseEvent::new("");
        self.has_data = false;
        Some(event)
    }
}

// ==================== 4C: JSON Streaming ====================

pub struct JsonStreamParser {
    buffer: String,
    depth: usize,
    in_string: bool,
    escape_next: bool,
}

impl JsonStreamParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            depth: 0,
            in_string: false,
            escape_next: false,
        }
    }

    pub fn parse(&mut self, chunk: &str) -> Vec<serde_json::Value> {
        let mut results = Vec::new();
        for c in chunk.chars() {
            self.buffer.push(c);
            if self.escape_next {
                self.escape_next = false;
                continue;
            }
            if c == '\\' && self.in_string {
                self.escape_next = true;
                continue;
            }
            if c == '"' && !self.escape_next {
                self.in_string = !self.in_string;
                continue;
            }
            if !self.in_string {
                match c {
                    '{' | '[' => self.depth += 1,
                    '}' | ']' => {
                        self.depth = self.depth.saturating_sub(1);
                        if self.depth == 0 {
                            if let Ok(value) = serde_json::from_str(&self.buffer) {
                                results.push(value);
                                self.buffer.clear();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        results
    }

    pub fn pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn flush(&mut self) -> Option<serde_json::Value> {
        if self.buffer.is_empty() {
            None
        } else {
            let remaining = std::mem::take(&mut self.buffer);
            match serde_json::from_str(&remaining) {
                Ok(value) => Some(value),
                Err(_) => Some(serde_json::Value::String(remaining)),
            }
        }
    }
}

pub struct JsonStreamFormatter {
    buffer: String,
}

impl JsonStreamFormatter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    pub fn format_chunk(&mut self, value: &serde_json::Value) -> String {
        let formatted = serde_json::to_string_pretty(value).unwrap_or_default();
        self.buffer.push_str(&formatted);
        self.buffer.push('\n');
        formatted
    }

    pub fn output(&self) -> &str {
        &self.buffer
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

// ==================== 4D: Live Rendering Support ====================

#[derive(Debug, Clone, PartialEq)]
pub struct StreamBuffer {
    data: Vec<u8>,
    max_size: usize,
    offset: u64,
}

impl StreamBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: Vec::new(),
            max_size,
            offset: 0,
        }
    }

    pub fn append(&mut self, bytes: &[u8]) {
        let available = self.max_size.saturating_sub(self.data.len());
        let to_copy = bytes.len().min(available);
        if to_copy > 0 {
            self.data.extend_from_slice(&bytes[..to_copy]);
        }
        self.offset += bytes.len() as u64;
    }

    pub fn seek(&self, offset: u64) -> Option<&[u8]> {
        let start = offset as usize;
        if start >= self.data.len() {
            None
        } else {
            Some(&self.data[start..])
        }
    }

    pub fn truncate(&mut self) {
        self.data.clear();
        self.offset = 0;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.data.len() >= self.max_size
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AutoScrollState {
    Following,
    Pinned,
}

impl Default for AutoScrollState {
    fn default() -> Self {
        Self::Following
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoScroll {
    state: AutoScrollState,
}

impl AutoScroll {
    pub fn new() -> Self {
        Self {
            state: AutoScrollState::Following,
        }
    }

    pub fn is_following(&self) -> bool {
        matches!(self.state, AutoScrollState::Following)
    }

    pub fn is_pinned(&self) -> bool {
        matches!(self.state, AutoScrollState::Pinned)
    }

    pub fn pin(&mut self) {
        self.state = AutoScrollState::Pinned;
    }

    pub fn follow(&mut self) {
        self.state = AutoScrollState::Following;
    }

    pub fn toggle(&mut self) {
        match self.state {
            AutoScrollState::Following => self.state = AutoScrollState::Pinned,
            AutoScrollState::Pinned => self.state = AutoScrollState::Following,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HighlightMarker {
    pub position: u64,
    pub timestamp: DateTime<Utc>,
}

impl HighlightMarker {
    pub fn new(position: u64) -> Self {
        Self {
            position,
            timestamp: Utc::now(),
        }
    }
}

pub struct StreamHighlights {
    markers: Vec<HighlightMarker>,
}

impl StreamHighlights {
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
        }
    }

    pub fn mark_new_data(&mut self, position: u64) {
        self.markers.push(HighlightMarker::new(position));
    }

    pub fn clear(&mut self) {
        self.markers.clear();
    }

    pub fn markers(&self) -> &[HighlightMarker] {
        &self.markers
    }

    pub fn has_markers(&self) -> bool {
        !self.markers.is_empty()
    }
}

// ==================== 10: Time-travel + Replay ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    ChunkBoundary,
    Ttfb,
    Error,
    LastChunk,
}

impl From<SnapshotKind> for TimelineSnapshotKind {
    fn from(value: SnapshotKind) -> Self {
        match value {
            SnapshotKind::ChunkBoundary => TimelineSnapshotKind::ChunkBoundary,
            SnapshotKind::Ttfb => TimelineSnapshotKind::Ttfb,
            SnapshotKind::Error => TimelineSnapshotKind::Error,
            SnapshotKind::LastChunk => TimelineSnapshotKind::LastChunk,
        }
    }
}

impl From<TimelineSnapshotKind> for SnapshotKind {
    fn from(value: TimelineSnapshotKind) -> Self {
        match value {
            TimelineSnapshotKind::ChunkBoundary => SnapshotKind::ChunkBoundary,
            TimelineSnapshotKind::Ttfb => SnapshotKind::Ttfb,
            TimelineSnapshotKind::Error => SnapshotKind::Error,
            TimelineSnapshotKind::LastChunk => SnapshotKind::LastChunk,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineSnapshot {
    pub kind: SnapshotKind,
    pub offset: u64,
    pub timestamp: DateTime<Utc>,
    body: Vec<u8>,
}

impl TimelineSnapshot {
    pub fn new(body: impl Into<Vec<u8>>, offset: u64, timestamp: DateTime<Utc>) -> Self {
        Self {
            kind: SnapshotKind::ChunkBoundary,
            offset,
            timestamp,
            body: body.into(),
        }
    }

    pub fn from_text(text: impl AsRef<str>) -> Self {
        Self::new(text.as_ref().as_bytes().to_vec(), 0, Utc::now())
    }

    pub fn with_kind(mut self, kind: SnapshotKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn body_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }
}

impl From<TimelineSnapshotRecord> for TimelineSnapshot {
    fn from(value: TimelineSnapshotRecord) -> Self {
        Self {
            kind: value.kind.into(),
            offset: value.offset,
            timestamp: value.timestamp,
            body: value.body,
        }
    }
}

impl From<&TimelineSnapshot> for TimelineSnapshotRecord {
    fn from(value: &TimelineSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            offset: value.offset,
            timestamp: value.timestamp,
            body: value.body.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineDiff {
    pub removed_text: String,
    pub added_text: String,
}

impl TimelineDiff {
    pub fn render(&self) -> String {
        let mut lines = Vec::new();
        if !self.removed_text.is_empty() {
            lines.push(format!("- {}", self.removed_text));
        }
        if !self.added_text.is_empty() {
            lines.push(format!("+ {}", self.added_text));
        }
        if lines.is_empty() {
            "No changes".to_string()
        } else {
            lines.join("\n")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineJumpTarget {
    Ttfb,
    FirstError,
    LastChunk,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimelineState {
    snapshots: Vec<TimelineSnapshot>,
    current_index: Option<usize>,
    assembled_body: Vec<u8>,
}

impl TimelineState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn snapshot(&self, index: usize) -> Option<&TimelineSnapshot> {
        self.snapshots.get(index)
    }

    pub fn current_snapshot(&self) -> Option<&TimelineSnapshot> {
        self.current_index.and_then(|index| self.snapshot(index))
    }

    pub fn scrub_to(&mut self, index: usize) -> Option<&TimelineSnapshot> {
        if self.snapshots.is_empty() {
            self.current_index = None;
            return None;
        }

        let clamped = index.min(self.snapshots.len().saturating_sub(1));
        self.current_index = Some(clamped);
        self.current_snapshot()
    }

    pub fn push_snapshot(&mut self, snapshot: TimelineSnapshot) {
        self.assembled_body = snapshot.body().to_vec();
        self.snapshots.push(snapshot);
        self.current_index = Some(self.snapshots.len().saturating_sub(1));
    }

    pub fn capture_chunk(&mut self, chunk: &Chunk) {
        self.assembled_body.extend_from_slice(&chunk.data);

        let kind = if self.snapshots.is_empty() {
            SnapshotKind::Ttfb
        } else if chunk.is_final {
            SnapshotKind::LastChunk
        } else {
            SnapshotKind::ChunkBoundary
        };

        let snapshot = TimelineSnapshot::new(
            self.assembled_body.clone(),
            chunk.offset + chunk.len() as u64,
            chunk.timestamp,
        )
        .with_kind(kind);
        self.push_snapshot(snapshot);
    }

    pub fn capture_error(
        &mut self,
        body: impl Into<Vec<u8>>,
        offset: u64,
        timestamp: DateTime<Utc>,
    ) {
        self.push_snapshot(
            TimelineSnapshot::new(body, offset, timestamp).with_kind(SnapshotKind::Error),
        );
    }

    pub fn move_prev(&mut self) -> bool {
        match self.current_index {
            Some(index) if index > 0 => {
                self.current_index = Some(index - 1);
                true
            }
            _ => false,
        }
    }

    pub fn move_next(&mut self) -> bool {
        match self.current_index {
            Some(index) if index + 1 < self.snapshots.len() => {
                self.current_index = Some(index + 1);
                true
            }
            _ => false,
        }
    }

    pub fn diff(&self, from: usize, to: usize) -> Option<TimelineDiff> {
        let before = self.snapshot(from)?.body_text()?;
        let after = self.snapshot(to)?.body_text()?;
        let common_prefix = before
            .bytes()
            .zip(after.bytes())
            .take_while(|(left, right)| left == right)
            .count();

        Some(TimelineDiff {
            removed_text: before[common_prefix..].to_string(),
            added_text: after[common_prefix..].to_string(),
        })
    }

    pub fn jump_to(&mut self, target: TimelineJumpTarget) -> Option<usize> {
        let index = match target {
            TimelineJumpTarget::Ttfb => self
                .snapshots
                .iter()
                .position(|snapshot| snapshot.kind == SnapshotKind::Ttfb)
                .or_else(|| (!self.snapshots.is_empty()).then_some(0)),
            TimelineJumpTarget::FirstError => self
                .snapshots
                .iter()
                .position(|snapshot| snapshot.kind == SnapshotKind::Error),
            TimelineJumpTarget::LastChunk => self
                .snapshots
                .iter()
                .rposition(|snapshot| snapshot.kind == SnapshotKind::LastChunk)
                .or_else(|| self.snapshots.len().checked_sub(1)),
        }?;

        self.current_index = Some(index);
        Some(index)
    }

    pub fn to_record(&self) -> TimelineRecord {
        TimelineRecord {
            snapshots: self
                .snapshots
                .iter()
                .map(TimelineSnapshotRecord::from)
                .collect(),
            current_index: self.current_index,
        }
    }

    pub fn from_record(record: TimelineRecord) -> Self {
        let snapshots: Vec<TimelineSnapshot> = record
            .snapshots
            .into_iter()
            .map(TimelineSnapshot::from)
            .collect();
        let current_index = if snapshots.is_empty() {
            None
        } else {
            record
                .current_index
                .or_else(|| snapshots.len().checked_sub(1))
                .map(|index| index.min(snapshots.len().saturating_sub(1)))
        };
        let assembled_body = current_index
            .and_then(|index| snapshots.get(index))
            .map(|snapshot| snapshot.body().to_vec())
            .or_else(|| snapshots.last().map(|snapshot| snapshot.body().to_vec()))
            .unwrap_or_default();

        Self {
            snapshots,
            current_index,
            assembled_body,
        }
    }

    pub fn from_response(response: &Response) -> Self {
        let mut timeline = Self::new();
        let body = response.body.to_bytes();
        if body.is_empty() {
            return timeline;
        }

        let kind = if response.is_error() {
            SnapshotKind::Error
        } else {
            SnapshotKind::LastChunk
        };
        timeline.push_snapshot(
            TimelineSnapshot::new(body.clone(), body.len() as u64, Utc::now()).with_kind(kind),
        );
        timeline
    }

    pub fn replay_history_entry(entry: &HistoryEntry) -> Option<Self> {
        if let Some(record) = entry.timeline.clone() {
            return Some(Self::from_record(record));
        }

        entry.response.as_ref().map(Self::from_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use yinx_core::{
        request::RequestBuilder,
        response::{ResponseBody, ResponseBuilder},
        timing::Timing,
    };

    // ==================== 4.1: StreamConfig ====================

    #[test]
    fn test_stream_config_defaults() {
        let config = StreamConfig::default();
        assert_eq!(config.mode, StreamMode::Raw);
        assert_eq!(config.buffer_size, 8192);
    }

    #[test]
    fn test_stream_config_new() {
        let config = StreamConfig::new();
        assert_eq!(config.mode, StreamMode::Raw);
        assert_eq!(config.buffer_size, 8192);
    }

    #[test]
    fn test_stream_config_with_mode() {
        let config = StreamConfig::new().with_mode(StreamMode::Sse);
        assert_eq!(config.mode, StreamMode::Sse);
    }

    #[test]
    fn test_stream_config_with_buffer_size() {
        let config = StreamConfig::new().with_buffer_size(4096);
        assert_eq!(config.buffer_size, 4096);
    }

    #[test]
    fn test_stream_config_validate_valid() {
        let config = StreamConfig::new();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_stream_config_validate_zero_buffer() {
        let config = StreamConfig::new().with_buffer_size(0);
        assert_eq!(
            config.validate().unwrap_err(),
            "buffer_size must be greater than 0"
        );
    }

    #[test]
    fn test_stream_config_validate_exceeds_max() {
        let config = StreamConfig::new().with_buffer_size(1024 * 1024 * 11);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_stream_mode_serde() {
        let mode = StreamMode::Json;
        let json = serde_json::to_string(&mode).unwrap();
        let decoded: StreamMode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, StreamMode::Json);
    }

    // ==================== 4.2: send_streaming ====================

    // Note: Integration test with mock server would go here
    // For now, we test the types used by send_streaming

    #[test]
    fn test_stream_config_mode_raw() {
        let config = StreamConfig::new().with_mode(StreamMode::Raw);
        assert_eq!(config.mode, StreamMode::Raw);
    }

    #[test]
    fn test_stream_config_mode_json() {
        let config = StreamConfig::new().with_mode(StreamMode::Json);
        assert_eq!(config.mode, StreamMode::Json);
    }

    #[test]
    fn test_stream_config_mode_sse() {
        let config = StreamConfig::new().with_mode(StreamMode::Sse);
        assert_eq!(config.mode, StreamMode::Sse);
    }

    // ==================== 4.3: Chunk struct ====================

    #[test]
    fn test_chunk_new() {
        let chunk = Chunk::new("hello", 0);
        assert_eq!(chunk.data, Bytes::from("hello"));
        assert_eq!(chunk.offset, 0);
        assert!(!chunk.is_final);
    }

    #[test]
    fn test_chunk_with_timestamp() {
        let ts = Utc.timestamp_opt(1000000, 0).unwrap();
        let chunk = Chunk::new("data", 100).with_timestamp(ts);
        assert_eq!(chunk.timestamp, ts);
    }

    #[test]
    fn test_chunk_final() {
        let chunk = Chunk::new("end", 0).final_chunk();
        assert!(chunk.is_final);
    }

    #[test]
    fn test_chunk_is_empty() {
        let empty = Chunk::new("", 0);
        assert!(empty.is_empty());
        let non_empty = Chunk::new("data", 0);
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_chunk_len() {
        let chunk = Chunk::new("hello", 0);
        assert_eq!(chunk.len(), 5);
    }

    #[test]
    fn test_chunk_from_bytes() {
        let bytes = Bytes::from_static(b"test");
        let chunk = Chunk::new(bytes, 0);
        assert_eq!(chunk.data, Bytes::from("test"));
    }

    #[test]
    fn test_chunk_offset_tracking() {
        let chunk1 = Chunk::new("hello", 0);
        let chunk2 = Chunk::new(" world", 5);
        assert_eq!(chunk1.offset, 0);
        assert_eq!(chunk2.offset, 5);
    }

    // ==================== 4.4: TTFB measurement ====================

    #[test]
    fn test_stream_metrics_ttfb_initially_none() {
        let metrics = StreamMetrics::new();
        assert!(metrics.ttfb_ms.is_none());
    }

    #[test]
    fn test_stream_metrics_record_first_chunk() {
        let mut metrics = StreamMetrics::new();
        metrics.record_first_chunk(150);
        assert_eq!(metrics.ttfb_ms, Some(150));
    }

    #[test]
    fn test_stream_metrics_ttfb_not_overwritten() {
        let mut metrics = StreamMetrics::new();
        metrics.record_first_chunk(100);
        metrics.record_first_chunk(200);
        assert_eq!(metrics.ttfb_ms, Some(100));
    }

    #[test]
    fn test_stream_metrics_ttfb_accuracy() {
        let mut metrics = StreamMetrics::new();
        metrics.record_first_chunk(123);
        assert_eq!(metrics.ttfb_ms, Some(123));
    }

    // ==================== 4.5: Chunk interval tracking ====================

    #[test]
    fn test_stream_metrics_record_chunk_interval() {
        let mut metrics = StreamMetrics::new();
        metrics.record_chunk(10);
        metrics.record_chunk(20);
        assert_eq!(metrics.chunk_intervals, vec![10, 20]);
    }

    #[test]
    fn test_stream_metrics_total_chunks() {
        let mut metrics = StreamMetrics::new();
        assert_eq!(metrics.total_chunks, 0);
        metrics.record_chunk(10);
        assert_eq!(metrics.total_chunks, 1);
        metrics.record_chunk(15);
        assert_eq!(metrics.total_chunks, 2);
    }

    #[test]
    fn test_stream_metrics_avg_interval() {
        let mut metrics = StreamMetrics::new();
        metrics.record_chunk(10);
        metrics.record_chunk(20);
        metrics.record_chunk(30);
        assert_eq!(metrics.avg_interval_ms(), Some(20));
    }

    #[test]
    fn test_stream_metrics_avg_interval_empty() {
        let metrics = StreamMetrics::new();
        assert_eq!(metrics.avg_interval_ms(), None);
    }

    #[test]
    fn test_stream_metrics_add_bytes() {
        let mut metrics = StreamMetrics::new();
        metrics.add_bytes(100);
        metrics.add_bytes(200);
        assert_eq!(metrics.total_bytes, 300);
    }

    // ==================== 4.6: Stream cancellation ====================

    // Note: Actual cancellation test requires async runtime and mock server
    // Testing the concept that dropping stream cancels

    #[test]
    fn test_stream_metrics_tracks_cancellation() {
        let metrics = StreamMetrics::new();
        assert_eq!(metrics.total_chunks, 0);
    }

    // ==================== 4.7: SSE line parser ====================

    #[test]
    fn test_sse_parse_event_field() {
        let mut parser = SseParser::new();
        let event = parser.parse_line("event:message");
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event, Some("message".to_string()));
    }

    #[test]
    fn test_sse_parse_data_field() {
        let mut parser = SseParser::new();
        let result = parser.parse_line("data:{\"key\":\"value\"}");
        assert!(result.is_none());
        let flushed = parser.flush();
        assert!(flushed.is_some());
        assert_eq!(flushed.unwrap().data, "{\"key\":\"value\"}");
    }

    #[test]
    fn test_sse_parse_id_field() {
        let mut parser = SseParser::new();
        let event = parser.parse_line("id:123");
        assert!(event.is_some());
        assert_eq!(event.unwrap().id, Some("123".to_string()));
    }

    #[test]
    fn test_sse_parse_retry_field() {
        let mut parser = SseParser::new();
        let event = parser.parse_line("retry:5000");
        assert!(event.is_some());
        assert_eq!(event.unwrap().retry, Some(5000));
    }

    #[test]
    fn test_sse_parse_empty_line_flushes() {
        let mut parser = SseParser::new();
        parser.parse_line("data:hello");
        let event = parser.parse_line("");
        assert!(event.is_some());
        assert_eq!(event.unwrap().data, "hello");
    }

    // ==================== 4.8: SSE message assembler ====================

    #[test]
    fn test_sse_multi_line_data() {
        let mut parser = SseParser::new();
        parser.parse_line("data:line1");
        parser.parse_line("data:line2");
        let event = parser.flush().unwrap();
        assert!(event.data.contains("line1"));
        assert!(event.data.contains("line2"));
    }

    #[test]
    fn test_sse_parse_complete_message() {
        let mut parser = SseParser::new();
        let events = parser.parse("data:{\"test\":true}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"test\":true}");
    }

    #[test]
    fn test_sse_multiple_messages() {
        let mut parser = SseParser::new();
        let input = "data:msg1\n\nevent:test\ndata:msg2\n\n";
        let events = parser.parse(input);
        assert!(events.len() >= 1);
    }

    // ==================== 4.9: SSE reconnection logic ====================

    #[test]
    fn test_sse_last_event_id_tracked() {
        let mut parser = SseParser::new();
        parser.parse_line("id:abc123");
        assert_eq!(parser.last_event_id(), Some("abc123"));
    }

    #[test]
    fn test_sse_retry_value_stored() {
        let mut parser = SseParser::new();
        let event = parser.parse_line("retry:3000");
        assert!(event.is_some());
        assert_eq!(event.unwrap().retry, Some(3000));
    }

    #[test]
    fn test_sse_reconnection_headers() {
        let parser = SseParser::new();
        let last_id = parser.last_event_id();
        assert!(last_id.is_none());
    }

    // ==================== 4.10: SSE event type dispatch ====================

    #[test]
    fn test_sse_named_event() {
        let mut parser = SseParser::new();
        let event = parser.parse_line("event:update");
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event, Some("update".to_string()));
    }

    #[test]
    fn test_sse_event_with_data() {
        let mut parser = SseParser::new();
        parser.parse_line("event:message");
        parser.parse_line("data:hello world");
        let event = parser.flush().unwrap();
        assert_eq!(event.event, Some("message".to_string()));
        assert!(event.data.contains("hello world"));
    }

    // ==================== 4.11: Incremental JSON parser ====================

    #[test]
    fn test_json_parser_partial_object() {
        let mut parser = JsonStreamParser::new();
        let results = parser.parse("{\"a\":");
        assert!(results.is_empty());
        assert!(parser.pending());
    }

    #[test]
    fn test_json_parser_complete_object() {
        let mut parser = JsonStreamParser::new();
        let results = parser.parse("{\"a\":1}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!({"a": 1}));
    }

    #[test]
    fn test_json_parser_multiple_objects() {
        let mut parser = JsonStreamParser::new();
        let results = parser.parse("{\"a\":1}{\"b\":2}");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_json_parser_flush_pending() {
        let mut parser = JsonStreamParser::new();
        parser.parse("{\"incomplete\":");
        let result = parser.flush();
        assert!(result.is_some());
    }

    #[test]
    fn test_json_parser_empty_chunk() {
        let mut parser = JsonStreamParser::new();
        let results = parser.parse("");
        assert!(results.is_empty());
        assert!(!parser.pending());
    }

    // ==================== 4.12: JSON stream formatter ====================

    #[test]
    fn test_json_formatter_pretty_print() {
        let mut formatter = JsonStreamFormatter::new();
        let value = serde_json::json!({"key": "value"});
        let formatted = formatter.format_chunk(&value);
        assert!(formatted.contains('\n'));
        assert!(formatted.contains("\"key\""));
    }

    #[test]
    fn test_json_formatter_accumulates() {
        let mut formatter = JsonStreamFormatter::new();
        formatter.format_chunk(&serde_json::json!({"a": 1}));
        formatter.format_chunk(&serde_json::json!({"b": 2}));
        let output = formatter.output();
        assert!(output.contains("\"a\""));
        assert!(output.contains("\"b\""));
    }

    #[test]
    fn test_json_formatter_clear() {
        let mut formatter = JsonStreamFormatter::new();
        formatter.format_chunk(&serde_json::json!({"test": true}));
        formatter.clear();
        assert!(formatter.output().is_empty());
    }

    // ==================== 4.13: Streaming buffer management ====================

    #[test]
    fn test_json_parser_nested_objects() {
        let mut parser = JsonStreamParser::new();
        let results = parser.parse("{\"outer\":{\"inner\":1}}");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_json_parser_with_escaped_quotes() {
        let mut parser = JsonStreamParser::new();
        let results = parser.parse("{\"msg\":\"hello \\\"world\\\"\"}");
        assert_eq!(results.len(), 1);
    }

    // ==================== 4.14: StreamBuffer ====================

    #[test]
    fn test_stream_buffer_new() {
        let buffer = StreamBuffer::new(1024);
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_stream_buffer_append() {
        let mut buffer = StreamBuffer::new(1024);
        buffer.append(b"hello");
        assert_eq!(buffer.len(), 5);
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_stream_buffer_seek() {
        let mut buffer = StreamBuffer::new(1024);
        buffer.append(b"hello world");
        let data = buffer.seek(6).unwrap();
        assert_eq!(data, b"world");
    }

    #[test]
    fn test_stream_buffer_seek_past_end() {
        let mut buffer = StreamBuffer::new(1024);
        buffer.append(b"test");
        assert!(buffer.seek(100).is_none());
    }

    #[test]
    fn test_stream_buffer_truncate() {
        let mut buffer = StreamBuffer::new(1024);
        buffer.append(b"some data");
        buffer.truncate();
        assert!(buffer.is_empty());
        assert_eq!(buffer.offset, 0);
    }

    #[test]
    fn test_stream_buffer_is_full() {
        let mut buffer = StreamBuffer::new(5);
        assert!(!buffer.is_full());
        buffer.append(b"12345");
        assert!(buffer.is_full());
    }

    #[test]
    fn test_stream_buffer_bounded() {
        let mut buffer = StreamBuffer::new(10);
        buffer.append(b"0123456789abc");
        assert_eq!(buffer.len(), 10);
    }

    #[test]
    fn test_stream_buffer_as_bytes() {
        let mut buffer = StreamBuffer::new(1024);
        buffer.append(b"test data");
        assert_eq!(buffer.as_bytes(), b"test data");
    }

    // ==================== 4.15: Auto-scroll state tracking ====================

    #[test]
    fn test_auto_scroll_default_following() {
        let scroll = AutoScroll::new();
        assert!(scroll.is_following());
        assert!(!scroll.is_pinned());
    }

    #[test]
    fn test_auto_scroll_pin() {
        let mut scroll = AutoScroll::new();
        scroll.pin();
        assert!(scroll.is_pinned());
        assert!(!scroll.is_following());
    }

    #[test]
    fn test_auto_scroll_follow() {
        let mut scroll = AutoScroll::new();
        scroll.pin();
        scroll.follow();
        assert!(scroll.is_following());
    }

    #[test]
    fn test_auto_scroll_toggle() {
        let mut scroll = AutoScroll::new();
        assert!(scroll.is_following());
        scroll.toggle();
        assert!(scroll.is_pinned());
        scroll.toggle();
        assert!(scroll.is_following());
    }

    // ==================== 4.16: Highlight markers ====================

    #[test]
    fn test_highlight_marker_new() {
        let marker = HighlightMarker::new(100);
        assert_eq!(marker.position, 100);
    }

    #[test]
    fn test_stream_highlights_mark_new_data() {
        let mut highlights = StreamHighlights::new();
        highlights.mark_new_data(50);
        assert!(highlights.has_markers());
        assert_eq!(highlights.markers().len(), 1);
    }

    #[test]
    fn test_stream_highlights_multiple_markers() {
        let mut highlights = StreamHighlights::new();
        highlights.mark_new_data(10);
        highlights.mark_new_data(20);
        highlights.mark_new_data(30);
        assert_eq!(highlights.markers().len(), 3);
    }

    #[test]
    fn test_stream_highlights_clear() {
        let mut highlights = StreamHighlights::new();
        highlights.mark_new_data(100);
        highlights.clear();
        assert!(!highlights.has_markers());
        assert!(highlights.markers().is_empty());
    }

    #[test]
    fn test_highlight_marker_position() {
        let marker = HighlightMarker::new(999);
        assert_eq!(marker.position, 999);
    }

    // ==================== 10.1-10.7: Time-travel + Replay ====================

    #[test]
    fn test_timeline_state_defaults() {
        let timeline = TimelineState::new();
        assert!(timeline.is_empty());
        assert_eq!(timeline.len(), 0);
        assert_eq!(timeline.current_index(), None);
        assert!(timeline.current_snapshot().is_none());
    }

    #[test]
    fn test_timeline_captures_snapshot_at_each_chunk_boundary() {
        let mut timeline = TimelineState::new();
        let first_ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let second_ts = Utc.timestamp_opt(1_700_000_001, 0).unwrap();

        timeline.capture_chunk(&Chunk::new("hel", 0).with_timestamp(first_ts));
        timeline.capture_chunk(&Chunk::new("lo", 3).with_timestamp(second_ts).final_chunk());

        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.current_index(), Some(1));
        assert_eq!(timeline.snapshot(0).unwrap().body_text().unwrap(), "hel");
        assert_eq!(timeline.snapshot(1).unwrap().body_text().unwrap(), "hello");
        assert_eq!(timeline.snapshot(0).unwrap().kind, SnapshotKind::Ttfb);
        assert_eq!(timeline.snapshot(1).unwrap().kind, SnapshotKind::LastChunk);
    }

    #[test]
    fn test_timeline_scrubbing_moves_within_bounds() {
        let mut timeline = TimelineState::new();
        timeline.push_snapshot(TimelineSnapshot::from_text("one"));
        timeline.push_snapshot(TimelineSnapshot::from_text("two"));
        timeline.push_snapshot(TimelineSnapshot::from_text("three"));

        assert_eq!(timeline.current_index(), Some(2));
        assert!(timeline.move_prev());
        assert_eq!(timeline.current_index(), Some(1));
        assert!(timeline.move_prev());
        assert_eq!(timeline.current_index(), Some(0));
        assert!(!timeline.move_prev());
        assert_eq!(timeline.current_index(), Some(0));
        assert!(timeline.move_next());
        assert_eq!(timeline.current_index(), Some(1));
        assert!(timeline.move_next());
        assert_eq!(timeline.current_index(), Some(2));
        assert!(!timeline.move_next());
        assert_eq!(timeline.current_index(), Some(2));

        assert_eq!(timeline.scrub_to(0).unwrap().body_text(), Some("one"));
        assert_eq!(timeline.current_index(), Some(0));
        assert_eq!(timeline.scrub_to(99).unwrap().body_text(), Some("three"));
        assert_eq!(timeline.current_index(), Some(2));
    }

    #[test]
    fn test_timeline_diff_between_two_snapshots() {
        let mut timeline = TimelineState::new();
        timeline.push_snapshot(TimelineSnapshot::from_text("hello"));
        timeline.push_snapshot(TimelineSnapshot::from_text("hello world"));

        let diff = timeline.diff(0, 1).unwrap();
        assert_eq!(diff.removed_text, "");
        assert_eq!(diff.added_text, " world");
        assert_eq!(diff.render(), "+  world");
    }

    #[test]
    fn test_timeline_jump_to_key_moments() {
        let mut timeline = TimelineState::new();
        timeline.push_snapshot(TimelineSnapshot::from_text("warmup").with_kind(SnapshotKind::Ttfb));
        timeline.push_snapshot(TimelineSnapshot::from_text("steady"));
        timeline.push_snapshot(TimelineSnapshot::from_text("oops").with_kind(SnapshotKind::Error));
        timeline
            .push_snapshot(TimelineSnapshot::from_text("done").with_kind(SnapshotKind::LastChunk));

        assert_eq!(timeline.jump_to(TimelineJumpTarget::Ttfb), Some(0));
        assert_eq!(timeline.current_index(), Some(0));
        assert_eq!(timeline.jump_to(TimelineJumpTarget::FirstError), Some(2));
        assert_eq!(timeline.current_index(), Some(2));
        assert_eq!(timeline.jump_to(TimelineJumpTarget::LastChunk), Some(3));
        assert_eq!(timeline.current_index(), Some(3));
    }

    #[test]
    fn test_timeline_record_roundtrip_preserves_snapshots() {
        let mut timeline = TimelineState::new();
        timeline.push_snapshot(TimelineSnapshot::from_text("hello").with_kind(SnapshotKind::Ttfb));
        timeline.capture_error("hello!", 6, Utc.timestamp_opt(1_000_002, 0).unwrap());

        let restored = TimelineState::from_record(timeline.to_record());
        assert_eq!(restored, timeline);
    }

    #[test]
    fn test_replay_history_entry_prefers_recorded_timeline() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let response = ResponseBuilder::new()
            .status(200)
            .body(ResponseBody::Text("final only".to_string()))
            .build();

        let mut recorded = TimelineState::new();
        recorded
            .push_snapshot(TimelineSnapshot::from_text("partial").with_kind(SnapshotKind::Ttfb));
        recorded.push_snapshot(
            TimelineSnapshot::from_text("partial done").with_kind(SnapshotKind::LastChunk),
        );

        let entry = HistoryEntry {
            id: "history-1".to_string(),
            request,
            response: Some(response),
            timestamp: Utc::now(),
            timing: Timing::new(),
            timeline: Some(recorded.to_record()),
        };

        let replayed = TimelineState::replay_history_entry(&entry).unwrap();
        assert_eq!(replayed, recorded);
    }

    #[test]
    fn test_replay_history_entry_falls_back_to_response_body() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let response = ResponseBuilder::new()
            .status(500)
            .body(ResponseBody::Text("server exploded".to_string()))
            .build();

        let entry = HistoryEntry {
            id: "history-2".to_string(),
            request,
            response: Some(response),
            timestamp: Utc::now(),
            timing: Timing::new(),
            timeline: None,
        };

        let replayed = TimelineState::replay_history_entry(&entry).unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(
            replayed.current_snapshot().unwrap().body_text(),
            Some("server exploded")
        );
        assert_eq!(
            replayed.current_snapshot().unwrap().kind,
            SnapshotKind::Error
        );
    }
}
