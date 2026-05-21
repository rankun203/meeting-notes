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

    /// Take ownership of any sources that lost their device, leaving the
    /// writer/sender/descriptor slots in place so the source can be put back
    /// after off-thread restart. Returns one LostSource per slot taken.
    ///
    /// The caller is expected to either `put_back_source` after a successful
    /// restart or `clear_source` after giving up — otherwise the slot stays
    /// empty and the recorder cannot record from that source again.
    pub fn take_lost_sources(&mut self) -> Vec<LostSource> {
        let mut out = Vec::new();
        for active in &mut self.sources {
            let is_lost = active.source.as_ref().map_or(false, |s| s.is_device_lost());
            if !is_lost {
                continue;
            }
            let sender = match active.sender.as_ref() {
                Some(s) => s.clone(),
                None => continue,
            };
            if let Some(source) = active.source.take() {
                out.push(LostSource {
                    label: active.descriptor.label.clone(),
                    source,
                    sender,
                });
            }
        }
        out
    }

    /// Put a previously-taken source back into its slot. Returns true if a
    /// matching slot was found.
    pub fn put_back_source(&mut self, label: &str, source: Box<dyn AudioSource>) -> bool {
        for active in &mut self.sources {
            if active.descriptor.label == label {
                active.source = Some(source);
                return true;
            }
        }
        false
    }

    /// Mark a source slot as permanently lost (e.g. its restart panicked or
    /// hung past the deadline and the box was leaked or dropped). The writer
    /// keeps draining whatever chunks already shipped; further data won't
    /// arrive until/unless a future call repopulates the slot.
    pub fn clear_source(&mut self, label: &str) {
        for active in &mut self.sources {
            if active.descriptor.label == label {
                active.source = None;
                return;
            }
        }
    }

    /// True if every source slot is empty — i.e. every source was taken and
    /// not put back. A recorder in this state can't produce any further audio.
    pub fn has_no_live_sources(&self) -> bool {
        self.sources.iter().all(|a| a.source.is_none())
    }
}

/// A source that has been taken out of the Recorder for off-thread restart.
pub struct LostSource {
    pub label: String,
    pub source: Box<dyn AudioSource>,
    pub sender: Sender<AudioChunk>,
}

impl LostSource {
    /// Restart this source: stop the old stream, then start a fresh one with
    /// the original sender so the writer channel stays connected.
    pub fn restart(mut self) -> Result<(String, Box<dyn AudioSource>), (String, AudioError)> {
        warn!("Restarting lost source: {}", self.label);
        if let Err(e) = self.source.stop() {
            return Err((self.label, e));
        }
        if let Err(e) = self.source.start(self.sender) {
            return Err((self.label, e));
        }
        info!("Source reconnected: {}", self.label);
        Ok((self.label, self.source))
    }
}
