use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A conversation stored on disk as `{conversations_dir}/{id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Message>,
}

/// A message in a conversation. Tagged by role for JSON serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User {
        id: String,
        content: String,
        #[serde(default)]
        mentions: Vec<Mention>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        id: String,
        content: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "context_result")]
    ContextResult {
        id: String,
        criteria: ContextCriteria,
        chunks: Vec<ContextChunk>,
        timestamp: DateTime<Utc>,
    },
}

impl Message {
    pub fn id(&self) -> &str {
        match self {
            Message::User { id, .. } => id,
            Message::Assistant { id, .. } => id,
            Message::ContextResult { id, .. } => id,
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Message::User { timestamp, .. } => *timestamp,
            Message::Assistant { timestamp, .. } => *timestamp,
            Message::ContextResult { timestamp, .. } => *timestamp,
        }
    }

    /// Get a short preview of the message content.
    pub fn preview(&self, max_len: usize) -> Option<String> {
        match self {
            Message::User { content, .. } | Message::Assistant { content, .. } => {
                let truncated: String = content.chars().take(max_len).collect();
                if truncated.len() < content.len() {
                    Some(format!("{}...", truncated))
                } else {
                    Some(content.clone())
                }
            }
            Message::ContextResult { chunks, .. } => {
                Some(format!("[Context: {} segments]", chunks.len()))
            }
        }
    }
}

/// An @ mention in a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    pub kind: String,  // "tag", "person", "session"
    pub id: String,    // tag name, person_id, or session_id
    pub label: String, // display name
    /// Context mode: "transcript", "summary", or "both". Defaults to "transcript".
    #[serde(default = "default_context_mode")]
    pub context_mode: String,
}

fn default_context_mode() -> String { "transcript".to_string() }

/// Criteria for context retrieval, derived from mentions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextCriteria {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub person_ids: Vec<String>,
    #[serde(default)]
    pub session_ids: Vec<String>,
    /// Context mode per session ID: "transcript", "summary", or "both".
    /// Sessions not in this map default to "transcript".
    #[serde(default)]
    pub session_context_modes: std::collections::HashMap<String, String>,
}

impl ContextCriteria {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.person_ids.is_empty() && self.session_ids.is_empty()
    }

    /// Get the context mode for a session. Defaults to "transcript".
    pub fn context_mode_for(&self, session_id: &str) -> &str {
        self.session_context_modes.get(session_id).map(|s| s.as_str()).unwrap_or("transcript")
    }

    /// Merge another criteria into this one, deduplicating entries.
    pub fn merge(&mut self, other: &ContextCriteria) {
        for t in &other.tags {
            if !self.tags.contains(t) { self.tags.push(t.clone()); }
        }
        for p in &other.person_ids {
            if !self.person_ids.contains(p) { self.person_ids.push(p.clone()); }
        }
        for s in &other.session_ids {
            if !self.session_ids.contains(s) { self.session_ids.push(s.clone()); }
        }
        for (k, v) in &other.session_context_modes {
            self.session_context_modes.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    /// Build criteria from a list of mentions.
    pub fn from_mentions(mentions: &[Mention]) -> Self {
        let mut criteria = ContextCriteria::default();
        for m in mentions {
            let mode = m.context_mode.clone();
            match m.kind.as_str() {
                "tag" => {
                    criteria.tags.push(m.id.clone());
                    // Tag mode is stored temporarily; applied to resolved sessions later
                    criteria.session_context_modes.insert(format!("_tag:{}", m.id), mode);
                }
                "person" => {
                    criteria.person_ids.push(m.id.clone());
                    // Person mode stored; applied to resolved sessions later
                    criteria.session_context_modes.insert(format!("_person:{}", m.id), mode);
                }
                "session" => {
                    criteria.session_ids.push(m.id.clone());
                    criteria.session_context_modes.insert(m.id.clone(), mode);
                }
                _ => {}
            }
        }
        criteria
    }
}

/// A context chunk — either a transcript segment or a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChunk {
    /// "segment" for transcript lines, "note" for user notes.
    #[serde(default = "default_chunk_kind")]
    pub kind: String,
    /// Source session/person/tag ID.
    pub source_id: String,
    /// Display label (session name, person name, or tag name).
    pub source_label: Option<String>,
    /// Source type: "session", "person", or "tag".
    pub source_type: String,
    pub created_at: DateTime<Utc>,
    /// Full segment object from transcript.json (for kind="segment").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<Value>,
    /// Note text (for kind="note").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn default_chunk_kind() -> String { "segment".to_string() }

/// Lightweight conversation info for list responses.
#[derive(Debug, Clone, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub message_count: usize,
    pub last_message_preview: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub size_bytes: u64,
}
