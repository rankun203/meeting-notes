use std::collections::HashMap;
use std::convert::Infallible;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    response::sse::{Event, Sse},
    routing::{delete, get, patch, post, put},
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, warn, error};

use crate::chat::types::{ContextCriteria, Mention, Message};
use crate::filesdb::FilesDb;
use crate::llm::claude_code::ClaudeCodeRunner;
use crate::llm::client::LlmClient;
use crate::llm::secrets::SharedSecrets;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::session::config::SessionConfig;
use crate::session::session::SessionInfo;
use crate::settings::SharedSettings;
use crate::tags::TagsManager;
use crate::understanding::{ExtractionClient, ExtractionOutput, TrackInput};

use crate::services;

// `AppState` now lives in `services::state`. Re-exported here so existing
// callers inside this file (and in `server::mod`) keep compiling unchanged
// while the rest of the service-layer refactor lands incrementally.
pub use crate::services::AppState;

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
        .route("/sessions/{id}/summarize", post(summarize_session))
        .route("/sessions/{id}/summary", get(get_summary))
        .route("/sessions/{id}/summary", patch(update_summary))
        .route("/sessions/{id}/summary", delete(delete_summary))
        .route("/sessions/{id}/todos", get(get_session_todos))
        .route("/sessions/{id}/todos/{idx}", patch(toggle_todo))
        .route("/sessions/{id}/waveform/{filename}", get(get_waveform))
        .route("/people", get(list_people))
        .route("/people", post(create_person))
        .route("/people/{id}", get(get_person))
        .route("/people/{id}", patch(update_person))
        .route("/people/{id}", delete(delete_person))
        .route("/people/{id}/sessions", get(get_person_sessions))
        .route("/people/{id}/todos", get(get_person_todos))
        .route("/sessions/{id}/tags", put(set_session_tags))
        .route("/tags", get(list_tags))
        .route("/tags", post(create_tag))
        .route("/tags/{name}", get(get_tag_sessions))
        .route("/tags/{name}", patch(update_tag))
        .route("/tags/{name}", delete(delete_tag))
        .route("/settings", get(get_settings))
        .route("/settings", put(update_settings))
        .route("/config", get(get_config))
}

// ---- Session handlers (now thin shims over `services::sessions`) ----

async fn create_session(
    State(state): State<AppState>,
    Json(config): Json<SessionConfig>,
) -> Result<(StatusCode, Json<SessionInfo>), services::ServiceError> {
    let info = services::sessions::create_session(&state, config).await?;
    Ok((StatusCode::CREATED, Json(info)))
}

async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<services::sessions::ListParams>,
) -> Result<Json<services::sessions::SessionListPage>, services::ServiceError> {
    let page = services::sessions::list_sessions(&state, params).await?;
    Ok(Json(page))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionInfo>, services::ServiceError> {
    let info = services::sessions::get_session(&state, &id).await?;
    Ok(Json(info))
}

async fn rename_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<services::sessions::UpdateSessionInput>,
) -> Result<Json<SessionInfo>, services::ServiceError> {
    let info = services::sessions::update_session(&state, &id, input).await?;
    Ok(Json(info))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, services::ServiceError> {
    services::sessions::delete_session(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<services::sessions::RecordingStarted>, services::ServiceError> {
    let out = services::sessions::start_recording(&state, &id).await?;
    Ok(Json(out))
}

async fn stop_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<services::sessions::RecordingStopped>, services::ServiceError> {
    // Clones captured so the auto-transcribe closure can be spawned onto
    // a background tokio task that outlives the HTTP handler.
    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let files_db = state.files_db.clone();
    let settings_clone = state.settings.clone();
    let llm_secrets = state.llm_secrets.clone();
    let tags_mgr = state.tags_manager.clone();

    let out = services::sessions::stop_recording(&state, &id, move |req| {
        tokio::spawn(async move {
            let result = run_transcription_pipeline(
                &req.session_id,
                &req.session_dir,
                &req.language,
                &req.source_meta,
                &req.extraction_url,
                &req.extraction_key,
                &req.file_drop_url,
                &req.file_drop_api_key,
                req.diarize,
                req.people_recognition,
                req.match_threshold,
                &session_manager,
                &people_manager,
                &files_db,
            )
            .await;

            match result {
                Ok(unconfirmed) => {
                    session_manager
                        .set_processing_state(&req.session_id, None)
                        .await;
                    session_manager
                        .emit_transcription_completed(&req.session_id, unconfirmed);
                    services::sessions::log_auto_transcribe_completed(&req.session_id);

                    maybe_auto_summarize(
                        &req.session_id,
                        &session_manager,
                        &settings_clone,
                        &llm_secrets,
                        &tags_mgr,
                        &people_manager,
                        &files_db,
                    )
                    .await;
                }
                Err(e) => {
                    services::sessions::log_auto_transcribe_failed(&req.session_id, &e);
                    session_manager
                        .set_processing_state(&req.session_id, None)
                        .await;
                    session_manager
                        .emit_transcription_failed(&req.session_id, &e);
                }
            }
        });
    })
    .await?;

    Ok(Json(out))
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
                    { "value": "zh-cn", "label": "Chinese (Simplified)" },
                    { "value": "zh-tw", "label": "Chinese (Traditional)" },
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
        },
    }))
}

// --- Transcript routes ---

async fn get_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.files_db.get_transcript(&id).await {
        Some(data) => Ok(Json(data)),
        None => Err((StatusCode::NOT_FOUND, Json(json!({"error": "transcript not found"})))),
    }
}

async fn delete_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    // Remove from cache + index
    state.files_db.remove_transcript(&id).await;

    // Remove files from disk
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
    match state.files_db.get_transcript(&id).await {
        Some(data) => {
            let embs = data.get("speaker_embeddings").cloned().unwrap_or(json!({}));
            Ok(Json(embs))
        }
        None => Err((StatusCode::NOT_FOUND, Json(json!({"error": "transcript not found"})))),
    }
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
    // Read transcript from cache
    let mut transcript = state.files_db.get_transcript(&id).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "transcript not found"}))))?;

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

    // Write through cache (updates memory + disk + indexes)
    state.files_db.put_transcript(&id, transcript).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

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
    let result = state
        .people_manager
        .create_person(body.name, body.notes)
        .await
        .map(|p| (StatusCode::CREATED, Json(serde_json::to_value(p).unwrap())))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))));
    if result.is_ok() {
        state.refresh_people_index();
    }
    result
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
    #[serde(default)]
    starred: Option<bool>,
}

async fn update_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdatePersonRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let result = state
        .people_manager
        .update_person(&id, body.name, body.notes, body.starred)
        .await
        .map(|p| Json(serde_json::to_value(p).unwrap()))
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))));
    if result.is_ok() {
        state.refresh_people_index();
    }
    result
}

async fn delete_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let result = state
        .people_manager
        .delete_person(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))));
    if result.is_ok() {
        state.refresh_people_index();
    }
    result
}

async fn get_person_sessions(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Json<Value> {
    // Instant index lookup — no file I/O
    let session_ids = state.files_db.get_person_session_ids(&person_id).await;

    let mut result: Vec<Value> = Vec::new();
    for sid in &session_ids {
        if let Some(info) = state.session_manager.get_session(sid).await {
            // Find which speakers in this session matched this person
            let mut matched_speakers: Vec<Value> = Vec::new();
            if let Some(transcript) = state.files_db.get_transcript(sid).await {
                if let Some(embs) = transcript.get("speaker_embeddings").and_then(|v| v.as_object()) {
                    for (speaker_key, entry) in embs {
                        if entry.get("person_id").and_then(|v| v.as_str()) == Some(&person_id) {
                            matched_speakers.push(json!({
                                "speaker": speaker_key,
                                "confidence": entry.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            }));
                        }
                    }
                }
            }

            result.push(json!({
                "id": info.id,
                "name": info.name,
                "state": info.state,
                "created_at": info.created_at,
                "updated_at": info.updated_at,
                "duration_secs": info.duration_secs,
                "matched_speakers": matched_speakers,
            }));
        }
    }

    result.sort_by(|a, b| {
        let a_t = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        let b_t = b.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        b_t.cmp(a_t)
    });

    Json(json!({ "sessions": result }))
}

// --- Settings routes ---

async fn get_settings(
    State(state): State<AppState>,
) -> Json<Value> {
    let settings = state.settings.read().await;
    let mut result = settings.to_masked_json();
    // Add llm_api_key_set indicator for the current host (never expose the actual key)
    let secrets = state.llm_secrets.read().await;
    result.as_object_mut().unwrap().insert(
        "llm_api_key_set".to_string(),
        json!(secrets.has_api_key(&settings.llm_host)),
    );
    Json(result)
}

async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Determine the host to associate the key with.
    // If llm_host is being updated in this request, use the new value;
    // otherwise fall back to the current setting.
    let host_for_key = body.get("llm_host")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Route llm_api_key to the secrets file, keyed by host provider
    if let Some(v) = body.get("llm_api_key") {
        let key = v.as_str().map(|s| s.to_string());
        let host = match &host_for_key {
            Some(h) => h.clone(),
            None => state.settings.read().await.llm_host.clone(),
        };
        let mut secrets = state.llm_secrets.write().await;
        secrets.set_api_key(&host, key)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
        info!("LLM API key updated for host");
    }

    let mut settings = state.settings.write().await;
    settings
        .merge_and_save(&body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    info!("Settings updated");

    let mut result = settings.to_masked_json();
    let secrets = state.llm_secrets.read().await;
    result.as_object_mut().unwrap().insert(
        "llm_api_key_set".to_string(),
        json!(secrets.has_api_key(&settings.llm_host)),
    );
    Ok(Json(result))
}

// --- Tag routes ---

async fn list_tags(
    State(state): State<AppState>,
) -> Json<Value> {
    let tags = state.tags_manager.list_tags().await;
    let counts = state.session_manager.tag_session_counts().await;
    let list: Vec<Value> = tags.iter().map(|t| {
        json!({
            "name": t.name,
            "hidden": t.hidden,
            "notes": t.notes,
            "session_count": counts.get(&t.name).copied().unwrap_or(0),
        })
    }).collect();
    Json(json!({ "tags": list }))
}

async fn create_tag(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let tag = state.tags_manager.create_tag(name).await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;
    Ok(Json(json!(tag)))
}

async fn update_tag(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let new_name = body.get("name").and_then(|v| v.as_str());
    let hidden = body.get("hidden").and_then(|v| v.as_bool());
    let notes = if body.get("notes").is_some() {
        Some(body.get("notes").unwrap().as_str().map(|s| s.to_string()))
    } else { None };
    let (tag, old_name) = state.tags_manager.update_tag(&name, new_name, hidden, notes).await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;
    // If renamed, cascade to all sessions
    if let Some(old) = old_name {
        state.session_manager.rename_tag_in_all_sessions(&old, &tag.name).await;
    }
    Ok(Json(json!(tag)))
}

async fn delete_tag(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    state.tags_manager.delete_tag(&name).await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))?;
    // Cascade: remove tag from all sessions
    state.session_manager.remove_tag_from_all_sessions(&name).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_tag_sessions(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.tags_manager.tag_exists(&name).await {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "tag not found"}))));
    }
    let sessions = state.session_manager.sessions_for_tag(&name).await;
    Ok(Json(json!({ "sessions": sessions })))
}

async fn set_session_tags(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let tags: Vec<String> = body.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // Validate all tags exist
    for tag in &tags {
        if !state.tags_manager.tag_exists(tag).await {
            return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("tag '{}' does not exist", tag)}))));
        }
    }

    let info = state.session_manager.update_session_tags(&id, tags).await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))?;
    state.refresh_recordings_index();
    Ok(Json(serde_json::to_value(info).unwrap()))
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
    let files_db = state.files_db.clone();
    let settings_clone = state.settings.clone();
    let llm_secrets = state.llm_secrets.clone();
    let tags_mgr = state.tags_manager.clone();
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
            &files_db,
        )
        .await;

        match result {
            Ok(unconfirmed) => {
                session_manager
                    .set_processing_state(&session_id, None)
                    .await;
                session_manager.emit_transcription_completed(&session_id, unconfirmed);
                info!("Transcription completed for session {}", session_id);

                // Auto-summarize if enabled
                maybe_auto_summarize(
                    &session_id, &session_manager,
                    &settings_clone, &llm_secrets, &tags_mgr, &people_manager,
                    &files_db,
                ).await;
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
    files_db: &FilesDb,
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

    // Persist job info so it can be resumed if daemon restarts
    session_manager.set_audio_extraction(session_id, Some(
        crate::session::session::AudioExtractionJob {
            job_id: job_id.clone(),
            status: "in_progress".to_string(),
            submitted_at: Some(chrono::Utc::now()),
            extraction_url: Some(extraction_url.to_string()),
        }
    )).await;

    // Poll until completion — no timeout, keep checking forever
    let output = poll_extraction_job(&client, &job_id, session_id, session_manager).await?;

    info!("[{}] Extraction complete, {} tracks returned", session_id, output.tracks.len());

    // Process the extraction output (merge, match speakers, write transcript)
    let result = process_extraction_output(
        session_id, session_dir, source_meta,
        output, people_recognition, match_threshold,
        session_manager, people_manager, files_db,
    ).await;

    result
}

/// Process extraction output: save raw, merge segments, match speakers, write transcript.
/// Used by both the initial pipeline and the resume-on-restart path.
async fn process_extraction_output(
    session_id: &str,
    session_dir: &std::path::Path,
    _source_meta: &[crate::session::session::SourceMetadata],
    output: ExtractionOutput,
    people_recognition: bool,
    match_threshold: f64,
    session_manager: &SessionManager,
    people_manager: &PeopleManager,
    files_db: &FilesDb,
) -> Result<u32, String> {
    // Save raw extraction output
    let raw_path = session_dir.join("extraction_raw.json");
    let raw_json = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("Failed to serialize raw output: {e}"))?;
    std::fs::write(&raw_path, raw_json)
        .map_err(|e| format!("Failed to write extraction_raw.json: {e}"))?;

    // Merge segments from all tracks, sorted by start time
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

    all_segments.sort_by(|a, b| {
        let a_start = a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b_start = b.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        a_start.partial_cmp(&b_start).unwrap_or(std::cmp::Ordering::Equal)
    });

    // People matching
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

    // Write enriched transcript via FilesDb
    let transcript = json!({
        "language": output.language,
        "model": output.model,
        "segments": all_segments,
        "speaker_embeddings": speaker_info,
    });

    files_db.put_transcript(session_id, transcript).await?;

    // Clear extraction job from metadata
    session_manager.set_audio_extraction(session_id, None).await;

    info!("[{}] Transcript saved: {} segments, {} speakers",
          session_id, all_segments.len(), speaker_info.len());

    Ok(unconfirmed)
}

// ── Summary endpoints ──

async fn get_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let dir = state.session_manager.session_dir(&id);
    let path = dir.join("summary.json");
    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "summary not found"}))));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to read summary: {e}")}))))?;
    let json: Value = serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to parse summary: {e}")}))))?;
    Ok(Json(json))
}

#[derive(Deserialize)]
struct UpdateSummaryRequest {
    content: String,
}

async fn update_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSummaryRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let dir = state.session_manager.session_dir(&id);
    let json_path = dir.join("summary.json");
    if !json_path.exists() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "summary not found"}))));
    }

    // Read existing summary, update content
    let existing = std::fs::read_to_string(&json_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    let mut summary: Value = serde_json::from_str(&existing)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    summary["content"] = json!(body.content);

    let json_str = serde_json::to_string_pretty(&summary)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    std::fs::write(&json_path, json_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;

    // Also update .md file
    let md_path = dir.join("summary.md");
    let _ = std::fs::write(&md_path, &body.content);

    Ok(Json(summary))
}

async fn delete_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let dir = state.session_manager.session_dir(&id);
    let path = dir.join("summary.json");
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to delete summary: {e}")}))))?;
    }
    Ok(Json(json!({"status": "deleted"})))
}

// ── TODO endpoints ──

async fn get_session_todos(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let path = state.session_manager.session_dir(&id).join("todos.json");
    if !path.exists() {
        return Ok(Json(json!({"items": []})));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    let json: Value = serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    Ok(Json(json))
}

async fn toggle_todo(
    State(state): State<AppState>,
    Path((id, idx)): Path<(String, usize)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let dir = state.session_manager.session_dir(&id);
    let todos_path = dir.join("todos.json");
    if !todos_path.exists() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "no todos"}))));
    }
    let content = std::fs::read_to_string(&todos_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    let mut todos: Value = serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;

    let items = todos.get_mut("items")
        .and_then(|i| i.as_array_mut())
        .ok_or((StatusCode::BAD_REQUEST, Json(json!({"error": "invalid todos format"}))))?;

    if idx >= items.len() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "todo index out of range"}))));
    }

    // Toggle completed
    let completed = items[idx].get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
    items[idx]["completed"] = json!(!completed);

    let json_str = serde_json::to_string_pretty(&todos)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;
    std::fs::write(&todos_path, json_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e}")}))))?;

    // Also update the summary.md and summary.json checkbox states
    let summary_json_path = dir.join("summary.json");
    if summary_json_path.exists() {
        if let Ok(s) = std::fs::read_to_string(&summary_json_path) {
            if let Ok(mut sj) = serde_json::from_str::<Value>(&s) {
                if let Some(md) = sj.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                    let mut n = 0usize;
                    let new_md = regex::Regex::new(r"- \[([ xX])\]").unwrap()
                        .replace_all(&md, |_caps: &regex::Captures| {
                            let result = if n == idx {
                                if !completed { "- [x]" } else { "- [ ]" }
                            } else {
                                _caps.get(0).unwrap().as_str()
                            };
                            n += 1;
                            result.to_string()
                        }).to_string();
                    sj["content"] = json!(new_md);
                    let _ = std::fs::write(&summary_json_path, serde_json::to_string_pretty(&sj).unwrap_or_default());
                    let _ = std::fs::write(dir.join("summary.md"), &new_md);
                }
            }
        }
    }

    Ok(Json(todos))
}

async fn get_person_todos(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Get all sessions this person appears in
    let session_ids = state.files_db.get_person_session_ids(&person_id).await;

    let mut result: Vec<Value> = Vec::new();
    for sid in &session_ids {
        let todos_path = state.session_manager.session_dir(sid).join("todos.json");
        if !todos_path.exists() { continue; }
        let content = match std::fs::read_to_string(&todos_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let todos: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let items = match todos.get("items").and_then(|i| i.as_array()) {
            Some(items) => items,
            None => continue,
        };

        // Get session info for display
        let session_info = state.session_manager.get_session(sid).await;
        let session_name = session_info.as_ref().and_then(|s| s.name.clone()).unwrap_or_else(|| sid.clone());
        let session_created = session_info.as_ref().map(|s| s.created_at.to_rfc3339());

        for (idx, item) in items.iter().enumerate() {
            // Include items assigned to this person
            if item.get("person_id").and_then(|v| v.as_str()) == Some(&person_id) {
                let mut todo = item.clone();
                todo["session_id"] = json!(sid);
                todo["session_name"] = json!(session_name);
                todo["session_created_at"] = json!(session_created);
                todo["todo_index"] = json!(idx);
                result.push(todo);
            }
        }
    }

    Ok(Json(json!({"todos": result})))
}

#[derive(Deserialize, Default)]
struct SummarizeRequest {
    #[serde(default)]
    additional_instructions: Option<String>,
}

async fn summarize_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<SummarizeRequest>>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let additional = body.and_then(|b| b.0.additional_instructions);

    // Check LLM configuration
    let settings = state.settings.read().await;
    let host = settings.llm_host.clone();
    let model = settings.summarization_model.clone()
        .unwrap_or_else(|| settings.llm_model.clone());
    let mut prompt = settings.summarization_prompt.clone().unwrap_or_default();
    let sum_sort = settings.summarization_openrouter_sort.clone();
    drop(settings);

    if let Some(extra) = additional {
        if !extra.trim().is_empty() {
            prompt.push_str(&format!("\n\nAdditional instructions: {}", extra.trim()));
        }
    }

    let secrets = state.llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    if api_key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "LLM API key not configured"}))));
    }

    // Check transcript exists and get session info
    let dir = state.session_manager.session_dir(&id);
    let transcript_path = dir.join("transcript.json");
    if !transcript_path.exists() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No transcript available to summarize"}))));
    }

    let session_info = state.session_manager.get_session(&id).await;

    // Spawn background task
    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let tags_mgr = state.tags_manager.clone();
    let files_db = state.files_db.clone();
    let session_id = id.clone();

    tokio::spawn(async move {
        session_manager.emit_summary_progress(&session_id, "summarizing").await;
        match crate::chat::summarize::run_summarization(&session_id, &dir, &host, &api_key, &model, &prompt, session_info.as_ref(), &tags_mgr, &people_manager, &session_manager, sum_sort.as_deref()).await {
            Ok(_) => {
                session_manager.refresh_files(&session_id).await;
                session_manager.emit_summary_completed(&session_id).await;
                // Refresh recordings index with new summary description
                let recordings_dir = files_db.recordings_dir().to_path_buf();
                let mut sessions = session_manager.session_entries().await;
                crate::markdown::write_recordings_index(&recordings_dir, &mut sessions);
                info!("Summary generated for session {}", session_id);
            }
            Err(e) => {
                error!("Summary failed for session {}: {}", session_id, e);
                session_manager.emit_summary_failed(&session_id, &e).await;
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(json!({"status": "processing"}))))
}

/// Check settings and run auto-summarization if enabled.
async fn maybe_auto_summarize(
    session_id: &str,
    session_manager: &SessionManager,
    settings: &SharedSettings,
    llm_secrets: &SharedSecrets,
    tags_manager: &TagsManager,
    people_manager: &PeopleManager,
    files_db: &FilesDb,
) {
    let s = settings.read().await;
    if !s.auto_summarize {
        return;
    }
    let host = s.llm_host.clone();
    let model = s.summarization_model.clone().unwrap_or_else(|| s.llm_model.clone());
    let prompt = s.summarization_prompt.clone().unwrap_or_default();
    let sum_sort = s.summarization_openrouter_sort.clone();
    drop(s);

    let secrets = llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    if api_key.is_empty() {
        warn!("[{}] Auto-summarize skipped: no LLM API key configured", session_id);
        return;
    }

    let dir = session_manager.session_dir(session_id);
    let session_info = session_manager.get_session(session_id).await;

    session_manager.emit_summary_progress(session_id, "summarizing").await;

    match crate::chat::summarize::run_summarization(session_id, &dir, &host, &api_key, &model, &prompt, session_info.as_ref(), tags_manager, people_manager, session_manager, sum_sort.as_deref()).await {
        Ok(_) => {
            session_manager.refresh_files(session_id).await;
            session_manager.emit_summary_completed(session_id).await;
            // Refresh recordings index with new summary description
            let recordings_dir = files_db.recordings_dir().to_path_buf();
            let mut sessions = session_manager.session_entries().await;
            crate::markdown::write_recordings_index(&recordings_dir, &mut sessions);
            info!("[{}] Auto-summary generated", session_id);
        }
        Err(e) => {
            error!("[{}] Auto-summary failed: {}", session_id, e);
            session_manager.emit_summary_failed(session_id, &e).await;
        }
    }
}


/// Poll an extraction job until completion. No timeout — keeps polling forever.
async fn poll_extraction_job(
    client: &ExtractionClient,
    job_id: &str,
    session_id: &str,
    session_manager: &SessionManager,
) -> Result<ExtractionOutput, String> {
    let mut delay = std::time::Duration::from_secs(2);
    let max_delay = std::time::Duration::from_secs(15);

    loop {
        tokio::time::sleep(delay).await;

        match client.poll_status(job_id).await? {
            Some(output) => return Ok(output),
            None => {
                session_manager.emit_transcription_progress(session_id, "extracting");
                delay = (delay * 2).min(max_delay);
            }
        }
    }
}

/// Resume polling for any sessions with in-progress extraction jobs.
/// Called once on daemon startup.
pub async fn resume_pending_extractions(
    session_manager: SessionManager,
    people_manager: PeopleManager,
    files_db: FilesDb,
    settings: SharedSettings,
    llm_secrets: SharedSecrets,
    tags_manager: TagsManager,
) {
    let pending = session_manager.get_pending_extractions().await;
    if pending.is_empty() { return; }

    info!("Resuming {} pending extraction job(s)...", pending.len());

    for (session_id, job) in pending {
        let extraction_url = match &job.extraction_url {
            Some(url) => url.clone(),
            None => {
                // Fall back to current settings
                let s = settings.read().await;
                match &s.audio_extraction_url {
                    Some(url) => url.clone(),
                    None => {
                        warn!("No extraction URL for pending job {} (session {})", job.job_id, session_id);
                        continue;
                    }
                }
            }
        };

        let extraction_key = {
            let s = settings.read().await;
            match &s.audio_extraction_api_key {
                Some(key) => key.clone(),
                None => {
                    warn!("No extraction API key for pending job {} (session {})", job.job_id, session_id);
                    continue;
                }
            }
        };

        let sm = session_manager.clone();
        let pm = people_manager.clone();
        let fdb = files_db.clone();
        let stg = settings.clone();
        let secrets = llm_secrets.clone();
        let tm = tags_manager.clone();

        info!("Resuming extraction job {} for session {}", job.job_id, session_id);

        tokio::spawn(async move {
            let client = ExtractionClient::new(extraction_url, extraction_key);

            // Resume polling
            let result = poll_extraction_job(&client, &job.job_id, &session_id, &sm).await;

            match result {
                Ok(output) => {
                    info!("[{}] Resumed extraction completed, processing results...", session_id);

                    // Get session info for the pipeline continuation
                    let (session_dir, _language, source_meta) = match sm
                        .get_session_extraction_info(&session_id)
                        .await
                    {
                        Ok(info) => info,
                        Err(e) => {
                            error!("[{}] Failed to get session info for resumed job: {}", session_id, e);
                            sm.set_audio_extraction(&session_id, None).await;
                            return;
                        }
                    };

                    let stg_r = stg.read().await;
                    let people_recognition = stg_r.people_recognition;
                    let match_threshold = stg_r.speaker_match_threshold;
                    drop(stg_r);

                    // Process the output (same as post-extraction in the pipeline)
                    let result = process_extraction_output(
                        &session_id, &session_dir, &source_meta,
                        output, people_recognition, match_threshold,
                        &sm, &pm, &fdb,
                    ).await;

                    match result {
                        Ok(unconfirmed) => {
                            sm.set_processing_state(&session_id, None).await;
                            sm.emit_transcription_completed(&session_id, unconfirmed);
                            info!("[{}] Resumed transcription completed", session_id);

                            maybe_auto_summarize(&session_id, &sm, &stg, &secrets, &tm, &pm, &fdb).await;
                        }
                        Err(e) => {
                            error!("[{}] Resumed transcription post-processing failed: {}", session_id, e);
                            sm.set_processing_state(&session_id, None).await;
                            sm.emit_transcription_failed(&session_id, &e);
                            sm.set_audio_extraction(&session_id, None).await;
                        }
                    }
                }
                Err(e) => {
                    error!("[{}] Resumed extraction job failed: {}", session_id, e);
                    sm.set_processing_state(&session_id, None).await;
                    sm.emit_transcription_failed(&session_id, &e);
                    sm.set_audio_extraction(&session_id, None).await;
                }
            }
        });
    }
}

// --- Conversation routes ---

pub fn conversation_routes() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list_conversations))
        .route("/conversations", post(create_conversation))
        .route("/conversations/{id}", get(get_conversation))
        .route("/conversations/{id}", delete(delete_conversation))
        .route("/conversations/{id}/messages", post(send_message))
        .route("/conversations/{id}/messages/{msg_id}", delete(delete_message))
        .route("/conversations/{id}/claude-sync", post(sync_claude_messages))
        .route("/conversations/{id}/export-prompt", get(export_prompt))
        .route("/llm/models", get(list_models))
}

async fn list_conversations(
    State(state): State<AppState>,
) -> Json<Value> {
    let summaries = state.conversation_manager.list(10);
    Json(json!({ "conversations": summaries }))
}

async fn create_conversation(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let title = body.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
    let chat_backend = body.get("chat_backend").and_then(|v| v.as_str()).map(|s| s.to_string());
    let mut conv = state.conversation_manager.create(title)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    if chat_backend.is_some() {
        conv.chat_backend = chat_backend;
        state.conversation_manager.save(&conv)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    }
    Ok((StatusCode::CREATED, Json(serde_json::to_value(&conv).unwrap_or_default())))
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conv = state.conversation_manager.get_transformed(&id)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "conversation not found"}))))?;
    Ok(Json(conv))
}

async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    state.conversation_manager.delete(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct SendMessageBody {
    content: String,
    #[serde(default)]
    mentions: Vec<Mention>,
}

async fn send_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    // Load conversation
    let mut conv = state.conversation_manager.get(&id)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "conversation not found"}))))?;

    let now = chrono::Utc::now();

    // Append user message
    let user_msg_id = format!("msg_{}", now.timestamp_nanos_opt().unwrap_or(0));
    conv.messages.push(Message::User {
        id: user_msg_id,
        content: body.content.clone(),
        mentions: body.mentions.clone(),
        timestamp: now,
    });

    // Auto-title from first user message
    if conv.title.is_empty() {
        conv.title = body.content.chars().take(60).collect();
    }

    // Build context: fetch only new chunks, dedupe against existing context
    let new_criteria = ContextCriteria::from_mentions(&body.mentions);

    // Get existing context from the most recent context_result
    let existing_context: Vec<crate::chat::types::ContextChunk> = conv.messages.iter().rev()
        .find_map(|m| {
            if let Message::ContextResult { chunks, .. } = m { Some(chunks.clone()) }
            else { None }
        })
        .unwrap_or_default();

    let context_chunks = if !new_criteria.is_empty() {
        // Retrieve only for the new criteria
        let new_chunks = crate::llm::context::retrieve_context(
            &new_criteria,
            &state.files_db,
            &state.session_manager,
            &state.tags_manager,
            &state.people_manager,
        ).await;

        // Dedupe: build a set of (kind, source_id, start_time_or_note_hash) from existing chunks
        let chunk_key = |c: &crate::chat::types::ContextChunk| -> (String, String, u64) {
            if c.kind == "note" {
                let hash = c.note.as_deref().unwrap_or("").len() as u64;
                (c.kind.clone(), c.source_id.clone(), hash)
            } else {
                let start_bits = c.segment.as_ref()
                    .and_then(|s| s.get("start")).and_then(|v| v.as_f64())
                    .unwrap_or(0.0).to_bits();
                (c.kind.clone(), c.source_id.clone(), start_bits)
            }
        };

        let existing_keys: std::collections::HashSet<_> = existing_context.iter()
            .map(chunk_key)
            .collect();

        let delta: Vec<_> = new_chunks.into_iter()
            .filter(|c| !existing_keys.contains(&chunk_key(c)))
            .collect();

        // Merge: existing + new unique chunks
        let mut combined = existing_context;
        combined.extend(delta.clone());

        // Re-sort: notes first, then by time
        combined.sort_by(|a, b| {
            let a_note = a.kind == "note";
            let b_note = b.kind == "note";
            if a_note != b_note {
                return if a_note { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
            }
            a.created_at.cmp(&b.created_at)
                .then_with(|| {
                    let as_ = a.segment.as_ref().and_then(|s| s.get("start")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let bs = b.segment.as_ref().and_then(|s| s.get("start")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    as_.partial_cmp(&bs).unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        // Build merged criteria for storage
        let mut merged_criteria = ContextCriteria::default();
        for msg in &conv.messages {
            if let Message::ContextResult { criteria: prev, .. } = msg {
                merged_criteria.merge(prev);
            }
        }
        merged_criteria.merge(&new_criteria);

        if !combined.is_empty() {
            let ctx_msg_id = format!("ctx_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
            conv.messages.push(Message::ContextResult {
                id: ctx_msg_id,
                criteria: merged_criteria,
                chunks: combined.clone(),
                timestamp: chrono::Utc::now(),
            });
        }

        combined
    } else if !existing_context.is_empty() {
        // No new mentions — reuse existing context
        existing_context
    } else {
        Vec::new()
    };

    conv.updated_at = chrono::Utc::now();

    // Save conversation with user message (and possibly context)
    state.conversation_manager.save(&conv)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    // Prepare LLM request
    let context_str = crate::llm::prompt::format_context(&context_chunks);

    let settings = state.settings.read().await;
    let host = settings.llm_host.clone();
    let model = settings.llm_model.clone();
    let openrouter_sort = settings.openrouter_sort.clone();
    let self_intro = settings.chat_self_intro.clone();
    drop(settings);

    let llm_messages = crate::llm::prompt::build_messages(&conv, &context_str, self_intro.as_deref());

    let secrets = state.llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    let chunk_count = context_chunks.len();
    let session_count = {
        let mut sids = std::collections::HashSet::new();
        for c in &context_chunks { sids.insert(&c.source_id); }
        sids.len()
    };

    let conv_manager = state.conversation_manager.clone();
    let conv_id = id.clone();

    // Build SSE stream
    let stream = async_stream::stream! {
        // Emit context loaded event if applicable
        if chunk_count > 0 {
            yield Ok(Event::default()
                .event("context_loaded")
                .data(json!({"chunk_count": chunk_count, "session_count": session_count}).to_string()));
        }

        if api_key.is_empty() {
            yield Ok(Event::default()
                .event("error")
                .data(json!({"error": "LLM API key not configured. Set it in Settings > Services."}).to_string()));
            return;
        }

        // Stream from LLM
        let model_name = model.clone();
        let client = LlmClient::new(host, api_key, model)
            .with_provider_sort(openrouter_sort);
        match client.stream_chat(llm_messages).await {
            Ok(llm_stream) => {
                let mut full_content = String::new();
                let mut usage_value: Option<Value> = None;
                let assistant_msg_id = format!("msg_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));

                futures::pin_mut!(llm_stream);

                while let Some(result) = llm_stream.next().await {
                    match result {
                        Ok(content) => {
                            if let Some(thinking) = content.strip_prefix('\x01') {
                                yield Ok(Event::default()
                                    .event("thinking")
                                    .data(json!({"content": thinking}).to_string()));
                            } else if let Some(usage_str) = content.strip_prefix('\x02') {
                                if let Ok(u) = serde_json::from_str::<Value>(usage_str) {
                                    let prompt = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let completion = u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let total = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(prompt + completion);
                                    let cost = u.get("cost").and_then(|v| v.as_f64());
                                    let cost_str = cost.map(|c| format!(", cost ${:.4}", c)).unwrap_or_default();
                                    info!(
                                        "[{}] Chat {} — {} prompt + {} completion = {} tokens{}",
                                        conv_id, model_name, prompt, completion, total, cost_str
                                    );
                                }
                                usage_value = serde_json::from_str(usage_str).ok();
                                yield Ok(Event::default()
                                    .event("usage")
                                    .data(usage_str.to_string()));
                            } else {
                                full_content.push_str(&content);
                                yield Ok(Event::default()
                                    .event("delta")
                                    .data(json!({"content": content}).to_string()));
                            }
                        }
                        Err(e) => {
                            yield Ok(Event::default()
                                .event("error")
                                .data(json!({"error": e}).to_string()));
                            return;
                        }
                    }
                }

                // Save assistant message (skip if empty, e.g. client cancelled before any content)
                if !full_content.is_empty() {
                    if let Some(mut conv) = conv_manager.get(&conv_id) {
                        conv.messages.push(Message::Assistant {
                            id: assistant_msg_id.clone(),
                            content: full_content,
                            timestamp: chrono::Utc::now(),
                            usage: usage_value,
                        });
                        conv.updated_at = chrono::Utc::now();
                        if let Err(e) = conv_manager.save(&conv) {
                            warn!("Failed to save assistant message: {}", e);
                        }
                    }
                }

                yield Ok(Event::default()
                    .event("done")
                    .data(json!({"message_id": assistant_msg_id}).to_string()));
            }
            Err(e) => {
                yield Ok(Event::default()
                    .event("error")
                    .data(json!({"error": e}).to_string()));
            }
        }
    };

    Ok(Sse::new(stream))
}

/// Sync messages from a Claude Code session into an app conversation.
/// Receives user + assistant messages and the claude session_id.
async fn sync_claude_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut conv = state.conversation_manager.get(&id)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "conversation not found"}))))?;

    if let Some(sid) = body.get("claude_session_id").and_then(|v| v.as_str()) {
        conv.claude_session_id = Some(sid.to_string());
    }

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        let now = chrono::Utc::now();
        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let msg_id = msg.get("id").and_then(|v| v.as_str())
                .unwrap_or(&format!("msg_{}", now.timestamp_nanos_opt().unwrap_or(0)))
                .to_string();

            if content.is_empty() { continue; }

            match role {
                "user" => {
                    // Parse mentions if present
                    let mentions: Vec<crate::chat::types::Mention> = msg.get("mentions")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    conv.messages.push(Message::User {
                        id: msg_id, content, mentions, timestamp: now,
                    });
                }
                "assistant" => {
                    conv.messages.push(Message::Assistant {
                        id: msg_id, content, timestamp: now, usage: None,
                    });
                }
                _ => {}
            }
        }
    }

    conv.updated_at = chrono::Utc::now();
    if conv.title.is_empty() {
        if let Some(first_user) = conv.messages.iter().find(|m| matches!(m, Message::User { .. })) {
            if let Message::User { content, .. } = first_user {
                conv.title = content.chars().take(60).collect();
            }
        }
    }

    state.conversation_manager.save(&conv)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    Ok(Json(json!({ "ok": true })))
}

async fn delete_message(
    State(state): State<AppState>,
    Path((id, msg_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    state.conversation_manager.delete_message(&id, &msg_id)
        .map_err(|e| {
            let status = if e.contains("not found") { StatusCode::NOT_FOUND } else { StatusCode::INTERNAL_SERVER_ERROR };
            (status, Json(json!({"error": e})))
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn export_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, [(axum::http::header::HeaderName, &'static str); 1], String), (StatusCode, Json<Value>)> {
    let conv = state.conversation_manager.get(&id)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "conversation not found"}))))?;

    // Get context from the most recent context_result
    let context_chunks: Vec<crate::chat::types::ContextChunk> = conv.messages.iter().rev()
        .find_map(|m| {
            if let Message::ContextResult { chunks, .. } = m { Some(chunks.clone()) }
            else { None }
        })
        .unwrap_or_default();

    let context_str = crate::llm::prompt::format_context(&context_chunks);
    let self_intro = state.settings.read().await.chat_self_intro.clone();
    let text = crate::llm::prompt::format_as_text(&conv, &context_str, self_intro.as_deref());

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        text,
    ))
}

// --- Claude Code routes ---

pub fn claude_routes() -> Router<AppState> {
    Router::new()
        .route("/claude/status", get(claude_status))
        .route("/claude/send", post(claude_send))
        .route("/claude/stop", post(claude_stop))
        .route("/claude/sessions", get(claude_list_sessions))
        .route("/claude/sessions/{id}", get(claude_get_session))
        .route("/claude/approve-tools", post(claude_approve_tools))
}

async fn claude_status(
    State(state): State<AppState>,
) -> Json<Value> {
    let available = ClaudeCodeRunner::is_available().await;
    let running = state.claude_runner.is_running().await;
    Json(json!({
        "available": available,
        "running": running,
    }))
}

async fn claude_send(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let prompt = body.get("prompt").and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, Json(json!({"error": "prompt is required"}))))?
        .to_string();
    let session_id = body.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Resolve @ mentions to detailed context for Claude Code
    let mentions_context = if let Some(mentions) = body.get("mentions").and_then(|v| v.as_array()) {
        let mut lines = Vec::new();
        for m in mentions {
            let mtype = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let label = m.get("label").and_then(|v| v.as_str()).unwrap_or(id);
            match mtype {
                "session" => {
                    lines.push(format!("- Session \"{}\" (id: {})", label, id));
                }
                "person" => {
                    let person = state.people_manager.get_person(id).await;
                    let name = person.as_ref().map(|p| p.name.as_str()).unwrap_or(label);
                    lines.push(format!("- Person \"{}\" (id: {})", name, id));
                }
                "tag" => {
                    let tag = state.tags_manager.get_tag(label).await;
                    let notes = tag.as_ref().and_then(|t| t.notes.as_deref()).unwrap_or("");
                    if notes.is_empty() {
                        lines.push(format!("- Tag \"{}\"", label));
                    } else {
                        lines.push(format!("- Tag \"{}\": {}", label, notes));
                    }
                }
                _ => {}
            }
        }
        if lines.is_empty() {
            None
        } else {
            Some(format!("Referenced:\n{}", lines.join("\n")))
        }
    } else {
        None
    };

    let claude_model = state.settings.read().await.claude_code_model.clone();

    // Mentions context prepended to the prompt
    let combined_context = mentions_context;

    // Build the full prompt so we can include it in the stream for export
    let full_prompt = match &combined_context {
        Some(ctx) => format!("{}\n\n---\n{}", ctx, prompt),
        None => prompt.clone(),
    };

    let mut rx = state.claude_runner.run(
        &prompt,
        session_id.as_deref(),
        combined_context.as_deref(),
        claude_model.as_deref(),
    ).await.map_err(|e| (StatusCode::CONFLICT, Json(json!({"error": e}))))?;

    let stream = async_stream::stream! {
        // Emit the full prompt (with resolved mentions) so the frontend can export it
        yield Ok(Event::default().event("prompt").data(json!({"full_prompt": full_prompt}).to_string()));

        while let Some(event) = rx.recv().await {
            let (event_name, data) = match &event {
                crate::llm::claude_code::ClaudeEvent::Init { .. } => ("init", serde_json::to_string(&event).unwrap()),
                crate::llm::claude_code::ClaudeEvent::Delta { .. } => ("delta", serde_json::to_string(&event).unwrap()),
                crate::llm::claude_code::ClaudeEvent::ToolUse { .. } => ("tool_use", serde_json::to_string(&event).unwrap()),
                crate::llm::claude_code::ClaudeEvent::Done { .. } => ("done", serde_json::to_string(&event).unwrap()),
                crate::llm::claude_code::ClaudeEvent::Error { .. } => ("error", serde_json::to_string(&event).unwrap()),
                crate::llm::claude_code::ClaudeEvent::PermissionRequest { .. } => ("permission_request", serde_json::to_string(&event).unwrap()),
            };
            yield Ok(Event::default().event(event_name).data(data));
        }
    };

    Ok(Sse::new(stream))
}

async fn claude_stop(
    State(state): State<AppState>,
) -> Json<Value> {
    let stopped = state.claude_runner.stop().await;
    Json(json!({ "stopped": stopped }))
}

async fn claude_approve_tools(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let tools: Vec<String> = body.get("tools")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let scope = body.get("scope").and_then(|v| v.as_str()).unwrap_or(
        if body.get("permanent").and_then(|v| v.as_bool()).unwrap_or(false) { "permanent" } else { "once" }
    );

    if tools.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "no tools specified"}))));
    }

    match scope {
        "permanent" => {
            state.claude_runner.approve_tools_permanent(&tools)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
            state.claude_runner.approve_tools_session(&tools).await;
        }
        "session" => {
            state.claude_runner.approve_tools_session(&tools).await;
        }
        _ => {
            // "once": add to session list so the immediate retry works,
            // removed automatically after the next run completes.
            state.claude_runner.approve_tools_once(&tools).await;
        }
    }

    Ok(Json(json!({ "approved": tools, "scope": scope })))
}

async fn claude_list_sessions(
    State(state): State<AppState>,
) -> Json<Value> {
    let sessions = state.claude_runner.list_sessions();
    Json(json!({ "sessions": sessions }))
}

async fn claude_get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let messages = state.claude_runner.load_session(&id)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))))?;
    Ok(Json(json!({ "session_id": id, "messages": messages })))
}

async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let settings = state.settings.read().await;
    let host = settings.llm_host.clone();
    drop(settings);

    let secrets = state.llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    if api_key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "LLM API key not configured"}))));
    }

    let result = LlmClient::list_models(&host, &api_key).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    Ok(Json(result))
}
