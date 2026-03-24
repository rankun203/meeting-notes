pub mod config;
pub mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use tracing::{info, warn};

use self::config::SessionConfig;
use self::session::{Session, SessionInfo, SessionState};
use crate::audio::mic::MicSource;
use crate::audio::recorder::Recorder;
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

    pub fn output_dir(&self) -> &PathBuf {
        &self.output_dir
    }

    pub async fn create_session(&self, mut config: SessionConfig) -> SessionInfo {
        let id = Uuid::new_v4().to_string();
        config.output_dir = self.output_dir.clone();
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
        // Sort by updated_at descending (most recent first)
        infos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let page = infos.into_iter().skip(offset).take(limit).collect();
        (page, total)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        if let Some(mut session) = sessions.remove(id) {
            if session.state == SessionState::Recording {
                if let Some(ref mut recorder) = session.recorder {
                    let _ = recorder.stop();
                }
            }
            Ok(())
        } else {
            Err("session not found".to_string())
        }
    }

    pub async fn start_recording(&self, id: &str) -> Result<Vec<String>, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;

        if session.state == SessionState::Recording {
            return Err("session is already recording".to_string());
        }

        // Create mic source
        let mic_source = MicSource::new(
            session.config.mic_device.clone(),
            session.config.sample_rate,
        );
        info!("Mic source created for session {}", session.id);

        // Create system audio source (platform-specific)
        let system_source = match SystemAudioSource::new(session.config.sample_rate) {
            Ok(source) => {
                info!("System audio source created for session {}", session.id);
                Some(Box::new(source) as Box<dyn crate::audio::source::AudioSource>)
            }
            Err(e) => {
                warn!("System audio not available: {}. Recording mic only.", e);
                None
            }
        };

        let mut recorder = Recorder::new(
            session.id.clone(),
            session.config.output_dir.clone(),
            session.config.sample_rate,
            session.config.format,
            session.config.mp3,
            Some(Box::new(mic_source)),
            system_source,
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
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;

        if session.state != SessionState::Recording {
            return Err("session is not recording".to_string());
        }

        if let Some(ref mut recorder) = session.recorder {
            recorder.stop().map_err(|e| e.to_string())?;
        }

        session.recorder = None;
        session.state = SessionState::Stopped;
        session.touch();

        Ok(session.files.clone())
    }

    pub async fn get_files(&self, id: &str) -> Result<Vec<String>, String> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or("session not found")?;
        Ok(session.files.clone())
    }
}
