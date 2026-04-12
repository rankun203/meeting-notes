use meeting_notes_daemon::services::summary as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;
use tracing::{error, info};

#[tauri::command]
pub async fn mn_get_summary(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_summary(&state, &id).await
}

#[tauri::command]
pub async fn mn_update_summary(
    state: State<'_, AppState>,
    id: String,
    input: svc::UpdateSummaryInput,
) -> Result<Value, ServiceError> {
    svc::update_summary(&state, &id, input).await
}

#[tauri::command]
pub async fn mn_delete_summary(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ServiceError> {
    svc::delete_summary(&state, &id).await
}

#[tauri::command]
pub async fn mn_get_session_todos(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_session_todos(&state, &id).await
}

#[tauri::command]
pub async fn mn_toggle_todo(
    state: State<'_, AppState>,
    id: String,
    idx: usize,
) -> Result<Value, ServiceError> {
    svc::toggle_todo(&state, &id, idx).await
}

#[tauri::command]
pub async fn mn_get_person_todos(
    state: State<'_, AppState>,
    person_id: String,
) -> Result<Value, ServiceError> {
    svc::get_person_todos(&state, &person_id).await
}

#[tauri::command]
pub async fn mn_summarize_session(
    state: State<'_, AppState>,
    id: String,
    input: Option<svc::SummarizeInput>,
) -> Result<svc::SummarizeAccepted, ServiceError> {
    let session_manager = state.session_manager.clone();
    let people_manager = state.people_manager.clone();
    let tags_mgr = state.tags_manager.clone();
    let files_db = state.files_db.clone();

    svc::summarize_session(
        &state,
        &id,
        input.unwrap_or_default(),
        move |args| {
            tokio::spawn(async move {
                session_manager
                    .emit_summary_progress(&args.session_id, "summarizing")
                    .await;
                let session_info = session_manager.get_session(&args.session_id).await;
                match meeting_notes_daemon::chat::summarize::run_summarization(
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
                        meeting_notes_daemon::markdown::write_recordings_index(
                            &recordings_dir,
                            &mut sessions,
                        );
                        info!("Summary generated for session {}", args.session_id);
                    }
                    Err(e) => {
                        error!("Summary failed for session {}: {}", args.session_id, e);
                        session_manager
                            .emit_summary_failed(&args.session_id, &e)
                            .await;
                    }
                }
            });
        },
    )
    .await
}
