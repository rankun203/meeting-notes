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

    /// Stop all sources and finalize all writers. Best-effort: a failing
    /// source or writer never prevents the others from stopping — otherwise
    /// one wedged Core Audio object would leak every other stream. Returns
    /// the first error encountered, if any.
    pub fn stop(&mut self) -> Result<(), AudioError> {
        let mut first_err: Option<AudioError> = None;

        // Stop and drop all sources — dropping ensures cpal callbacks release
        // their sender clones so the writer channels can disconnect
        for active in &mut self.sources {
            if let Some(mut source) = active.source.take() {
                info!("Stopping source: {}", active.descriptor.label);
                match source.stop() {
                    Ok(()) => info!("Stopped source: {}", active.descriptor.label),
                    Err(e) => {
                        warn!("Failed to stop source {}: {}", active.descriptor.label, e);
                        first_err.get_or_insert(e);
                    }
                }
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
                match writer.finish() {
                    Ok(()) => info!("Writer finished for: {}", active.descriptor.label),
                    Err(e) => {
                        warn!("Writer for {} failed to finalize: {}", active.descriptor.label, e);
                        first_err.get_or_insert(e);
                    }
                }
            }
        }

        info!("Recording stopped for session {}", self.session_id);
        match first_err {
            None => Ok(()),
            Some(e) => Err(e),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::source::{AudioChunk, SourceType};
    use crate::audio::writer::{AudioFormat, Mp3Config, OpusConfig};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// A source that behaves like the wedged post-AirPods-disconnect mic:
    /// it leaks a Sender clone into an outside holder (standing in for the
    /// orphaned Core Audio restart thread) and errors on stop().
    struct WedgedSource {
        leak_slot: Arc<Mutex<Option<Sender<AudioChunk>>>>,
    }

    impl AudioSource for WedgedSource {
        fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
            let _ = sender.try_send(AudioChunk {
                samples: vec![0.1f32; 960],
                channels: 1,
                sample_rate: 48000,
                timestamp_us: 0,
            });
            *self.leak_slot.lock().unwrap() = Some(sender);
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            Err(AudioError::DeviceError("simulated Core Audio failure".into()))
        }

        fn name(&self) -> &str {
            "wedged"
        }
    }

    /// A well-behaved source, standing in for system audio.
    struct HealthySource {
        sender: Option<Sender<AudioChunk>>,
    }

    impl AudioSource for HealthySource {
        fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
            let _ = sender.try_send(AudioChunk {
                samples: vec![0.1f32; 960],
                channels: 1,
                sample_rate: 48000,
                timestamp_us: 0,
            });
            self.sender = Some(sender);
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            self.sender.take();
            Ok(())
        }

        fn name(&self) -> &str {
            "healthy"
        }
    }

    fn descriptor(id: &str, label: &str, source_type: SourceType) -> SourceDescriptor {
        SourceDescriptor {
            id: id.to_string(),
            source_type,
            label: label.to_string(),
            device_name: None,
        }
    }

    /// Regression test for the stop hang: one source leaks its sender and
    /// fails to stop, yet stop() must return in bounded time, finalize every
    /// writer, and report the error.
    #[test]
    fn stop_completes_despite_wedged_source() {
        let dir = std::env::temp_dir().join(format!("mn-recorder-test-{}", std::process::id()));
        let leak_slot = Arc::new(Mutex::new(None));

        let sources: Vec<(SourceDescriptor, Box<dyn AudioSource>)> = vec![
            (
                descriptor("mic", "System Microphone", SourceType::Mic),
                Box::new(WedgedSource { leak_slot: leak_slot.clone() }),
            ),
            (
                descriptor("system_mix", "System Audio", SourceType::SystemMix),
                Box::new(HealthySource { sender: None }),
            ),
        ];

        let mut recorder = Recorder::new(
            "test-session".to_string(),
            dir.clone(),
            48000,
            AudioFormat::Opus,
            Mp3Config::default(),
            OpusConfig::default(),
            sources,
        );

        let files = recorder.start().unwrap();
        assert_eq!(files.len(), 2);

        let started = Instant::now();
        let result = recorder.stop();
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "stop() took {:?} with a wedged source",
            started.elapsed()
        );
        assert!(result.is_err(), "wedged source error should be reported");
        assert!(leak_slot.lock().unwrap().is_some(), "sender must still be leaked for this test to be valid");

        // Every file must be finalized (non-empty) despite the wedged source.
        for file in &files {
            assert!(
                std::fs::metadata(file).unwrap().len() > 0,
                "{} was not finalized",
                file.display()
            );
        }
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
    ///
    /// On failure the source is returned so the caller can put it back and
    /// retry on a later tick — dropping it here would silently burn the whole
    /// retry budget on the first transient failure.
    pub fn restart(mut self) -> Result<(String, Box<dyn AudioSource>), (String, Box<dyn AudioSource>, AudioError)> {
        warn!("Restarting lost source: {}", self.label);
        // Stop is best-effort cleanup of the old stream; a failure here must
        // not prevent the start attempt.
        if let Err(e) = self.source.stop() {
            warn!("Stopping lost source {} failed (continuing to restart): {}", self.label, e);
        }
        if let Err(e) = self.source.start(self.sender) {
            return Err((self.label, self.source, e));
        }
        info!("Source reconnected: {}", self.label);
        Ok((self.label, self.source))
    }
}
