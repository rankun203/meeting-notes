use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crossbeam_channel::{self, Sender};
use tracing::{info, warn};

use super::source::{AudioChunk, AudioError, AudioSource, SourceDescriptor, SourceType, sanitize_label};
use super::writer::{AudioFormat, AudioWriterHandle, Mp3Config, OpusConfig};

struct ActiveSource {
    descriptor: SourceDescriptor,
    source: Option<Box<dyn AudioSource>>,
    writer: Option<AudioWriterHandle>,
    sender: Option<Sender<AudioChunk>>,
    file_path: Option<PathBuf>,
    /// Epoch millis of last non-silent audio chunk (updated by writer thread).
    last_active_ms: Arc<AtomicU64>,
}

pub struct Recorder {
    session_id: String,
    output_dir: PathBuf,
    sample_rate: u32,
    format: AudioFormat,
    mp3_config: Mp3Config,
    opus_config: OpusConfig,
    sources: Vec<ActiveSource>,
}

impl Recorder {
    pub fn new(
        session_id: String,
        output_dir: PathBuf,
        sample_rate: u32,
        format: AudioFormat,
        mp3_config: Mp3Config,
        opus_config: OpusConfig,
        sources: Vec<(SourceDescriptor, Box<dyn AudioSource>)>,
    ) -> Self {
        let sources = sources
            .into_iter()
            .map(|(desc, source)| ActiveSource {
                descriptor: desc,
                source: Some(source),
                writer: None,
                sender: None,
                file_path: None,
                last_active_ms: Arc::new(AtomicU64::new(0)),
            })
            .collect();
        Self {
            session_id,
            output_dir,
            sample_rate,
            format,
            mp3_config,
            opus_config,
            sources,
        }
    }

    pub fn start(&mut self) -> Result<Vec<PathBuf>, AudioError> {
        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| AudioError::DeviceError(format!("failed to create output dir: {}", e)))?;

        let ext = self.format.extension();
        let mut files = Vec::new();

        for active in &mut self.sources {
            let label = sanitize_label(&active.descriptor.label);
            let path = self.output_dir.join(format!("{}.{}", label, ext));
            info!(
                "Recording {} to \"{}\"",
                active.descriptor.label,
                path.display()
            );

            let (sender, receiver) = crossbeam_channel::bounded(1024);
            let writer = AudioWriterHandle::start(
                self.format,
                path.clone(),
                self.sample_rate,
                self.mp3_config,
                self.opus_config,
                receiver,
                active.last_active_ms.clone(),
            )?;
            active.source.as_mut().unwrap().start(sender.clone())?;
            active.writer = Some(writer);
            active.sender = Some(sender);
            active.file_path = Some(path.clone());
            files.push(path);
        }

        Ok(files)
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        // Stop and drop all sources — dropping ensures cpal callbacks release
        // their sender clones so the writer channels can disconnect
        for active in &mut self.sources {
            if let Some(mut source) = active.source.take() {
                info!("Stopping source: {}", active.descriptor.label);
                source.stop()?;
                info!("Stopped source: {}", active.descriptor.label);
                // source dropped here, freeing callback's sender clone
            }
        }

        // Drop our sender copies to fully disconnect writer channels
        info!("Dropping senders for session {}", self.session_id);
        for active in &mut self.sources {
            active.sender.take();
        }

        // Wait for writers to finalize
        info!("Waiting for writers to finalize for session {}", self.session_id);
        for active in &mut self.sources {
            if let Some(writer) = active.writer.take() {
                info!("Finishing writer for: {}", active.descriptor.label);
                writer.finish()?;
                info!("Writer finished for: {}", active.descriptor.label);
            }
        }

        info!("Recording stopped for session {}", self.session_id);
        Ok(())
    }

    /// Returns (descriptor, file_path) pairs for metadata generation.
    pub fn source_metadata(&self) -> Vec<(&SourceDescriptor, Option<&PathBuf>)> {
        self.sources
            .iter()
            .map(|a| (&a.descriptor, a.file_path.as_ref()))
            .collect()
    }

    /// Check if any source has lost its device (e.g. Core Audio graph change).
    pub fn has_device_lost_sources(&self) -> bool {
        self.sources.iter().any(|a| {
            a.source.as_ref().map_or(false, |s| s.is_device_lost())
        })
    }

    /// Returns the epoch millis of the last non-silent audio chunk from the
    /// system audio source, or None if no system audio source exists.
    pub fn system_audio_last_active_ms(&self) -> Option<u64> {
        self.sources.iter()
            .find(|a| a.descriptor.source_type == SourceType::SystemMix)
            .map(|a| a.last_active_ms.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Restart sources that lost their device. Stops the old stream and starts
    /// a new one with the existing sender channel, so the writer keeps running.
    pub fn restart_lost_sources(&mut self) -> Result<Vec<String>, AudioError> {
        let mut restarted = Vec::new();
        for active in &mut self.sources {
            let source = match active.source.as_mut() {
                Some(s) if s.is_device_lost() => s,
                _ => continue,
            };

            let sender = match active.sender.as_ref() {
                Some(s) => s.clone(),
                None => continue,
            };

            let label = active.descriptor.label.clone();
            warn!("Restarting lost source: {}", label);

            // Drop old stream, then build fresh one with new device handle
            source.stop()?;
            source.start(sender)?;

            info!("Source reconnected: {}", label);
            restarted.push(label);
        }
        Ok(restarted)
    }
}
