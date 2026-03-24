use std::path::PathBuf;

use crossbeam_channel::{self, Sender};
use tracing::info;

use super::source::{AudioChunk, AudioError, AudioSource};
use super::writer::{AudioFormat, AudioWriterHandle, Mp3Config};

pub struct Recorder {
    session_id: String,
    output_dir: PathBuf,
    sample_rate: u32,
    format: AudioFormat,
    mp3_config: Mp3Config,
    mic_source: Option<Box<dyn AudioSource>>,
    system_source: Option<Box<dyn AudioSource>>,
    mic_writer: Option<AudioWriterHandle>,
    system_writer: Option<AudioWriterHandle>,
    mic_sender: Option<Sender<AudioChunk>>,
    system_sender: Option<Sender<AudioChunk>>,
}

impl Recorder {
    pub fn new(
        session_id: String,
        output_dir: PathBuf,
        sample_rate: u32,
        format: AudioFormat,
        mp3_config: Mp3Config,
        mic_source: Option<Box<dyn AudioSource>>,
        system_source: Option<Box<dyn AudioSource>>,
    ) -> Self {
        Self {
            session_id,
            output_dir,
            sample_rate,
            format,
            mp3_config,
            mic_source,
            system_source,
            mic_writer: None,
            system_writer: None,
            mic_sender: None,
            system_sender: None,
        }
    }

    pub fn start(&mut self) -> Result<Vec<PathBuf>, AudioError> {
        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| AudioError::DeviceError(format!("failed to create output dir: {}", e)))?;

        let ext = self.format.extension();
        let mut files = Vec::new();

        if let Some(ref mut mic) = self.mic_source {
            let path = self.output_dir.join(format!("{}_mic.{}", self.session_id, ext));
            info!("Recording mic audio to \"{}\"", path.display());
            let (sender, receiver) = crossbeam_channel::bounded(1024);
            let writer = AudioWriterHandle::start(self.format, path.clone(), self.sample_rate, self.mp3_config, receiver)?;
            mic.start(sender.clone())?;
            self.mic_writer = Some(writer);
            self.mic_sender = Some(sender);
            files.push(path);
        }

        if let Some(ref mut system) = self.system_source {
            let path = self.output_dir.join(format!("{}_system.{}", self.session_id, ext));
            info!("Recording system audio to \"{}\"", path.display());
            let (sender, receiver) = crossbeam_channel::bounded(1024);
            let writer = AudioWriterHandle::start(self.format, path.clone(), self.sample_rate, self.mp3_config, receiver)?;
            system.start(sender.clone())?;
            self.system_writer = Some(writer);
            self.system_sender = Some(sender);
            files.push(path);
        }

        Ok(files)
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        if let Some(ref mut mic) = self.mic_source {
            mic.stop()?;
        }
        if let Some(ref mut system) = self.system_source {
            system.stop()?;
        }

        // Drop senders to signal writer threads to finish
        self.mic_sender.take();
        self.system_sender.take();

        // Wait for writers to finalize
        if let Some(writer) = self.mic_writer.take() {
            writer.finish()?;
        }
        if let Some(writer) = self.system_writer.take() {
            writer.finish()?;
        }

        info!("Recording stopped for session {}", self.session_id);
        Ok(())
    }
}
