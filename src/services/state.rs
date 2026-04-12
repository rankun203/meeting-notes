use std::sync::Arc;

use crate::chat::manager::ConversationManager;
use crate::filesdb::FilesDb;
use crate::llm::claude_code::ClaudeCodeRunner;
use crate::llm::secrets::SharedSecrets;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::settings::SharedSettings;
use crate::tags::TagsManager;
use crate::tracing_setup::TracingHandle;

/// Shared application state passed into every service function.
///
/// Held as `axum::extract::State<AppState>` in the REST layer and as
/// `tauri::State<'_, AppState>` in the desktop app — both wrap the same
/// underlying managers so the services layer is transport-agnostic.
#[derive(Clone)]
pub struct AppState {
    pub session_manager: SessionManager,
    pub people_manager: PeopleManager,
    pub settings: SharedSettings,
    pub files_db: FilesDb,
    pub tags_manager: TagsManager,
    pub conversation_manager: ConversationManager,
    pub llm_secrets: SharedSecrets,
    pub claude_runner: ClaudeCodeRunner,
    /// Root of the app's persistent state (recordings, settings, logs, …).
    /// Exposed for services::diagnostics and anywhere a service needs to
    /// know where data lives without plumbing it as a separate arg.
    pub data_dir: std::path::PathBuf,
    /// Shared handle for the rotating file logger. `Arc` so cloning
    /// `AppState` (axum does it per-request) doesn't try to duplicate the
    /// underlying worker guard.
    pub tracing: Arc<TracingHandle>,
}

impl AppState {
    /// Regenerate the recordings/index.md file in the background.
    pub fn refresh_recordings_index(&self) {
        let recordings_dir = self.files_db.recordings_dir().to_path_buf();
        let session_manager = self.session_manager.clone();
        tokio::spawn(async move {
            let mut sessions = session_manager.session_entries().await;
            let _ = tokio::task::spawn_blocking(move || {
                crate::markdown::write_recordings_index(&recordings_dir, &mut sessions);
            })
            .await;
        });
    }

    /// Regenerate the people/index.md file in the background.
    pub fn refresh_people_index(&self) {
        let people_manager = self.people_manager.clone();
        tokio::spawn(async move {
            let mut people = people_manager.person_entries().await;
            let people_dir = people_manager.people_dir().to_path_buf();
            let _ = tokio::task::spawn_blocking(move || {
                crate::markdown::write_people_index(&people_dir, &mut people);
            })
            .await;
        });
    }
}
