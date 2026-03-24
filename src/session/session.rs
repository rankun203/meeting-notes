use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::audio::recorder::Recorder;
use crate::audio::source::SourceType;
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
    pub sources: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub files: Vec<String>,
}

/// Written to {session_id}_metadata.json alongside audio files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub language: String,
    pub format: AudioFormat,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub sources: Vec<SourceMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub filename: String,
    pub source_type: SourceType,
    pub source_label: String,
    pub channels: u16,
    pub sample_rate: u32,
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

    /// Reconstruct a stopped session from on-disk metadata.
    pub fn from_metadata(
        meta: &SessionMetadata,
        recordings_dir: &std::path::Path,
        files: Vec<String>,
    ) -> Self {
        let config = SessionConfig {
            language: meta.language.clone(),
            summarization_instruction: None,
            sample_rate: meta.sources.first().map(|s| s.sample_rate).unwrap_or(48000),
            format: meta.format,
            mp3: Mp3Config::default(),
            sources: None,
            output_dir: recordings_dir.join(&meta.session_id),
        };
        Self {
            id: meta.session_id.clone(),
            config,
            state: SessionState::Stopped,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            recorder: None,
            files,
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
            sources: self.config.sources.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            files: self.files.clone(),
        }
    }
}
