use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry {
    cached_at: u64,
    value: Value,
}

#[derive(Clone)]
pub struct CacheState {
    file: PathBuf,
    entries: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedList<T> {
    pub items: Vec<T>,
    pub cached_at: u64,
    pub from_cache: bool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl CacheState {
    pub fn load(app: &AppHandle) -> Self {
        let dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("catalog-cache.json");
        let entries = std::fs::read_to_string(&file)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            file,
            entries: Arc::new(Mutex::new(entries)),
        }
    }

    pub fn get<T: DeserializeOwned>(&self, kind: &str, env: &str) -> Option<CachedList<T>> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(&format!("{kind}:{env}"))?;
        Some(CachedList {
            items: serde_json::from_value(entry.value.clone()).ok()?,
            cached_at: entry.cached_at,
            from_cache: true,
        })
    }

    pub fn put<T: Serialize + Clone>(
        &self,
        kind: &str,
        env: &str,
        items: &[T],
    ) -> CachedList<T> {
        let cached_at = now_ms();
        if let Ok(mut entries) = self.entries.lock() {
            if let Ok(value) = serde_json::to_value(items) {
                entries.insert(
                    format!("{kind}:{env}"),
                    CacheEntry { cached_at, value },
                );
                if let Ok(json) = serde_json::to_string_pretty(&*entries) {
                    let _ = std::fs::write(&self.file, json);
                }
            }
        }
        CachedList {
            items: items.to_vec(),
            cached_at,
            from_cache: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_entry_roundtrip() {
        let entry = CacheEntry {
            cached_at: 42,
            value: serde_json::json!(["one", "two"]),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let restored: CacheEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.cached_at, 42);
        assert_eq!(restored.value, serde_json::json!(["one", "two"]));
    }
}
