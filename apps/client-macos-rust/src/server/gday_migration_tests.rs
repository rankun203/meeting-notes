use super::*;
use axum::{body::Bytes, extract::State as AxumState};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("gday-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn auth_file(directory: &Path, origin: &str) {
    crate::storage::write_json(
        &directory.join("gday-auth.json"),
        &json!({
            "origin": origin, "issuer":format!("{origin}/api/auth"), "client_id":"migration-test",
            "token_endpoint":format!("{origin}/api/auth/token"), "revocation_endpoint":null,
            "access_token":"synthetic-migration-token", "refresh_token":null,
            "expires_at":chrono::Utc::now().timestamp()+3600, "subject":"fixture-user", "email":null
        }),
    )
    .unwrap();
}

async fn meeting(state: &AppState) -> SessionInfo {
    state
        .session_manager
        .create_session(crate::session::config::SessionConfig::default())
        .await
}

#[tokio::test]
async fn audio_less_snapshot_preserves_documents_notes_and_speaker_files() {
    let directory = Directory::new();
    let state = test_state(&directory.0, "http://127.0.0.1:1");
    let session = meeting(&state).await;
    let session = state
        .session_manager
        .update_session_notes(&session.id, Some("Keep these meeting notes".into()))
        .await
        .unwrap();
    let path = state.session_manager.session_dir(&session.id);
    let artifacts = [
        (
            "transcript.json",
            json!({"segments":[{"text":"Meeting transcript", "person_id":"speaker-fixture"}]}),
        ),
        (
            "summary.json",
            json!({"summary":"Existing generated summary"}),
        ),
        (
            "extraction.json",
            json!({"model":"original-model", "tracks":{}}),
        ),
    ];
    for (name, value) in &artifacts {
        crate::storage::write_json(&path.join(name), value).unwrap();
    }
    std::fs::write(path.join("summary.md"), "# Existing summary\n").unwrap();
    std::fs::write(path.join("references.txt"), "Existing reference").unwrap();
    let person = state.people_manager.people_dir().join("speaker-fixture");
    std::fs::create_dir_all(&person).unwrap();
    crate::storage::write_json(
        &person.join("profile.json"),
        &json!({"name":"Fixture person"}),
    )
    .unwrap();
    crate::storage::write_json(
        &person.join("embeddings.json"),
        &json!({"embeddings":[[0.1,0.2]]}),
    )
    .unwrap();
    let snap = snapshot(&state, &session).await.unwrap();
    assert_eq!(snap.body["audio"], json!([]));
    for (name, value) in &artifacts {
        assert_eq!(&snap.body["artifacts"][name], value);
    }
    assert_eq!(
        snap.body["artifacts"]["metadata.json"]["notes"],
        "Keep these meeting notes"
    );
    assert_eq!(snap.body["artifacts"]["summary.md"], "# Existing summary\n");
    assert_eq!(
        snap.body["artifacts"]["references.txt"],
        "Existing reference"
    );
    assert!(snap.body["artifacts"]["metadata.json"].is_object());
    assert_eq!(
        snap.body["metadata"]["people"]["speaker-fixture"]["profile.json"]["name"],
        "Fixture person"
    );
    assert_eq!(
        snap.body["metadata"]["people"]["speaker-fixture"]["embeddings.json"]["embeddings"],
        json!([[0.1, 0.2]])
    );
    assert_eq!(snap.key, snapshot(&state, &session).await.unwrap().key);
    std::fs::write(path.join("references.txt"), "Edited reference").unwrap();
    assert_ne!(snap.key, snapshot(&state, &session).await.unwrap().key);
}

#[tokio::test]
async fn inspection_blocks_recording_oversize_and_symlink_inputs() {
    let directory = Directory::new();
    let state = test_state(&directory.0, "http://127.0.0.1:1");
    let mut session = meeting(&state).await;
    let path = state.session_manager.session_dir(&session.id);
    session.state = SessionState::Recording;
    assert!(
        inspect(&session, &path)
            .await
            .err()
            .unwrap()
            .contains("recording")
    );
    session.state = SessionState::Stopped;
    let audio = std::fs::File::create(path.join("huge.wav")).unwrap();
    audio.set_len(MAX_AUDIO + 1).unwrap();
    assert!(
        inspect(&session, &path)
            .await
            .err()
            .unwrap()
            .contains("500 MB")
    );
    drop(audio);
    std::fs::remove_file(path.join("huge.wav")).unwrap();
    let artifact = std::fs::File::create(path.join("huge.txt")).unwrap();
    artifact.set_len(MAX_SNAPSHOT + 1).unwrap();
    assert!(
        inspect(&session, &path)
            .await
            .err()
            .unwrap()
            .contains("limit")
    );
    drop(artifact);
    std::fs::remove_file(path.join("huge.txt")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(path.join("metadata.json"), path.join("linked.json")).unwrap();
        assert!(
            inspect(&session, &path)
                .await
                .err()
                .unwrap()
                .contains("regular files")
        );
    }
}

#[derive(Clone, Default)]
struct Service {
    uploads: Arc<AtomicUsize>,
    posts: Arc<AtomicUsize>,
    // 1: fail before commit; 2: commit but lose acknowledgement; 0: success.
    failure: Arc<AtomicUsize>,
    existing: Arc<Mutex<Option<Value>>>,
    bodies: Arc<Mutex<Vec<Value>>>,
}
fn authorized(headers: &HeaderMap) {
    assert_eq!(headers["authorization"], "Bearer synthetic-migration-token");
}
async fn upload(
    AxumState(service): AxumState<Service>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
    authorized(&headers);
    assert_eq!(body.as_ref(), b"original recording bytes");
    service.uploads.fetch_add(1, Ordering::SeqCst);
    Json(json!({"url":"/d/fixture.opus"}))
}
async fn lookup(AxumState(service): AxumState<Service>, headers: HeaderMap) -> Response {
    authorized(&headers);
    match service.existing.lock().await.clone() {
        Some(value) => Json(value).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn import(
    AxumState(service): AxumState<Service>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    authorized(&headers);
    service.posts.fetch_add(1, Ordering::SeqCst);
    service.bodies.lock().await.push(body.clone());
    let failure = service.failure.swap(0, Ordering::SeqCst);
    if failure == 1 {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let result = json!({"id":"imported-fixture", "externalId":body["externalId"], "importKey":body["importKey"],
        "audioCount":body["audio"].as_array().unwrap().len(), "artifactCount":body["artifacts"].as_object().unwrap().len()});
    *service.existing.lock().await = Some(result.clone());
    if failure == 2 {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    Json(result).into_response()
}

async fn transfer_retry(failure: usize) {
    let service = Service::default();
    service.failure.store(failure, Ordering::SeqCst);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/upload", post(upload))
        .route("/api/platform/meetings/import", post(import))
        .route("/api/platform/meetings/import/{id}", get(lookup))
        .with_state(service.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let directory = Directory::new();
    auth_file(&directory.0, &origin);
    let state = test_state(&directory.0, &origin);
    let session = meeting(&state).await;
    let path = state.session_manager.session_dir(&session.id);
    std::fs::write(path.join("mic.opus"), b"original recording bytes").unwrap();
    std::fs::write(path.join("summary.md"), "Existing summary").unwrap();
    let session = state
        .session_manager
        .get_session(&session.id)
        .await
        .unwrap();
    let original_metadata = std::fs::read(path.join("metadata.json")).unwrap();
    let client = PlatformClient::for_user(&origin, state.gday_auth.clone());
    if failure != 0 {
        assert!(
            import_one(&state, &client, &origin, &session)
                .await
                .is_err()
        );
        let saved: Checkpoint = crate::storage::read_json(&path.join(CHECKPOINT)).unwrap();
        assert_eq!(
            saved.uploads["mic.opus"].sha256,
            checksum(&path.join("mic.opus")).await.unwrap()
        );
        assert!(saved.meeting_id.is_none());
    }
    assert_eq!(
        import_one(&state, &client, &origin, &session)
            .await
            .unwrap(),
        failure == 2
    );
    assert_eq!(service.uploads.load(Ordering::SeqCst), 1);
    assert_eq!(
        service.posts.load(Ordering::SeqCst),
        if failure == 1 { 2 } else { 1 }
    );
    let bodies = service.bodies.lock().await;
    let body = bodies.last().unwrap();
    assert_eq!(
        body["audio"][0]["sha256"],
        "b54d397754d0850482876ce89f22d69158cdbc73c045498e9ad29c71fb9964ae"
    );
    assert_eq!(
        body["audio"][0]["sha256"],
        checksum(&path.join("mic.opus")).await.unwrap()
    );
    assert_eq!(body["audio"][0]["url"], format!("{origin}/d/fixture.opus"));
    assert_eq!(body["artifacts"]["summary.md"], "Existing summary");
    if failure == 1 {
        assert_eq!(
            bodies[0], bodies[1],
            "retry must reuse the exact uploaded snapshot"
        );
    }
    drop(bodies);
    let saved: Checkpoint = crate::storage::read_json(&path.join(CHECKPOINT)).unwrap();
    assert_eq!(saved.meeting_id.as_deref(), Some("imported-fixture"));
    assert!(
        import_one(&state, &client, &origin, &session)
            .await
            .unwrap()
    );
    assert_eq!(
        service.posts.load(Ordering::SeqCst),
        if failure == 1 { 2 } else { 1 }
    );
    assert_eq!(
        std::fs::read(path.join("mic.opus")).unwrap(),
        b"original recording bytes"
    );
    assert_eq!(
        std::fs::read(path.join("metadata.json")).unwrap(),
        original_metadata
    );
    assert_eq!(
        std::fs::read_to_string(path.join("summary.md")).unwrap(),
        "Existing summary"
    );
    service.existing.lock().await.as_mut().unwrap()["importKey"] = json!("different-snapshot");
    assert!(
        import_one(&state, &client, &origin, &session)
            .await
            .err()
            .unwrap()
            .contains("differs")
    );
    assert_eq!(service.uploads.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn import_roundtrip_verifies_copy_and_preserves_originals() {
    transfer_retry(0).await;
}
#[tokio::test]
async fn retry_reuses_uploaded_checkpoint_after_failed_import() {
    transfer_retry(1).await;
}
#[tokio::test]
async fn lost_acknowledgement_recovers_without_reupload_or_reimport() {
    transfer_retry(2).await;
}

fn test_state(directory: &Path, _origin: &str) -> AppState {
    AppState {
        session_manager: crate::session::SessionManager::new(directory.join("recordings")),
        people_manager: crate::people::PeopleManager::new(directory),
        tags_manager: crate::tags::TagsManager::new(directory),
        files_db: crate::filesdb::FilesDb::new(directory.join("recordings")),
        settings: Arc::new(tokio::sync::RwLock::new(
            crate::settings::AppSettings::load_or_create(directory),
        )),
        llm_secrets: Arc::new(tokio::sync::RwLock::new(
            crate::llm::secrets::LlmSecrets::load_or_create(directory),
        )),
        conversation_manager: crate::chat::manager::ConversationManager::new(directory),
        claude_runner: crate::llm::claude_code::ClaudeCodeRunner::new(directory),
        gday_auth: super::super::gday_auth::GdayAuth::load(directory),
    }
}
