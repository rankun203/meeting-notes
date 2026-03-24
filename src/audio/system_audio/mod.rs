#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_bindings;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

use crossbeam_channel::Sender;

use super::source::{AudioChunk, AudioError, AudioSource};

/// Platform-agnostic system audio source.
/// Captures all system audio output (e.g. Teams, browser, media players).
pub struct SystemAudioSource {
    #[cfg(target_os = "macos")]
    inner: macos::MacosSystemAudio,

    #[cfg(target_os = "linux")]
    inner: linux::LinuxSystemAudio,

    #[cfg(target_os = "windows")]
    inner: windows::WindowsSystemAudio,
}

impl SystemAudioSource {
    pub fn new(sample_rate: u32) -> Result<Self, AudioError> {
        Ok(Self {
            #[cfg(target_os = "macos")]
            inner: macos::MacosSystemAudio::new(sample_rate)?,

            #[cfg(target_os = "linux")]
            inner: linux::LinuxSystemAudio::new(sample_rate)?,

            #[cfg(target_os = "windows")]
            inner: windows::WindowsSystemAudio::new(sample_rate)?,
        })
    }
}

impl AudioSource for SystemAudioSource {
    fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
        self.inner.start(sender)
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.inner.stop()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

// Stream contains raw pointers but we manage lifecycle safely
unsafe impl Sync for SystemAudioSource {}
unsafe impl Send for SystemAudioSource {}
