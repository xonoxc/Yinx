use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use yinx_core::collections::{Collection, CollectionSummary};

#[derive(Debug, Error)]
pub enum CollectionStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Collection not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CollectionStoreError>;

pub trait CollectionStore: Send + Sync {
    fn save(&self, collection: &Collection) -> Result<()>;
    fn get(&self, id: &str) -> Result<Option<Collection>>;
    fn delete(&self, id: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<CollectionSummary>>;
    fn search(&self, query: &str) -> Result<Vec<CollectionSummary>>;
    fn path(&self) -> &Path;
}

pub struct JsonCollectionStore {
    dir: PathBuf,
}

impl JsonCollectionStore {
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        Ok(JsonCollectionStore { dir })
    }

    fn file_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }

    fn scan_collections(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }
}

impl CollectionStore for JsonCollectionStore {
    fn save(&self, collection: &Collection) -> Result<()> {
        let path = self.file_path(&collection.id);
        let json = serde_json::to_string_pretty(collection)?;
        let tmp_path = path.with_extension("tmp");
        {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(json.as_bytes())?;
            tmp.flush()?;
        }
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Collection>> {
        let path = self.file_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let collection: Collection = serde_json::from_str(&contents)?;
        Ok(Some(collection))
    }

    fn delete(&self, id: &str) -> Result<()> {
        let path = self.file_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<CollectionSummary>> {
        let files = self.scan_collections()?;
        let mut summaries = Vec::new();
        for file in files {
            let mut contents = String::new();
            File::open(&file)?.read_to_string(&mut contents)?;
            if let Ok(collection) = serde_json::from_str::<Collection>(&contents) {
                summaries.push(collection.summary());
            }
        }
        Ok(summaries)
    }

    fn search(&self, query: &str) -> Result<Vec<CollectionSummary>> {
        let query_lower = query.to_lowercase();
        let summaries = self.list()?;
        Ok(summaries
            .into_iter()
            .filter(|s| s.name.to_lowercase().contains(&query_lower))
            .collect())
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use yinx_core::collections::CollectionItem;
    use yinx_core::request::RequestBuilder;
    use yinx_core::state::SavedRequest;

    fn setup_store() -> (JsonCollectionStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = JsonCollectionStore::new(dir.path().join("collections")).unwrap();
        (store, dir)
    }

    fn make_collection(name: &str) -> Collection {
        let mut c = Collection::new(name.to_string());
        let saved = SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: "GET /users".to_string(),
            request: RequestBuilder::new()
                .url("https://example.com/users")
                .build()
                .unwrap(),
            tags: Vec::new(),
        };
        c.add_item(CollectionItem::Request(Box::new(saved)));
        c
    }

    #[test]
    fn test_collection_store_save_and_get() {
        let (store, _dir) = setup_store();
        let c = make_collection("API v2");
        let id = c.id.clone();
        store.save(&c).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "API v2");
        assert_eq!(loaded.item_count(), 1);
    }

    #[test]
    fn test_collection_store_get_nonexistent() {
        let (store, _dir) = setup_store();
        let loaded = store.get("nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_collection_store_delete() {
        let (store, _dir) = setup_store();
        let c = make_collection("API v2");
        let id = c.id.clone();
        store.save(&c).unwrap();
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }

    #[test]
    fn test_collection_store_list() {
        let (store, _dir) = setup_store();
        store.save(&make_collection("API v2")).unwrap();
        store.save(&make_collection("Auth")).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_collection_store_list_empty() {
        let (store, _dir) = setup_store();
        let list = store.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_collection_store_search() {
        let (store, _dir) = setup_store();
        store.save(&make_collection("API v2")).unwrap();
        store.save(&make_collection("Auth Service")).unwrap();
        let results = store.search("api").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "API v2");
    }

    #[test]
    fn test_collection_store_search_case_insensitive() {
        let (store, _dir) = setup_store();
        store.save(&make_collection("API v2")).unwrap();
        let results = store.search("Api").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_collection_store_search_no_match() {
        let (store, _dir) = setup_store();
        store.save(&make_collection("API v2")).unwrap();
        let results = store.search("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_collection_store_overwrite() {
        let (store, _dir) = setup_store();
        let mut c = make_collection("API v2");
        let id = c.id.clone();
        store.save(&c).unwrap();
        c.name = "Updated".to_string();
        store.save(&c).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "Updated");
    }

    #[test]
    fn test_collection_store_atomic_write() {
        let (store, _dir) = setup_store();
        let c = make_collection("API v2");
        let path = store.file_path(&c.id);
        store.save(&c).unwrap();
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn test_collection_store_summary_fields() {
        let (store, _dir) = setup_store();
        let c = make_collection("API v2");
        store.save(&c).unwrap();
        let summaries = store.list().unwrap();
        let summary = &summaries[0];
        assert_eq!(summary.name, "API v2");
        assert_eq!(summary.item_count, 1);
    }

    #[test]
    fn test_collection_store_delete_nonexistent() {
        let (store, _dir) = setup_store();
        store.delete("nonexistent").unwrap();
    }

    #[test]
    fn test_collection_store_persistence_across_reloads() {
        let dir = TempDir::new().unwrap();
        let collections_dir = dir.path().join("collections");
        let c = make_collection("Persistent");
        let id = c.id.clone();
        {
            let store = JsonCollectionStore::new(&collections_dir).unwrap();
            store.save(&c).unwrap();
        }
        let store = JsonCollectionStore::new(&collections_dir).unwrap();
        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "Persistent");
    }
}
