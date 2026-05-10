use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::tabs::SerializedTab;

#[derive(Debug, Error)]
pub enum TabStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, TabStoreError>;

pub struct TabStore {
    path: PathBuf,
}

impl TabStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(TabStore { path })
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn save(&self, tabs: &[SerializedTab]) -> Result<()> {
        let json = serde_json::to_string_pretty(tabs)?;
        let tmp_path = self.path.with_extension("tmp");
        {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(json.as_bytes())?;
            tmp.flush()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<SerializedTab>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let tabs: Vec<SerializedTab> = serde_json::from_str(&contents)?;
        Ok(tabs)
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use yinx_core::tabs::Tab;

    fn setup_store() -> (TabStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tabs.json");
        let store = TabStore::new(&path).unwrap();
        (store, dir)
    }

    fn make_tabs() -> Vec<SerializedTab> {
        vec![
            Tab::new("users".to_string()).into(),
            Tab::with_request("auth".to_string(), "req-1".to_string()).into(),
        ]
    }

    #[test]
    fn test_tab_store_save_and_load() {
        let (store, _dir) = setup_store();
        let tabs = make_tabs();
        store.save(&tabs).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "users");
        assert_eq!(loaded[1].title, "auth");
        assert_eq!(loaded[1].request_id, Some("req-1".to_string()));
    }

    #[test]
    fn test_tab_store_load_empty_when_no_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tabs.json");
        let store = TabStore::new(&path).unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_tab_store_exists() {
        let (store, _dir) = setup_store();
        assert!(!store.exists());
        store.save(&make_tabs()).unwrap();
        assert!(store.exists());
    }

    #[test]
    fn test_tab_store_clear() {
        let (store, _dir) = setup_store();
        store.save(&make_tabs()).unwrap();
        assert!(store.exists());
        store.clear().unwrap();
        assert!(!store.exists());
    }

    #[test]
    fn test_tab_store_overwrite() {
        let (store, _dir) = setup_store();
        let tabs1 = vec![Tab::new("first".to_string()).into()];
        store.save(&tabs1).unwrap();
        let tabs2 = vec![Tab::new("second".to_string()).into()];
        store.save(&tabs2).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "second");
    }

    #[test]
    fn test_tab_store_atomic_write() {
        let (store, _dir) = setup_store();
        store.save(&make_tabs()).unwrap();
        assert!(!store.path.with_extension("tmp").exists());
    }

    #[test]
    fn test_tab_store_serde_roundtrip() {
        let (store, _dir) = setup_store();
        let tabs = make_tabs();
        store.save(&tabs).unwrap();
        let loaded = store.load().unwrap();
        let json1 = serde_json::to_string(&tabs).unwrap();
        let json2 = serde_json::to_string(&loaded).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn test_tab_store_creates_dir() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("nested").join("tabs.json");
        let store = TabStore::new(&nested).unwrap();
        assert!(nested.parent().unwrap().exists());
        store.save(&make_tabs()).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn test_tab_store_empty_array() {
        let (store, _dir) = setup_store();
        store.save(&[]).unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.is_empty());
    }
}
