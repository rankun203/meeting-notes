use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Type of audio source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Mic,
    SystemMix,
    App,
}

/// Identifies a specific audio source instance. Attached to recorded files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDescriptor {
    pub id: String,
    pub source_type: SourceType,
    pub label: String,
    pub device_name: Option<String>,
}

/// Returned by the /sources discovery endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub id: String,
    pub source_type: SourceType,
    pub label: String,
    pub default_selected: bool,
}

/// Sanitize a label for use in filenames.
pub fn sanitize_label(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    s.trim_matches('_').to_string()
}

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
    pub timestamp_us: u64,
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no input device available")]
    NoInputDevice,

    #[error("device error: {0}")]
    DeviceError(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("WAV write error: {0}")]
    WavError(#[from] hound::Error),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("platform not supported for system audio capture")]
    PlatformNotSupported,

    #[error("already recording")]
    AlreadyRecording,

    #[error("not recording")]
    NotRecording,
}

pub trait AudioSource: Send + Sync {
    fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError>;
    fn stop(&mut self) -> Result<(), AudioError>;
    fn name(&self) -> &str;
}
