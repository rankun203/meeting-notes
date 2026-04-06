//! Meeting transcript summarization via LLM.
//!
//! Reads a session's transcript, formats it with full meeting context
//! (participants, title, notes), builds a system prompt with metadata,
//! and calls the LLM to produce a summary saved as `summary.json`.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Local, Utc};
use futures::StreamExt;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::llm::client::LlmClient;
use crate::llm::context::{collect_tag_notes, collect_person_notes, extract_person_ids};
use crate::llm::prompt::format_segment;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::session::session::SessionInfo;
use crate::tags::TagsManager;

/// Metadata about the session passed to the summarization prompt.
pub struct SummarizationContext<'a> {
    pub session_info: Option<&'a SessionInfo>,
    pub language: &'a str,
}

/// Build the full system prompt for summarization.
///
/// Combines the user-configured prompt with session metadata:
/// language, current local time, session start time.
pub fn build_system_prompt(user_prompt: &str, ctx: &SummarizationContext) -> String {
    let mut prompt = user_prompt.to_string();

    if !user_prompt.contains("TODO") {
        prompt.push_str("\n\nIMPORTANT: You MUST also include a TODO section listing action items with owners. \
If there are no action items, write `No action items.` under the TODO heading. \
Place the TODO section near the top of the summary, right after the attendees/participants section.");
    }

    prompt.push_str("\n\nWhen listing action items or TODOs, \
use markdown checkbox syntax: `- [ ] **Owner**: task description` for incomplete and `- [x] **Owner**: task description` for completed. \
Each action item corresponds to exactly one owner. \
If ownership is ambiguous, put it to the most likely owner. If multiple people share responsibility, create separate items for each person.");

    // Citation instructions
    prompt.push_str(
        "\n\nFor every key point, decision, action item, or claim, add citations using the exact [MM:SS] timestamp from the transcript line it came from. \
Place citations inline at the end of the relevant sentence or item. \
When a claim spans multiple moments, chain them: [12:45][15:20]. \
Example: The team decided to use React for the frontend. [12:45][14:02]"
    );

    prompt.push_str(&format!("\n\nLanguage: {}", ctx.language));

    let now_local = Local::now();
    prompt.push_str(&format!(
        "\nCurrent time: {}",
        now_local.format("%A, %Y-%m-%d %H:%M %Z")
    ));

    prompt
}

/// Format a session's transcript into a rich text block for the LLM.
///
/// Includes meeting title, unique participants, notes, and timestamped
/// speaker-attributed transcript lines.
pub fn format_meeting_transcript(
    transcript: &Value,
    session_info: Option<&SessionInfo>,
    tag_notes: &[(String, String)],
    person_notes: &[(String, String)],
) -> Result<String, String> {
    let segments = transcript
        .get("segments")
        .and_then(|s| s.as_array())
        .ok_or("Transcript has no segments")?;

    if segments.is_empty() {
        return Err("Transcript is empty".to_string());
    }

    let mut output = String::new();

    // Meeting title, start time, and duration
    if let Some(info) = session_info {
        if let Some(name) = &info.name {
            output.push_str(&format!("Meeting: {}\n", name));
        }
        let started_local: DateTime<Local> = info.created_at.with_timezone(&Local);
        output.push_str(&format!(
            "Started: {}\n",
            started_local.format("%A, %Y-%m-%d %H:%M %Z")
        ));
        if let Some(dur) = info.duration_secs {
            let h = (dur / 3600.0) as u32;
            let m = ((dur % 3600.0) / 60.0) as u32;
            let s = (dur % 60.0) as u32;
            if h > 0 {
                output.push_str(&format!("Duration: {}h {}m {}s\n", h, m, s));
            } else {
                output.push_str(&format!("Duration: {}m {}s\n", m, s));
            }
        }
    }

    // Unique participants: known names first, then unknown speaker IDs
    let mut known = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    for seg in segments {
        let person_name = seg.get("person_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        let speaker_id = seg.get("speaker").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        if let Some(name) = person_name {
            known.insert(name.to_string());
        } else if let Some(id) = speaker_id {
            unknown.insert(id.to_string());
        }
    }
    if !known.is_empty() || !unknown.is_empty() {
        let mut parts: Vec<String> = known.into_iter().collect();
        for id in unknown {
            parts.push(format!("{} (unknown participant)", id));
        }
        output.push_str(&format!("Participants: {}\n", parts.join(", ")));
    }

    // Session notes
    if let Some(info) = session_info {
        if let Some(notes) = &info.notes {
            if !notes.trim().is_empty() {
                output.push_str(&format!("Notes: {}\n", notes.trim()));
            }
        }
    }

    // Tag notes
    for (tag_name, notes) in tag_notes {
        if !notes.trim().is_empty() {
            output.push_str(&format!("Tag \"{}\" notes: {}\n", tag_name, notes.trim()));
        }
    }

    // Participant notes
    for (name, notes) in person_notes {
        if !notes.trim().is_empty() {
            output.push_str(&format!("Participant \"{}\" notes: {}\n", name, notes.trim()));
        }
    }

    // ASR disclaimer
    output.push_str("\nNote: This transcript was generated by automatic speech recognition (ASR). ");
    output.push_str("Names and words may be mis-recognized, especially those marked as low confidence. ");
    output.push_str("If a name sounds similar to a known participant, it likely refers to that person.\n\n");

    // Timestamped transcript lines with low-confidence annotations
    for seg in segments {
        output.push_str(&format_segment(seg));
    }

    Ok(output)
}

/// Extract TODO items from summary markdown and match person names to known people.
///
/// Parses lines matching `- [ ] ...` or `- [x] ...`, extracts the person name
/// (typically bold at the start), and looks up their person_id.
async fn extract_todos(content: &str, people_manager: &PeopleManager) -> Vec<Value> {
    // Cache all people for name matching
    let all_people = people_manager.list_people().await;

    let mut todos = Vec::new();
    let re_todo = regex::Regex::new(r"- \[([ xX])\] (.+)").unwrap();
    let re_bold = regex::Regex::new(r"^\*\*(.+?)\*\*").unwrap();

    for cap in re_todo.captures_iter(content) {
        let completed = cap[1].trim().len() > 0; // "x" or "X" = completed
        let text = cap[2].trim().to_string();

        // Try to extract person name from bold prefix: **Name** – task text
        let mut person_name: Option<String> = None;
        let mut person_id: Option<String> = None;
        let mut task_text = text.clone();

        if let Some(bold_cap) = re_bold.captures(&text) {
            let name = bold_cap[1].trim().to_string();
            // Find matching person (case-insensitive prefix match)
            let name_lower = name.to_lowercase();
            for p in &all_people {
                if p.name.to_lowercase() == name_lower
                    || p.name.to_lowercase().starts_with(&name_lower)
                    || name_lower.starts_with(&p.name.to_lowercase())
                {
                    person_id = Some(p.id.clone());
                    person_name = Some(p.name.clone());
                    break;
                }
            }
            if person_name.is_none() {
                person_name = Some(name);
            }
            // Extract the task portion after the name
            let after_bold = &text[bold_cap.get(0).unwrap().end()..];
            task_text = after_bold.trim_start_matches(&[' ', '–', '—', '-', ':'][..]).trim().to_string();
        }

        todos.push(json!({
            "text": task_text,
            "full_text": text,
            "completed": completed,
            "person_name": person_name,
            "person_id": person_id,
        }));
    }

    todos
}

/// Run the full summarization pipeline for a session.
///
/// 1. Reads `transcript.json` from the session directory
/// 2. Formats the transcript with meeting context
/// 3. Builds the system prompt with metadata
/// 4. Streams the LLM response, emitting delta events via WebSocket
/// 5. Writes the result to `summary.json`
pub async fn run_summarization(
    session_id: &str,
    session_dir: &Path,
    host: &str,
    api_key: &str,
    model: &str,
    user_prompt: &str,
    session_info: Option<&SessionInfo>,
    tags_manager: &TagsManager,
    people_manager: &PeopleManager,
    session_manager: &SessionManager,
) -> Result<(), String> {
    let language = session_info
        .map(|s| s.language.as_str())
        .unwrap_or("en");

    // Read and format transcript
    let transcript_path = session_dir.join("transcript.json");
    let transcript_str = std::fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Failed to read transcript: {e}"))?;
    let transcript: Value = serde_json::from_str(&transcript_str)
        .map_err(|e| format!("Failed to parse transcript: {e}"))?;

    // Collect notes from tags and participants
    let session_tags = session_info.map(|s| s.tags.as_slice()).unwrap_or(&[]);
    let tag_notes = collect_tag_notes(tags_manager, session_tags).await;
    let person_ids = extract_person_ids(&transcript);
    let person_notes = collect_person_notes(people_manager, &person_ids).await;

    let meeting_text = format_meeting_transcript(&transcript, session_info, &tag_notes, &person_notes)?;

    // Build prompt
    let ctx = SummarizationContext {
        session_info,
        language,
    };
    let system_prompt = build_system_prompt(user_prompt, &ctx);

    info!("[{}] Generating summary with model {}", session_id, model);

    let client = LlmClient::new(host.to_string(), api_key.to_string(), model.to_string());
    let messages = vec![
        json!({"role": "system", "content": system_prompt}),
        json!({"role": "user", "content": format!("Here is the meeting transcript:\n\n{}", meeting_text)}),
    ];

    // Stream the response, emitting deltas via WebSocket
    let stream_start = std::time::Instant::now();
    let stream = client.stream_chat(messages.clone()).await?;
    futures::pin_mut!(stream);

    let mut content = String::new();
    let mut first_chunk = true;
    let mut was_thinking = false;
    while let Some(result) = stream.next().await {
        match result {
            Ok(delta) => {
                // \x01 prefix = thinking/reasoning token (not part of final content)
                let is_thinking = delta.starts_with('\x01');
                if is_thinking {
                    if first_chunk {
                        info!("[{}] Thinking started ({:.1}s)", session_id, stream_start.elapsed().as_secs_f64());
                        first_chunk = false;
                        was_thinking = true;
                        session_manager.emit_summary_progress(session_id, "thinking").await;
                    }
                    continue; // Don't include thinking in output
                }

                if first_chunk {
                    info!("[{}] Stream started ({:.1}s)", session_id, stream_start.elapsed().as_secs_f64());
                    first_chunk = false;
                } else if was_thinking {
                    info!("[{}] Content started ({:.1}s)", session_id, stream_start.elapsed().as_secs_f64());
                    was_thinking = false;
                }

                content.push_str(&delta);
                session_manager.emit_summary_delta(session_id, &delta);
            }
            Err(e) => {
                warn!("[{}] Summary stream error: {}", session_id, e);
                return Err(e);
            }
        }
    }

    if content.is_empty() {
        return Err("LLM returned empty response".to_string());
    }

    let summary = json!({
        "content": content,
        "model": model,
        "messages": messages,
        "generated_at": Utc::now().to_rfc3339(),
    });

    let summary_path = session_dir.join("summary.json");
    let json_str = serde_json::to_string_pretty(&summary)
        .map_err(|e| format!("Failed to serialize summary: {e}"))?;
    std::fs::write(&summary_path, json_str)
        .map_err(|e| format!("Failed to write summary: {e}"))?;

    let md_path = session_dir.join("summary.md");
    std::fs::write(&md_path, &content)
        .map_err(|e| format!("Failed to write summary.md: {e}"))?;

    // Extract TODOs and save per-session
    let todos = extract_todos(&content, people_manager).await;
    if !todos.is_empty() {
        let todos_path = session_dir.join("todos.json");
        let todos_json = serde_json::to_string_pretty(&json!({"items": todos}))
            .map_err(|e| format!("Failed to serialize todos: {e}"))?;
        std::fs::write(&todos_path, todos_json)
            .map_err(|e| format!("Failed to write todos.json: {e}"))?;
        info!("[{}] Extracted {} TODOs", session_id, todos.len());
    }

    info!(
        "[{}] Summary saved ({} words)",
        session_id,
        content.split_whitespace().count()
    );
    Ok(())
}
