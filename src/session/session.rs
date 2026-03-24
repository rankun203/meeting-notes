use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::audio::recorder::Recorder;
use crate::audio::source::SourceType;
use crate::audio::writer::{AudioFormat, Mp3Config};

use super::config::SessionConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// When recording started (None if never recorded).
    pub started_at: Option<DateTime<Utc>>,
    pub recorder: Option<Recorder>,
    pub files: Vec<String>,
    /// Source metadata captured when recording starts, persists after recorder is taken.
    pub source_meta: Vec<SourceMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub state: SessionState,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarization_instruction: Option<String>,
    pub sample_rate: u32,
    pub format: AudioFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mp3: Option<Mp3Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    pub files: Vec<String>,
    pub file_sizes: HashMap<String, u64>,
}

/// Written to metadata.json in the session folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    #[serde(default = "default_stopped_state")]
    pub state: SessionState,
    pub language: String,
    pub format: AudioFormat,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default)]
    pub mp3: Mp3Config,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub sources: Vec<SourceMetadata>,
}

fn default_stopped_state() -> SessionState {
    SessionState::Stopped
}

fn default_sample_rate() -> u32 {
    48000
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
            started_at: None,
            recorder: None,
            files: Vec::new(),
            source_meta: Vec::new(),
        }
    }

    /// Reconstruct a session from on-disk metadata.
    /// If state was Recording (crash recovery), auto-transition to Stopped.
    pub fn from_metadata(
        meta: &SessionMetadata,
        recordings_dir: &std::path::Path,
        files: Vec<String>,
    ) -> Self {
        let state = match meta.state {
            SessionState::Recording => SessionState::Stopped,
            other => other,
        };
        let config = SessionConfig {
            language: meta.language.clone(),
            summarization_instruction: None,
            sample_rate: meta.sample_rate,
            format: meta.format,
            mp3: meta.mp3,
            sources: None,
            output_dir: recordings_dir.join(&meta.session_id),
        };
        Self {
            id: meta.session_id.clone(),
            config,
            state,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            started_at: meta.started_at,
            recorder: None,
            files,
            source_meta: meta.sources.clone(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn to_metadata(&self) -> SessionMetadata {
        let duration_secs = self.started_at.map(|start| {
            let end = self.updated_at;
            (end - start).num_milliseconds() as f64 / 1000.0
        });
        SessionMetadata {
            session_id: self.id.clone(),
            state: self.state,
            language: self.config.language.clone(),
            format: self.config.format,
            sample_rate: self.config.sample_rate,
            mp3: self.config.mp3,
            created_at: self.created_at,
            updated_at: self.updated_at,
            started_at: self.started_at,
            duration_secs,
            sources: self.source_meta.clone(),
        }
    }

    /// Populate source_meta from the recorder's current sources.
    pub fn capture_source_meta(&mut self) {
        if let Some(recorder) = &self.recorder {
            self.source_meta = recorder
                .source_metadata()
                .iter()
                .map(|(desc, path)| SourceMetadata {
                    filename: path
                        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
                        .unwrap_or_default(),
                    source_type: desc.source_type,
                    source_label: desc.label.clone(),
                    channels: 0,
                    sample_rate: self.config.sample_rate,
                })
                .collect();
        }
    }

    pub fn info(&self) -> SessionInfo {
        let file_sizes: HashMap<String, u64> = self
            .files
            .iter()
            .filter_map(|f| {
                let path = self.config.output_dir.join(f);
                std::fs::metadata(&path).ok().map(|m| (f.clone(), m.len()))
            })
            .collect();
        let duration_secs = self.started_at.map(|start| {
            let end = self.updated_at;
            (end - start).num_milliseconds() as f64 / 1000.0
        });
        SessionInfo {
            id: self.id.clone(),
            state: self.state,
            language: self.config.language.clone(),
            summarization_instruction: self.config.summarization_instruction.clone(),
            sample_rate: self.config.sample_rate,
            format: self.config.format,
            mp3: if self.config.format == AudioFormat::Mp3 { Some(self.config.mp3) } else { None },
            sources: self.config.sources.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            duration_secs,
            files: self.files.clone(),
            file_sizes,
        }
    }
}
