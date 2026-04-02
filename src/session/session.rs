use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::audio::recorder::Recorder;
use crate::audio::source::SourceType;
use crate::audio::writer::{AudioFormat, Mp3Config, OpusConfig};

use super::config::SessionConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Recording,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notice {
    /// Unique key for auto-managed notices (e.g. "silent:mic").
    /// Notices with a key are live — they appear/disappear as conditions change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub level: NoticeLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
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
    /// In-memory notices (not persisted to disk).
    pub notices: Vec<Notice>,
    /// Current processing state (transcribing, matching, completed, failed).
    pub processing_state: Option<String>,
    /// Persisted audio extraction job info (for resume on restart).
    pub audio_extraction: Option<AudioExtractionJob>,
    /// User-assigned tags.
    pub tags: Vec<String>,
    /// User notes for this session.
    pub notes: Option<String>,
    /// When true, auto-stop recording after system audio is silent for 1 minute.
    pub auto_stop: bool,
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
    pub opus: Option<OpusConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    pub files: Vec<String>,
    pub file_sizes: HashMap<String, u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<Notice>,
    pub transcript_available: bool,
    pub summary_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_state: Option<String>,
    pub unconfirmed_speakers: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_meta: Vec<SourceMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub auto_stop: bool,
}

/// Persisted state of an audio extraction job (RunPod).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioExtractionJob {
    pub job_id: String,
    pub status: String,  // "in_progress", "completed", "failed", "cancelled"
    #[serde(default)]
    pub submitted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub extraction_url: Option<String>,
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
    #[serde(default)]
    pub opus: OpusConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub sources: Vec<SourceMetadata>,
    #[serde(default)]
    pub audio_extraction: Option<AudioExtractionJob>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub auto_stop: bool,
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
            notices: Vec::new(),
            processing_state: None,
            audio_extraction: None,
            tags: Vec::new(),
            notes: None,
            auto_stop: false,
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
            opus: meta.opus,
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
            notices: Vec::new(),
            processing_state: if meta.audio_extraction.as_ref().map_or(false, |j| j.status == "in_progress") {
                Some("extracting".to_string())
            } else {
                None
            },
            audio_extraction: meta.audio_extraction.clone(),
            tags: meta.tags.clone(),
            notes: meta.notes.clone(),
            auto_stop: meta.auto_stop,
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
            opus: self.config.opus,
            created_at: self.created_at,
            updated_at: self.updated_at,
            started_at: self.started_at,
            duration_secs,
            sources: self.source_meta.clone(),
            audio_extraction: self.audio_extraction.clone(),
            tags: self.tags.clone(),
            notes: self.notes.clone(),
            auto_stop: self.auto_stop,
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
        let transcript_available = self.config.output_dir.join("transcript.json").exists();
        let summary_available = self.config.output_dir.join("summary.json").exists();
        // unconfirmed_speakers is set to 0 here; enriched from FilesDb by the caller if needed.
        let unconfirmed_speakers = 0;

        SessionInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            state: self.state,
            language: self.config.language.clone(),
            summarization_instruction: self.config.summarization_instruction.clone(),
            raw_sample_rate: self.config.raw_sample_rate,
            format: self.config.format,
            mp3: if self.config.format == AudioFormat::Mp3 { Some(self.config.mp3) } else { None },
            opus: if self.config.format == AudioFormat::Opus { Some(self.config.opus) } else { None },
            sources: self.config.sources.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            duration_secs,
            files: self.files.clone(),
            file_sizes,
            notices: self.notices.clone(),
            transcript_available,
            summary_available,
            processing_state: self.processing_state.clone(),
            unconfirmed_speakers,
            source_meta: self.source_meta.clone(),
            tags: self.tags.clone(),
            notes: self.notes.clone(),
            auto_stop: self.auto_stop,
        }
    }

    /// Compute duration from audio files (max across all tracks).
    /// WAV: reads 44-byte header only (fast). MP3: estimates from file size and bitrate.
    /// Opus: reads last Ogg page granule position for exact duration.
    fn compute_duration(dir: &std::path::Path, files: &[String], mp3_bitrate_kbps: u32) -> Option<f64> {
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
                let bps = if mp3_bitrate_kbps > 0 { mp3_bitrate_kbps as u64 * 1000 } else { 64000 };
                std::fs::metadata(&path).ok().map(|m| (m.len() * 8) as f64 / bps as f64)
            } else if f.ends_with(".opus") {
                // Read exact duration from the last Ogg page's granule position.
                // Ogg Opus granule = sample count at 48kHz, so duration = granule / 48000.
                ogg_opus_duration(&path)
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

/// Read the exact duration of an Ogg Opus file by finding the last Ogg page's
/// granule position. Ogg Opus granule = cumulative sample count at 48kHz.
/// Reads only the tail of the file (up to 65536 bytes) and scans backward for "OggS".
fn ogg_opus_duration(path: &std::path::Path) -> Option<f64> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    if file_len < 27 {
        return None; // too small to contain an Ogg page
    }

    // Read the last 65KB (or entire file if smaller)
    let tail_size = file_len.min(65536) as usize;
    file.seek(SeekFrom::End(-(tail_size as i64))).ok()?;
    let mut buf = vec![0u8; tail_size];
    file.read_exact(&mut buf).ok()?;

    // Scan backward for the last "OggS" magic
    let mut granule_pos: Option<u64> = None;
    for i in (0..buf.len().saturating_sub(26)).rev() {
        if &buf[i..i + 4] == b"OggS" {
            // Granule position is at offset 6 from page start, 8 bytes LE
            let gp = u64::from_le_bytes(buf[i + 6..i + 14].try_into().ok()?);
            granule_pos = Some(gp);
            break;
        }
    }

    // Also read the pre-skip from the OpusHead header (first 19 bytes of stream).
    // Pre-skip is at byte offset 10 of OpusHead, 2 bytes LE.
    let pre_skip = if file_len > 100 {
        file.seek(SeekFrom::Start(0)).ok()?;
        let mut head = [0u8; 100];
        file.read_exact(&mut head).ok()?;
        // Find "OpusHead" in the first page
        head.windows(8)
            .position(|w| w == b"OpusHead")
            .and_then(|pos| {
                if pos + 12 <= head.len() {
                    Some(u16::from_le_bytes([head[pos + 10], head[pos + 11]]) as u64)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    } else {
        0
    };

    granule_pos.map(|gp| (gp.saturating_sub(pre_skip)) as f64 / 48000.0)
}

