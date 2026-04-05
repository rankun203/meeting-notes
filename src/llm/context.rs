//! Context retrieval: collects transcript segments and notes matching criteria.

use std::collections::HashSet;

use crate::chat::types::{ContextChunk, ContextCriteria};
use crate::filesdb::FilesDb;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::tags::TagsManager;

/// Retrieve context chunks matching the given criteria.
///
/// Returns transcript segments and any user notes attached to matching
/// sessions, persons, or tags.
pub async fn retrieve_context(
    criteria: &ContextCriteria,
    files_db: &FilesDb,
    session_manager: &SessionManager,
    tags_manager: &TagsManager,
    people_manager: &PeopleManager,
) -> Vec<ContextChunk> {
    if criteria.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<ContextChunk> = Vec::new();

    // ── Collect notes from tags ──
    for tag_name in &criteria.tags {
        if let Some(tag) = tags_manager.get_tag(tag_name).await {
            if let Some(notes) = &tag.notes {
                if !notes.is_empty() {
                    chunks.push(ContextChunk {
                        kind: "note".to_string(),
                        source_id: tag_name.clone(),
                        source_label: Some(tag_name.clone()),
                        source_type: "tag".to_string(),
                        created_at: chrono::Utc::now(),
                        segment: None,
                        note: Some(notes.clone()),
                    });
                }
            }
        }
    }

    // ── Collect notes from persons ──
    for pid in &criteria.person_ids {
        if let Some(person) = people_manager.get_person(pid).await {
            if let Some(notes) = &person.notes {
                if !notes.is_empty() {
                    chunks.push(ContextChunk {
                        kind: "note".to_string(),
                        source_id: pid.clone(),
                        source_label: Some(person.name.clone()),
                        source_type: "person".to_string(),
                        created_at: person.updated_at,
                        segment: None,
                        note: Some(notes.clone()),
                    });
                }
            }
        }
    }

    // ── Collect session IDs from all criteria, tracking context mode ──
    let mut session_modes: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for sid in &criteria.session_ids {
        let mode = criteria.context_mode_for(sid).to_string();
        session_modes.entry(sid.clone()).or_insert(mode);
    }
    for tag in &criteria.tags {
        let tag_mode = criteria.session_context_modes
            .get(&format!("_tag:{}", tag))
            .map(|s| s.as_str())
            .unwrap_or("transcript")
            .to_string();
        for s in session_manager.sessions_for_tag(tag).await {
            session_modes.entry(s.id).or_insert_with(|| tag_mode.clone());
        }
    }
    for pid in &criteria.person_ids {
        let person_mode = criteria.session_context_modes
            .get(&format!("_person:{}", pid))
            .map(|s| s.as_str())
            .unwrap_or("transcript")
            .to_string();
        for sid in files_db.get_person_session_ids(pid).await {
            session_modes.entry(sid).or_insert_with(|| person_mode.clone());
        }
    }

    // ── For each session: collect notes, transcript segments, and/or summary ──
    let filter_by_person = !criteria.person_ids.is_empty();
    let person_ids_set: HashSet<&str> = criteria.person_ids.iter().map(|s| s.as_str()).collect();

    for (session_id, mode) in &session_modes {
        let (session_name, session_created_at, session_notes) = session_manager
            .get_session(session_id)
            .await
            .map(|info| (info.name, info.created_at, info.notes))
            .unwrap_or((None, chrono::Utc::now(), None));

        // Session notes (always included)
        if let Some(notes) = &session_notes {
            if !notes.is_empty() {
                chunks.push(ContextChunk {
                    kind: "note".to_string(),
                    source_id: session_id.clone(),
                    source_label: session_name.clone(),
                    source_type: "session".to_string(),
                    created_at: session_created_at,
                    segment: None,
                    note: Some(notes.clone()),
                });
            }
        }

        let mut include_transcript = mode == "transcript" || mode == "both";
        let include_summary = mode == "summary" || mode == "both";

        // Summary content
        if include_summary {
            let summary_path = session_manager.session_dir(session_id).join("summary.json");
            let mut summary_found = false;
            if let Ok(summary_str) = std::fs::read_to_string(&summary_path) {
                if let Ok(summary_json) = serde_json::from_str::<serde_json::Value>(&summary_str) {
                    if let Some(content) = summary_json.get("content").and_then(|c| c.as_str()) {
                        chunks.push(ContextChunk {
                            kind: "summary".to_string(),
                            source_id: session_id.clone(),
                            source_label: session_name.clone(),
                            source_type: "session".to_string(),
                            created_at: session_created_at,
                            segment: None,
                            note: Some(content.to_string()),
                        });
                        summary_found = true;
                    }
                }
            }
            // Fall back to transcript if summary was requested but not available
            if !summary_found && mode == "summary" {
                include_transcript = true;
            }
        }

        // Transcript segments
        if include_transcript {
            let transcript = match files_db.get_transcript(session_id).await {
                Some(t) => t,
                None => continue,
            };

            let segments = match transcript.get("segments").and_then(|s| s.as_array()) {
                Some(segs) => segs,
                None => continue,
            };

            for segment in segments {
                if filter_by_person {
                    let segment_person = segment.get("person_id").and_then(|p| p.as_str());
                    match segment_person {
                        Some(pid) if person_ids_set.contains(pid) => {}
                        _ if criteria.session_ids.contains(session_id)
                            || has_tag_match(session_id, &criteria.tags, session_manager).await => {}
                        _ => continue,
                    }
                }

                chunks.push(ContextChunk {
                    kind: "segment".to_string(),
                    source_id: session_id.clone(),
                    source_label: session_name.clone(),
                    source_type: "session".to_string(),
                    created_at: session_created_at,
                    segment: Some(segment.clone()),
                    note: None,
                });
            }
        }
    }

    // Sort: notes first, then summaries, then segments by time
    chunks.sort_by(|a, b| {
        // Order: note=0, summary=1, segment=2
        let kind_order = |k: &str| match k { "note" => 0, "summary" => 1, _ => 2 };
        let a_ord = kind_order(&a.kind);
        let b_ord = kind_order(&b.kind);
        if a_ord != b_ord {
            return a_ord.cmp(&b_ord);
        }
        // Then by created_at
        let cmp = a.created_at.cmp(&b.created_at);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        // Then by segment start time
        let a_start = a.segment.as_ref().and_then(|s| s.get("start")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b_start = b.segment.as_ref().and_then(|s| s.get("start")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        a_start.partial_cmp(&b_start).unwrap_or(std::cmp::Ordering::Equal)
    });

    chunks
}

/// Collect notes for a list of tags. Returns `(tag_name, notes)` pairs.
pub async fn collect_tag_notes(
    tags_manager: &TagsManager,
    tag_names: &[String],
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for name in tag_names {
        if let Some(tag) = tags_manager.get_tag(name).await {
            if let Some(notes) = tag.notes {
                if !notes.trim().is_empty() {
                    result.push((name.clone(), notes));
                }
            }
        }
    }
    result
}

/// Collect notes for people by their IDs. Returns `(person_name, notes)` pairs.
pub async fn collect_person_notes(
    people_manager: &PeopleManager,
    person_ids: &[(String, String)], // (person_id, display_name)
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for (pid, display_name) in person_ids {
        if let Some(person) = people_manager.get_person(pid).await {
            if let Some(notes) = person.notes {
                if !notes.trim().is_empty() {
                    result.push((display_name.clone(), notes));
                }
            }
        }
    }
    result
}

/// Extract unique person IDs and display names from transcript segments.
pub fn extract_person_ids(transcript: &serde_json::Value) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    if let Some(segments) = transcript.get("segments").and_then(|s| s.as_array()) {
        for seg in segments {
            if let Some(pid) = seg.get("person_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                if seen.insert(pid.to_string()) {
                    let name = seg.get("person_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(pid)
                        .to_string();
                    result.push((pid.to_string(), name));
                }
            }
        }
    }
    result
}

async fn has_tag_match(session_id: &str, tags: &[String], session_manager: &SessionManager) -> bool {
    if tags.is_empty() {
        return false;
    }
    for tag in tags {
        let sessions = session_manager.sessions_for_tag(tag).await;
        if sessions.iter().any(|s| s.id == session_id) {
            return true;
        }
    }
    false
}
