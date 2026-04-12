use meeting_notes_daemon::services::files as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::ipc::Response;
use tauri::State;

#[tauri::command]
pub async fn mn_list_files(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>, ServiceError> {
    svc::list_files(&state, &id).await
}

/// Return the validated absolute path of a session file so the frontend
/// can use Tauri's `convertFileSrc` / asset protocol to stream it. Matches
/// the security check the REST handler does before letting tower-http
/// serve the file.
#[tauri::command]
pub async fn mn_resolve_session_file(
    state: State<'_, AppState>,
    id: String,
    filename: String,
) -> Result<String, ServiceError> {
    let path = svc::resolve_session_file(&state, &id, &filename).await?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn mn_get_waveform(
    state: State<'_, AppState>,
    id: String,
    filename: String,
) -> Result<Value, ServiceError> {
    let waveform = svc::get_waveform(&state, &id, &filename).await?;
    Ok(serde_json::to_value(waveform).unwrap())
}

/// Return the raw bytes of a session file. `tauri::ipc::Response` lets
/// us send the bytes as a binary IPC payload rather than a JSON array
/// of numbers — on the JS side the invoke resolves to an ArrayBuffer
/// directly, so a 10 MB audio file is a single 10 MB transfer and
/// wraps trivially into a Blob URL for the <audio> element.
#[tauri::command]
pub async fn mn_read_session_file(
    state: State<'_, AppState>,
    id: String,
    filename: String,
) -> Result<Response, ServiceError> {
    let bytes = svc::read_file_bytes(&state, &id, &filename).await?;
    Ok(Response::new(bytes))
}
