pub mod config;
pub mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait};
use tokio::sync::RwLock;
use uuid::Uuid;

use tracing::{info, warn};

use self::config::SessionConfig;
use self::session::{Session, SessionInfo, SessionMetadata, SessionState, SourceMetadata};
use crate::audio::mic::MicSource;
use crate::audio::recorder::Recorder;
use crate::audio::source::{AudioSource, SourceDescriptor, SourceType};
use crate::audio::system_audio::SystemAudioSource;

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    output_dir: PathBuf,
}

impl SessionManager {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            output_dir,
        }
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

            // Collect filenames from the folder (excluding metadata.json)
            let files: Vec<String> = std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name == "metadata.json" {
                        None
                    } else {
                        Some(name)
                    }
                })
                .collect();

            let session = Session::from_metadata(&metadata, &self.output_dir, files);
            info!(
                "Loaded session {} from disk ({} files)",
                session.id,
                session.files.len()
            );
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
        let id = Uuid::new_v4().to_string();
        let session_dir = self.session_dir(&id);
        config.output_dir = session_dir;
        let session = Session::new(id.clone(), config);
        let info = session.info();
        self.sessions.write().await.insert(id, session);
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

    pub async fn delete_session(&self, id: &str) -> Result<(), String> {
        // Extract recorder under lock, then stop outside lock
        let mut recorder_to_stop = None;
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.remove(id) {
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
            match resolve_source(source_id, session.config.sample_rate) {
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
            session.config.sample_rate,
            session.config.format,
            session.config.mp3,
            sources,
        );

        let files = recorder.start().map_err(|e| e.to_string())?;
        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .collect();

        session.recorder = Some(recorder);
        session.state = SessionState::Recording;
        session.files = file_names.clone();
        session.touch();

        Ok(file_names)
    }

    pub async fn stop_recording(&self, id: &str) -> Result<Vec<String>, String> {
        // Phase 1: Extract recorder and metadata info under write lock
        let (mut recorder, source_meta, session_id, language, format, created_at, output_dir) = {
            let mut sessions = self.sessions.write().await;
            let session = sessions.get_mut(id).ok_or("session not found")?;

            if session.state != SessionState::Recording {
                return Err("session is not recording".to_string());
            }

            let recorder = session.recorder.take()
                .ok_or("no recorder found")?;

            let source_meta: Vec<SourceMetadata> = recorder
                .source_metadata()
                .iter()
                .map(|(desc, path)| SourceMetadata {
                    filename: path
                        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
                        .unwrap_or_default(),
                    source_type: desc.source_type,
                    source_label: desc.label.clone(),
                    channels: 0,
                    sample_rate: session.config.sample_rate,
                })
                .collect();

            // Mark as stopping so UI knows
            session.state = SessionState::Stopped;
            session.touch();

            (
                recorder,
                source_meta,
                session.id.clone(),
                session.config.language.clone(),
                session.config.format,
                session.created_at,
                session.config.output_dir.clone(),
            )
        };
        // Write lock released here

        // Phase 2: Stop recorder (blocking) without holding any lock
        recorder.stop().map_err(|e| e.to_string())?;

        // Phase 3: Write metadata.json
        let metadata = SessionMetadata {
            session_id: session_id.clone(),
            language,
            format,
            created_at,
            updated_at: Utc::now(),
            sources: source_meta,
        };
        let meta_path = output_dir.join("metadata.json");
        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("failed to serialize metadata: {}", e))?;
        std::fs::write(&meta_path, json)
            .map_err(|e| format!("failed to write metadata: {}", e))?;
        info!("Wrote metadata to \"{}\"", meta_path.display());

        // Phase 4: Re-acquire lock to update session files
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.files.push("metadata.json".to_string());
            session.touch();
            Ok(session.files.clone())
        } else {
            // Session was deleted while we were stopping - that's ok
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
