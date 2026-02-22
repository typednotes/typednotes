use crate::objects::Sha;
use crate::repo::ObjectStore;
use rexie::{ObjectStore as RexieObjectStore, Rexie, TransactionMode};
use wasm_bindgen::JsValue;

const DB_NAME: &str = "typednotes";
const DB_VERSION: u32 = 1;
const OBJECTS_STORE: &str = "objects";
const REFS_STORE: &str = "refs";

/// IndexedDB-backed ObjectStore for web platform.
#[derive(Clone)]
pub struct IdbStore {
    // We open the database on each operation because Rexie doesn't implement Clone.
    // This is cheap since IndexedDB caches open connections.
}

impl IdbStore {
    pub fn new() -> Self {
        Self {}
    }

    async fn open_db() -> Result<Rexie, rexie::Error> {
        Rexie::builder(DB_NAME)
            .version(DB_VERSION)
            .add_object_store(RexieObjectStore::new(OBJECTS_STORE))
            .add_object_store(RexieObjectStore::new(REFS_STORE))
            .build()
            .await
    }
}

impl ObjectStore for IdbStore {
    async fn get(&self, sha: &Sha) -> Option<Vec<u8>> {
        let db = Self::open_db().await.ok()?;
        let tx = db
            .transaction(&[OBJECTS_STORE], TransactionMode::ReadOnly)
            .ok()?;
        let store = tx.store(OBJECTS_STORE).ok()?;

        let key = JsValue::from_str(&sha.to_hex());
        let value = store.get(key).await.ok()?;

        let js_val = value?;
        let bytes: Vec<u8> = serde_wasm_bindgen::from_value(js_val).ok()?;
        Some(bytes)
    }

    async fn put(&self, sha: &Sha, data: Vec<u8>) {
        let Ok(db) = Self::open_db().await else {
            return;
        };
        let Ok(tx) = db.transaction(&[OBJECTS_STORE], TransactionMode::ReadWrite) else {
            return;
        };
        let Ok(store) = tx.store(OBJECTS_STORE) else {
            return;
        };

        let key = JsValue::from_str(&sha.to_hex());
        let value = serde_wasm_bindgen::to_value(&data).unwrap_or(JsValue::NULL);
        let _ = store.put(&value, Some(&key)).await;
        let _ = tx.done().await;
    }

    async fn get_ref(&self, name: &str) -> Option<Sha> {
        let db = Self::open_db().await.ok()?;
        let tx = db
            .transaction(&[REFS_STORE], TransactionMode::ReadOnly)
            .ok()?;
        let store = tx.store(REFS_STORE).ok()?;

        let key = JsValue::from_str(name);
        let value = store.get(key).await.ok()?;

        let js_val = value?;
        let hex: String = serde_wasm_bindgen::from_value(js_val).ok()?;
        Sha::from_hex(&hex)
    }

    async fn set_ref(&self, name: &str, sha: &Sha) {
        let Ok(db) = Self::open_db().await else {
            return;
        };
        let Ok(tx) = db.transaction(&[REFS_STORE], TransactionMode::ReadWrite) else {
            return;
        };
        let Ok(store) = tx.store(REFS_STORE) else {
            return;
        };

        let key = JsValue::from_str(name);
        let value = serde_wasm_bindgen::to_value(&sha.to_hex()).unwrap_or(JsValue::NULL);
        let _ = store.put(&value, Some(&key)).await;
        let _ = tx.done().await;
    }
}
