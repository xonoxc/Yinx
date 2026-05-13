use serde::{Deserialize, Serialize};

use crate::state::SavedRequest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub item_count: usize,
    pub modified: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollectionItem {
    Request(Box<SavedRequest>),
    Folder {
        name: String,
        children: Vec<CollectionItem>,
    },
}

impl CollectionItem {
    pub fn name(&self) -> &str {
        match self {
            CollectionItem::Request(r) => &r.name,
            CollectionItem::Folder { name, .. } => name,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, CollectionItem::Folder { .. })
    }

    pub fn as_folder_mut(&mut self) -> Option<&mut Vec<CollectionItem>> {
        match self {
            CollectionItem::Folder {
                ref mut children, ..
            } => Some(children),
            _ => None,
        }
    }

    pub fn count_items(&self) -> usize {
        match self {
            CollectionItem::Request(_) => 1,
            CollectionItem::Folder { children, .. } => {
                children.iter().map(|c| c.count_items()).sum()
            }
        }
    }

    pub fn collect_requests(&self) -> Vec<&SavedRequest> {
        match self {
            CollectionItem::Request(r) => vec![r],
            CollectionItem::Folder { children, .. } => {
                children.iter().flat_map(|c| c.collect_requests()).collect()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub items: Vec<CollectionItem>,
    pub created: chrono::DateTime<chrono::Utc>,
    pub modified: chrono::DateTime<chrono::Utc>,
}

impl Collection {
    pub fn new(name: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            items: Vec::new(),
            created: now,
            modified: now,
        }
    }

    pub fn item_count(&self) -> usize {
        self.items.iter().map(|i| i.count_items()).sum()
    }

    pub fn summary(&self) -> CollectionSummary {
        CollectionSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            item_count: self.item_count(),
            modified: self.modified,
        }
    }

    pub fn add_item(&mut self, item: CollectionItem) {
        self.items.push(item);
        self.modified = chrono::Utc::now();
    }

    pub fn remove_item(&mut self, index: usize) -> Option<CollectionItem> {
        if index < self.items.len() {
            self.modified = chrono::Utc::now();
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    pub fn flatten_requests(&self) -> Vec<&SavedRequest> {
        self.items
            .iter()
            .flat_map(|i| i.collect_requests())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::RequestBuilder;

    fn make_request(name: &str) -> SavedRequest {
        SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            request: RequestBuilder::new()
                .url("https://example.com")
                .build()
                .unwrap(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn test_collection_new() {
        let c = Collection::new("My API".to_string());
        assert_eq!(c.name, "My API");
        assert!(c.items.is_empty());
        assert!(!c.id.is_empty());
    }

    #[test]
    fn test_collection_add_item() {
        let mut c = Collection::new("Test".to_string());
        let req = make_request("GET /users");
        c.add_item(CollectionItem::Request(Box::new(req)));
        assert_eq!(c.items.len(), 1);
        assert_eq!(c.item_count(), 1);
    }

    #[test]
    fn test_collection_add_folder() {
        let mut c = Collection::new("Test".to_string());
        let req = make_request("GET /users");
        let folder = CollectionItem::Folder {
            name: "Users".to_string(),
            children: vec![CollectionItem::Request(Box::new(req))],
        };
        c.add_item(folder);
        assert_eq!(c.items.len(), 1);
        assert_eq!(c.item_count(), 1);
    }

    #[test]
    fn test_collection_remove_item() {
        let mut c = Collection::new("Test".to_string());
        let req = make_request("GET /users");
        c.add_item(CollectionItem::Request(Box::new(req)));
        assert!(c.remove_item(0).is_some());
        assert!(c.items.is_empty());
    }

    #[test]
    fn test_collection_remove_item_out_of_bounds() {
        let mut c = Collection::new("Test".to_string());
        assert!(c.remove_item(0).is_none());
        assert!(c.remove_item(5).is_none());
    }

    #[test]
    fn test_collection_summary() {
        let mut c = Collection::new("Test".to_string());
        c.add_item(CollectionItem::Request(Box::new(make_request(
            "GET /users",
        ))));
        let summary = c.summary();
        assert_eq!(summary.name, "Test");
        assert_eq!(summary.item_count, 1);
    }

    #[test]
    fn test_collection_flatten_requests() {
        let mut c = Collection::new("Test".to_string());
        c.add_item(CollectionItem::Request(Box::new(make_request("GET /a"))));
        c.add_item(CollectionItem::Folder {
            name: "Folder".to_string(),
            children: vec![CollectionItem::Request(Box::new(make_request("GET /b")))],
        });
        let flat = c.flatten_requests();
        assert_eq!(flat.len(), 2);
    }

    #[test]
    fn test_collection_item_name() {
        let req = CollectionItem::Request(Box::new(make_request("GET /test")));
        assert_eq!(req.name(), "GET /test");
        let folder = CollectionItem::Folder {
            name: "My Folder".to_string(),
            children: Vec::new(),
        };
        assert_eq!(folder.name(), "My Folder");
    }

    #[test]
    fn test_collection_item_is_folder() {
        let req = CollectionItem::Request(Box::new(make_request("GET /test")));
        assert!(!req.is_folder());
        let folder = CollectionItem::Folder {
            name: "F".to_string(),
            children: Vec::new(),
        };
        assert!(folder.is_folder());
    }

    #[test]
    fn test_collection_item_as_folder_mut() {
        let mut folder = CollectionItem::Folder {
            name: "F".to_string(),
            children: Vec::new(),
        };
        assert!(folder.as_folder_mut().is_some());
        let mut req = CollectionItem::Request(Box::new(make_request("GET /test")));
        assert!(req.as_folder_mut().is_none());
    }

    #[test]
    fn test_collection_item_count_nested() {
        let item = CollectionItem::Folder {
            name: "Root".to_string(),
            children: vec![
                CollectionItem::Request(Box::new(make_request("GET /a"))),
                CollectionItem::Folder {
                    name: "Nested".to_string(),
                    children: vec![
                        CollectionItem::Request(Box::new(make_request("GET /b"))),
                        CollectionItem::Request(Box::new(make_request("GET /c"))),
                    ],
                },
            ],
        };
        assert_eq!(item.count_items(), 3);
    }

    #[test]
    fn test_collection_item_collect_requests() {
        let req_a = make_request("GET /a");
        let req_b = make_request("GET /b");
        let folder = CollectionItem::Folder {
            name: "Root".to_string(),
            children: vec![
                CollectionItem::Request(Box::new(req_a)),
                CollectionItem::Request(Box::new(req_b)),
            ],
        };
        let requests = folder.collect_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].name, "GET /a");
        assert_eq!(requests[1].name, "GET /b");
    }

    #[test]
    fn test_collection_serde_roundtrip() {
        let mut c = Collection::new("Test".to_string());
        c.add_item(CollectionItem::Request(Box::new(make_request(
            "GET /users",
        ))));
        let json = serde_json::to_string(&c).unwrap();
        let decoded: Collection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, c.name);
        assert_eq!(decoded.items.len(), c.items.len());
    }

    #[test]
    fn test_collection_modified_on_add() {
        let mut c = Collection::new("Test".to_string());
        let before = c.modified;
        std::thread::sleep(std::time::Duration::from_millis(2));
        c.add_item(CollectionItem::Request(Box::new(make_request(
            "GET /users",
        ))));
        assert!(c.modified > before);
    }

    #[test]
    fn test_collection_modified_on_remove() {
        let mut c = Collection::new("Test".to_string());
        c.add_item(CollectionItem::Request(Box::new(make_request(
            "GET /users",
        ))));
        let before = c.modified;
        std::thread::sleep(std::time::Duration::from_millis(2));
        c.remove_item(0);
        assert!(c.modified > before);
    }
}
