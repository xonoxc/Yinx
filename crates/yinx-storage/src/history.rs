use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::request::Request;
use yinx_core::response::Response;
use yinx_core::state::{HistoryEntry, TimelineRecord};
use yinx_core::timing::Timing;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Entry not found: {0}")]
    NotFound(String),
    #[error("Invalid entry format at line {0}")]
    InvalidFormat(usize),
}

pub type Result<T> = std::result::Result<T, HistoryError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryLine {
    id: String,
    timestamp: DateTime<Utc>,
    request: Request,
    response: Option<Response>,
    timing: Timing,
    timeline: Option<TimelineRecord>,
}

impl From<&HistoryEntry> for HistoryLine {
    fn from(entry: &HistoryEntry) -> Self {
        HistoryLine {
            id: entry.id.clone(),
            timestamp: entry.timestamp,
            request: entry.request.clone(),
            response: entry.response.clone(),
            timing: entry.timing.clone(),
            timeline: entry.timeline.clone(),
        }
    }
}

impl From<HistoryLine> for HistoryEntry {
    fn from(line: HistoryLine) -> Self {
        HistoryEntry {
            id: line.id,
            request: line.request,
            response: line.response,
            timestamp: line.timestamp,
            timing: line.timing,
            timeline: line.timeline,
        }
    }
}

pub struct HistoryStore {
    path: PathBuf,
    max_entries: Option<usize>,
}

impl HistoryStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            File::create(&path)?;
        }
        Ok(HistoryStore {
            path,
            max_entries: None,
        })
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = Some(max);
        self
    }

    pub fn append(&mut self, entry: &HistoryEntry) -> Result<()> {
        let line: HistoryLine = entry.into();
        let json = serde_json::to_string(&line)?;
        let file = OpenOptions::new().append(true).open(&self.path)?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "{}", json)?;
        writer.flush()?;

        if let Some(max) = self.max_entries {
            let count = self.count()?;
            if count > max {
                self.compact_by_count(max)?;
            }
        }
        Ok(())
    }

    pub fn list(&self, limit: Option<usize>, offset: Option<usize>) -> Result<Vec<HistoryEntry>> {
        let offset = offset.unwrap_or(0);
        let mut entries = Vec::new();
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        for (idx, line) in reader.lines().enumerate() {
            if idx < offset {
                continue;
            }
            if let Some(limit) = limit {
                if entries.len() >= limit {
                    break;
                }
            }
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<HistoryLine>(&line) {
                Ok(hl) => entries.push(hl.into()),
                Err(_) => return Err(HistoryError::InvalidFormat(idx + 1)),
            }
        }
        Ok(entries)
    }

    pub fn count(&self) -> Result<usize> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        Ok(reader.lines().count())
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<HistoryEntry>> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let hl: HistoryLine =
                serde_json::from_str(&line).map_err(|_| HistoryError::InvalidFormat(0))?;
            if hl.id == id {
                return Ok(Some(hl.into()));
            }
        }
        Ok(None)
    }

    pub fn search_by_url(&self, url_pattern: &str) -> Result<Vec<HistoryEntry>> {
        let mut results = Vec::new();
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let hl: HistoryLine =
                serde_json::from_str(&line).map_err(|_| HistoryError::InvalidFormat(0))?;
            if hl.request.url.as_str().contains(url_pattern) {
                results.push(hl.into());
            }
        }
        Ok(results)
    }

    pub fn search_by_status(&self, status: u16) -> Result<Vec<HistoryEntry>> {
        let mut results = Vec::new();
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let hl: HistoryLine =
                serde_json::from_str(&line).map_err(|_| HistoryError::InvalidFormat(0))?;
            if let Some(ref response) = hl.response {
                if response.status.0 == status {
                    results.push(hl.into());
                }
            }
        }
        Ok(results)
    }

    pub fn search_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<HistoryEntry>> {
        let mut results = Vec::new();
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let hl: HistoryLine =
                serde_json::from_str(&line).map_err(|_| HistoryError::InvalidFormat(0))?;
            if hl.timestamp >= start && hl.timestamp <= end {
                results.push(hl.into());
            }
        }
        Ok(results)
    }

    pub fn compact_by_count(&mut self, keep: usize) -> Result<usize> {
        let entries = self.list(None, None)?;
        if entries.len() <= keep {
            return Ok(0);
        }
        let to_remove = entries.len() - keep;
        let retained: Vec<HistoryLine> = entries
            .into_iter()
            .skip(to_remove)
            .map(|e: HistoryEntry| HistoryLine::from(&e))
            .collect();
        self.rewrite_all(&retained)?;
        Ok(to_remove)
    }

    pub fn compact_by_age(&mut self, before: DateTime<Utc>) -> Result<usize> {
        let entries = self.list(None, None)?;
        let retained: Vec<HistoryLine> = entries
            .into_iter()
            .filter(|e| e.timestamp >= before)
            .map(|e: HistoryEntry| HistoryLine::from(&e))
            .collect();
        let removed = self.count()? - retained.len();
        self.rewrite_all(&retained)?;
        Ok(removed)
    }

    fn rewrite_all(&mut self, entries: &[HistoryLine]) -> Result<()> {
        let tmp_path = self.path.with_extension("tmp");
        {
            let file = File::create(&tmp_path)?;
            let mut writer = BufWriter::new(file);
            for entry in entries {
                let json = serde_json::to_string(entry)?;
                writeln!(writer, "{}", json)?;
            }
            writer.flush()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    pub fn replay_request(&self, entry_id: &str) -> Result<Option<Request>> {
        match self.get_by_id(entry_id)? {
            Some(entry) => Ok(Some(entry.request)),
            None => Ok(None),
        }
    }

    pub fn replay_entry(&self, entry_id: &str) -> Result<Option<HistoryEntry>> {
        self.get_by_id(entry_id)
    }

    pub fn replay_timeline(&self, entry_id: &str) -> Result<Option<TimelineRecord>> {
        Ok(self.get_by_id(entry_id)?.and_then(|entry| entry.timeline))
    }

    pub fn clear(&mut self) -> Result<()> {
        File::create(&self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;
    use yinx_core::request::RequestBuilder;
    use yinx_core::response::ResponseBuilder;

    fn create_test_entry() -> HistoryEntry {
        let request = RequestBuilder::new()
            .method(yinx_core::request::Method::Get)
            .url("https://api.example.com/users")
            .build()
            .unwrap();
        let response = ResponseBuilder::new()
            .status(200)
            .body(yinx_core::response::ResponseBody::Text("test".to_string()))
            .timing_ms(100)
            .build();
        HistoryEntry {
            id: "test-id-1".to_string(),
            request,
            response: Some(response),
            timestamp: Utc::now(),
            timing: yinx_core::timing::Timing::new().with_total(100),
            timeline: None,
        }
    }

    fn setup_store() -> (HistoryStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");
        let store = HistoryStore::new(&path).unwrap();
        (store, dir)
    }

    #[test]
    fn test_append_and_list() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        store.append(&entry).unwrap();
        let entries = store.list(None, None).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, entry.id);
    }

    #[test]
    fn test_pagination_limit() {
        let (mut store, _dir) = setup_store();
        for _ in 0..5 {
            store.append(&create_test_entry()).unwrap();
        }
        let entries = store.list(Some(3), None).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_pagination_offset() {
        let (mut store, _dir) = setup_store();
        for _ in 0..5 {
            store.append(&create_test_entry()).unwrap();
        }
        let entries = store.list(None, Some(3)).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_pagination_limit_and_offset() {
        let (mut store, _dir) = setup_store();
        for _ in 0..10 {
            store.append(&create_test_entry()).unwrap();
        }
        let entries = store.list(Some(3), Some(5)).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_search_by_url() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        store.append(&entry).unwrap();
        let results = store.search_by_url("example.com").unwrap();
        assert_eq!(results.len(), 1);
        let results = store.search_by_url("nonexistent").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_by_status() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        store.append(&entry).unwrap();
        let results = store.search_by_status(200).unwrap();
        assert_eq!(results.len(), 1);
        let results = store.search_by_status(404).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_by_date_range() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        store.append(&entry).unwrap();
        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);
        let results = store.search_by_date_range(start, end).unwrap();
        assert_eq!(results.len(), 1);
        let past_start = now - Duration::hours(2);
        let past_end = now - Duration::hours(1);
        let results = store.search_by_date_range(past_start, past_end).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_by_id() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        let id = entry.id.clone();
        store.append(&entry).unwrap();
        let found = store.get_by_id(&id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
        let not_found = store.get_by_id("nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_replay_request() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        let id = entry.id.clone();
        store.append(&entry).unwrap();
        let request = store.replay_request(&id).unwrap();
        assert!(request.is_some());
        assert_eq!(request.unwrap(), entry.request);
    }

    #[test]
    fn test_replay_entry_exact_match() {
        let (mut store, _dir) = setup_store();
        let entry = create_test_entry();
        let id = entry.id.clone();
        store.append(&entry).unwrap();

        let replayed = store.replay_entry(&id).unwrap();
        assert_eq!(replayed, Some(entry));
    }

    #[test]
    fn test_replay_timeline_exact_match() {
        let (mut store, _dir) = setup_store();
        let mut entry = create_test_entry();
        entry.timeline = Some(yinx_core::state::TimelineRecord {
            snapshots: vec![yinx_core::state::TimelineSnapshotRecord {
                kind: yinx_core::state::TimelineSnapshotKind::LastChunk,
                offset: 4,
                timestamp: Utc::now(),
                body: b"test".to_vec(),
            }],
            current_index: Some(0),
        });
        let id = entry.id.clone();
        store.append(&entry).unwrap();

        let replayed = store.replay_timeline(&id).unwrap();
        assert_eq!(replayed, entry.timeline);
    }

    #[test]
    fn test_compact_by_count() {
        let (mut store, _dir) = setup_store();
        for _ in 0..10 {
            store.append(&create_test_entry()).unwrap();
        }
        let removed = store.compact_by_count(5).unwrap();
        assert_eq!(removed, 5);
        let entries = store.list(None, None).unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn test_compact_by_age() {
        let (mut store, _dir) = setup_store();
        let old_time = Utc::now() - Duration::days(2);
        let mut old_entry = create_test_entry();
        old_entry.timestamp = old_time;
        store.append(&old_entry).unwrap();
        let before = Utc::now() - Duration::days(1);
        let removed = store.compact_by_age(before).unwrap();
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_count() {
        let (mut store, _dir) = setup_store();
        assert_eq!(store.count().unwrap(), 0);
        store.append(&create_test_entry()).unwrap();
        store.append(&create_test_entry()).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_clear() {
        let (mut store, _dir) = setup_store();
        store.append(&create_test_entry()).unwrap();
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_atomic_compaction() {
        let (mut store, _dir) = setup_store();
        for _ in 0..10 {
            store.append(&create_test_entry()).unwrap();
        }
        store.compact_by_count(3).unwrap();
        assert!(!store.path.with_extension("tmp").exists());
        let entries = store.list(None, None).unwrap();
        assert_eq!(entries.len(), 3);
    }
}
