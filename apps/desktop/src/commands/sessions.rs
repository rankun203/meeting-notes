use meeting_notes_daemon::services::sessions as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use meeting_notes_daemon::session::config::SessionConfig;
use meeting_notes_daemon::session::session::SessionInfo;
use tauri::State;
use tracing::{error, info};

#[tauri::command]
pub async fn mn_create_session(
    state: State<'_, AppState>,
    config: SessionConfig,
) -> Result<SessionInfo, ServiceError> {
    svc::create_session(&state, config).await
}

#[tauri::command]
pub async fn mn_list_sessions(
    state: State<'_, AppState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<svc::SessionListPage, ServiceError> {
    svc::list_sessions(&state, svc::ListParams { limit, offset }).await
}

#[tauri::command]
pub async fn mn_get_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionInfo, ServiceError> {
    svc::get_session(&state, &id).await
}

#[tauri::command]
pub async fn mn_update_session(
    state: State<'_, AppState>,
    id: String,
    input: svc::UpdateSessionInput,
) -> Result<SessionInfo, ServiceError> {
    svc::update_session(&state, &id, input).await
}

#[tauri::command]
pub async fn mn_delete_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ServiceError> {
    svc::delete_session(&state, &id).await
}

#[tauri::command]
pub async fn mn_start_recording(
    state: State<'_, AppState>,
    id: String,
) -> Result<svc::RecordingStarted, ServiceError> {
    svc::start_recording(&state, &id).await
}

#[tauri::command]
pub async fn mn_stop_recording(
    state: State<'_, AppState>,
    id: String,
) -> Result<svc::RecordingStopped, ServiceError> {
    use meeting_notes_daemon::services::{summary, transcripts};

    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let files_db = state.files_db.clone();
    let settings_clone = state.settings.clone();
    let llm_secrets = state.llm_secrets.clone();
    let tags_mgr = state.tags_manager.clone();

    svc::stop_recording(&state, &id, move |req| {
        tokio::spawn(async move {
            let result = transcripts::run_transcription_pipeline(
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
                    info!(
                        "Auto-transcription completed for session {}",
                        req.session_id
                    );
                    summary::maybe_auto_summarize(
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
                    error!(
                        "Auto-transcription failed for session {}: {}",
                        req.session_id, e
                    );
                    session_manager
                        .set_processing_state(&req.session_id, None)
                        .await;
                    session_manager
                        .emit_transcription_failed(&req.session_id, &e);
                }
            }
        });
    })
    .await
}
