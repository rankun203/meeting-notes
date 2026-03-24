use crossbeam_channel::Sender;
use thiserror::Error;

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
