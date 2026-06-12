use rusqlite::{Connection, params};
use yinx_core::collections::Collection;

pub struct CollectionDb {
    conn: Connection,
}

#[derive(Debug, thiserror::Error)]
pub enum CollectionDbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl CollectionDb {
    pub fn open(path: &std::path::Path) -> Result<Self, CollectionDbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), CollectionDbError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );"
        )?;
        Ok(())
    }

    pub fn save_collection(&self, collection: &Collection) -> Result<(), CollectionDbError> {
        let data = serde_json::to_string(collection)?;
        let created = collection.created.to_rfc3339();
        let modified = collection.modified.to_rfc3339();
        self.conn.execute(
            "INSERT INTO collections (id, name, data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 data = excluded.data,
                 updated_at = excluded.updated_at",
            params![collection.id, collection.name, data, created, modified],
        )?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> Result<(), CollectionDbError> {
        self.conn.execute(
            "DELETE FROM collections WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn load_all_collections(&self) -> Result<Vec<Collection>, CollectionDbError> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM collections ORDER BY created_at ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            let data: String = row.get(0)?;
            Ok(data)
        })?;
        let mut collections = Vec::new();
        for row in rows {
            let data = row?;
            let collection: Collection = serde_json::from_str(&data)?;
            collections.push(collection);
        }
        Ok(collections)
    }

    pub fn rename_collection(&self, id: &str, new_name: &str) -> Result<(), CollectionDbError> {
        let mut stmt = self.conn.prepare("SELECT data FROM collections WHERE id = ?1")?;
        let data: String = stmt.query_row(params![id], |row| row.get(0))?;
        let mut collection: Collection = serde_json::from_str(&data)?;
        collection.name = new_name.to_string();
        let new_data = serde_json::to_string(&collection)?;
        self.conn.execute(
            "UPDATE collections SET name = ?1, data = ?2 WHERE id = ?3",
            params![new_name, new_data, id],
        )?;
        Ok(())
    }

    pub fn rename_request_in_collection(&self, collection_id: &str, request_id: &str, new_name: &str) -> Result<(), CollectionDbError> {
        let mut stmt = self.conn.prepare("SELECT data FROM collections WHERE id = ?1")?;
        let data: String = stmt.query_row(params![collection_id], |row| row.get(0))?;
        let mut collection: Collection = serde_json::from_str(&data)?;
        rename_in_items(&mut collection.items, request_id, new_name);
        let new_data = serde_json::to_string(&collection)?;
        self.conn.execute(
            "UPDATE collections SET data = ?1 WHERE id = ?2",
            params![new_data, collection_id],
        )?;
        Ok(())
    }
}

fn rename_in_items(items: &mut [yinx_core::collections::CollectionItem], request_id: &str, new_name: &str) {
    for item in items.iter_mut() {
        match item {
            yinx_core::collections::CollectionItem::Request(req) if req.id == request_id => {
                req.name = new_name.to_string();
                return;
            }
            yinx_core::collections::CollectionItem::Folder { children, .. } => {
                rename_in_items(children, request_id, new_name);
            }
            _ => {}
        }
    }
}
