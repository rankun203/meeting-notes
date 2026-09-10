//! Shared filesystem primitives. Source JSON is authoritative; only derived
//! projections may be cached. Revisions include identity and change time so an
//! editor's same-size atomic replacement also invalidates a projection.
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

pub fn revision(path: &Path) -> Option<Revision> {
    let meta = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Some(Revision {
        len: meta.len(),
        modified: meta.modified().ok(),
        #[cfg(unix)]
        identity: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
    })
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Bound disk/JSON tasks, including their allocations, across concurrent requests.
pub async fn blocking<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> T {
    static LIMIT: OnceLock<std::sync::Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let permit = LIMIT
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)))
        .clone()
        .acquire_owned()
        .await
        .unwrap();
    tokio::task::spawn_blocking(move || {
        // A cancelled request must not release capacity while its worker is
        // still parsing and allocating in the background.
        let _permit = permit;
        job()
    })
    .await
    .expect("storage worker panicked")
}

// Fixed lock stripes bound lock memory and serialize app writes to the same path.
pub fn write_lock(path: &Path) -> std::sync::MutexGuard<'static, ()> {
    use std::hash::{Hash, Hasher};
    static LOCKS: OnceLock<Vec<Mutex<()>>> = OnceLock::new();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    LOCKS.get_or_init(|| (0..64).map(|_| Mutex::new(())).collect())[hash.finish() as usize % 64]
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("file has no parent")?;
    let temp = parent.join(format!(".meeting-notes-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|e| e.to_string())?;
        if let Ok(meta) = std::fs::metadata(path) {
            file.set_permissions(meta.permissions())
                .map_err(|e| e.to_string())?;
        }
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    atomic_write(path, &bytes)
}

pub fn replace_json<T: Serialize>(
    path: &Path,
    expected: Option<Revision>,
    value: &T,
) -> Result<(), String> {
    let _lock = write_lock(path);
    if revision(path) != expected {
        return Err("file changed during operation; reload and retry".into());
    }
    write_json(path, value)
}

/// Merge only fields changed by this operation into the latest document. Unknown
/// fields and unrelated external edits survive. Same-field conflicts are errors.
pub fn merge_document(base: &Value, updated: &Value, latest: &Value) -> Result<Value, String> {
    if base == updated {
        return Ok(latest.clone());
    }
    if let (Some(base), Some(updated), Some(latest)) =
        (base.as_object(), updated.as_object(), latest.as_object())
    {
        let mut result = latest.clone();
        let keys: std::collections::HashSet<_> = base.keys().chain(updated.keys()).collect();
        for key in keys {
            if base.get(key) == updated.get(key) {
                continue;
            }
            match (base.get(key), updated.get(key), latest.get(key)) {
                (Some(b), Some(u), Some(l)) => {
                    result.insert(key.clone(), merge_document(b, u, l)?);
                }
                (b, u, l) if l == b || l == u => {
                    if let Some(u) = u {
                        result.insert(key.clone(), u.clone());
                    } else {
                        result.remove(key);
                    }
                }
                _ => {
                    return Err(format!(
                        "file changed externally: conflict in {key}; reload and retry"
                    ))
                }
            }
        }
        Ok(Value::Object(result))
    } else if latest == base || latest == updated {
        Ok(updated.clone())
    } else {
        Err("file changed externally; reload and retry".into())
    }
}

pub fn update_json(path: &Path, base: &Value, updated: &Value) -> Result<Value, String> {
    let _lock = write_lock(path);
    let before = revision(path);
    let latest = if before.is_some() {
        read_json(path)?
    } else if base.is_null() {
        Value::Null
    } else {
        return Err("file deleted externally; reload and retry".into());
    };
    let merged = merge_document(base, updated, &latest)?;
    if revision(path) != before {
        return Err("file changed during update; reload and retry".into());
    }
    write_json(path, &merged)?;
    Ok(merged)
}

/// Enumerate only immediate session directories, never audio/content bodies.
pub fn session_dirs(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect()
}

/// Parse a display/context projection without allocating word timing arrays.
pub fn read_without_words(path: &Path) -> Result<Value, String> {
    struct Lean(Value);
    impl<'de> serde::Deserialize<'de> for Lean {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Lean;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("JSON")
                }
                fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Lean, E> {
                    Ok(Lean(v.into()))
                }
                fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Lean, E> {
                    Ok(Lean(v.into()))
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Lean, E> {
                    Ok(Lean(v.into()))
                }
                fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Lean, E> {
                    Ok(Lean(serde_json::json!(v)))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Lean, E> {
                    Ok(Lean(v.into()))
                }
                fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Lean, E> {
                    Ok(Lean(v.into()))
                }
                fn visit_unit<E: serde::de::Error>(self) -> Result<Lean, E> {
                    Ok(Lean(Value::Null))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Lean, A::Error> {
                    let mut values = Vec::new();
                    while let Some(Lean(v)) = seq.next_element()? {
                        values.push(v);
                    }
                    Ok(Lean(Value::Array(values)))
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Lean, A::Error> {
                    let mut values = serde_json::Map::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if key == "words" {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        } else {
                            values.insert(key, map.next_value::<Lean>()?.0);
                        }
                    }
                    Ok(Lean(Value::Object(values)))
                }
            }
            d.deserialize_any(Visitor)
        }
    }
    read_json::<Lean>(path).map(|v| v.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_unrelated_edits_preserves_extensions_and_rejects_conflicts() {
        let base = json!({"notes":"old", "name":"meeting"});
        let updated = json!({"notes":"new", "name":"meeting"});
        let latest = json!({"notes":"old", "name":"renamed", "extension":{"keep":true}});
        assert_eq!(
            merge_document(&base, &updated, &latest).unwrap(),
            json!({"notes":"new", "name":"renamed", "extension":{"keep":true}})
        );
        assert!(merge_document(
            &base,
            &updated,
            &json!({"notes":"external", "name":"meeting"})
        )
        .is_err());
    }

    #[test]
    fn atomic_replacement_changes_revision_and_failed_write_preserves_source() {
        let dir = std::env::temp_dir().join(format!("mn-storage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("source.json");
        write_json(&path, &json!({"v":1})).unwrap();
        let old = revision(&path);
        write_json(&path, &json!({"v":2})).unwrap();
        assert_ne!(old, revision(&path));
        assert!(update_json(&path, &json!({"v":1}), &json!({"v":3})).is_err());
        assert_eq!(read_json::<Value>(&path).unwrap(), json!({"v":2}));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lean_projection_preserves_json_except_word_arrays() {
        let dir = std::env::temp_dir().join(format!("mn-lean-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("source.json");
        write_json(&path, &json!({"messages":[{"chunks":[{"segment":{"text":"你好", "start":1.5, "words":[{"text":"hi"}]}}]}], "null":null, "flag":true})).unwrap();
        let lean = read_without_words(&path).unwrap();
        assert_eq!(
            lean["messages"][0]["chunks"][0]["segment"],
            json!({"text":"你好", "start":1.5})
        );
        assert!(lean["flag"].as_bool().unwrap());
        assert!(lean["null"].is_null());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
