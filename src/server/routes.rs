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
use tracing::{error, info, warn};

use crate::chat::types::{ContextCriteria, Mention, Message};
use crate::llm::client::LlmClient;
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
