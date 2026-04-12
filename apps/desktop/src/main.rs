//! VoiceRecords (主簿) — Tauri desktop entry point.
//!
//! Mirrors the bootstrap sequence in `meeting-notes-daemon`'s `src/main.rs`:
//! build every manager, load state from disk, regenerate markdown indexes,
//! resume any in-flight transcription jobs, and then hand control to Tauri.
//! Every service function in `meeting_notes_daemon::services::*` is exposed
//! as a `mn_*` Tauri command so the existing webui can reach the same
//! business logic through `invoke()` instead of `fetch()`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use meeting_notes_daemon::chat::manager::ConversationManager;
use meeting_notes_daemon::filesdb::FilesDb;
use meeting_notes_daemon::llm::secrets::LlmSecrets;
use meeting_notes_daemon::people::PeopleManager;
use meeting_notes_daemon::services::AppState;
use meeting_notes_daemon::session::SessionManager;
use meeting_notes_daemon::settings::AppSettings;
use meeting_notes_daemon::tags::TagsManager;
use tauri::{Emitter, Manager};
use tracing::info;

mod commands;

const APP_NAME: &str = "org.rankun.meeting-notes";

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".local/share")
        .join(APP_NAME)
}

/// Bootstrap every manager and load state from disk. Runs synchronously
/// (via `tauri::async_runtime::block_on`) before `tauri::Builder::run()`
/// so that AppState is guaranteed to be `.manage()`d before the webview
/// can dispatch its first command. Otherwise `mn_list_sessions` (called by
/// the Sidebar's useWebSocket hook on mount) could race the bootstrap.
async fn bootstrap() -> AppState {
    let data_dir = default_data_dir();
    let recordings_dir = data_dir.join("recordings");
    std::fs::create_dir_all(&recordings_dir)
        .expect("failed to create recordings directory");

    let data_dir = std::fs::canonicalize(&data_dir).unwrap_or(data_dir);
    let recordings_dir = std::fs::canonicalize(&recordings_dir).unwrap_or(recordings_dir);

    info!("VoiceRecords starting — data dir: {}", data_dir.display());

    let session_manager = SessionManager::new(recordings_dir.clone());
    session_manager.load_from_disk().await;
    session_manager.start_file_size_ticker();

    let people_manager = PeopleManager::new(&data_dir);
    people_manager.load_from_disk().await;

    let tags_manager = TagsManager::new(&data_dir);
    tags_manager.load_from_disk().await;

    let files_db = FilesDb::load_from_disk(&recordings_dir).await;

    let settings = AppSettings::load_or_create(&data_dir);
    let shared_settings = std::sync::Arc::new(tokio::sync::RwLock::new(settings));

    let llm_secrets = LlmSecrets::load_or_create(&data_dir);
    let shared_secrets = std::sync::Arc::new(tokio::sync::RwLock::new(llm_secrets));

    let conversation_manager = ConversationManager::new(&data_dir);

    // Regenerate markdown indexes so Claude Code integrations stay in sync.
    {
        let self_intro = shared_settings.read().await.chat_self_intro.clone();
        meeting_notes_daemon::markdown::write_claude_md(&data_dir, self_intro.as_deref());
    }
    {
        use meeting_notes_daemon::markdown;
        let mut sessions = session_manager.session_entries().await;
        let mut people = people_manager.person_entries().await;
        let people_dir = people_manager.people_dir().to_path_buf();
        let rec_dir = recordings_dir.clone();
        let _ = tokio::task::spawn_blocking(move || {
            markdown::write_recordings_index(&rec_dir, &mut sessions);
            markdown::write_people_index(&people_dir, &mut people);
        })
        .await;
    }

    // Resume any in-flight extraction jobs from before restart.
    meeting_notes_daemon::services::transcripts::resume_pending_extractions(
        session_manager.clone(),
        people_manager.clone(),
        files_db.clone(),
        shared_settings.clone(),
        shared_secrets.clone(),
        tags_manager.clone(),
    )
    .await;

    let claude_runner =
        meeting_notes_daemon::llm::claude_code::ClaudeCodeRunner::new(&data_dir);

    AppState {
        session_manager,
        people_manager,
        settings: shared_settings,
        files_db,
        tags_manager,
        conversation_manager,
        llm_secrets: shared_secrets,
        claude_runner,
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "voicerecords=info,meeting_notes_daemon=info".into()),
        )
        .init();

    let state = tauri::async_runtime::block_on(bootstrap());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(|app| {
            // Apply macOS vibrancy (NSVisualEffectView Sidebar material) so
            // the window chrome blends with the OS — matches the 1Password
            // 8 look.
            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                if let Some(win) = app.get_webview_window("main") {
                    let _ = apply_vibrancy(&win, NSVisualEffectMaterial::Sidebar, None, None);
                }
            }

            // Bridge the SessionManager broadcast channel into Tauri app
            // events so the webui's useWebSocket hook can receive
            // session-updated / transcription-progress / etc. via the same
            // payload shapes the axum WS route forwards.
            let state = app.state::<AppState>();
            let mut rx = state.session_manager.subscribe();
            let handle_ev = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let _ = handle_ev.emit("mn:server-event", event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "Tauri event bridge lagged, skipped {} events",
                                n
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            info!("VoiceRecords ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::sessions::mn_create_session,
            commands::sessions::mn_list_sessions,
            commands::sessions::mn_get_session,
            commands::sessions::mn_update_session,
            commands::sessions::mn_delete_session,
            commands::sessions::mn_start_recording,
            commands::sessions::mn_stop_recording,
            commands::files::mn_list_files,
            commands::files::mn_resolve_session_file,
            commands::files::mn_get_waveform,
            commands::transcripts::mn_get_transcript,
            commands::transcripts::mn_delete_transcript,
            commands::transcripts::mn_get_attribution,
            commands::transcripts::mn_update_attribution,
            commands::transcripts::mn_transcribe_session,
            commands::summary::mn_get_summary,
            commands::summary::mn_update_summary,
            commands::summary::mn_delete_summary,
            commands::summary::mn_get_session_todos,
            commands::summary::mn_toggle_todo,
            commands::summary::mn_get_person_todos,
            commands::summary::mn_summarize_session,
            commands::people::mn_list_people,
            commands::people::mn_create_person,
            commands::people::mn_get_person,
            commands::people::mn_update_person,
            commands::people::mn_delete_person,
            commands::people::mn_get_person_sessions,
            commands::tags::mn_list_tags,
            commands::tags::mn_create_tag,
            commands::tags::mn_update_tag,
            commands::tags::mn_delete_tag,
            commands::tags::mn_get_tag_sessions,
            commands::tags::mn_set_session_tags,
            commands::settings::mn_get_settings,
            commands::settings::mn_update_settings,
            commands::config::mn_get_config,
            commands::config::mn_get_app_info,
            commands::chat::mn_list_conversations,
            commands::chat::mn_create_conversation,
            commands::chat::mn_get_conversation,
            commands::chat::mn_delete_conversation,
            commands::chat::mn_delete_message,
            commands::chat::mn_sync_claude_messages,
            commands::chat::mn_export_prompt,
            commands::chat::mn_list_models,
            commands::chat::mn_send_message,
            commands::claude::mn_claude_status,
            commands::claude::mn_claude_stop,
            commands::claude::mn_claude_approve_tools,
            commands::claude::mn_claude_list_sessions,
            commands::claude::mn_claude_get_session,
            commands::claude::mn_claude_send,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VoiceRecords");
}
