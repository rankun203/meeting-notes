use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use crossbeam_channel::Sender;
use tracing::{info, warn};

use super::source::{AudioChunk, AudioError, AudioSource};

pub struct MicSource {
    device_name: Option<String>,
    sample_rate: u32,
    stream: Option<Stream>,
    running: Arc<AtomicBool>,
}

impl MicSource {
    pub fn new(device_name: Option<String>, sample_rate: u32) -> Self {
        Self {
            device_name,
            sample_rate,
            stream: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    fn find_device(&self) -> Result<cpal::Device, AudioError> {
        let host = cpal::default_host();

        if let Some(ref name) = self.device_name {
            let devices = host.input_devices()
                .map_err(|e| AudioError::DeviceError(format!("failed to list devices: {}", e)))?;
            for device in devices {
                if let Ok(n) = device.name() {
                    if n == *name {
                        return Ok(device);
                    }
                }
            }
            return Err(AudioError::DeviceError(format!("mic device '{}' not found", name)));
        }

        host.default_input_device()
            .ok_or(AudioError::NoInputDevice)
    }
}

// cpal::Stream is !Send and !Sync but we manage it safely:
// - Stream is created and dropped on the same logical owner (MicSource)
// - start/stop are never called concurrently (protected by SessionManager's RwLock)
unsafe impl Send for MicSource {}
unsafe impl Sync for MicSource {}

impl AudioSource for MicSource {
    fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(AudioError::AlreadyRecording);
        }

        let device = self.find_device()?;
        let device_name = device.name().unwrap_or_else(|_| "unknown".into());
        info!("Using mic device: \"{}\"", device_name);

        let desired_sample_rate = cpal::SampleRate(self.sample_rate);

        // Try to find a config matching our desired sample rate
        let supported_configs = device.supported_input_configs()
            .map_err(|e| AudioError::DeviceError(format!("failed to query configs: {}", e)))?;

        let config = supported_configs
            .filter(|c| c.channels() <= 2 && c.sample_format() == cpal::SampleFormat::F32)
            .find(|c| c.min_sample_rate() <= desired_sample_rate && c.max_sample_rate() >= desired_sample_rate)
            .map(|c| c.with_sample_rate(desired_sample_rate))
            .or_else(|| {
                // Fallback: use default config
                device.default_input_config().ok()
            })
            .ok_or_else(|| AudioError::DeviceError("no suitable input config found".into()))?;

        let channels = config.channels();
        let actual_sample_rate = config.sample_rate().0;
        info!("Mic config: {} channels, {} Hz, {:?}", channels, actual_sample_rate, config.sample_format());

        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);
        let start_time = Instant::now();

        let err_fn = |err: cpal::StreamError| {
            warn!("Mic stream error: {}", err);
        };

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !running.load(Ordering::Relaxed) {
                            return;
                        }
                        let chunk = AudioChunk {
                            samples: data.to_vec(),
                            channels,
                            sample_rate: actual_sample_rate,
                            timestamp_us: start_time.elapsed().as_micros() as u64,
                        };
                        let _ = sender.try_send(chunk);
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !running.load(Ordering::Relaxed) {
                            return;
                        }
                        let samples: Vec<f32> = data.iter()
                            .map(|&s| s as f32 / i16::MAX as f32)
                            .collect();
                        let chunk = AudioChunk {
                            samples,
                            channels,
                            sample_rate: actual_sample_rate,
                            timestamp_us: start_time.elapsed().as_micros() as u64,
                        };
                        let _ = sender.try_send(chunk);
                    },
                    err_fn,
                    None,
                )
            }
            format => {
                return Err(AudioError::DeviceError(format!("unsupported sample format: {:?}", format)));
            }
        }.map_err(|e| AudioError::DeviceError(format!("failed to build stream: {}", e)))?;

        stream.play()
            .map_err(|e| AudioError::DeviceError(format!("failed to start stream: {}", e)))?;

        self.stream = Some(stream);
        info!("Mic recording started");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        info!("Mic recording stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "microphone"
    }
}
