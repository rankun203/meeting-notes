use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::session::SessionManager;
use crate::session::config::SessionConfig;
use crate::session::session::SessionInfo;

pub fn session_routes() -> Router<SessionManager> {
    Router::new()
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}", delete(delete_session))
        .route("/sessions/{id}/recording/start", post(start_recording))
        .route("/sessions/{id}/recording/stop", post(stop_recording))
        .route("/sessions/{id}/files", get(get_files))
        .route("/sessions/{id}/files/{filename}", get(serve_file))
        .route("/config", get(get_config))
        .route("/sources", get(list_sources))
        .route("/devices", get(list_devices))
}

async fn create_session(
    State(manager): State<SessionManager>,
    Json(config): Json<SessionConfig>,
) -> (StatusCode, Json<SessionInfo>) {
    let info = manager.create_session(config).await;
    (StatusCode::CREATED, Json(info))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_sessions(
    State(manager): State<SessionManager>,
    Query(params): Query<ListParams>,
) -> Json<Value> {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let (sessions, total) = manager.list_sessions(limit, offset).await;
    Json(json!({
        "sessions": sessions,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

async fn get_session(
    State(manager): State<SessionManager>,
    Path(id): Path<String>,
) -> Result<Json<SessionInfo>, (StatusCode, Json<Value>)> {
    manager
        .get_session(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))))
}

async fn delete_session(
    State(manager): State<SessionManager>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    manager
        .delete_session(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
}

async fn start_recording(
    State(manager): State<SessionManager>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    manager
        .start_recording(&id)
        .await
        .map(|files| Json(json!({"status": "recording", "files": files})))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))
}

async fn stop_recording(
    State(manager): State<SessionManager>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    manager
        .stop_recording(&id)
        .await
        .map(|files| Json(json!({"status": "stopped", "files": files})))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))
}

async fn get_files(
    State(manager): State<SessionManager>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<Value>)> {
    manager
        .get_files(&id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))))
}

async fn serve_file(
    State(manager): State<SessionManager>,
    Path((id, filename)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    // Verify session exists and file belongs to it
    let files = manager
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

    let file_path = manager.session_dir(&id).join(safe_name);
    let file = File::open(&file_path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, Json(json!({"error": "file not found on disk"}))))?;

    let content_type = if filename.ends_with(".mp3") {
        "audio/mpeg"
    } else if filename.ends_with(".wav") {
        "audio/wav"
    } else if filename.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    };

    let disposition = format!("inline; filename=\"{}\"", filename);
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    ))
}

async fn get_config() -> Json<Value> {
    let sources = crate::audio::discover_sources();
    Json(json!({
        "sources": sources,
        "fields": {
            "language": {
                "type": "text",
                "default": "en",
                "label": "Language",
                "description": "Language code for transcription (e.g. en, zh, ja)",
            },
            "format": {
                "type": "select",
                "default": "wav",
                "label": "Format",
                "description": "Audio file format",
                "options": [
                    { "value": "wav", "label": "WAV" },
                    { "value": "mp3", "label": "MP3" },
                ],
            },
            "sample_rate": {
                "type": "select",
                "default": 48000,
                "label": "Sample Rate",
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

async fn list_sources() -> Json<Value> {
    let sources = crate::audio::discover_sources();
    Json(json!({ "sources": sources }))
}

async fn list_devices() -> Json<Value> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let devices: Vec<String> = host
        .input_devices()
        .map(|devices| {
            devices
                .filter_map(|d| d.name().ok())
                .collect()
        })
        .unwrap_or_default();

    Json(json!({"devices": devices}))
}
