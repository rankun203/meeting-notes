use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoStopSettings {
    /// Stop after this many seconds without non-silent system audio. `None`
    /// disables silence-based auto-stop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_audio_silence_secs: Option<u64>,
    #[serde(default)]
    pub screen_lock: bool,
    #[serde(default)]
    pub system_sleep: bool,
}

impl Default for AutoStopSettings {
    fn default() -> Self {
        Self {
            system_audio_silence_secs: None,
            screen_lock: true,
            system_sleep: true,
        }
    }
}

fn deserialize_auto_stop<'de, D>(deserializer: D) -> Result<AutoStopSettings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StoredAutoStop {
        Legacy(bool),
        Settings(AutoStopSettings),
    }

    Ok(match StoredAutoStop::deserialize(deserializer)? {
        // Before dedicated settings existed, `true` meant 60 seconds of
        // system-audio silence. Preserve that behavior when loading old data.
        StoredAutoStop::Legacy(true) => AutoStopSettings {
            system_audio_silence_secs: Some(60),
            screen_lock: false,
            system_sleep: false,
        },
        StoredAutoStop::Legacy(false) => AutoStopSettings {
            system_audio_silence_secs: None,
            screen_lock: false,
            system_sleep: false,
        },
        StoredAutoStop::Settings(settings) => settings,
    })
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
    /// Live notice keys hidden by the user until that condition resolves.
    pub dismissed_notice_keys: HashSet<String>,
    /// Current processing state (transcribing, matching, completed, failed).
    pub processing_state: Option<String>,
    /// Persisted audio extraction job info (for resume on restart).
    pub audio_extraction: Option<AudioExtractionJob>,
    /// User-assigned tags.
    pub tags: Vec<String>,
    /// User notes for this session.
    pub notes: Option<String>,
    pub auto_stop: AutoStopSettings,
    /// When summary generation started (in-memory only, not persisted).
    pub summary_started_at: Option<DateTime<Utc>>,
    /// Duration carried over from metadata.json. Only used when the session has
    /// no audio files to measure — imported transcript-only sessions.
    pub duration_secs: Option<f64>,
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
    // Absent for a session with no audio — see Session::has_audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<AudioFormat>,
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
    pub summary_started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_state: Option<String>,
    pub unconfirmed_speakers: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_meta: Vec<SourceMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub auto_stop: AutoStopSettings,
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
    // Encoder settings describe the audio files, so they are only written when
    // the session actually has some. A session imported from an external
    // transcript has no audio and must not claim a codec it never used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<AudioFormat>,
    #[serde(default, alias = "sample_rate", skip_serializing_if = "Option::is_none")]
    pub raw_sample_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mp3: Option<Mp3Config>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opus: Option<OpusConfig>,
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
    #[serde(default, deserialize_with = "deserialize_auto_stop")]
    pub auto_stop: AutoStopSettings,
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
            dismissed_notice_keys: HashSet::new(),
            processing_state: None,
            audio_extraction: None,
            tags: Vec::new(),
            notes: None,
            auto_stop: AutoStopSettings::default(),
            summary_started_at: None,
            duration_secs: None,
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
            raw_sample_rate: meta.raw_sample_rate.unwrap_or_else(default_sample_rate),
            format: meta.format.unwrap_or_default(),
            mp3: meta.mp3.unwrap_or_default(),
            opus: meta.opus.unwrap_or_default(),
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
            dismissed_notice_keys: HashSet::new(),
            processing_state: if meta.audio_extraction.as_ref().map_or(false, |j| j.status == "in_progress") {
                Some("extracting".to_string())
            } else {
                None
            },
            audio_extraction: meta.audio_extraction.clone(),
            tags: meta.tags.clone(),
            notes: meta.notes.clone(),
            auto_stop: meta.auto_stop,
            summary_started_at: None,
            duration_secs: meta.duration_secs,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn to_metadata(&self) -> SessionMetadata {
        let duration_secs = self.effective_duration();
        SessionMetadata {
            session_id: self.id.clone(),
            name: self.name.clone(),
            state: self.state,
            language: self.config.language.clone(),
            format: self.has_audio().then_some(self.config.format),
            raw_sample_rate: self.has_audio().then_some(self.config.raw_sample_rate),
            mp3: self.has_audio().then_some(self.config.mp3),
            opus: self.has_audio().then_some(self.config.opus),
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
        let duration_secs = self.effective_duration();
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
            raw_sample_rate: self.has_audio().then_some(self.config.raw_sample_rate),
            format: self.has_audio().then_some(self.config.format),
            mp3: (self.has_audio() && self.config.format == AudioFormat::Mp3).then_some(self.config.mp3),
            opus: (self.has_audio() && self.config.format == AudioFormat::Opus).then_some(self.config.opus),
            sources: self.config.sources.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            duration_secs,
            files: self.files.clone(),
            file_sizes,
            notices: self.notices.clone(),
            transcript_available,
            summary_available,
            summary_started_at: self.summary_started_at,
            processing_state: self.processing_state.clone(),
            unconfirmed_speakers,
            source_meta: self.source_meta.clone(),
            tags: self.tags.clone(),
            notes: self.notes.clone(),
            auto_stop: self.auto_stop,
        }
    }

    /// Whether this session deals in audio at all. False only for a stopped
    /// session that produced no audio files — an imported transcript, or a
    /// recording that never wrote anything. A session still being set up or
    /// recording counts as audio-bearing even before its first file lands.
    pub fn has_audio(&self) -> bool {
        self.state != SessionState::Stopped || self.files.iter().any(|f| is_audio_file(f))
    }

    /// Duration of the session: measured from the audio files when there are
    /// any, otherwise the value carried in metadata.json. Sessions imported
    /// from an external transcript have no audio to measure, so without the
    /// fallback their duration would be dropped on every metadata rewrite.
    fn effective_duration(&self) -> Option<f64> {
        Self::compute_duration(&self.config.output_dir, &self.files, self.config.mp3.bitrate_kbps)
            .or(self.duration_secs)
    }

    /// Compute duration from audio files (max across all tracks).
    /// WAV: reads 44-byte header only (fast). MP3: estimates from file size and bitrate.
    /// Opus: reads last Ogg page granule position for exact duration.
    /// Returns None when the session has no audio files to measure.
    pub(crate) fn compute_duration(dir: &std::path::Path, files: &[String], mp3_bitrate_kbps: u32) -> Option<f64> {
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

/// Whether a filename in a session folder is one of the recorded audio tracks,
/// as opposed to a transcript, summary or metadata sidecar.
fn is_audio_file(name: &str) -> bool {
    name.ends_with(".wav") || name.ends_with(".mp3") || name.ends_with(".opus")
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata_with(auto_stop: serde_json::Value) -> SessionMetadata {
        serde_json::from_value(json!({
            "session_id": "legacy-session",
            "language": "en",
            "created_at": "2026-08-12T00:00:00Z",
            "updated_at": "2026-08-12T00:00:00Z",
            "auto_stop": auto_stop,
        }))
        .unwrap()
    }

    #[test]
    fn legacy_auto_stop_true_migrates_to_sixty_second_silence() {
        let metadata = metadata_with(json!(true));
        assert_eq!(metadata.auto_stop.system_audio_silence_secs, Some(60));
        assert!(!metadata.auto_stop.screen_lock);
        assert!(!metadata.auto_stop.system_sleep);
    }

    #[test]
    fn new_sessions_enable_lock_and_sleep_auto_stop_by_default() {
        let settings = AutoStopSettings::default();
        assert_eq!(settings.system_audio_silence_secs, None);
        assert!(settings.screen_lock);
        assert!(settings.system_sleep);
    }

    #[test]
    fn dedicated_auto_stop_settings_round_trip() {
        let metadata = metadata_with(json!({
            "system_audio_silence_secs": 90,
            "screen_lock": true,
            "system_sleep": true,
        }));
        assert_eq!(
            metadata.auto_stop,
            AutoStopSettings {
                system_audio_silence_secs: Some(90),
                screen_lock: true,
                system_sleep: true,
            }
        );

        let serialized = serde_json::to_value(&metadata).unwrap();
        assert_eq!(serialized["auto_stop"]["system_audio_silence_secs"], 90);
        assert_eq!(serialized["auto_stop"]["screen_lock"], true);
        assert_eq!(serialized["auto_stop"]["system_sleep"], true);
    }
}
