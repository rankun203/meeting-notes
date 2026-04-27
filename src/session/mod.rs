pub mod config;
pub mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{RwLock, broadcast};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{info, warn};

use self::config::SessionConfig;
use self::session::{Session, SessionInfo, SessionMetadata, SessionState, Notice, NoticeLevel};
use crate::audio::mic::MicSource;
use crate::audio::recorder::Recorder;
use crate::audio::source::{AudioSource, SourceDescriptor, SourceType};
use crate::audio::system_audio::SystemAudioSource;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum ServerEvent {
    SessionCreated(SessionInfo),
    SessionUpdated(SessionInfo),
    SessionDeleted { id: String },
    FileSizes {
        id: String,
        file_sizes: HashMap<String, u64>,
        /// Seconds remaining before auto-stop triggers (None = not counting down).
        #[serde(skip_serializing_if = "Option::is_none")]
        auto_stop_remaining_secs: Option<u64>,
    },
    SessionNotice {
        id: String,
        notice: Notice,
    },
    /// Full replacement of a session's live notices list.
    SessionNotices {
        id: String,
        notices: Vec<Notice>,
    },
    TranscriptionProgress {
        id: String,
        status: String,
    },
    TranscriptionCompleted {
        id: String,
        unconfirmed_speakers: u32,
    },
    TranscriptionFailed {
        id: String,
        error: String,
    },
    SummaryProgress {
        id: String,
        status: String,
        started_at: DateTime<Utc>,
    },
    SummaryDelta {
        id: String,
        delta: String,
    },
    SummaryThinking {
        id: String,
        delta: String,
    },
    SummaryCompleted {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todos: Option<Value>,
    },
    SummaryFailed {
        id: String,
        error: String,
    },
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    output_dir: PathBuf,
    event_tx: broadcast::Sender<ServerEvent>,
}

impl SessionManager {
    pub fn new(output_dir: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            output_dir,
            event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_tx.subscribe()
    }

    fn emit(&self, event: ServerEvent) {
        let _ = self.event_tx.send(event);
    }

    fn write_metadata(session: &Session) -> Result<(), String> {
        std::fs::create_dir_all(&session.config.output_dir)
            .map_err(|e| format!("failed to create session dir: {}", e))?;
        let meta = session.to_metadata();
        let path = session.config.output_dir.join("metadata.json");
        let json = serde_json::to_string_pretty(&meta)
            .map_err(|e| format!("failed to serialize metadata: {}", e))?;
        std::fs::write(&path, &json)
            .map_err(|e| format!("failed to write metadata: {}", e))?;

        // Write metadata.md with YAML frontmatter
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
            crate::markdown::write_metadata_md(&session.config.output_dir, &val);
        }

        Ok(())
    }

    /// Spawn a background task that broadcasts file sizes for recording sessions,
    /// detects audio issues (e.g., mic permission denied), and auto-reconnects
    /// sources that lost their device (e.g., Teams joining a call).
    pub fn start_file_size_ticker(&self) {
        let manager = self.clone();
        let sessions = self.sessions.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            /// Max reconnect attempts per session before giving up.
            const MAX_RECONNECT_ATTEMPTS: u32 = 3;
            let mut reconnect_attempts: HashMap<String, u32> = HashMap::new();
            loop {
                interval.tick().await;
                if event_tx.receiver_count() == 0 {
                    continue;
                }

                // Phase 1: broadcast file sizes, detect silent sources, find device-lost sessions.
                let mut device_lost_sessions: Vec<String> = Vec::new();
                let mut auto_stop_sessions: Vec<String> = Vec::new();
                {
                    let mut sessions = sessions.write().await;
                    for session in sessions.values_mut() {
                        if session.state != SessionState::Recording {
                            continue;
                        }

                        let info = session.info();

                        // Auto-stop: detect silence via RMS at the raw PCM level.
                        let mut auto_stop_remaining_secs: Option<u64> = None;
                        if session.auto_stop {
                            if let Some(ref recorder) = session.recorder {
                                if let Some(last_active_ms) = recorder.system_audio_last_active_ms() {
                                    if last_active_ms > 0 {
                                        let now_ms = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as u64;
                                        let silent_secs = now_ms.saturating_sub(last_active_ms) / 1000;
                                        if silent_secs >= 60 {
                                            auto_stop_sessions.push(session.id.clone());
                                        } else if silent_secs >= 10 {
                                            if silent_secs < 12 {
                                                info!("Session {}: system audio silent for {}s, auto-stop countdown started", session.id, silent_secs);
                                            }
                                            auto_stop_remaining_secs = Some(60 - silent_secs);
                                        }
                                    }
                                }
                            }
                        }

                        let _ = event_tx.send(ServerEvent::FileSizes {
                            id: session.id.clone(),
                            file_sizes: info.file_sizes.clone(),
                            auto_stop_remaining_secs,
                        });

                        // Update live notices (silent mic, no system audio, etc).
                        // 10s grace period: AVAudioEngine + opus buffering can delay
                        // the first bytes reaching disk.
                        if let Some(started) = session.started_at {
                            let elapsed = Utc::now() - started;
                            if elapsed.num_seconds() >= 10 {
                                update_source_notices(session, &info.file_sizes, &event_tx);
                            }
                        }

                        // Check for device-lost sources that need reconnection.
                        if let Some(ref recorder) = session.recorder {
                            if recorder.has_device_lost_sources() {
                                let attempts = reconnect_attempts.get(&session.id).copied().unwrap_or(0);
                                if attempts < MAX_RECONNECT_ATTEMPTS {
                                    device_lost_sessions.push(session.id.clone());
                                }
                            }
                        }
                    }
                } // write lock released

                // Phase 2: attempt reconnection outside the lock on a blocking thread.
                /// Hard timeout for a single restart attempt. If Core Audio
                /// deadlocks inside `source.start()`, the orphan thread keeps
                /// running but we stop blocking the ticker and free the session
                /// so the user can force-stop.
                const RESTART_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

                for session_id in device_lost_sessions {
                    let sessions_ref = sessions.clone();
                    let event_tx_ref = event_tx.clone();
                    let sid = session_id.clone();

                    // Take the recorder out, reconnect on a blocking thread, put it back.
                    let mut sessions_guard = sessions_ref.write().await;
                    let recorder = match sessions_guard.get_mut(&sid) {
                        Some(s) => s.recorder.take(),
                        None => continue,
                    };
                    drop(sessions_guard);

                    if let Some(mut recorder) = recorder {
                        let join = tokio::task::spawn_blocking(move || {
                            let r = recorder.restart_lost_sources();
                            (recorder, r)
                        });
                        let timed = tokio::time::timeout(RESTART_TIMEOUT, join).await;

                        let mut sessions_guard = sessions_ref.write().await;
                        // Outcomes:
                        //   Ok(Ok((rec, Ok(restarted))))  -> success, put back
                        //   Ok(Ok((rec, Err(e))))         -> restart failed, put back, retry
                        //   Ok(Err(join_err))             -> panicked, recorder dropped during unwind
                        //   Err(_)                        -> hung past timeout, recorder stuck in orphan thread
                        let (returned_recorder, restart_result, lost): (Option<Recorder>, Option<Result<Vec<String>, _>>, bool) = match timed {
                            Ok(Ok((rec, res))) => (Some(rec), Some(res), false),
                            Ok(Err(join_err)) => {
                                warn!("Mic reconnect thread panicked for session {}: {}", sid, join_err);
                                (None, None, true)
                            }
                            Err(_) => {
                                warn!(
                                    "Mic reconnect timed out after {:?} for session {} — recorder is unrecoverable",
                                    RESTART_TIMEOUT, sid
                                );
                                (None, None, true)
                            }
                        };

                        if let Some(session) = sessions_guard.get_mut(&sid) {
                            // Put recorder back if we still have it.
                            if let Some(rec) = returned_recorder {
                                session.recorder = Some(rec);
                            }

                            match restart_result {
                                Some(Ok(restarted)) => {
                                    reconnect_attempts.remove(&sid);
                                    for label in &restarted {
                                        let notice = Notice {
                                            key: None,
                                            level: NoticeLevel::Info,
                                            message: format!("Microphone \"{}\" reconnected after audio device change", label),
                                            platform: Some(std::env::consts::OS.to_string()),
                                            details: None,
                                            created_at: Utc::now(),
                                        };
                                        session.notices.push(notice.clone());
                                        let _ = event_tx_ref.send(ServerEvent::SessionNotice {
                                            id: sid.clone(),
                                            notice,
                                        });
                                    }
                                    info!("Reconnected mic sources for session {}: {:?}", sid, restarted);
                                }
                                Some(Err(e)) => {
                                    let attempts = reconnect_attempts.entry(sid.clone()).or_insert(0);
                                    *attempts += 1;
                                    warn!("Mic reconnect failed for session {} (attempt {}): {}", sid, attempts, e);

                                    if *attempts >= MAX_RECONNECT_ATTEMPTS {
                                        let notice = max_attempts_notice();
                                        session.notices.push(notice.clone());
                                        let _ = event_tx_ref.send(ServerEvent::SessionNotice {
                                            id: sid.clone(),
                                            notice,
                                        });
                                    }
                                }
                                None => {
                                    // Panic or timeout: recorder is gone. Skip retries
                                    // (a fresh attempt has nothing to retry against)
                                    // and surface the error immediately.
                                    reconnect_attempts.insert(sid.clone(), MAX_RECONNECT_ATTEMPTS);
                                    if lost {
                                        let notice = Notice {
                                            key: None,
                                            level: NoticeLevel::Error,
                                            message: "Recording is unrecoverable — please stop and start a new session".to_string(),
                                            platform: Some(std::env::consts::OS.to_string()),
                                            details: Some(
                                                "Core Audio failed during a device-change recovery and the recorder could not be restored. \
                                                Click Stop to finalize whatever has been written so far, then start a new recording. \
                                                If Stop also fails, restart the daemon."
                                                    .to_string(),
                                            ),
                                            created_at: Utc::now(),
                                        };
                                        session.notices.push(notice.clone());
                                        let _ = event_tx_ref.send(ServerEvent::SessionNotice {
                                            id: sid.clone(),
                                            notice,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                // Phase 3: auto-stop sessions where system audio went silent.
                for session_id in auto_stop_sessions {
                    info!("Auto-stopping session {} (system audio silent for 60s)", session_id);
                    match manager.stop_recording(&session_id).await {
                        Ok(_) => {
                            let mut sessions_guard = sessions.write().await;
                            if let Some(session) = sessions_guard.get_mut(&session_id) {
                                let notice = Notice {
                                    key: None,
                                    level: NoticeLevel::Info,
                                    message: "Recording auto-stopped: system audio was silent for 1 minute".to_string(),
                                    platform: None,
                                    details: None,
                                    created_at: Utc::now(),
                                };
                                session.notices.push(notice);
                                let info = session.info();
                                let _ = event_tx.send(ServerEvent::SessionUpdated(info));
                            }
                        }
                        Err(e) => {
                            warn!("Failed to auto-stop session {}: {}", session_id, e);
                        }
                    }
                }
            }
        });
    }

    /// Scan recordings/ for existing session folders and load their metadata.
    pub async fn load_from_disk(&self) {
        let entries = match std::fs::read_dir(&self.output_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read recordings dir: {}", e);
                return;
            }
        };

        let mut sessions = self.sessions.write().await;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let meta_path = path.join("metadata.json");
            if !meta_path.exists() {
                continue;
            }
            let json = match std::fs::read_to_string(&meta_path) {
                Ok(j) => j,
                Err(e) => {
                    warn!("Failed to read {}: {}", meta_path.display(), e);
                    continue;
                }
            };
            let metadata: SessionMetadata = match serde_json::from_str(&json) {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to parse {}: {}", meta_path.display(), e);
                    continue;
                }
            };

            // Collect all filenames from the folder
            let files: Vec<String> = std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|name| !name.starts_with('.'))
                .collect();

            let session = Session::from_metadata(&metadata, &self.output_dir, files);
            sessions.insert(session.id.clone(), session);
        }
        info!("Loaded {} sessions from disk", sessions.len());
    }

    pub fn output_dir(&self) -> &PathBuf {
        &self.output_dir
    }

    /// Returns the folder path for a given session: recordings/{session_id}/
    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.output_dir.join(session_id)
    }

    pub async fn create_session(&self, mut config: SessionConfig) -> SessionInfo {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let id = format_base36(nanos);
        let session_dir = self.session_dir(&id);
        config.output_dir = session_dir;
        let session = Session::new(id.clone(), config);
        if let Err(e) = Self::write_metadata(&session) {
            warn!("Failed to write metadata on create: {}", e);
        }
        let info = session.info();
        self.sessions.write().await.insert(id, session);
        self.emit(ServerEvent::SessionCreated(info.clone()));
        info
    }

    pub async fn get_session(&self, id: &str) -> Option<SessionInfo> {
        self.sessions.read().await.get(id).map(|s| s.info())
    }

    /// IDs of all sessions currently in `Recording` state. Used for graceful
    /// shutdown so writers can finalize before the process exits.
    pub async fn recording_session_ids(&self) -> Vec<String> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| s.state == SessionState::Recording)
            .map(|s| s.id.clone())
            .collect()
    }

    /// Stop every recording session, finalizing audio writers. Called on
    /// SIGINT/SIGTERM so opus files get their trailing pages written.
    pub async fn shutdown(&self) {
        let ids = self.recording_session_ids().await;
        if ids.is_empty() {
            return;
        }
        info!("Shutdown: stopping {} active recording session(s)", ids.len());
        for id in ids {
            match self.stop_recording(&id).await {
                Ok(_) => info!("Shutdown: stopped session {}", id),
                Err(e) => warn!("Shutdown: failed to stop session {}: {}", id, e),
            }
        }
    }

    pub async fn list_sessions(
        &self,
        limit: usize,
        offset: usize,
        hidden_tags: &std::collections::HashSet<String>,
    ) -> (Vec<SessionInfo>, usize) {
        let sessions = self.sessions.read().await;
        let mut infos: Vec<SessionInfo> = sessions.values()
            .filter(|s| {
                // Hide session if it has tags AND all of them are hidden
                if hidden_tags.is_empty() || s.tags.is_empty() {
                    return true;
                }
                !s.tags.iter().all(|t| hidden_tags.contains(t))
            })
            .map(|s| s.info())
            .collect();
        let total = infos.len();
        infos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let page = infos.into_iter().skip(offset).take(limit).collect();
        (page, total)
    }

    pub async fn rename_session(&self, id: &str, name: String) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.name = if name.trim().is_empty() { None } else { Some(name.trim().to_string()) };
        session.touch();
        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on rename: {}", e);
        }
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn set_auto_stop(&self, id: &str, auto_stop: bool) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        if auto_stop {
            info!("Auto-stop enabled for session {}", id);
        } else {
            info!("Auto-stop disabled for session {}", id);
        }
        session.auto_stop = auto_stop;
        session.touch();
        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on auto_stop update: {}", e);
        }
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn update_session_language(&self, id: &str, language: String) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.config.language = language;
        session.touch();
        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on language update: {}", e);
        }
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), String> {
        // Extract recorder under lock, then stop outside lock
        let mut recorder_to_stop = None;
        let session_dir;
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.remove(id) {
                session_dir = session.config.output_dir.clone();
                if session.state == SessionState::Recording {
                    recorder_to_stop = session.recorder.take();
                }
            } else {
                return Err("session not found".to_string());
            }
        }
        if let Some(mut recorder) = recorder_to_stop {
            let _ = recorder.stop();
        }
        // Delete the session directory and all its files from disk
        if session_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&session_dir) {
                warn!("Failed to delete session directory {}: {}", session_dir.display(), e);
            }
        }
        self.emit(ServerEvent::SessionDeleted { id: id.to_string() });
        Ok(())
    }

    pub async fn start_recording(&self, id: &str) -> Result<Vec<String>, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;

        if session.state == SessionState::Recording {
            return Err("session is already recording".to_string());
        }

        // Resolve source IDs to (descriptor, AudioSource) pairs
        let source_ids = session
            .config
            .sources
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(default_source_ids);

        let mut sources: Vec<(SourceDescriptor, Box<dyn AudioSource>)> = Vec::new();

        for source_id in &source_ids {
            match resolve_source(source_id, session.config.raw_sample_rate) {
                Ok(pair) => {
                    info!("Source '{}' created for session {}", source_id, session.id);
                    sources.push(pair);
                }
                Err(e) => {
                    warn!("Skipping source '{}': {}", source_id, e);
                }
            }
        }

        if sources.is_empty() {
            return Err("no audio sources could be initialized".to_string());
        }

        let mut recorder = Recorder::new(
            session.id.clone(),
            session.config.output_dir.clone(),
            session.config.raw_sample_rate,
            session.config.format,
            session.config.mp3,
            session.config.opus,
            sources,
        );

        // Drop the write lock before the blocking start — source.start() calls
        // Core Audio APIs that can hang when devices are being reconfigured.
        let session_id = session.id.clone();
        drop(sessions);

        // Run on a blocking thread so Core Audio calls don't stall the async runtime.
        // Timeout after 15s — Core Audio can hang when the audio device graph is
        // being reconfigured (USB-to-HDMI adapter, virtual audio devices, etc.).
        let (mut recorder, files) = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            tokio::task::spawn_blocking(move || {
                recorder.start().map(|files| (recorder, files))
            }),
        )
        .await
        .map_err(|_| "recording start timed out after 15s — audio system may be busy (try restarting the app)".to_string())?
        .map_err(|e| format!("recorder thread panicked: {e}"))?
        .map_err(|e| e.to_string())?;

        // Re-acquire lock to update session state
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(&session_id).ok_or("session not found")?;

        // Guard against concurrent start (session may have been modified while unlocked)
        if session.state == SessionState::Recording {
            // Another start_recording won the race — stop what we just started
            let _ = recorder.stop();
            return Err("session is already recording".to_string());
        }

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .collect();

        session.recorder = Some(recorder);
        session.state = SessionState::Recording;
        session.started_at = Some(Utc::now());
        session.files = file_names.clone();
        session.capture_source_meta();
        session.touch();

        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on start: {}", e);
        }
        self.emit(ServerEvent::SessionUpdated(session.info()));

        Ok(file_names)
    }

    pub async fn stop_recording(&self, id: &str) -> Result<Vec<String>, String> {
        // Phase 1: Extract recorder under write lock, mark stopped.
        // If the recorder is missing (e.g. lost during a failed Core Audio
        // recovery), force-stop: mark Stopped anyway so the user is unstuck.
        // Whatever audio writers are still alive will be cleaned up on
        // process exit.
        let recorder = {
            let mut sessions = self.sessions.write().await;
            let session = sessions.get_mut(id).ok_or("session not found")?;

            if session.state != SessionState::Recording {
                return Err("session is not recording".to_string());
            }

            let recorder = session.recorder.take();

            session.state = SessionState::Stopped;
            session.touch();

            // Emit early update so UI sees "stopped" state immediately
            self.emit(ServerEvent::SessionUpdated(session.info()));

            recorder
        };
        // Write lock released here

        // Phase 2: Stop recorder (blocking) without holding any lock.
        // Force-stop path: no recorder means we just finalize the session
        // metadata with whatever files exist on disk.
        let mut force_stopped = false;
        if let Some(mut rec) = recorder {
            if let Err(e) = rec.stop() {
                return Err(e.to_string());
            }
        } else {
            warn!("Force-stopping session {} with no active recorder", id);
            force_stopped = true;
        }

        // Phase 3: Re-acquire lock, update files, write metadata
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            if !session.files.contains(&"metadata.json".to_string()) {
                session.files.push("metadata.json".to_string());
            }
            session.touch();
            if force_stopped {
                let notice = Notice {
                    key: None,
                    level: NoticeLevel::Warning,
                    message: "Recording was force-stopped — files may be missing trailing audio".to_string(),
                    platform: Some(std::env::consts::OS.to_string()),
                    details: Some(
                        "The audio recorder was lost during a Core Audio device change. \
                        The session was marked stopped without finalizing the encoders, so \
                        the final ~1s of audio and the file's trailing metadata may be missing. \
                        Most decoders can still play the partial files."
                            .to_string(),
                    ),
                    created_at: Utc::now(),
                };
                session.notices.push(notice.clone());
                self.emit(ServerEvent::SessionNotice {
                    id: id.to_string(),
                    notice,
                });
            }
            if let Err(e) = Self::write_metadata(session) {
                warn!("Failed to write metadata on stop: {}", e);
            }
            let info = session.info();
            self.emit(ServerEvent::SessionUpdated(info));
            Ok(session.files.clone())
        } else {
            Ok(vec![])
        }
    }

    pub async fn get_files(&self, id: &str) -> Result<Vec<String>, String> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or("session not found")?;
        Ok(session.files.clone())
    }

    /// Re-scan the session directory and update the files list.
    pub async fn refresh_files(&self, id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            let dir = self.output_dir.join(id);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                session.files = entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|name| !name.starts_with('.'))
                    .collect();
                self.emit(ServerEvent::SessionUpdated(session.info()));
            }
        }
    }

    /// Set the processing state for a session.
    pub async fn set_processing_state(&self, id: &str, state: Option<String>) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.processing_state = state;
            session.touch();
            self.emit(ServerEvent::SessionUpdated(session.info()));
        }
    }

    pub fn emit_transcription_progress(&self, id: &str, status: &str) {
        self.emit(ServerEvent::TranscriptionProgress {
            id: id.to_string(),
            status: status.to_string(),
        });
    }

    pub fn emit_transcription_completed(&self, id: &str, unconfirmed_speakers: u32) {
        self.emit(ServerEvent::TranscriptionCompleted {
            id: id.to_string(),
            unconfirmed_speakers,
        });
    }

    pub fn emit_transcription_failed(&self, id: &str, error: &str) {
        self.emit(ServerEvent::TranscriptionFailed {
            id: id.to_string(),
            error: error.to_string(),
        });
    }

    pub fn emit_summary_delta(&self, id: &str, delta: &str) {
        self.emit(ServerEvent::SummaryDelta {
            id: id.to_string(),
            delta: delta.to_string(),
        });
    }

    pub fn emit_summary_thinking(&self, id: &str, delta: &str) {
        self.emit(ServerEvent::SummaryThinking {
            id: id.to_string(),
            delta: delta.to_string(),
        });
    }

    pub async fn emit_summary_progress(&self, id: &str, status: &str) {
        let started_at;
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(id) {
                if session.summary_started_at.is_none() {
                    session.summary_started_at = Some(Utc::now());
                }
                started_at = session.summary_started_at.unwrap();
            } else {
                started_at = Utc::now();
            }
        }
        self.emit(ServerEvent::SummaryProgress {
            id: id.to_string(),
            status: status.to_string(),
            started_at,
        });
    }

    pub async fn emit_summary_completed(&self, id: &str) {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(id) {
                session.summary_started_at = None;
            }
        }
        let dir = self.session_dir(id);
        // Read saved summary and todos so frontend has them immediately
        let summary = std::fs::read_to_string(dir.join("summary.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let todos = std::fs::read_to_string(dir.join("todos.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        self.emit(ServerEvent::SummaryCompleted {
            id: id.to_string(),
            summary,
            todos,
        });
    }

    pub async fn emit_summary_failed(&self, id: &str, error: &str) {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(id) {
                session.summary_started_at = None;
            }
        }
        self.emit(ServerEvent::SummaryFailed {
            id: id.to_string(),
            error: error.to_string(),
        });
    }

    /// Set the audio extraction job info and persist to metadata.json.
    pub async fn set_audio_extraction(&self, id: &str, job: Option<session::AudioExtractionJob>) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.audio_extraction = job;
            session.touch();
            if let Err(e) = Self::write_metadata(session) {
                tracing::warn!("Failed to write metadata for {}: {}", id, e);
            }
        }
    }

    /// Get all sessions with in-progress extraction jobs (for resume on startup).
    pub async fn get_pending_extractions(&self) -> Vec<(String, session::AudioExtractionJob)> {
        let sessions = self.sessions.read().await;
        sessions.values()
            .filter_map(|s| {
                s.audio_extraction.as_ref()
                    .filter(|j| j.status == "in_progress")
                    .map(|j| (s.id.clone(), j.clone()))
            })
            .collect()
    }

    /// Set the tags for a session.
    pub async fn update_session_tags(&self, id: &str, tags: Vec<String>) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.tags = tags;
        session.touch();
        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on tag update: {}", e);
        }
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn update_session_notes(&self, id: &str, notes: Option<String>) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.notes = notes;
        session.touch();
        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on notes update: {}", e);
        }
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    /// Rename a tag across all sessions.
    pub async fn rename_tag_in_all_sessions(&self, old_name: &str, new_name: &str) {
        let mut sessions = self.sessions.write().await;
        for session in sessions.values_mut() {
            if let Some(pos) = session.tags.iter().position(|t| t == old_name) {
                session.tags[pos] = new_name.to_string();
                session.touch();
                if let Err(e) = Self::write_metadata(session) {
                    warn!("Failed to write metadata after tag rename for {}: {}", session.id, e);
                }
                self.emit(ServerEvent::SessionUpdated(session.info()));
            }
        }
    }

    /// Remove a tag from all sessions (cascade on tag deletion).
    pub async fn remove_tag_from_all_sessions(&self, tag_name: &str) {
        let mut sessions = self.sessions.write().await;
        for session in sessions.values_mut() {
            if session.tags.contains(&tag_name.to_string()) {
                session.tags.retain(|t| t != tag_name);
                session.touch();
                if let Err(e) = Self::write_metadata(session) {
                    warn!("Failed to write metadata after tag removal for {}: {}", session.id, e);
                }
                self.emit(ServerEvent::SessionUpdated(session.info()));
            }
        }
    }

    /// Count sessions per tag.
    pub async fn tag_session_counts(&self) -> HashMap<String, usize> {
        let sessions = self.sessions.read().await;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for session in sessions.values() {
            for tag in &session.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Get sessions that have a given tag.
    pub async fn sessions_for_tag(&self, tag_name: &str) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos: Vec<SessionInfo> = sessions.values()
            .filter(|s| s.tags.contains(&tag_name.to_string()))
            .map(|s| s.info())
            .collect();
        infos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        infos
    }

    /// Get the session directory path, session language, and source metadata.
    pub async fn get_session_extraction_info(
        &self,
        id: &str,
    ) -> Result<(std::path::PathBuf, String, Vec<session::SourceMetadata>), String> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or("session not found")?;
        if session.state == SessionState::Recording {
            return Err("cannot transcribe while recording".to_string());
        }
        if session.files.iter().all(|f| f == "metadata.json") {
            return Err("no audio files to transcribe".to_string());
        }
        Ok((
            session.config.output_dir.clone(),
            session.config.language.clone(),
            session.source_meta.clone(),
        ))
    }

    /// Export all sessions as entries for index generation.
    pub async fn session_entries(&self) -> Vec<crate::markdown::SessionEntry> {
        let sessions = self.sessions.read().await;
        sessions.values().map(|s| {
            let duration_secs = Session::compute_duration(
                &s.config.output_dir, &s.files, s.config.mp3.bitrate_kbps,
            );
            crate::markdown::SessionEntry {
                id: s.id.clone(),
                name: s.name.clone(),
                language: s.config.language.clone(),
                tags: s.tags.clone(),
                created_at: s.created_at,
                duration_secs,
                state: format!("{:?}", s.state).to_lowercase(),
            }
        }).collect()
    }
}

/// Default source IDs when none specified: mic + system_mix.
fn default_source_ids() -> Vec<String> {
    vec!["mic".to_string(), "system_mix".to_string()]
}

/// Resolve a source ID string to a (descriptor, AudioSource) pair.
fn resolve_source(
    source_id: &str,
    sample_rate: u32,
) -> Result<(SourceDescriptor, Box<dyn AudioSource>), String> {
    if source_id == "system_mix" {
        let source = SystemAudioSource::new(sample_rate).map_err(|e| e.to_string())?;
        let desc = SourceDescriptor {
            id: "system_mix".to_string(),
            source_type: SourceType::SystemMix,
            label: "System Audio".to_string(),
            device_name: None,
        };
        Ok((desc, Box::new(source)))
    } else if source_id == "mic" || source_id.starts_with("mic:") {
        // All mic source IDs resolve to the same AVAudioEngine-based source
        // that uses the system default input device.
        let source = MicSource::new(sample_rate);
        let desc = SourceDescriptor {
            id: "mic".to_string(),
            source_type: SourceType::Mic,
            label: "System Microphone".to_string(),
            device_name: None,
        };
        Ok((desc, Box::new(source)))
    } else {
        Err(format!("unknown source: {}", source_id))
    }
}

fn format_base36(mut n: u64) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::with_capacity(12);
    while n > 0 {
        buf.push(CHARS[(n % 36) as usize]);
        n /= 36;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

/// Recompute live notices for recording sources based on current file sizes.
/// Notices with a `key` are auto-managed: added when a condition is detected,
/// removed when it resolves. Emits SessionNotices when the set changes.
fn max_attempts_notice() -> Notice {
    Notice {
        key: None,
        level: NoticeLevel::Error,
        message: "Microphone lost — could not reconnect".to_string(),
        platform: Some(std::env::consts::OS.to_string()),
        details: Some(
            "A video conferencing app (e.g. Teams) may have disrupted Core Audio. \
            Stop and restart recording. If that fails, restart the audio system: \
            sudo launchctl kickstart -kp system/com.apple.audio.coreaudiod"
                .to_string(),
        ),
        created_at: Utc::now(),
    }
}

fn update_source_notices(
    session: &mut Session,
    file_sizes: &HashMap<String, u64>,
    event_tx: &broadcast::Sender<ServerEvent>,
) {
    let platform = std::env::consts::OS;
    let mut expected_keys: HashMap<String, Notice> = HashMap::new();

    for meta in &session.source_meta {
        let size = file_sizes.get(&meta.filename).copied().unwrap_or(0);
        let key = format!("silent:{}", meta.filename);

        match meta.source_type {
            SourceType::Mic => {
                // Under 1 KB after 10s means no real audio data.
                if size < 1024 {
                    let (message, details) = if platform == "macos" {
                        (
                            format!("\"{}\" is not receiving audio", meta.source_label),
                            Some(
                                "macOS may have denied microphone access. \
                                Check System Settings > Privacy & Security > Microphone \
                                and ensure your terminal app (or VS Code) is allowed."
                                    .to_string(),
                            ),
                        )
                    } else {
                        (
                            format!("\"{}\" is not receiving audio", meta.source_label),
                            Some("Check that your microphone is connected and permissions are granted.".to_string()),
                        )
                    };
                    expected_keys.insert(key, Notice {
                        key: Some(format!("silent:{}", meta.filename)),
                        level: NoticeLevel::Warning,
                        message,
                        platform: Some(platform.to_string()),
                        details,
                        created_at: Utc::now(),
                    });
                }
            }
            SourceType::SystemMix => {
                // 0 bytes = no system audio captured. Could be no permission
                // or just nothing playing.
                if size == 0 {
                    let (message, details) = if platform == "macos" {
                        (
                            "System audio is not receiving data".to_string(),
                            Some(
                                "Either nothing is playing, or permission is missing. \
                                Check System Settings > Privacy & Security > Screen & System Audio Recording."
                                    .to_string(),
                            ),
                        )
                    } else {
                        (
                            "System audio is not receiving data".to_string(),
                            None,
                        )
                    };
                    expected_keys.insert(key, Notice {
                        key: Some(format!("silent:{}", meta.filename)),
                        level: NoticeLevel::Info,
                        message,
                        platform: Some(platform.to_string()),
                        details,
                        created_at: Utc::now(),
                    });
                }
            }
            _ => {}
        }
    }

    // Compute what changed: compare current keyed notices with expected
    let current_keys: std::collections::HashSet<String> = session.notices.iter()
        .filter_map(|n| n.key.clone())
        .collect();
    let expected_key_set: std::collections::HashSet<String> = expected_keys.keys().cloned().collect();

    if current_keys == expected_key_set {
        return; // No change
    }

    // Remove stale keyed notices, keep non-keyed (manual) notices
    session.notices.retain(|n| n.key.is_none());
    // Add current keyed notices
    for (_, notice) in expected_keys {
        session.notices.push(notice);
    }

    // Emit full notices list
    let _ = event_tx.send(ServerEvent::SessionNotices {
        id: session.id.clone(),
        notices: session.notices.clone(),
    });
}
