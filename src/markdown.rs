//! Markdown file generation from session JSON data.
//!
//! Each session JSON file (metadata, transcript, todos) has a corresponding
//! `.md` file that is kept in sync. These markdown files are human-readable
//! representations of the JSON data.

use std::path::Path;

use chrono::{DateTime, Local, Utc};
use serde_json::Value;

/// Format byte size to a human-readable string.
pub fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Write `metadata.md` with YAML frontmatter from a metadata JSON value.
pub fn write_metadata_md(session_dir: &Path, metadata: &Value) {
    let mut md = String::from("---\n");

    if let Some(obj) = metadata.as_object() {
        let key_order = [
            "session_id", "name", "state", "language", "format",
            "raw_sample_rate", "created_at", "updated_at", "started_at",
            "duration_secs", "tags", "notes", "auto_stop",
        ];
        for key in &key_order {
            if let Some(val) = obj.get(*key) {
                if val.is_null() {
                    continue;
                }
                md.push_str(&format_yaml_field(key, val));
            }
        }
    }

    md.push_str("---\n");

    // Add sources section if present
    if let Some(sources) = metadata.get("sources").and_then(|s| s.as_array()) {
        if !sources.is_empty() {
            md.push_str("\n## Sources\n\n");
            for src in sources {
                let label = src.get("source_label").and_then(|v| v.as_str()).unwrap_or("unknown");
                let stype = src.get("source_type").and_then(|v| v.as_str()).unwrap_or("");
                let filename = src.get("filename").and_then(|v| v.as_str()).unwrap_or("");
                md.push_str(&format!("- **{}** ({}): `{}`\n", label, stype, filename));
            }
        }
    }

    let path = session_dir.join("metadata.md");
    let _ = std::fs::write(&path, md);
}

/// Write `transcript.md` from transcript JSON data.
pub fn write_transcript_md(session_dir: &Path, transcript: &Value) {
    let segments = match transcript.get("segments").and_then(|s| s.as_array()) {
        Some(segs) if !segs.is_empty() => segs,
        _ => return,
    };

    let mut md = String::new();

    for seg in segments {
        let start = seg.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let text = seg.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let speaker = seg.get("person_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| seg.get("speaker").and_then(|v| v.as_str()))
            .unwrap_or("Unknown");

        let mins = (start / 60.0) as u32;
        let secs = (start % 60.0) as u32;

        md.push_str(&format!("[{:02}:{:02}] **{}**: {}\n", mins, secs, speaker, text.trim()));
    }

    let path = session_dir.join("transcript.md");
    let _ = std::fs::write(&path, md);
}

/// Write `todos.md` from todos JSON data.
pub fn write_todos_md(session_dir: &Path, todos: &Value) {
    let items = match todos.get("items").and_then(|i| i.as_array()) {
        Some(items) if !items.is_empty() => items,
        _ => return,
    };

    let mut md = String::from("## TODO\n\n");

    for item in items {
        let completed = item.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
        let full_text = item.get("full_text").and_then(|v| v.as_str()).unwrap_or("");
        let checkbox = if completed { "[x]" } else { "[ ]" };
        md.push_str(&format!("- {} {}\n", checkbox, full_text));
    }

    let path = session_dir.join("todos.md");
    let _ = std::fs::write(&path, md);
}

/// Extract the title and one-line description from summary markdown content.
///
/// The summary format is always:
/// ```text
/// # Title
///
/// {description}, meeting duration.
/// ```
/// Returns "Title: {description}" combining the H1 title and first body line.
fn extract_description(summary_content: &str) -> Option<String> {
    let mut title: Option<&str> = None;
    let mut found_title = false;
    for line in summary_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
            title = Some(trimmed.trim_start_matches("# ").trim());
            found_title = true;
            continue;
        }
        if found_title && !trimmed.is_empty() {
            return Some(format!("{}: {}", title.unwrap_or("Untitled"), trimmed));
        }
    }
    // Title found but no description line
    title.map(|t| t.to_string())
}

/// Format duration in seconds to a compact string like "30m 34s" or "1h 11m".
fn format_duration(secs: f64) -> String {
    let h = (secs / 3600.0) as u32;
    let m = ((secs % 3600.0) / 60.0) as u32;
    let s = (secs % 60.0) as u32;
    if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m {}s", m, s)
    }
}

/// Information about a session for index generation.
pub struct SessionEntry {
    pub id: String,
    pub name: Option<String>,
    pub language: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub duration_secs: Option<f64>,
    pub state: String,
}

/// Information about a person for index generation.
pub struct PersonEntry {
    pub id: String,
    pub name: String,
    pub starred: bool,
    pub created_at: DateTime<Utc>,
}

/// Generate `recordings/index.md` listing all sessions. Returns bytes written.
pub fn write_recordings_index(recordings_dir: &Path, sessions: &mut [SessionEntry]) -> usize {
    // Sort by created_at descending (newest first)
    sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let mut md = String::from("# Recordings\n\n");

    for s in sessions.iter() {
        let name = s.name.as_deref().unwrap_or("Untitled");
        let date: DateTime<Local> = s.created_at.with_timezone(&Local);
        let date_str = date.format("%Y-%m-%d %H:%M").to_string();
        let dur = s.duration_secs.map(|d| format_duration(d)).unwrap_or_default();

        // Try to read description from summary
        let summary_path = recordings_dir.join(&s.id).join("summary.json");
        let desc = std::fs::read_to_string(&summary_path).ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .and_then(|v| v.get("content")?.as_str().map(|s| s.to_string()))
            .and_then(|content| extract_description(&content));

        // Compact line: name | date | duration | tags
        let tags_str = if s.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", s.tags.join(", "))
        };

        md.push_str(&format!("- **{}** ({}){} — {} {}\n",
            name, s.id, tags_str, date_str, dur));

        if let Some(d) = &desc {
            md.push_str(&format!("  {}\n", d));
        }
    }

    let size = md.len();
    let path = recordings_dir.join("index.md");
    let _ = std::fs::write(&path, &md);
    size
}

/// Generate `people/index.md` listing all people. Returns bytes written.
pub fn write_people_index(people_dir: &Path, people: &mut [PersonEntry]) -> usize {
    // Starred first, then by name
    people.sort_by(|a, b| b.starred.cmp(&a.starred).then(a.name.cmp(&b.name)));

    let mut md = String::from("# People\n\n");

    for p in people.iter() {
        let star = if p.starred { " *" } else { "" };
        let date: DateTime<Local> = p.created_at.with_timezone(&Local);
        let date_str = date.format("%Y-%m-%d").to_string();
        md.push_str(&format!("- **{}**{} ({}) — {}\n", p.name, star, p.id, date_str));
    }

    let size = md.len();
    let path = people_dir.join("index.md");
    std::fs::create_dir_all(people_dir).ok();
    let _ = std::fs::write(&path, &md);
    size
}

/// Format a single YAML field for the frontmatter.
fn format_yaml_field(key: &str, val: &Value) -> String {
    match val {
        Value::String(s) => {
            // Quote strings that contain special YAML chars
            if s.contains(':') || s.contains('#') || s.contains('\n') {
                format!("{}: \"{}\"\n", key, s.replace('"', "\\\""))
            } else {
                format!("{}: {}\n", key, s)
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return String::new();
            }
            let mut out = format!("{}:\n", key);
            for item in arr {
                if let Some(s) = item.as_str() {
                    out.push_str(&format!("  - {}\n", s));
                }
            }
            out
        }
        _ => format!("{}: {}\n", key, val),
    }
}
