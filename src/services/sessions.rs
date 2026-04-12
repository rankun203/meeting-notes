//! Session service — transport-agnostic business logic for session CRUD,
//! recording control, and the stop-recording auto-transcription kickoff.
//!
//! Called by `server::routes` (REST) today and by `apps/desktop` Tauri commands
//! in the follow-up work.

use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::session::config::SessionConfig;
use crate::session::session::SessionInfo;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Serialize)]
pub struct SessionListPage {
    pub sessions: Vec<SessionInfo>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateSessionInput {
    pub name: Option<String>,
    pub language: Option<String>,
    /// Tri-state so callers can distinguish "don't touch" (field absent)
    /// from "clear" (field present with value `null`) from "set to X"
    /// (field present with string value).
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub notes: Option<Option<String>>,
    pub auto_stop: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct RecordingStarted {
    pub status: &'static str,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RecordingStopped {
    pub status: &'static str,
    pub files: Vec<String>,
}

pub async fn create_session(state: &AppState, config: SessionConfig) -> ServiceResult<SessionInfo> {
    let info = state.session_manager.create_session(config).await;
    state.refresh_recordings_index();
    Ok(info)
}

pub async fn list_sessions(state: &AppState, params: ListParams) -> ServiceResult<SessionListPage> {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let hidden_tags = state.tags_manager.hidden_tag_names().await;
    let (mut sessions, total) = state
        .session_manager
        .list_sessions(limit, offset, &hidden_tags)
        .await;
    for s in &mut sessions {
        s.unconfirmed_speakers = state.files_db.unconfirmed_speakers(&s.id).await;
    }
    Ok(SessionListPage {
        sessions,
        total,
        limit,
        offset,
    })
}

pub async fn get_session(state: &AppState, id: &str) -> ServiceResult<SessionInfo> {
    let mut info = state
        .session_manager
        .get_session(id)
        .await
        .ok_or_else(|| ServiceError::NotFound("session not found".into()))?;
    info.unconfirmed_speakers = state.files_db.unconfirmed_speakers(id).await;
    Ok(info)
}

pub async fn update_session(
    state: &AppState,
    id: &str,
    input: UpdateSessionInput,
) -> ServiceResult<SessionInfo> {
    if let Some(name) = input.name {
        state
            .session_manager
            .rename_session(id, name)
            .await
            .map_err(ServiceError::NotFound)?;
    }
    if let Some(language) = input.language {
        state
            .session_manager
            .update_session_language(id, language)
            .await
            .map_err(ServiceError::NotFound)?;
    }
    if let Some(notes) = input.notes {
        state
            .session_manager
            .update_session_notes(id, notes)
            .await
            .map_err(ServiceError::NotFound)?;
    }
    if let Some(auto_stop) = input.auto_stop {
        state
            .session_manager
            .set_auto_stop(id, auto_stop)
            .await
            .map_err(ServiceError::NotFound)?;
    }
    state.refresh_recordings_index();
    state
        .session_manager
        .get_session(id)
        .await
        .ok_or_else(|| ServiceError::NotFound("session not found".into()))
}

pub async fn delete_session(state: &AppState, id: &str) -> ServiceResult<()> {
    // Remove transcript from cache before deleting session files
    state.files_db.remove_transcript(id).await;

    state
        .session_manager
        .delete_session(id)
        .await
        .map_err(ServiceError::NotFound)?;
    state.refresh_recordings_index();
    Ok(())
}

pub async fn start_recording(state: &AppState, id: &str) -> ServiceResult<RecordingStarted> {
    let files = state
        .session_manager
        .start_recording(id)
        .await
        .map_err(ServiceError::BadRequest)?;
    Ok(RecordingStarted {
        status: "recording",
        files,
    })
}

/// Stop recording and, if auto-transcribe is enabled in settings, spawn the
/// transcription pipeline in the background. The pipeline function still
/// lives in `server::routes` for now — a later refactor pass will move it
/// into `services::transcripts` along with the summary kickoff.
pub async fn stop_recording(
    state: &AppState,
    id: &str,
    auto_transcribe: impl FnOnce(AutoTranscribeRequest) + Send + 'static,
) -> ServiceResult<RecordingStopped> {
    let files = state
        .session_manager
        .stop_recording(id)
        .await
        .map_err(ServiceError::BadRequest)?;

    let settings = state.settings.read().await;
    let should_auto_transcribe = settings.auto_transcribe && settings.is_extraction_configured();
    let extraction_url = settings.audio_extraction_url.clone();
    let extraction_key = settings.audio_extraction_api_key.clone();
    let file_drop_url = settings.file_drop_url.clone();
    let file_drop_api_key = settings.file_drop_api_key.clone();
    let diarize = settings.diarize;
    let people_recognition = settings.people_recognition;
    let match_threshold = settings.speaker_match_threshold;
    drop(settings);

    if should_auto_transcribe {
        if let Ok((session_dir, language, source_meta)) = state
            .session_manager
            .get_session_extraction_info(id)
            .await
        {
            let already_processing = state
                .session_manager
                .get_session(id)
                .await
                .map(|s| s.processing_state.is_some())
                .unwrap_or(false);

            if !already_processing {
                state
                    .session_manager
                    .set_processing_state(id, Some("starting".to_string()))
                    .await;

                let req = AutoTranscribeRequest {
                    session_id: id.to_string(),
                    session_dir,
                    language,
                    source_meta,
                    extraction_url: extraction_url.unwrap(),
                    extraction_key: extraction_key.unwrap(),
                    file_drop_url,
                    file_drop_api_key,
                    diarize,
                    people_recognition,
                    match_threshold,
                };
                auto_transcribe(req);
            }
        }
    }

    Ok(RecordingStopped {
        status: "stopped",
        files,
    })
}

/// All the inputs the transcription pipeline needs, snapshotted at stop time
/// so the spawned task doesn't have to re-read settings.
///
/// The actual pipeline kickoff (`run_transcription_pipeline` + the summary
/// follow-up) still lives in `server::routes`; the REST handler passes a
/// closure into `stop_recording` that spawns it. This lets us extract
/// `stop_recording` now without also untangling the 100+ lines of pipeline
/// plumbing in this commit.
pub struct AutoTranscribeRequest {
    pub session_id: String,
    pub session_dir: std::path::PathBuf,
    pub language: String,
    pub source_meta: Vec<crate::session::session::SourceMetadata>,
    pub extraction_url: String,
    pub extraction_key: String,
    pub file_drop_url: String,
    pub file_drop_api_key: String,
    pub diarize: bool,
    pub people_recognition: bool,
    pub match_threshold: f64,
}

// Re-export a helper used by the route layer to log consistent messages.
pub fn log_auto_transcribe_completed(session_id: &str) {
    info!("Auto-transcription completed for session {}", session_id);
}

pub fn log_auto_transcribe_failed(session_id: &str, err: &str) {
    error!("Auto-transcription failed for session {}: {}", session_id, err);
}
