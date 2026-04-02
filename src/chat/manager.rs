use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::Value;
use tracing::{info, warn};

use super::types::{Conversation, ConversationSummary};

/// Manages conversation JSON files on disk.
#[derive(Clone)]
pub struct ConversationManager {
    conversations_dir: PathBuf,
    data_dir: PathBuf,
}

impl ConversationManager {
    pub fn new(data_dir: &Path) -> Self {
        let conversations_dir = data_dir.join("conversations");
        if let Err(e) = std::fs::create_dir_all(&conversations_dir) {
            warn!("Failed to create conversations directory: {}", e);
        }
        Self {
            conversations_dir,
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// List conversations (lightweight summaries, sorted by most recent first).
    /// Uses file modification time to pre-sort and only parses the top `limit` files.
    pub fn list(&self, limit: usize) -> Vec<ConversationSummary> {
        let entries = match std::fs::read_dir(&self.conversations_dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        // Collect paths with modification time, sort by most recent first
        let mut files: Vec<(std::path::PathBuf, std::time::SystemTime)> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                let mtime = std::fs::metadata(&path).ok()?.modified().ok()?;
                Some((path, mtime))
            })
            .collect();

        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Only parse the top N files
        let mut summaries = Vec::new();
        for (path, _) in files.into_iter().take(limit) {
            let size_bytes = std::fs::metadata(&path)
                .map(|m| m.len())
                .unwrap_or(0);

            match std::fs::read_to_string(&path) {
                Ok(json) => {
                    match serde_json::from_str::<Conversation>(&json) {
                        Ok(conv) => {
                            let last_msg = conv.messages.last()
                                .and_then(|m| m.preview(80));
                            summaries.push(ConversationSummary {
                                id: conv.id,
                                title: conv.title,
                                message_count: conv.messages.len(),
                                last_message_preview: last_msg,
                                created_at: conv.created_at,
                                updated_at: conv.updated_at,
                                size_bytes,
                            });
                        }
                        Err(e) => warn!("Failed to parse conversation {:?}: {}", path, e),
                    }
                }
                Err(e) => warn!("Failed to read conversation {:?}: {}", path, e),
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        summaries
    }

    /// Get a full conversation by ID.
    pub fn get(&self, id: &str) -> Option<Conversation> {
        let path = self.conversation_path(id);
        let json = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Get a conversation with words arrays stripped from context chunks
    /// (for efficient transfer to the frontend).
    pub fn get_transformed(&self, id: &str) -> Option<Value> {
        let conv = self.get(id)?;
        let mut value = serde_json::to_value(&conv).ok()?;

        // Strip words from context_result messages
        if let Some(messages) = value.get_mut("messages").and_then(|m| m.as_array_mut()) {
            for msg in messages.iter_mut() {
                if msg.get("role").and_then(|r| r.as_str()) == Some("context_result") {
                    if let Some(chunks) = msg.get_mut("chunks").and_then(|c| c.as_array_mut()) {
                        for chunk in chunks.iter_mut() {
                            if let Some(segment) = chunk.get_mut("segment") {
                                segment.as_object_mut().map(|s| s.remove("words"));
                            }
                        }
                    }
                }
            }
        }

        Some(value)
    }

    /// Create a new empty conversation.
    pub fn create(&self, title: Option<String>) -> Result<Conversation, String> {
        let now = Utc::now();
        let id = format!("conv_{}", now.timestamp_nanos_opt().unwrap_or(0));
        let conv = Conversation {
            id: id.to_string(),
            title: title.unwrap_or_default(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };
        self.save(&conv)?;
        info!("Created conversation {}", id);
        Ok(conv)
    }

    /// Save a conversation to disk.
    pub fn save(&self, conv: &Conversation) -> Result<(), String> {
        let path = self.conversation_path(&conv.id);
        let json = serde_json::to_string_pretty(conv)
            .map_err(|e| format!("Failed to serialize conversation: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write conversation: {e}"))?;
        Ok(())
    }

    /// Delete a conversation file.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.conversation_path(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete conversation: {e}"))?;
            info!("Deleted conversation {}", id);
        }
        Ok(())
    }

    /// Get the data directory (for secrets access).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn conversation_path(&self, id: &str) -> PathBuf {
        self.conversations_dir.join(format!("{}.json", id))
    }
}
