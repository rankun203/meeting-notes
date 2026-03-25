pub mod config;
pub mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
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
    },
    SessionNotice {
        id: String,
        notice: Notice,
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
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write metadata: {}", e))?;
        Ok(())
    }

    /// Spawn a background task that broadcasts file sizes for recording sessions
    /// and detects audio issues (e.g., mic permission denied).
    pub fn start_file_size_ticker(&self) {
        let sessions = self.sessions.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;
                if event_tx.receiver_count() == 0 {
                    continue;
                }
                let mut sessions = sessions.write().await;
                for session in sessions.values_mut() {
                    if session.state == SessionState::Recording {
                        let info = session.info();
                        let _ = event_tx.send(ServerEvent::FileSizes {
                            id: session.id.clone(),
                            file_sizes: info.file_sizes.clone(),
                        });

                        // Detect mic sources producing no audio data.
                        // After 5+ seconds of recording, a mic file under 200 bytes
                        // means the callback never fired — likely a permission issue.
                        if let Some(started) = session.started_at {
                            let elapsed = Utc::now() - started;
                            if elapsed.num_seconds() >= 5 {
                                detect_silent_sources(session, &info.file_sizes, &event_tx);
                            }
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

    pub async fn list_sessions(&self, limit: usize, offset: usize) -> (Vec<SessionInfo>, usize) {
        let sessions = self.sessions.read().await;
        let total = sessions.len();
        let mut infos: Vec<SessionInfo> = sessions.values().map(|s| s.info()).collect();
        infos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
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
        // Phase 1: Extract recorder under write lock, mark stopped
        let mut recorder = {
            let mut sessions = self.sessions.write().await;
            let session = sessions.get_mut(id).ok_or("session not found")?;

            if session.state != SessionState::Recording {
                return Err("session is not recording".to_string());
            }

            let recorder = session.recorder.take()
                .ok_or("no recorder found")?;

            session.state = SessionState::Stopped;
            session.touch();

            // Emit early update so UI sees "stopped" state immediately
            self.emit(ServerEvent::SessionUpdated(session.info()));

            recorder
        };
        // Write lock released here

        // Phase 2: Stop recorder (blocking) without holding any lock
        recorder.stop().map_err(|e| e.to_string())?;

        // Phase 3: Re-acquire lock, update files, write metadata
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            if !session.files.contains(&"metadata.json".to_string()) {
                session.files.push("metadata.json".to_string());
            }
            session.touch();
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
}

/// Default source IDs when none specified: default mic + system_mix.
fn default_source_ids() -> Vec<String> {
    let host = cpal::default_host();
    let default_mic_id = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .map(|name| format!("mic:{}", name))
        .unwrap_or_else(|| "mic:".to_string());
    vec![default_mic_id, "system_mix".to_string()]
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
    } else if let Some(device_name) = source_id.strip_prefix("mic:") {
        let device_opt = if device_name.is_empty() {
            None
        } else {
            Some(device_name.to_string())
        };
        let label = device_opt.clone().unwrap_or_else(|| "Microphone".to_string());
        let source = MicSource::new(device_opt.clone(), sample_rate);
        let desc = SourceDescriptor {
            id: source_id.to_string(),
            source_type: SourceType::Mic,
            label,
            device_name: device_opt,
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

/// Check recording sources for missing audio data and emit notices.
/// A mic source with < 200 bytes after 5+ seconds of recording likely means
/// macOS denied microphone access (TCC). The stream starts but callbacks never fire.
fn detect_silent_sources(
    session: &mut Session,
    file_sizes: &HashMap<String, u64>,
    event_tx: &broadcast::Sender<ServerEvent>,
) {
    // Only check mic sources — system audio has different failure modes.
    for meta in &session.source_meta {
        if meta.source_type != SourceType::Mic {
            continue;
        }
        let size = file_sizes.get(&meta.filename).copied().unwrap_or(0);
        // Opus header-only is ~164 bytes, WAV header is 44 bytes.
        // If the file is under 200 bytes, the callback never delivered audio.
        if size >= 200 {
            continue;
        }
        // Don't add duplicate notices for the same source.
        let already_warned = session.notices.iter().any(|n| {
            n.message.contains(&meta.source_label)
                && n.message.contains("not receiving audio")
        });
        if already_warned {
            continue;
        }

        let platform = std::env::consts::OS;
        let (message, details) = if platform == "macos" {
            (
                format!("Microphone \"{}\" is not receiving audio", meta.source_label),
                Some(
                    "macOS may have denied microphone access. \
                    Check System Settings > Privacy & Security > Microphone \
                    and ensure your terminal app (or VS Code) is allowed."
                        .to_string(),
                ),
            )
        } else {
            (
                format!("Microphone \"{}\" is not receiving audio", meta.source_label),
                Some("Check that your microphone is connected and permissions are granted.".to_string()),
            )
        };

        let notice = Notice {
            level: NoticeLevel::Warning,
            message,
            platform: Some(platform.to_string()),
            details,
            created_at: Utc::now(),
        };
        session.notices.push(notice.clone());
        let _ = event_tx.send(ServerEvent::SessionNotice {
            id: session.id.clone(),
            notice,
        });
        warn!(
            "Session {}: mic source '{}' has {} bytes after 5+ seconds — possible permission issue",
            session.id, meta.source_label, size
        );
    }
}
