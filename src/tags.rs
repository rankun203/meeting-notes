use crate::storage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone)]
pub struct TagsManager {
    tags_path: PathBuf,
}

/// Normalize a string to snake_case tag name: lowercase a-z0-9_ only.
pub fn normalize_tag_name(input: &str) -> String {
    let s: String = input
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Collapse multiple underscores and trim edges
    let mut result = String::new();
    let mut prev_underscore = true; // treat start as underscore to trim leading
    for c in s.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push('_');
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    // Trim trailing underscore
    if result.ends_with('_') {
        result.pop();
    }
    result
}

impl TagsManager {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            tags_path: data_dir.join("tags.json"),
        }
    }

    pub async fn list_tags(&self) -> Vec<Tag> {
        let path = self.tags_path.clone();
        storage::blocking(move || {
            #[derive(Deserialize)]
            struct Tags {
                tags: Vec<Tag>,
            }
            if !path.exists() {
                return Vec::new();
            }
            match storage::read_json::<Tags>(&path) {
                Ok(file) => file.tags,
                Err(e) => {
                    tracing::warn!("{}", e);
                    Vec::new()
                }
            }
        })
        .await
    }

    pub async fn create_tag(&self, raw_name: &str) -> Result<Tag, String> {
        let name = normalize_tag_name(raw_name);
        if name.is_empty() {
            return Err("Tag name cannot be empty".into());
        }
        let path = self.tags_path.clone();
        storage::blocking(move || {
            mutate(&path, |tags| {
                if tags.iter().any(|v| v["name"].as_str() == Some(&name)) {
                    return Err(format!("Tag '{name}' already exists"));
                }
                let tag = Tag {
                    name,
                    hidden: false,
                    notes: None,
                };
                tags.push(serde_json::to_value(&tag).map_err(|e| e.to_string())?);
                Ok(tag)
            })
        })
        .await
    }

    pub async fn update_tag(
        &self,
        name: &str,
        new_name: Option<&str>,
        hidden: Option<bool>,
        notes: Option<Option<String>>,
    ) -> Result<(Tag, Option<String>), String> {
        let name = name.to_string();
        let new_name = new_name.map(normalize_tag_name);
        if new_name.as_deref() == Some("") {
            return Err("Tag name cannot be empty".into());
        }
        let path = self.tags_path.clone();
        storage::blocking(move || {
            mutate(&path, |tags| {
                let idx = tags
                    .iter()
                    .position(|v| v["name"].as_str() == Some(&name))
                    .ok_or_else(|| format!("Tag '{name}' not found"))?;
                let mut old_name = None;
                if let Some(new_name) = new_name.filter(|n| *n != name) {
                    if tags.iter().any(|v| v["name"].as_str() == Some(&new_name)) {
                        return Err(format!("Tag '{new_name}' already exists"));
                    }
                    old_name = Some(name);
                    tags[idx]["name"] = new_name.into();
                }
                if let Some(hidden) = hidden {
                    tags[idx]["hidden"] = hidden.into();
                }
                if let Some(notes) = notes {
                    tags[idx]["notes"] = json!(notes);
                }
                let tag = serde_json::from_value(tags[idx].clone()).map_err(|e| e.to_string())?;
                Ok((tag, old_name))
            })
        })
        .await
    }

    pub async fn delete_tag(&self, name: &str) -> Result<(), String> {
        let name = name.to_string();
        let path = self.tags_path.clone();
        storage::blocking(move || {
            mutate(&path, |tags| {
                let before = tags.len();
                tags.retain(|v| v["name"].as_str() != Some(&name));
                if tags.len() == before {
                    return Err(format!("Tag '{name}' not found"));
                }
                Ok(())
            })
        })
        .await
    }

    pub async fn get_tag(&self, name: &str) -> Option<Tag> {
        self.list_tags().await.into_iter().find(|t| t.name == name)
    }

    pub async fn tag_exists(&self, name: &str) -> bool {
        self.get_tag(name).await.is_some()
    }

    pub async fn hidden_tag_names(&self) -> std::collections::HashSet<String> {
        self.list_tags()
            .await
            .into_iter()
            .filter(|t| t.hidden)
            .map(|t| t.name)
            .collect()
    }
}

fn mutate<T>(
    path: &Path,
    edit: impl FnOnce(&mut Vec<Value>) -> Result<T, String>,
) -> Result<T, String> {
    let _lock = storage::write_lock(path);
    let before = storage::revision(path);
    let mut document: Value = if before.is_some() {
        storage::read_json(path)?
    } else {
        json!({"tags":[]})
    };
    let tags = document
        .get_mut("tags")
        .and_then(Value::as_array_mut)
        .ok_or("invalid tags document")?;
    // Reject malformed entries before any edit instead of silently overwriting them.
    for tag in tags.iter() {
        serde_json::from_value::<Tag>(tag.clone()).map_err(|e| e.to_string())?;
    }
    let result = edit(tags)?;
    if storage::revision(path) != before {
        return Err("tags changed during update; reload and retry".into());
    }
    storage::write_json(path, &document)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn concurrent_tag_edits_and_extensions_survive() {
        let dir = std::env::temp_dir().join(format!("mn-tags-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        storage::write_json(
            &dir.join("tags.json"),
            &json!({"extension":true,"tags":[{"name":"old","extra":42}]}),
        )
        .unwrap();
        let tags = TagsManager::new(&dir);
        let (a, b) = tokio::join!(tags.create_tag("one"), tags.create_tag("two"));
        a.unwrap();
        b.unwrap();
        assert_eq!(tags.list_tags().await.len(), 3);
        tags.update_tag("old", Some("renamed"), Some(true), None)
            .await
            .unwrap();
        let saved: Value = storage::read_json(&dir.join("tags.json")).unwrap();
        assert_eq!(saved["extension"], true);
        assert_eq!(saved["tags"][0]["extra"], 42);
        assert!(tags.hidden_tag_names().await.contains("renamed"));
        std::fs::remove_file(dir.join("tags.json")).unwrap();
        assert!(tags.list_tags().await.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
