//! Resumable, non-destructive import of existing local meeting snapshots.
use super::{platform::PlatformClient, routes::AppState};
use crate::session::session::{SessionInfo, SessionState};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tokio::{io::AsyncReadExt, sync::Mutex};

const CHECKPOINT: &str = ".gday-migration.json";
const MAX_AUDIO: u64 = 500_000_000;
const MAX_SNAPSHOT: u64 = 19 * 1024 * 1024;

#[derive(Clone, Default, Serialize)]
struct Progress {
    running: bool,
    total: usize,
    completed: usize,
    current: Option<String>,
    results: Vec<MigrationResult>,
}
#[derive(Clone, Serialize)]
struct MigrationResult {
    id: String,
    title: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}
fn progress() -> &'static Mutex<Progress> {
    static STATUS: OnceLock<Mutex<Progress>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(Progress::default()))
}
#[derive(Default, Serialize, Deserialize)]
struct Checkpoint {
    origin: String,
    import_key: String,
    #[serde(default)]
    uploads: BTreeMap<String, Uploaded>,
    #[serde(default)]
    meeting_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Uploaded {
    sha256: String,
    size: u64,
    url: String,
}
#[derive(Clone)]
struct LocalFile {
    name: String,
    path: PathBuf,
    size: u64,
    audio: bool,
}
struct Snapshot {
    body: Value,
    key: String,
    files: Vec<LocalFile>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/gday/migration/preview", get(preview))
        .route("/gday/migration/status", get(status))
        .route("/gday/migration", post(start))
}
async fn status() -> Json<Progress> {
    Json(progress().lock().await.clone())
}
fn title(session: &SessionInfo) -> String {
    session.name.clone().unwrap_or_else(|| session.id.clone())
}
fn fail(message: impl Into<String>, code: StatusCode) -> Response {
    (code, Json(json!({"error":message.into()}))).into_response()
}
async fn preview(State(state): State<AppState>) -> Json<Value> {
    let (sessions, _) = state
        .session_manager
        .list_sessions(usize::MAX, 0, &HashSet::new())
        .await;
    let mut blocked = Vec::new();
    let mut ready = 0;
    let mut audio_bytes = 0;
    for session in &sessions {
        match inspect(session, &state.session_manager.session_dir(&session.id)).await {
            Ok(files) => {
                ready += 1;
                audio_bytes += files
                    .iter()
                    .filter(|f| f.audio)
                    .map(|f| f.size)
                    .sum::<u64>();
            }
            Err(error) => {
                blocked.push(json!({"id":session.id,"title":title(session),"error":error}))
            }
        }
    }
    Json(json!({"total":sessions.len(),"ready":ready,"blocked":blocked,"audioBytes":audio_bytes}))
}
async fn start(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(error) = super::gday_auth::request_origin(&headers) {
        return fail(error, StatusCode::FORBIDDEN);
    }
    let Some(origin) = state.gday_auth.connected_origin().await else {
        return fail("Sign in to Gday Meetings first", StatusCode::UNAUTHORIZED);
    };
    if let Err(error) = state.gday_auth.access_token(&origin).await {
        return fail(error, StatusCode::UNAUTHORIZED);
    }
    let client = PlatformClient::for_user(&origin, state.gday_auth.clone());
    if let Err(error) = client.ensure_import_available().await {
        return fail(error, StatusCode::BAD_REQUEST);
    }
    let mut current = progress().lock().await;
    if current.running {
        return fail("A migration is already running", StatusCode::CONFLICT);
    }
    *current = Progress {
        running: true,
        ..Default::default()
    };
    let response = current.clone();
    drop(current);
    tokio::spawn(async move {
        run(state, origin).await;
    });
    (StatusCode::ACCEPTED, Json(response)).into_response()
}
async fn run(state: AppState, origin: String) {
    let (sessions, _) = state
        .session_manager
        .list_sessions(usize::MAX, 0, &HashSet::new())
        .await;
    progress().lock().await.total = sessions.len();
    let client = PlatformClient::for_user(&origin, state.gday_auth.clone());
    for session in sessions {
        progress().lock().await.current = Some(title(&session));
        let result = import_one(&state, &client, &origin, &session).await;
        let mut status = progress().lock().await;
        status.completed += 1;
        status.results.push(MigrationResult {
            id: session.id.clone(),
            title: title(&session),
            status: match &result {
                Ok(true) => "already_imported",
                Ok(false) => "imported",
                Err(_) => "failed",
            }
            .into(),
            error: result.err(),
        });
    }
    let mut status = progress().lock().await;
    status.running = false;
    status.current = None;
}
async fn inspect(session: &SessionInfo, directory: &Path) -> Result<Vec<LocalFile>, String> {
    if session.state == SessionState::Recording
        || session.processing_state.is_some()
        || session.summary_started_at.is_some()
    {
        return Err("Meeting is recording or processing; retry when it finishes".into());
    }
    let mut entries = tokio::fs::read_dir(directory)
        .await
        .map_err(|_| "Cannot read meeting directory")?;
    let mut files = Vec::new();
    let mut artifact_bytes = 0;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| "Cannot read meeting files")?
    {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Meeting contains an invalid filename")?;
        if name == CHECKPOINT || name == ".DS_Store" {
            continue;
        }
        let kind = entry
            .file_type()
            .await
            .map_err(|_| "Cannot inspect meeting file")?;
        if !kind.is_file() {
            return Err(format!("{name}: only regular files can be migrated; links and subdirectories require review"));
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_lowercase();
        let audio = matches!(
            extension.as_str(),
            "wav" | "flac" | "mp3" | "m4a" | "ogg" | "opus" | "mp4" | "webm" | "aac"
        );
        if !audio && (!matches!(extension.as_str(), "json" | "md" | "txt") || name.starts_with('.'))
        {
            return Err(format!(
                "Unsupported meeting file {name}; retained locally for review"
            ));
        }
        let size = entry
            .metadata()
            .await
            .map_err(|_| "Cannot inspect file size")?
            .len();
        if audio && (size == 0 || size > MAX_AUDIO) {
            return Err(format!("{name}: audio must be nonempty and no larger than 500 MB; compress it before migration"));
        }
        if !audio {
            artifact_bytes += size;
        }
        files.push(LocalFile {
            name,
            path: entry.path(),
            size,
            audio,
        });
    }
    if artifact_bytes > MAX_SNAPSHOT {
        return Err(
            "Meeting documents exceed the import request limit; retained locally for review".into(),
        );
    }
    if !files.iter().any(|file| file.name == "metadata.json") {
        return Err("Meeting metadata.json is missing".into());
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    for source in &session.source_meta {
        if !source.filename.is_empty() && !files.iter().any(|file| file.name == source.filename) {
            return Err(format!("Recording {} is missing", source.filename));
        }
    }
    Ok(files)
}
async fn checksum(path: &Path) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| "Unable to open recording for verification")?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        let size = file
            .read(&mut buffer)
            .await
            .map_err(|_| "Unable to verify recording bytes")?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn people_ids(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Object(values) => {
            if let Some(id) = values.get("person_id").and_then(Value::as_str) {
                if !id.is_empty() {
                    ids.insert(id.into());
                }
            }
            for child in values.values() {
                people_ids(child, ids);
            }
        }
        Value::Array(values) => {
            for child in values {
                people_ids(child, ids);
            }
        }
        _ => {}
    }
}
async fn snapshot(state: &AppState, session: &SessionInfo) -> Result<Snapshot, String> {
    let files = inspect(session, &state.session_manager.session_dir(&session.id)).await?;
    let mut artifacts = BTreeMap::new();
    let mut audio = Vec::new();
    for file in &files {
        if file.audio {
            audio.push(
                json!({"filename":file.name,"size":file.size,"sha256":checksum(&file.path).await?}),
            );
        } else {
            let bytes = tokio::fs::read(&file.path)
                .await
                .map_err(|_| format!("Cannot read {}", file.name))?;
            let value = if file.name.ends_with(".json") {
                serde_json::from_slice(&bytes).map_err(|_| {
                    format!("{} is invalid JSON; fix it before migration", file.name)
                })?
            } else {
                Value::String(
                    String::from_utf8(bytes)
                        .map_err(|_| format!("{} is not UTF-8 text", file.name))?,
                )
            };
            artifacts.insert(file.name.clone(), value);
        }
    }
    let mut ids = BTreeSet::new();
    if let Some(transcript) = artifacts.get("transcript.json") {
        people_ids(transcript, &mut ids);
    }
    let mut people = BTreeMap::new();
    let mut missing = Vec::new();
    for id in ids {
        if id.contains('/') || id.contains('\\') || id == "." || id == ".." {
            return Err("Invalid speaker identity path in transcript".into());
        }
        let mut person = serde_json::Map::new();
        for name in ["profile.json", "embeddings.json"] {
            let path = state.people_manager.people_dir().join(&id).join(name);
            match tokio::fs::read(path).await {
                Ok(bytes) => {
                    person.insert(
                        name.into(),
                        serde_json::from_slice::<Value>(&bytes)
                            .map_err(|_| format!("Speaker {id}/{name} is invalid JSON"))?,
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(format!("{id}/{name}"))
                }
                Err(_) => return Err(format!("Unable to read speaker {id}/{name}")),
            }
        }
        people.insert(id, Value::Object(person));
    }
    let mut tags = Vec::new();
    for name in &session.tags {
        if let Some(tag) = state.tags_manager.get_tag(name).await {
            tags.push(tag);
        }
    }
    let mut body = json!({"externalId":session.id,"title":title(session),"recordedAt":session.created_at.to_rfc3339(),
        "metadata":{"session":session,"people":people,"tags":tags,"missingSpeakerFiles":missing},"artifacts":artifacts,"audio":audio});
    let bytes = serde_json::to_vec(&body).map_err(|_| "Cannot serialize meeting snapshot")?;
    if bytes.len() as u64 > MAX_SNAPSHOT {
        return Err("Meeting snapshot exceeds import limit; retained locally for review".into());
    }
    let key = format!("{:x}", Sha256::digest(&bytes));
    body["importKey"] = json!(key);
    Ok(Snapshot { body, key, files })
}
fn verify_response(value: &Value, snapshot: &Snapshot) -> Result<String, String> {
    if value["externalId"] != snapshot.body["externalId"]
        || value["importKey"] != snapshot.key
        || value["audioCount"].as_u64()
            != Some(snapshot.body["audio"].as_array().unwrap().len() as u64)
        || value["artifactCount"].as_u64()
            != Some(snapshot.body["artifacts"].as_object().unwrap().len() as u64)
    {
        return Err(
            "Gday meeting differs from the local snapshot; review it before migrating again".into(),
        );
    }
    value["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Import verification missing meeting ID".into())
}
async fn import_one(
    state: &AppState,
    client: &PlatformClient,
    origin: &str,
    session: &SessionInfo,
) -> Result<bool, String> {
    let mut snap = snapshot(state, session).await?;
    let checkpoint_path = state
        .session_manager
        .session_dir(&session.id)
        .join(CHECKPOINT);
    let mut checkpoint: Checkpoint = if checkpoint_path.exists() {
        crate::storage::read_json(&checkpoint_path)?
    } else {
        Checkpoint::default()
    };
    if checkpoint.origin != origin || checkpoint.import_key != snap.key {
        checkpoint = Checkpoint {
            origin: origin.into(),
            import_key: snap.key.clone(),
            ..Default::default()
        };
    }
    if let Some(existing) = client.imported_meeting(&session.id).await? {
        checkpoint.meeting_id = Some(verify_response(&existing, &snap)?);
        crate::storage::write_json(&checkpoint_path, &checkpoint)?;
        return Ok(true);
    }
    for file in snap.files.iter().filter(|file| file.audio) {
        let entry = snap.body["audio"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["filename"] == file.name)
            .unwrap();
        let hash = entry["sha256"].as_str().unwrap().to_owned();
        let url = if let Some(cached) = checkpoint
            .uploads
            .get(&file.name)
            .filter(|cached| cached.sha256 == hash && cached.size == file.size)
        {
            cached.url.clone()
        } else {
            let url = client.upload_file(&file.name, &file.path).await?;
            checkpoint.uploads.insert(
                file.name.clone(),
                Uploaded {
                    sha256: hash,
                    size: file.size,
                    url: url.clone(),
                },
            );
            crate::storage::write_json(&checkpoint_path, &checkpoint)?;
            url
        };
        entry["url"] = json!(url);
    }
    // A local editor/recorder may have changed the source during upload. Never certify a mixed snapshot.
    let current = state
        .session_manager
        .get_session(&session.id)
        .await
        .ok_or("Meeting was removed during migration")?;
    if snapshot(state, &current).await?.key != snap.key {
        return Err(
            "Meeting changed during upload; retry migration to copy its current contents".into(),
        );
    }
    let result = client.import_meeting(&snap.body).await?;
    verify_response(&result, &snap)?;
    let verified = client
        .imported_meeting(&session.id)
        .await?
        .ok_or("Imported meeting not found during verification")?;
    checkpoint.meeting_id = Some(verify_response(&verified, &snap)?);
    crate::storage::write_json(&checkpoint_path, &checkpoint)?;
    Ok(false)
}

#[cfg(test)]
#[path = "gday_migration_tests.rs"]
mod tests;
