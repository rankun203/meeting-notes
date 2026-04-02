use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TagsFile {
    tags: Vec<Tag>,
}

#[derive(Clone)]
pub struct TagsManager {
    tags_path: PathBuf,
    tags: Arc<RwLock<Vec<Tag>>>,
}

/// Normalize a string to snake_case tag name: lowercase a-z0-9_ only.
pub fn normalize_tag_name(input: &str) -> String {
    let s: String = input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
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
            tags: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn load_from_disk(&self) {
        if !self.tags_path.exists() {
            return;
        }
        match std::fs::read_to_string(&self.tags_path) {
            Ok(json) => match serde_json::from_str::<TagsFile>(&json) {
                Ok(file) => {
                    let count = file.tags.len();
                    *self.tags.write().await = file.tags;
                    info!("Loaded {} tags from disk", count);
                }
                Err(e) => warn!("Failed to parse tags.json: {}", e),
            },
            Err(e) => warn!("Failed to read tags.json: {}", e),
        }
    }

    fn save_to_disk(&self, tags: &[Tag]) {
        let file = TagsFile { tags: tags.to_vec() };
        match serde_json::to_string_pretty(&file) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.tags_path, json) {
                    warn!("Failed to write tags.json: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize tags: {}", e),
        }
    }

    pub async fn list_tags(&self) -> Vec<Tag> {
        self.tags.read().await.clone()
    }

    pub async fn create_tag(&self, raw_name: &str) -> Result<Tag, String> {
        let name = normalize_tag_name(raw_name);
        if name.is_empty() {
            return Err("Tag name cannot be empty".into());
        }

        let mut tags = self.tags.write().await;
        if tags.iter().any(|t| t.name == name) {
            return Err(format!("Tag '{}' already exists", name));
        }

        let tag = Tag { name, hidden: false, notes: None };
        tags.push(tag.clone());
        self.save_to_disk(&tags);
        info!("Created tag: {}", tag.name);
        Ok(tag)
    }

    pub async fn update_tag(&self, name: &str, new_name: Option<&str>, hidden: Option<bool>, notes: Option<Option<String>>) -> Result<(Tag, Option<String>), String> {
        let mut tags = self.tags.write().await;
        let idx = tags.iter().position(|t| t.name == name)
            .ok_or_else(|| format!("Tag '{}' not found", name))?;

        let mut old_name: Option<String> = None;

        if let Some(raw) = new_name {
            let normalized = normalize_tag_name(raw);
            if normalized.is_empty() {
                return Err("Tag name cannot be empty".into());
            }
            if normalized != name {
                if tags.iter().any(|t| t.name == normalized) {
                    return Err(format!("Tag '{}' already exists", normalized));
                }
                old_name = Some(name.to_string());
                tags[idx].name = normalized;
            }
        }
        if let Some(h) = hidden {
            tags[idx].hidden = h;
        }
        if let Some(n) = notes {
            tags[idx].notes = n;
        }

        let result = tags[idx].clone();
        self.save_to_disk(&tags);
        if let Some(ref old) = old_name {
            info!("Renamed tag '{}' -> '{}'", old, result.name);
        }
        Ok((result, old_name))
    }

    pub async fn delete_tag(&self, name: &str) -> Result<(), String> {
        let mut tags = self.tags.write().await;
        let before = tags.len();
        tags.retain(|t| t.name != name);
        if tags.len() == before {
            return Err(format!("Tag '{}' not found", name));
        }
        self.save_to_disk(&tags);
        info!("Deleted tag: {}", name);
        Ok(())
    }

    pub async fn get_tag(&self, name: &str) -> Option<Tag> {
        self.tags.read().await.iter().find(|t| t.name == name).cloned()
    }

    pub async fn tag_exists(&self, name: &str) -> bool {
        self.tags.read().await.iter().any(|t| t.name == name)
    }

    /// Return the set of hidden tag names.
    pub async fn hidden_tag_names(&self) -> std::collections::HashSet<String> {
        self.tags.read().await.iter()
            .filter(|t| t.hidden)
            .map(|t| t.name.clone())
            .collect()
    }
}
