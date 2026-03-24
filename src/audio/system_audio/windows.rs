// Windows system audio capture via WASAPI loopback
// TODO: implement

use crossbeam_channel::Sender;
use crate::audio::source::{AudioChunk, AudioError};

pub struct WindowsSystemAudio;

impl WindowsSystemAudio {
    pub fn new(_sample_rate: u32) -> Result<Self, AudioError> {
        Err(AudioError::PlatformNotSupported)
    }

    pub fn start(&mut self, _sender: Sender<AudioChunk>) -> Result<(), AudioError> {
        Err(AudioError::PlatformNotSupported)
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        Ok(())
    }

    pub fn name(&self) -> &str {
        "system_audio_windows"
    }
}
