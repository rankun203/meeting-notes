use meeting_notes_daemon::services::transcripts as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;
use tracing::{error, info};

#[tauri::command]
pub async fn mn_get_transcript(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_transcript(&state, &id).await
}

#[tauri::command]
pub async fn mn_delete_transcript(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ServiceError> {
    svc::delete_transcript(&state, &id).await
}

#[tauri::command]
pub async fn mn_get_attribution(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_attribution(&state, &id).await
}

#[tauri::command]
pub async fn mn_update_attribution(
    state: State<'_, AppState>,
    id: String,
    body: svc::AttributionRequest,
) -> Result<(), ServiceError> {
    svc::update_attribution(&state, &id, body).await
}

#[tauri::command]
pub async fn mn_transcribe_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<svc::TranscribeAccepted, ServiceError> {
    use meeting_notes_daemon::services::summary;

    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let files_db = state.files_db.clone();
    let settings_clone = state.settings.clone();
    let llm_secrets = state.llm_secrets.clone();
    let tags_mgr = state.tags_manager.clone();

    svc::transcribe_session(&state, &id, move |args| {
        tokio::spawn(async move {
            let result = svc::run_transcription_pipeline(
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
                    summary::maybe_auto_summarize(
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
    .await
}
