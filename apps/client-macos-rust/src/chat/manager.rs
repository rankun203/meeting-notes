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

            match crate::storage::read_json::<ListConversation>(&path) {
                Ok(conv) => {
                    let last_msg = conv.messages.last().and_then(|m| m.preview());
                    summaries.push(ConversationSummary {
                        id: conv.id, title: conv.title, message_count: conv.messages.len(),
                        last_message_preview: last_msg, created_at: conv.created_at,
                        updated_at: conv.updated_at, size_bytes, chat_backend: conv.chat_backend,
                        claude_session_id: conv.claude_session_id,
                    });
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
        let revision = crate::storage::revision(&path);
        let mut conv: Conversation = crate::storage::read_json(&path).ok()?;
        if crate::storage::revision(&path) != revision { return None; }
        conv.revision = revision;
        Some(conv)
    }

    /// Skip word arrays while parsing, before allocating their object graphs.
    pub fn get_transformed(&self, id: &str) -> Option<Value> {
        crate::storage::read_without_words(&self.conversation_path(id)).ok()
    }

    /// Create a new empty conversation.
    pub fn create(&self, title: Option<String>) -> Result<Conversation, String> {
        let now = Utc::now();
        let id = format!("conv_{}", now.timestamp_nanos_opt().unwrap_or(0));
        let mut conv = Conversation {
            revision: None,
            id: id.to_string(),
            title: title.unwrap_or_default(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            chat_backend: None,
            claude_session_id: None,
        };
        self.save(&conv)?;
        conv.revision = crate::storage::revision(&self.conversation_path(&id));
        info!("Created conversation {}", id);
        Ok(conv)
    }

    /// Save a conversation to disk.
    pub fn save(&self, conv: &Conversation) -> Result<(), String> {
        let path = self.conversation_path(&conv.id);
        let _lock = crate::storage::write_lock(&path);
        if crate::storage::revision(&path) != conv.revision {
            return Err("conversation changed externally; reload and retry".into());
        }
        let mut value = serde_json::to_value(conv).map_err(|e| e.to_string())?;
        // Preserve extension fields added by other tools.
        let mut stored: Value = if path.exists() { crate::storage::read_json(&path)? } else { serde_json::json!({}) };
        // Preserve per-message extension fields by stable message ID while
        // honoring deleted messages. Move JSON trees instead of cloning them.
        let old_messages = stored.get_mut("messages").map(std::mem::take)
            .and_then(|v| if let Value::Array(a) = v { Some(a) } else { None }).unwrap_or_default();
        let mut by_id: std::collections::HashMap<String, Value> = old_messages.into_iter()
            .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_owned).map(|id| (id, v))).collect();
        if let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) {
            for message in messages {
                let old = message.get("id").and_then(Value::as_str).and_then(|id| by_id.remove(id));
                if let (Some(Value::Object(mut old)), Value::Object(fields)) = (old, &mut *message) {
                    old.extend(std::mem::take(fields));
                    *fields = old;
                }
            }
        }
        if let (Some(target), Value::Object(fields)) = (stored.as_object_mut(), value) { target.extend(fields); }
        if crate::storage::revision(&path) != conv.revision {
            return Err("conversation changed during save; reload and retry".into());
        }
        crate::storage::write_json(&path, &stored)?;
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

    /// Delete a single message from a conversation by message ID.
    pub fn delete_message(&self, conv_id: &str, msg_id: &str) -> Result<(), String> {
        let mut conv = self.get(conv_id)
            .ok_or_else(|| "conversation not found".to_string())?;
        let before = conv.messages.len();
        conv.messages.retain(|m| m.id() != msg_id);
        if conv.messages.len() == before {
            return Err("message not found".to_string());
        }
        conv.updated_at = chrono::Utc::now();
        self.save(&conv)?;
        info!("Deleted message {} from conversation {}", msg_id, conv_id);
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

// Deserialization ignores context payloads for list responses.
#[derive(serde::Deserialize)]
struct ListConversation {
    id: String,
    title: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    messages: Vec<ListMessage>,
    chat_backend: Option<String>,
    claude_session_id: Option<String>,
}
#[derive(serde::Deserialize)]
struct ListMessage {
    content: Option<String>,
    #[serde(default, deserialize_with = "count_chunks")]
    chunks: usize,
}
fn count_chunks<'de, D: serde::Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    struct Counter;
    impl<'de> serde::de::Visitor<'de> for Counter {
        type Value = usize;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("chunks") }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<usize, A::Error> {
            let mut n = 0;
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() { n += 1; }
            Ok(n)
        }
    }
    d.deserialize_seq(Counter)
}
impl ListMessage {
    fn preview(&self) -> Option<String> {
        Some(if let Some(content) = &self.content {
            let preview: String = content.chars().take(80).collect();
            if preview.len() < content.len() { format!("{preview}...") } else { preview }
        } else { format!("[Context: {} segments]", self.chunks) })
    }
}
