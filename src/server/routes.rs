use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, warn, error};

use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::session::config::SessionConfig;
use crate::session::session::SessionInfo;
use crate::settings::SharedSettings;
use crate::understanding::{ExtractionClient, TrackInput};

/// Shared state for routes.
#[derive(Clone)]
pub struct AppState {
    pub session_manager: SessionManager,
    pub people_manager: PeopleManager,
    pub settings: SharedSettings,
}

pub fn session_routes() -> Router<AppState> {
    Router::new()
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}", patch(rename_session))
        .route("/sessions/{id}", delete(delete_session))
        .route("/sessions/{id}/recording/start", post(start_recording))
        .route("/sessions/{id}/recording/stop", post(stop_recording))
        .route("/sessions/{id}/files", get(get_files))
        .route("/sessions/{id}/files/{filename}", get(serve_file))
        .route("/sessions/{id}/transcript", get(get_transcript))
        .route("/sessions/{id}/transcript", delete(delete_transcript))
        .route("/sessions/{id}/attribution", get(get_attribution))
        .route("/sessions/{id}/attribution", post(update_attribution))
        .route("/sessions/{id}/transcribe", post(transcribe_session))
        .route("/sessions/{id}/waveform/{filename}", get(get_waveform))
        .route("/people", get(list_people))
        .route("/people", post(create_person))
        .route("/people/{id}", get(get_person))
        .route("/people/{id}", patch(update_person))
        .route("/people/{id}", delete(delete_person))
        .route("/people/{id}/sessions", get(get_person_sessions))
        .route("/settings", get(get_settings))
        .route("/settings", put(update_settings))
        .route("/config", get(get_config))
}

async fn create_session(
    State(state): State<AppState>,
    Json(config): Json<SessionConfig>,
) -> (StatusCode, Json<SessionInfo>) {
    let info = state.session_manager.create_session(config).await;
    (StatusCode::CREATED, Json(info))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<Value> {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let (sessions, total) = state.session_manager.list_sessions(limit, offset).await;
    Json(json!({
        "sessions": sessions,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionInfo>, (StatusCode, Json<Value>)> {
    state.session_manager
        .get_session(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))))
}

async fn rename_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<SessionInfo>, (StatusCode, Json<Value>)> {
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        state.session_manager
            .rename_session(&id, name.to_string())
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))?;
    }
    if let Some(lang) = body.get("language").and_then(|v| v.as_str()) {
        state.session_manager
            .update_session_language(&id, lang.to_string())
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))?;
    }
    state.session_manager
        .get_session(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    state.session_manager
        .delete_session(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
}

async fn start_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state.session_manager
        .start_recording(&id)
        .await
        .map(|files| Json(json!({"status": "recording", "files": files})))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))
}

async fn stop_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state.session_manager
        .stop_recording(&id)
        .await
        .map(|files| Json(json!({"status": "stopped", "files": files})))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))
}

async fn get_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<Value>)> {
    state.session_manager
        .get_files(&id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
}

async fn serve_file(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    // Verify session exists and file belongs to it
    let files = state.session_manager
        .get_files(&id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))?;

    if !files.contains(&filename) {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "file not found"}))));
    }

    // Sanitize filename to prevent path traversal
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .ok_or((StatusCode::BAD_REQUEST, Json(json!({"error": "invalid filename"}))))?;

    let file_path = state.session_manager.session_dir(&id).join(safe_name);

    // Use tower-http ServeFile for proper Content-Length, Accept-Ranges, and range requests
    let serve = tower_http::services::ServeFile::new(&file_path);
    let result = tower::ServiceExt::oneshot(serve, req)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{}", e)}))))?;
    Ok(result)
}

async fn get_waveform(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Verify session exists and file belongs to it
    let files = state.session_manager
        .get_files(&id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))?;

    if !files.contains(&filename) {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "file not found"}))));
    }

    let session_dir = state.session_manager.session_dir(&id);

    // Generate waveform on a blocking thread (decoding is CPU-intensive)
    let waveform = tokio::task::spawn_blocking(move || {
        crate::waveform::get_or_generate_waveform(&session_dir, &filename)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("join: {}", e)}))))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    Ok(Json(serde_json::to_value(waveform).unwrap()))
}

async fn get_config() -> Json<Value> {
    let sources = crate::audio::discover_sources();
    Json(json!({
        "sources": sources,
        "fields": {
            "language": {
                "type": "select",
                "default": "en",
                "label": "Language",
                "description": "Language for transcription",
                "options": [
                    { "value": "en", "label": "English" },
                    { "value": "zh", "label": "Chinese" },
                    { "value": "ja", "label": "Japanese" },
                    { "value": "ko", "label": "Korean" },
                    { "value": "es", "label": "Spanish" },
                    { "value": "fr", "label": "French" },
                    { "value": "de", "label": "German" },
                    { "value": "pt", "label": "Portuguese" },
                    { "value": "ru", "label": "Russian" },
                    { "value": "ar", "label": "Arabic" },
                ],
            },
            "format": {
                "type": "select",
                "default": "opus",
                "label": "Format",
                "description": "Audio file format",
                "options": [
                    { "value": "wav", "label": "WAV", "title": "Lossless, lowest CPU (~2%), but large files" },
                    { "value": "mp3", "label": "MP3", "title": "Lossy, widely compatible, ~6% CPU" },
                    { "value": "opus", "label": "Opus", "title": "Designed for speech, smallest files, ~4% CPU" },
                ],
            },
            "raw_sample_rate": {
                "type": "select",
                "default": 48000,
                "label": "Raw Sample Rate",
                "description": "Recording sample rate — higher means better quality but larger files",
                "advanced": true,
                "options": [
                    { "value": 16000, "label": "16000 Hz" },
                    { "value": 22050, "label": "22050 Hz" },
                    { "value": 44100, "label": "44100 Hz" },
                    { "value": 48000, "label": "48000 Hz" },
                ],
            },
            "mp3_bitrate": {
                "type": "select",
                "default": 64,
                "label": "MP3 Bitrate",
                "description": "MP3 encoder bitrate — higher means better quality and larger files",
                "advanced": true,
                "show_when": { "field": "format", "value": "mp3" },
                "config_path": "mp3.bitrate_kbps",
                "options": [
                    { "value": 32, "label": "32 kbps" },
                    { "value": 48, "label": "48 kbps" },
                    { "value": 64, "label": "64 kbps" },
                    { "value": 96, "label": "96 kbps" },
                    { "value": 128, "label": "128 kbps" },
                    { "value": 192, "label": "192 kbps" },
                    { "value": 256, "label": "256 kbps" },
                    { "value": 320, "label": "320 kbps" },
                ],
            },
            "mp3_sample_rate": {
                "type": "select",
                "default": 16000,
                "label": "MP3 Sample Rate",
                "description": "MP3 encoder output sample rate — can differ from recording rate; the encoder will resample",
                "advanced": true,
                "show_when": { "field": "format", "value": "mp3" },
                "config_path": "mp3.sample_rate",
                "options": [
                    { "value": 8000, "label": "8000 Hz" },
                    { "value": 16000, "label": "16000 Hz" },
                    { "value": 22050, "label": "22050 Hz" },
                    { "value": 44100, "label": "44100 Hz" },
                    { "value": 48000, "label": "48000 Hz" },
                ],
            },
            "opus_bitrate": {
                "type": "select",
                "default": 32,
                "label": "Opus Bitrate",
                "description": "Opus encoder target bitrate — 24-32 kbps is transparent for speech",
                "advanced": true,
                "show_when": { "field": "format", "value": "opus" },
                "config_path": "opus.bitrate_kbps",
                "options": [
                    { "value": 16, "label": "16 kbps" },
                    { "value": 24, "label": "24 kbps" },
                    { "value": 32, "label": "32 kbps" },
                    { "value": 48, "label": "48 kbps" },
                    { "value": 64, "label": "64 kbps" },
                    { "value": 96, "label": "96 kbps" },
                    { "value": 128, "label": "128 kbps" },
                ],
            },
            "opus_complexity": {
                "type": "select",
                "default": 5,
                "label": "Opus Complexity",
                "description": "Encoder complexity (0-10) — higher is better quality but more CPU",
                "advanced": true,
                "show_when": { "field": "format", "value": "opus" },
                "config_path": "opus.complexity",
                "options": [
                    { "value": 0, "label": "0 (fastest)" },
                    { "value": 3, "label": "3" },
                    { "value": 5, "label": "5 (default)" },
                    { "value": 7, "label": "7" },
                    { "value": 10, "label": "10 (best)" },
                ],
            },
            "summarization_instruction": {
                "type": "textarea",
                "default": "",
                "label": "Summary Prompt",
                "description": "Custom instruction for meeting summarization",
                "advanced": true,
                "placeholder": "Custom summarization...",
            },
        },
    }))
}

// --- Transcript routes ---

async fn get_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session_dir = state.session_manager.session_dir(&id);
    let path = session_dir.join("transcript.json");
    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "transcript not found"}))));
    }
    let json_str = std::fs::read_to_string(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    let value: Value = serde_json::from_str(&json_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    Ok(Json(value))
}

async fn delete_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let session_dir = state.session_manager.session_dir(&id);
    for filename in &["transcript.json", "extraction_raw.json"] {
        let path = session_dir.join(filename);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    state.session_manager.set_processing_state(&id, None).await;
    Ok(StatusCode::NO_CONTENT)
}

// --- Attribution routes ---

async fn get_attribution(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session_dir = state.session_manager.session_dir(&id);
    let path = session_dir.join("transcript.json");
    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "transcript not found"}))));
    }
    let json_str = std::fs::read_to_string(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    let value: Value = serde_json::from_str(&json_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    // Return just the speaker_embeddings section with attribution info
    let embs = value.get("speaker_embeddings").cloned().unwrap_or(json!({}));
    Ok(Json(embs))
}

#[derive(Deserialize)]
struct AttributionAction {
    speaker: String,
    #[serde(default)]
    person_id: Option<String>,
    action: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct AttributionRequest {
    attributions: Vec<AttributionAction>,
}

async fn update_attribution(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AttributionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session_dir = state.session_manager.session_dir(&id);
    let transcript_path = session_dir.join("transcript.json");
    if !transcript_path.exists() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "transcript not found"}))));
    }

    // Read current transcript
    let json_str = std::fs::read_to_string(&transcript_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    let mut transcript: Value = serde_json::from_str(&json_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;

    for action in &body.attributions {
        // Get the speaker's embedding from the transcript
        let embedding: Vec<f64> = transcript
            .get("speaker_embeddings")
            .and_then(|embs| embs.get(&action.speaker))
            .and_then(|e| e.get("embedding"))
            .and_then(|e| serde_json::from_value(e.clone()).ok())
            .unwrap_or_default();

        match action.action.as_str() {
            "confirm" => {
                // Save embedding to the confirmed person
                if let Some(pid) = &action.person_id {
                    let _ = state.people_manager
                        .add_embedding(pid, embedding, &id, None)
                        .await;
                }
            }
            "correct" => {
                // Reassign to a different person and save embedding
                if let Some(pid) = &action.person_id {
                    let _ = state.people_manager
                        .add_embedding(pid, embedding, &id, None)
                        .await;
                    // Update the transcript
                    let person = state.people_manager.get_person(pid).await;
                    if let Some(embs) = transcript.get_mut("speaker_embeddings") {
                        if let Some(entry) = embs.get_mut(&action.speaker) {
                            entry["person_id"] = json!(pid);
                            entry["person_name"] = json!(person.as_ref().map(|p| &p.name));
                            entry["confidence"] = json!(1.0);
                        }
                    }
                    update_segment_speakers(&mut transcript, &action.speaker, pid, person.as_ref().map(|p| p.name.as_str()));
                }
            }
            "create" => {
                // Create a new person from this speaker
                if let Some(name) = &action.name {
                    match state.people_manager
                        .create_person_from_speaker(name.clone(), embedding, &id)
                        .await
                    {
                        Ok(person) => {
                            if let Some(embs) = transcript.get_mut("speaker_embeddings") {
                                if let Some(entry) = embs.get_mut(&action.speaker) {
                                    entry["person_id"] = json!(&person.id);
                                    entry["person_name"] = json!(&person.name);
                                    entry["confidence"] = json!(1.0);
                                }
                            }
                            update_segment_speakers(&mut transcript, &action.speaker, &person.id, Some(&person.name));
                        }
                        Err(e) => {
                            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))));
                        }
                    }
                }
            }
            "reject" => {
                // Remove the attribution, don't save embedding
                if let Some(embs) = transcript.get_mut("speaker_embeddings") {
                    if let Some(entry) = embs.get_mut(&action.speaker) {
                        entry["person_id"] = json!(null);
                        entry["person_name"] = json!(null);
                        entry["confidence"] = json!(0.0);
                    }
                }
                update_segment_speakers(&mut transcript, &action.speaker, "", None);
            }
            _ => {}
        }
    }

    // Write updated transcript
    let updated = serde_json::to_string_pretty(&transcript)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    std::fs::write(&transcript_path, updated)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;

    Ok(Json(json!({"status": "ok"})))
}

/// Update person_id and person_name in all transcript segments matching the speaker.
fn update_segment_speakers(transcript: &mut Value, speaker: &str, person_id: &str, person_name: Option<&str>) {
    if let Some(segments) = transcript.get_mut("segments").and_then(|s| s.as_array_mut()) {
        for seg in segments {
            if seg.get("speaker").and_then(|s| s.as_str()) == Some(speaker) {
                if person_id.is_empty() {
                    seg["person_id"] = json!(null);
                    seg["person_name"] = json!(null);
                } else {
                    seg["person_id"] = json!(person_id);
                    seg["person_name"] = json!(person_name);
                }
            }
        }
    }
}

// --- People routes ---

async fn list_people(
    State(state): State<AppState>,
) -> Json<Value> {
    let people = state.people_manager.list_people().await;
    Json(json!({ "people": people }))
}

#[derive(Deserialize)]
struct CreatePersonRequest {
    name: String,
    #[serde(default)]
    notes: Option<String>,
}

async fn create_person(
    State(state): State<AppState>,
    Json(body): Json<CreatePersonRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    state
        .people_manager
        .create_person(body.name, body.notes)
        .await
        .map(|p| (StatusCode::CREATED, Json(serde_json::to_value(p).unwrap())))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))
}

async fn get_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .people_manager
        .get_person(&id)
        .await
        .map(|p| Json(serde_json::to_value(p).unwrap()))
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "person not found"}))))
}

#[derive(Deserialize)]
struct UpdatePersonRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    notes: Option<Option<String>>,
}

async fn update_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdatePersonRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .people_manager
        .update_person(&id, body.name, body.notes)
        .await
        .map(|p| Json(serde_json::to_value(p).unwrap()))
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
}

async fn delete_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    state
        .people_manager
        .delete_person(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
}

async fn get_person_sessions(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Json<Value> {
    let (sessions, _) = state.session_manager.list_sessions(1000, 0).await;

    // Collect candidates: (session_info, transcript_path) for sessions with transcripts
    let candidates: Vec<_> = sessions.iter()
        .filter(|s| s.transcript_available)
        .map(|s| {
            let path = state.session_manager.session_dir(&s.id).join("transcript.json");
            (s.clone(), path)
        })
        .collect();

    // Do all file I/O on a blocking thread
    let result = tokio::task::spawn_blocking(move || {
        let mut matched: Vec<Value> = Vec::new();
        for (session, path) in &candidates {
            // Quick string search before full JSON parse — skip files that
            // don't even contain the person_id string.
            let content = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if !content.contains(&person_id) { continue; }

            let transcript: Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let found = transcript.get("speaker_embeddings")
                .and_then(|e| e.as_object())
                .map(|embs| embs.values().any(|v| {
                    v.get("person_id").and_then(|p| p.as_str()) == Some(&person_id)
                }))
                .unwrap_or(false);

            if found {
                matched.push(json!({
                    "id": session.id,
                    "name": session.name,
                    "state": session.state,
                    "created_at": session.created_at,
                    "updated_at": session.updated_at,
                    "duration_secs": session.duration_secs,
                }));
            }
        }
        matched.sort_by(|a, b| {
            let a_t = a.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
            let b_t = b.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
            b_t.cmp(a_t)
        });
        matched
    }).await.unwrap_or_default();

    Json(json!({ "sessions": result }))
}

// --- Settings routes ---

async fn get_settings(
    State(state): State<AppState>,
) -> Json<Value> {
    let settings = state.settings.read().await;
    Json(settings.to_masked_json())
}

async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut settings = state.settings.write().await;
    settings
        .merge_and_save(&body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    info!("Settings updated");
    Ok(Json(settings.to_masked_json()))
}

// --- Transcribe route ---

async fn transcribe_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // 1. Check settings
    let settings = state.settings.read().await;
    if !settings.is_extraction_configured() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Audio extraction not configured. Set audio_extraction_url and audio_extraction_api_key in settings."})),
        ));
    }
    let extraction_url = settings.audio_extraction_url.clone().unwrap();
    let extraction_key = settings.audio_extraction_api_key.clone().unwrap();
    let file_drop_url = settings.file_drop_url.clone();
    let file_drop_api_key = settings.file_drop_api_key.clone();
    let diarize = settings.diarize;
    let people_recognition = settings.people_recognition;
    let match_threshold = settings.speaker_match_threshold;
    drop(settings);

    // 2. Check session
    let (session_dir, language, source_meta) = state
        .session_manager
        .get_session_extraction_info(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;

    // 3. Prevent double-submit
    if let Some(info) = state.session_manager.get_session(&id).await {
        if info.processing_state.is_some() {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({"error": "Transcription already in progress"})),
            ));
        }
    }

    // 4. Set state and return 202
    state
        .session_manager
        .set_processing_state(&id, Some("starting".to_string()))
        .await;

    // 5. Spawn background task
    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let session_id = id.clone();

    tokio::spawn(async move {
        let result = run_transcription_pipeline(
            &session_id,
            &session_dir,
            &language,
            &source_meta,
            &extraction_url,
            &extraction_key,
            &file_drop_url,
            &file_drop_api_key,
            diarize,
            people_recognition,
            match_threshold,
            &session_manager,
            &people_manager,
        )
        .await;

        match result {
            Ok(unconfirmed) => {
                session_manager
                    .set_processing_state(&session_id, None)
                    .await;
                session_manager.emit_transcription_completed(&session_id, unconfirmed);
                info!("Transcription completed for session {}", session_id);
            }
            Err(e) => {
                error!("Transcription failed for session {}: {}", session_id, e);
                session_manager
                    .set_processing_state(&session_id, None)
                    .await;
                session_manager.emit_transcription_failed(&session_id, &e);
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(json!({"status": "processing"}))))
}

/// Run the full transcription pipeline in a background task.
async fn run_transcription_pipeline(
    session_id: &str,
    session_dir: &std::path::Path,
    language: &str,
    source_meta: &[crate::session::session::SourceMetadata],
    extraction_url: &str,
    extraction_key: &str,
    file_drop_url: &str,
    file_drop_api_key: &str,
    diarize: bool,
    people_recognition: bool,
    match_threshold: f64,
    session_manager: &SessionManager,
    people_manager: &PeopleManager,
) -> Result<u32, String> {
    // Step 1: Upload audio files to file-drop
    session_manager
        .set_processing_state(session_id, Some("uploading".to_string()))
        .await;

    let http = reqwest::Client::new();
    let mut tracks: Vec<TrackInput> = Vec::new();
    let mut drop_urls: Vec<String> = Vec::new(); // for cleanup on error

    for meta in source_meta.iter().filter(|m| !m.filename.is_empty()) {
        let file_path = session_dir.join(&meta.filename);
        if !file_path.exists() {
            warn!("[{}] Audio file not found: {}", session_id, file_path.display());
            continue;
        }

        info!("[{}] Uploading {} to file-drop...", session_id, meta.filename);
        let bytes = std::fs::read(&file_path)
            .map_err(|e| format!("Failed to read {}: {e}", meta.filename))?;
        let file_size = bytes.len();

        let upload_url = format!(
            "{}/upload?filename={}", file_drop_url, meta.filename
        );
        let resp = http
            .post(&upload_url)
            .header("Authorization", format!("Bearer {}", file_drop_api_key))
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("Failed to upload {} to file-drop: {e}", meta.filename))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("file-drop upload failed for {} ({}): {}", meta.filename, status, body));
        }

        let upload_result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse file-drop response: {e}"))?;

        let download_path = upload_result["url"]
            .as_str()
            .ok_or_else(|| "file-drop response missing 'url' field".to_string())?;
        let download_url = format!("{}{}", file_drop_url, download_path);
        drop_urls.push(download_url.clone());

        let source_type = match meta.source_type {
            crate::audio::source::SourceType::Mic => "mic",
            crate::audio::source::SourceType::SystemMix => "system_mix",
            _ => "unknown",
        };

        info!(
            "[{}] Uploaded {} ({} bytes) -> {}",
            session_id, meta.filename, file_size, download_url
        );

        tracks.push(TrackInput {
            audio_url: download_url,
            track_name: meta.filename.split('.').next().unwrap_or(&meta.filename).to_string(),
            source_type: source_type.to_string(),
            channels: meta.channels,
        });
    }

    if tracks.is_empty() {
        return Err("No audio tracks to transcribe".to_string());
    }

    info!("[{}] Submitting {} tracks to RunPod", session_id, tracks.len());

    // Step 3: Submit and poll
    session_manager
        .set_processing_state(session_id, Some("extracting".to_string()))
        .await;

    let client = ExtractionClient::new(extraction_url.to_string(), extraction_key.to_string());

    let job_id = client
        .submit_job(tracks, language, diarize, None, None)
        .await?;

    info!("[{}] RunPod job submitted: {}", session_id, job_id);

    // Poll with progress events
    let mut delay = std::time::Duration::from_secs(2);
    let max_delay = std::time::Duration::from_secs(15);
    let timeout = std::time::Duration::from_secs(600);
    let start = std::time::Instant::now();

    let output = loop {
        if start.elapsed() > timeout {
            return Err("Extraction timed out after 10 minutes".to_string());
        }

        tokio::time::sleep(delay).await;

        match client.poll_status(&job_id).await? {
            Some(output) => break output,
            None => {
                session_manager.emit_transcription_progress(session_id, "extracting");
                delay = (delay * 2).min(max_delay);
            }
        }
    };

    info!("[{}] Extraction complete, {} tracks returned", session_id, output.tracks.len());

    // Save raw extraction output
    let raw_path = session_dir.join("extraction_raw.json");
    let raw_json = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("Failed to serialize raw output: {e}"))?;
    std::fs::write(&raw_path, raw_json)
        .map_err(|e| format!("Failed to write extraction_raw.json: {e}"))?;

    // Step 4: Merge segments from all tracks, sorted by start time
    let mut all_segments: Vec<Value> = Vec::new();
    let mut all_embeddings: HashMap<String, Vec<f64>> = HashMap::new();

    for (track_name, track_result) in &output.tracks {
        for seg in &track_result.segments {
            let mut seg_json = serde_json::to_value(seg)
                .map_err(|e| format!("Failed to serialize segment: {e}"))?;
            seg_json["track"] = json!(track_name);
            seg_json["source_type"] = json!(&track_result.source_type);
            all_segments.push(seg_json);
        }
        for (speaker, emb) in &track_result.speaker_embeddings {
            all_embeddings.insert(speaker.clone(), emb.clone());
        }
    }

    // Sort by start time
    all_segments.sort_by(|a, b| {
        let a_start = a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b_start = b.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        a_start.partial_cmp(&b_start).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Step 5: People matching
    session_manager
        .set_processing_state(session_id, Some("matching".to_string()))
        .await;

    let mut speaker_info: HashMap<String, Value> = HashMap::new();
    let mut unconfirmed: u32 = 0;

    if people_recognition && !all_embeddings.is_empty() {
        info!("[{}] Matching {} speakers against People library", session_id, all_embeddings.len());

        let embeddings_f64: HashMap<String, Vec<f64>> = all_embeddings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let attributions = people_manager
            .match_speakers(&embeddings_f64, match_threshold)
            .await;

        for attr in &attributions {
            if attr.person_id.is_none() {
                unconfirmed += 1;
            }
            speaker_info.insert(attr.speaker.clone(), json!({
                "embedding": attr.embedding,
                "person_id": attr.person_id,
                "person_name": attr.person_name,
                "confidence": attr.confidence,
            }));

            // Update segment attributions
            for seg in &mut all_segments {
                if seg.get("speaker").and_then(|s| s.as_str()) == Some(&attr.speaker) {
                    seg["person_id"] = json!(attr.person_id);
                    seg["person_name"] = json!(attr.person_name);
                    seg["attribution_confidence"] = json!(attr.confidence);
                }
            }
        }

        info!("[{}] Matched speakers: {} confirmed, {} unconfirmed",
              session_id, attributions.len() - unconfirmed as usize, unconfirmed);
    } else {
        // No people recognition — just store raw embeddings
        for (speaker, emb) in &all_embeddings {
            unconfirmed += 1;
            speaker_info.insert(speaker.clone(), json!({
                "embedding": emb,
                "person_id": null,
                "person_name": null,
                "confidence": 0.0,
            }));
        }
    }

    // Step 6: Write enriched transcript.json
    let transcript = json!({
        "language": output.language,
        "model": output.model,
        "segments": all_segments,
        "speaker_embeddings": speaker_info,
    });

    let transcript_path = session_dir.join("transcript.json");
    let transcript_json = serde_json::to_string_pretty(&transcript)
        .map_err(|e| format!("Failed to serialize transcript: {e}"))?;
    std::fs::write(&transcript_path, transcript_json)
        .map_err(|e| format!("Failed to write transcript.json: {e}"))?;

    info!("[{}] Transcript saved: {} segments, {} speakers",
          session_id, all_segments.len(), speaker_info.len());

    Ok(unconfirmed)
}

