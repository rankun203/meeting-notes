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
    pub name: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub state: SessionState,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarization_instruction: Option<String>,
    pub raw_sample_rate: u32,
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
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_stopped_state")]
    pub state: SessionState,
    pub language: String,
    pub format: AudioFormat,
    #[serde(default = "default_sample_rate", alias = "sample_rate")]
    pub raw_sample_rate: u32,
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
    #[serde(alias = "sample_rate")]
    pub raw_sample_rate: u32,
}

impl Session {
    pub fn new(id: String, config: SessionConfig) -> Self {
        let now = Utc::now();
        Self {
            id,
            name: None,
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
            raw_sample_rate: meta.raw_sample_rate,
            format: meta.format,
            mp3: meta.mp3,
            sources: None,
            output_dir: recordings_dir.join(&meta.session_id),
        };
        Self {
            id: meta.session_id.clone(),
            name: meta.name.clone(),
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
        let duration_secs = Self::compute_duration(&self.config.output_dir, &self.files, self.config.mp3.bitrate_kbps);
        SessionMetadata {
            session_id: self.id.clone(),
            name: self.name.clone(),
            state: self.state,
            language: self.config.language.clone(),
            format: self.config.format,
            raw_sample_rate: self.config.raw_sample_rate,
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
                    raw_sample_rate: self.config.raw_sample_rate,
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
        let duration_secs = Self::compute_duration(&self.config.output_dir, &self.files, self.config.mp3.bitrate_kbps);
        SessionInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            state: self.state,
            language: self.config.language.clone(),
            summarization_instruction: self.config.summarization_instruction.clone(),
            raw_sample_rate: self.config.raw_sample_rate,
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

    /// Compute duration from audio files (max across all tracks).
    /// WAV: reads 44-byte header only (fast). MP3: estimates from file size and bitrate.
    fn compute_duration(dir: &std::path::Path, files: &[String], bitrate_kbps: u32) -> Option<f64> {
        let mut max_dur: Option<f64> = None;
        for f in files {
            let path = dir.join(f);
            let secs = if f.ends_with(".wav") {
                hound::WavReader::open(&path).ok().and_then(|r| {
                    let spec = r.spec();
                    if spec.sample_rate > 0 {
                        Some(r.duration() as f64 / spec.sample_rate as f64)
                    } else {
                        None
                    }
                })
            } else if f.ends_with(".mp3") {
                // CBR MP3: duration ≈ file_size_bytes * 8 / bitrate_bps.
                // Only accurate for CBR (which we use via set_brate). If VBR is ever
                // added, this must be replaced with MP3 frame parsing or a Xing header read.
                let bps = if bitrate_kbps > 0 { bitrate_kbps as u64 * 1000 } else { 64000 };
                std::fs::metadata(&path).ok().map(|m| (m.len() * 8) as f64 / bps as f64)
            } else {
                None
            };
            if let Some(s) = secs {
                max_dur = Some(max_dur.map_or(s, |d: f64| d.max(s)));
            }
        }
        max_dur
    }
}
