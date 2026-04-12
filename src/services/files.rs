//! File service — list/serve/waveform operations against session recordings.
//!
//! `serve_file` intentionally returns a validated `PathBuf` rather than a
//! response object: the axum handler wraps it in `ServeFile` (for Range
//! support), and the Tauri command layer will return the bytes directly.

use std::path::PathBuf;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;
use crate::waveform::WaveformData;

pub async fn list_files(state: &AppState, id: &str) -> ServiceResult<Vec<String>> {
    state
        .session_manager
        .get_files(id)
        .await
        .map_err(ServiceError::NotFound)
}

/// Validate `{session_id, filename}` and return the absolute path that the
/// transport layer should serve. Rejects path-traversal attempts.
pub async fn resolve_session_file(
    state: &AppState,
    id: &str,
    filename: &str,
) -> ServiceResult<PathBuf> {
    let files = state
        .session_manager
        .get_files(id)
        .await
        .map_err(ServiceError::NotFound)?;

    if !files.contains(&filename.to_string()) {
        return Err(ServiceError::NotFound("file not found".into()));
    }

    let safe_name = std::path::Path::new(filename)
        .file_name()
        .ok_or_else(|| ServiceError::BadRequest("invalid filename".into()))?;

    Ok(state.session_manager.session_dir(id).join(safe_name))
}

pub async fn get_waveform(
    state: &AppState,
    id: &str,
    filename: &str,
) -> ServiceResult<WaveformData> {
    // Validate the file belongs to the session before spending CPU on it.
    resolve_session_file(state, id, filename).await?;

    let session_dir = state.session_manager.session_dir(id);
    let filename_owned = filename.to_string();

    tokio::task::spawn_blocking(move || {
        crate::waveform::get_or_generate_waveform(&session_dir, &filename_owned)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("join: {e}")))?
    .map_err(ServiceError::Internal)
}
