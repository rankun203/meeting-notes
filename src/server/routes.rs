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
use serde_json::{Value, json};
use tracing::{error, info};

use crate::session::config::SessionConfig;
use crate::session::session::SessionInfo;

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
        .route("/diagnostics", get(get_diagnostics))
        .route("/diagnostics/logs", get(get_diagnostics_logs))
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
            let result = services::transcripts::run_transcription_pipeline(
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

                    services::summary::maybe_auto_summarize(
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

// ---- File handlers (thin shims over `services::files`) ----

async fn get_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, services::ServiceError> {
    let files = services::files::list_files(&state, &id).await?;
    Ok(Json(files))
}

async fn serve_file(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, services::ServiceError> {
    let file_path = services::files::resolve_session_file(&state, &id, &filename).await?;
    // Use tower-http ServeFile for Content-Length, Accept-Ranges, and range requests.
    let serve = tower_http::services::ServeFile::new(&file_path);
    tower::ServiceExt::oneshot(serve, req)
        .await
        .map_err(|e| services::ServiceError::Internal(format!("{e}")))
}

async fn get_waveform(
    State(state): State<AppState>,
    Path((id, filename)): Path<(String, String)>,
) -> Result<Json<Value>, services::ServiceError> {
    let waveform = services::files::get_waveform(&state, &id, &filename).await?;
    Ok(Json(serde_json::to_value(waveform).unwrap()))
}

// ---- Config handler (shim over `services::config`) ----

async fn get_config() -> Json<Value> {
    Json(services::config::get_config())
}

// ---- Diagnostics handlers (shims over `services::diagnostics`) ----

async fn get_diagnostics(
    State(state): State<AppState>,
) -> Result<Json<services::diagnostics::DiagnosticsInfo>, services::ServiceError> {
    Ok(Json(services::diagnostics::get_info(&state)?))
}

#[derive(serde::Deserialize)]
struct LogsQuery {
    lines: Option<usize>,
    file: Option<String>,
}

async fn get_diagnostics_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<services::diagnostics::LogTail>, services::ServiceError> {
    let n = q.lines.unwrap_or(100);
    Ok(Json(services::diagnostics::tail_logs(&state, n, q.file.as_deref())?))
}

// ---- Transcript + attribution handlers (shims over `services::transcripts`) ----

async fn get_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    let data = services::transcripts::get_transcript(&state, &id).await?;
    Ok(Json(data))
}

async fn delete_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, services::ServiceError> {
    services::transcripts::delete_transcript(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_attribution(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    let data = services::transcripts::get_attribution(&state, &id).await?;
    Ok(Json(data))
}

async fn update_attribution(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<services::transcripts::AttributionRequest>,
) -> Result<Json<Value>, services::ServiceError> {
    services::transcripts::update_attribution(&state, &id, body).await?;
    Ok(Json(json!({"status": "ok"})))
}

// ---- People / tags / settings handlers (shims over `services::*`) ----

async fn list_people(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::people::list_people(&state).await?))
}

async fn create_person(
    State(state): State<AppState>,
    Json(input): Json<services::people::CreatePersonInput>,
) -> Result<(StatusCode, Json<Value>), services::ServiceError> {
    let person = services::people::create_person(&state, input).await?;
    Ok((StatusCode::CREATED, Json(person)))
}

async fn get_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::people::get_person(&state, &id).await?))
}

async fn update_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<services::people::UpdatePersonInput>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::people::update_person(&state, &id, input).await?))
}

async fn delete_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, services::ServiceError> {
    services::people::delete_person(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_person_sessions(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::people::get_person_sessions(&state, &person_id).await?))
}

async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::settings::get_settings(&state).await?))
}

async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::settings::update_settings(&state, body).await?))
}

async fn list_tags(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::tags::list_tags(&state).await?))
}

async fn create_tag(
    State(state): State<AppState>,
    Json(input): Json<services::tags::CreateTagInput>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::tags::create_tag(&state, input).await?))
}

async fn update_tag(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<services::tags::UpdateTagInput>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::tags::update_tag(&state, &name, input).await?))
}

async fn delete_tag(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, services::ServiceError> {
    services::tags::delete_tag(&state, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_tag_sessions(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::tags::get_tag_sessions(&state, &name).await?))
}

async fn set_session_tags(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<services::tags::SetSessionTagsInput>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::tags::set_session_tags(&state, &id, input).await?))
}

// ---- Transcribe handler (shim over `services::transcripts::transcribe_session`) ----

async fn transcribe_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<services::transcripts::TranscribeAccepted>), services::ServiceError> {
    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let files_db = state.files_db.clone();
    let settings_clone = state.settings.clone();
    let llm_secrets = state.llm_secrets.clone();
    let tags_mgr = state.tags_manager.clone();

    let out = services::transcripts::transcribe_session(&state, &id, move |args| {
        tokio::spawn(async move {
            let result = services::transcripts::run_transcription_pipeline(
                &args.session_id,
                &args.session_dir,
                &args.language,
                &args.source_meta,
                &args.extraction_url,
                &args.extraction_key,
                &args.file_drop_url,
                &args.file_drop_api_key,
                args.diarize,
                args.people_recognition,
                args.match_threshold,
                &session_manager,
                &people_manager,
                &files_db,
            )
            .await;

            match result {
                Ok(unconfirmed) => {
                    session_manager
                        .set_processing_state(&args.session_id, None)
                        .await;
                    session_manager
                        .emit_transcription_completed(&args.session_id, unconfirmed);
                    info!("Transcription completed for session {}", args.session_id);

                    services::summary::maybe_auto_summarize(
                        &args.session_id,
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
                    error!("Transcription failed for session {}: {}", args.session_id, e);
                    session_manager
                        .set_processing_state(&args.session_id, None)
                        .await;
                    session_manager
                        .emit_transcription_failed(&args.session_id, &e);
                }
            }
        });
    })
    .await?;

    Ok((StatusCode::ACCEPTED, Json(out)))
}


// ---- Summary + todos + summarize handlers (shims over `services::summary`) ----

async fn get_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::summary::get_summary(&state, &id).await?))
}

async fn update_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<services::summary::UpdateSummaryInput>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::summary::update_summary(&state, &id, input).await?))
}

async fn delete_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    services::summary::delete_summary(&state, &id).await?;
    Ok(Json(json!({"status": "deleted"})))
}

async fn get_session_todos(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::summary::get_session_todos(&state, &id).await?))
}

async fn toggle_todo(
    State(state): State<AppState>,
    Path((id, idx)): Path<(String, usize)>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::summary::toggle_todo(&state, &id, idx).await?))
}

async fn get_person_todos(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::summary::get_person_todos(&state, &person_id).await?))
}

async fn summarize_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<services::summary::SummarizeInput>>,
) -> Result<(StatusCode, Json<services::summary::SummarizeAccepted>), services::ServiceError> {
    let input = body.map(|b| b.0).unwrap_or_default();

    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let tags_mgr = state.tags_manager.clone();
    let files_db = state.files_db.clone();

    let out = services::summary::summarize_session(&state, &id, input, move |args| {
        tokio::spawn(async move {
            session_manager
                .emit_summary_progress(&args.session_id, "summarizing")
                .await;
            let session_info = session_manager.get_session(&args.session_id).await;
            match crate::chat::summarize::run_summarization(
                &args.session_id,
                &args.session_dir,
                &args.host,
                &args.api_key,
                &args.model,
                &args.prompt,
                session_info.as_ref(),
                &tags_mgr,
                &people_manager,
                &session_manager,
                args.sum_sort.as_deref(),
            )
            .await
            {
                Ok(_) => {
                    session_manager.refresh_files(&args.session_id).await;
                    session_manager.emit_summary_completed(&args.session_id).await;
                    let recordings_dir = files_db.recordings_dir().to_path_buf();
                    let mut sessions = session_manager.session_entries().await;
                    crate::markdown::write_recordings_index(&recordings_dir, &mut sessions);
                    info!("Summary generated for session {}", args.session_id);
                }
                Err(e) => {
                    error!("Summary failed for session {}: {}", args.session_id, e);
                    session_manager.emit_summary_failed(&args.session_id, &e).await;
                }
            }
        });
    })
    .await?;

    Ok((StatusCode::ACCEPTED, Json(out)))
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

// ---- Non-streaming conversation handlers (shims over `services::chat`) ----

async fn list_conversations(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::chat::list_conversations(&state).await?))
}

async fn create_conversation(
    State(state): State<AppState>,
    Json(input): Json<services::chat::CreateConversationInput>,
) -> Result<(StatusCode, Json<Value>), services::ServiceError> {
    let conv = services::chat::create_conversation(&state, input).await?;
    Ok((StatusCode::CREATED, Json(conv)))
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::chat::get_conversation(&state, &id).await?))
}

async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, services::ServiceError> {
    services::chat::delete_conversation(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Streaming chat handler (shim over `services::chat::send_message_stream`) ----

async fn send_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<services::chat::SendMessageInput>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, services::ServiceError>
{
    let events = services::chat::send_message_stream(&state, &id, input).await?;
    let sse = events.map(|ev| Ok(chat_event_to_sse(&ev)));
    Ok(Sse::new(sse))
}

/// Serialize a `ChatEvent` into the SSE event shape the existing webui
/// already parses: named event type + JSON payload (minus the `type`
/// tag, since SSE carries the name separately).
fn chat_event_to_sse(ev: &services::chat::ChatEvent) -> Event {
    use services::chat::ChatEvent;
    match ev {
        ChatEvent::ContextLoaded { chunk_count, session_count } => Event::default()
            .event("context_loaded")
            .data(json!({ "chunk_count": chunk_count, "session_count": session_count }).to_string()),
        ChatEvent::Thinking { content } => Event::default()
            .event("thinking")
            .data(json!({ "content": content }).to_string()),
        ChatEvent::Delta { content } => Event::default()
            .event("delta")
            .data(json!({ "content": content }).to_string()),
        ChatEvent::Usage(value) => Event::default()
            .event("usage")
            .data(value.to_string()),
        ChatEvent::Done { message_id } => Event::default()
            .event("done")
            .data(json!({ "message_id": message_id }).to_string()),
        ChatEvent::Error { error } => Event::default()
            .event("error")
            .data(json!({ "error": error }).to_string()),
    }
}

async fn sync_claude_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::chat::sync_claude_messages(&state, &id, body).await?))
}

async fn delete_message(
    State(state): State<AppState>,
    Path((id, msg_id)): Path<(String, String)>,
) -> Result<StatusCode, services::ServiceError> {
    services::chat::delete_message(&state, &id, &msg_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn export_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, [(axum::http::header::HeaderName, &'static str); 1], String), services::ServiceError> {
    let text = services::chat::export_prompt(&state, &id).await?;
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
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::claude::status(&state).await?))
}

// ---- Streaming claude send handler (shim over `services::claude::send_stream`) ----

async fn claude_send(
    State(state): State<AppState>,
    Json(input): Json<services::claude::SendInput>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, services::ServiceError>
{
    let events = services::claude::send_stream(&state, input).await?;
    let sse = events.map(|ev| Ok(claude_event_to_sse(&ev)));
    Ok(Sse::new(sse))
}

fn claude_event_to_sse(ev: &services::claude::ClaudeStreamEvent) -> Event {
    use services::claude::ClaudeStreamEvent as E;
    let (name, payload) = match ev {
        E::Prompt { .. } => ("prompt", serde_json::to_value(ev).unwrap_or(json!({}))),
        E::Init { .. } => ("init", serde_json::to_value(ev).unwrap_or(json!({}))),
        E::Delta { .. } => ("delta", serde_json::to_value(ev).unwrap_or(json!({}))),
        E::ToolUse { .. } => ("tool_use", serde_json::to_value(ev).unwrap_or(json!({}))),
        E::Done { .. } => ("done", serde_json::to_value(ev).unwrap_or(json!({}))),
        E::Error { .. } => ("error", serde_json::to_value(ev).unwrap_or(json!({}))),
        E::PermissionRequest { .. } => (
            "permission_request",
            serde_json::to_value(ev).unwrap_or(json!({})),
        ),
    };
    Event::default().event(name).data(payload.to_string())
}

async fn claude_stop(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::claude::stop(&state).await?))
}

async fn claude_approve_tools(
    State(state): State<AppState>,
    Json(input): Json<services::claude::ApproveToolsInput>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::claude::approve_tools(&state, input).await?))
}

async fn claude_list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::claude::list_sessions(&state).await?))
}

async fn claude_get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::claude::get_session(&state, &id).await?))
}

async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<Value>, services::ServiceError> {
    Ok(Json(services::chat::list_models(&state).await?))
}
