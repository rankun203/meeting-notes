//! System prompt and context formatting for LLM requests.
//!
//! Context chunks are only formatted into a prompt string right before
//! sending to the LLM backend. They are stored as structured data in
//! conversation files.

use serde_json::Value;

use crate::chat::types::{Conversation, ContextChunk, Message};

/// The system prompt for the meeting notes assistant.
pub fn system_prompt() -> &'static str {
    r#"You are a helpful meeting notes assistant. You have access to transcripts from the user's meetings and can answer questions about what was discussed, who said what, action items, decisions made, and other meeting content.

When answering:

- Reference specific speakers by name when available
- Include approximate timestamps when relevant
- Be concise but thorough
- If asked about something not covered in the provided context, say so clearly
- Format your responses with markdown when helpful (lists, bold for emphasis, code blocks if relevant)
- Avoid using tables — the display is narrow mobile screen. Prefer bullet lists or short key-value pairs instead

If no meeting context is provided and the user isn't asking about a specific meeting, briefly remind them: "Use @ to mention sessions, people, or tags to give me context about your meetings."

The meeting transcript context is provided below. Use it to answer the user's questions."#
}

/// Format a single transcript segment into a timestamped line with speaker
/// and low-confidence word annotations.
///
/// Used by both chat context formatting and summarization transcript formatting.
pub fn format_segment(segment: &Value) -> String {
    let start = segment.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let text = segment.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let speaker = segment.get("person_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| segment.get("speaker").and_then(|v| v.as_str()))
        .unwrap_or("Unknown");

    let mins = (start / 60.0) as u32;
    let secs = (start % 60.0) as u32;

    // Collect low-confidence words from the words array
    let low_conf: Vec<&str> = segment.get("words")
        .and_then(|w| w.as_array())
        .map(|words| {
            words.iter()
                .filter(|w| w.get("score").and_then(|s| s.as_f64()).unwrap_or(1.0) < 0.5)
                .filter_map(|w| w.get("word").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();

    if low_conf.is_empty() {
        format!("[{:02}:{:02}] {}: {}\n", mins, secs, speaker, text.trim())
    } else {
        format!("[{:02}:{:02}] {}: {} (low confidence: {})\n",
            mins, secs, speaker, text.trim(), low_conf.join(", "))
    }
}

/// Format context chunks into a readable string for the LLM.
///
/// Groups segments by session and formats with timestamps and speaker names.
pub fn format_context(chunks: &[ContextChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    let mut current_source: Option<String> = None;

    // Pre-collect named attendees per session
    let mut session_attendees: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for chunk in chunks {
        if chunk.kind == "segment" {
            if let Some(segment) = &chunk.segment {
                if let Some(name) = segment.get("person_name").and_then(|v| v.as_str()) {
                    if !name.is_empty() {
                        let key = format!("{}:{}", chunk.source_type, chunk.source_id);
                        let names = session_attendees.entry(key).or_default();
                        if !names.contains(&name.to_string()) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    // Notes come first (sorted that way), then segments
    for chunk in chunks {
        if chunk.kind == "note" {
            let label = chunk.source_label.as_deref().unwrap_or(&chunk.source_id);
            let type_label = match chunk.source_type.as_str() {
                "session" => "Session",
                "person" => "Person",
                "tag" => "Tag",
                _ => "Note",
            };
            output.push_str(&format!("\n[{} note for \"{}\"]: {}\n",
                type_label, label, chunk.note.as_deref().unwrap_or("")));
            continue;
        }

        // Segment — group by source session
        let source_key = format!("{}:{}", chunk.source_type, chunk.source_id);
        if current_source.as_deref() != Some(&source_key) {
            current_source = Some(source_key.clone());
            let name = chunk.source_label.as_deref().unwrap_or("Untitled Session");
            let date = chunk.created_at.format("%Y-%m-%d %H:%M");
            output.push_str(&format!("\n=== Session: \"{}\" ({}) ===\n", name, date));
            if let Some(attendees) = session_attendees.get(&source_key) {
                if !attendees.is_empty() {
                    output.push_str(&format!("Attendees: {}\n", attendees.join(", ")));
                }
            }
        }

        if let Some(segment) = &chunk.segment {
            output.push_str(&format_segment(segment));
        }
    }

    output
}

/// Build the OpenAI messages array for a chat completion request.
///
/// Includes the system prompt with context, then user/assistant messages
/// from conversation history (skips ContextResult messages).
pub fn build_messages(conversation: &Conversation, context_str: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    // System message with context
    let system_content = if context_str.is_empty() {
        system_prompt().to_string()
    } else {
        format!("{}\n\n--- MEETING TRANSCRIPT CONTEXT ---\n{}", system_prompt(), context_str)
    };

    messages.push(serde_json::json!({
        "role": "system",
        "content": system_content,
    }));

    // User and assistant messages from history
    for msg in &conversation.messages {
        match msg {
            Message::User { content, .. } => {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": content,
                }));
            }
            Message::Assistant { content, .. } => {
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": content,
                }));
            }
            Message::ContextResult { .. } => {
                // Skip context results — they're stored for reference but
                // the formatted context is included in the system message
            }
        }
    }

    messages
}

/// Format the full prompt as human-readable text for export.
///
/// Renders the system prompt, context, and all user/assistant messages
/// in a copy-pasteable format.
pub fn format_as_text(conversation: &Conversation, context_str: &str) -> String {
    let mut out = String::new();

    // System prompt + context
    out.push_str("=== SYSTEM ===\n\n");
    out.push_str(system_prompt());
    if !context_str.is_empty() {
        out.push_str("\n\n--- MEETING TRANSCRIPT CONTEXT ---\n");
        out.push_str(context_str);
    }

    // Conversation messages
    for msg in &conversation.messages {
        match msg {
            Message::User { content, .. } => {
                out.push_str("\n\n=== USER ===\n\n");
                out.push_str(content);
            }
            Message::Assistant { content, .. } => {
                out.push_str("\n\n=== ASSISTANT ===\n\n");
                out.push_str(content);
            }
            Message::ContextResult { .. } => {}
        }
    }

    out.push('\n');
    out
}
