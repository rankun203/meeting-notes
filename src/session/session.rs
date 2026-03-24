use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::audio::recorder::Recorder;
use crate::audio::writer::{AudioFormat, Mp3Config};

use super::config::SessionConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Recording,
    Stopped,
}

pub struct Session {
    pub id: String,
    pub config: SessionConfig,
    pub state: SessionState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub recorder: Option<Recorder>,
    pub files: Vec<String>,
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub state: SessionState,
    pub language: String,
    pub summarization_instruction: Option<String>,
    pub sample_rate: u32,
    pub format: AudioFormat,
    pub mp3: Mp3Config,
    pub mic_device: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub files: Vec<String>,
}

impl Session {
    pub fn new(id: String, config: SessionConfig) -> Self {
        let now = Utc::now();
        Self {
            id,
            config,
            state: SessionState::Created,
            created_at: now,
            updated_at: now,
            recorder: None,
            files: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            state: self.state,
            language: self.config.language.clone(),
            summarization_instruction: self.config.summarization_instruction.clone(),
            sample_rate: self.config.sample_rate,
            format: self.config.format,
            mp3: self.config.mp3,
            mic_device: self.config.mic_device.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            files: self.files.clone(),
        }
    }
}
