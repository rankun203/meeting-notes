use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::session::config::SessionConfig;
use crate::session::session::SessionInfo;

/// Shared state for routes that need both managers.
#[derive(Clone)]
pub struct AppState {
    pub session_manager: SessionManager,
    pub people_manager: PeopleManager,
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
        .route("/people", get(list_people))
        .route("/people", post(create_person))
        .route("/people/{id}", get(get_person))
        .route("/people/{id}", patch(update_person))
        .route("/people/{id}", delete(delete_person))
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
    let name = body.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    state.session_manager
        .rename_session(&id, name)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
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

