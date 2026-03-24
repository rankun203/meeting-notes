use std::fmt;
use std::io::BufWriter;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use crossbeam_channel::Receiver;
use hound::{SampleFormat, WavSpec, WavWriter};
use mp3lame_encoder::{Builder as LameBuilder, Bitrate, Quality, InterleavedPcm, MonoPcm, FlushGap};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use super::source::{AudioChunk, AudioError};

const FLUSH_INTERVAL_CHUNKS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Wav,
    Mp3,
}

impl AudioFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
        }
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        AudioFormat::Wav
    }
}

impl fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.extension())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Mp3Config {
    #[serde(default = "default_mp3_bitrate")]
    pub bitrate_kbps: u32,
    #[serde(default = "default_mp3_sample_rate")]
    pub sample_rate: u32,
}

fn default_mp3_bitrate() -> u32 { 64 }
fn default_mp3_sample_rate() -> u32 { 16000 }

impl Default for Mp3Config {
    fn default() -> Self {
        Self {
            bitrate_kbps: default_mp3_bitrate(),
            sample_rate: default_mp3_sample_rate(),
        }
    }
}

pub trait AudioWriter: Send + 'static {
    fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError>;
    fn flush(&mut self) -> Result<(), AudioError>;
    fn finalize(self: Box<Self>) -> Result<(), AudioError>;
}

// -- WAV implementation --

pub struct WavAudioWriter {
    writer: WavWriter<BufWriter<std::fs::File>>,
}

impl WavAudioWriter {
    pub fn new(path: &PathBuf, channels: u16, sample_rate: u32) -> Result<Self, AudioError> {
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let file = std::fs::File::create(path)
            .map_err(|e| AudioError::DeviceError(format!("failed to create file: {}", e)))?;
        let buf_writer = BufWriter::new(file);
        let writer = WavWriter::new(buf_writer, spec)?;
        Ok(Self { writer })
    }
}

impl AudioWriter for WavAudioWriter {
    fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError> {
        for &sample in &chunk.samples {
            self.writer.write_sample(sample)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioError> {
        self.writer.flush()?;
        Ok(())
    }

    fn finalize(self: Box<Self>) -> Result<(), AudioError> {
        self.writer.finalize()?;
        Ok(())
    }
}

// -- MP3 implementation --

pub struct Mp3AudioWriter {
    encoder: mp3lame_encoder::Encoder,
    file: BufWriter<std::fs::File>,
    channels: u16,
}

fn bitrate_from_kbps(kbps: u32) -> Result<Bitrate, AudioError> {
    match kbps {
        8 => Ok(Bitrate::Kbps8),
        16 => Ok(Bitrate::Kbps16),
        24 => Ok(Bitrate::Kbps24),
        32 => Ok(Bitrate::Kbps32),
        40 => Ok(Bitrate::Kbps40),
        48 => Ok(Bitrate::Kbps48),
        64 => Ok(Bitrate::Kbps64),
        80 => Ok(Bitrate::Kbps80),
        96 => Ok(Bitrate::Kbps96),
        112 => Ok(Bitrate::Kbps112),
        128 => Ok(Bitrate::Kbps128),
        160 => Ok(Bitrate::Kbps160),
        192 => Ok(Bitrate::Kbps192),
        224 => Ok(Bitrate::Kbps224),
        256 => Ok(Bitrate::Kbps256),
        320 => Ok(Bitrate::Kbps320),
        _ => Err(AudioError::DeviceError(format!("unsupported MP3 bitrate: {}kbps", kbps))),
    }
}

impl Mp3AudioWriter {
    pub fn new(path: &PathBuf, channels: u16, input_sample_rate: u32, mp3_config: &Mp3Config) -> Result<Self, AudioError> {
        let mut builder = LameBuilder::new()
            .ok_or_else(|| AudioError::DeviceError("failed to create MP3 encoder".into()))?;

        builder.set_num_channels(channels as u8)
            .map_err(|e| AudioError::DeviceError(format!("set channels: {:?}", e)))?;
        // Input sample rate must match the actual audio data
        builder.set_sample_rate(input_sample_rate)
            .map_err(|e| AudioError::DeviceError(format!("set sample rate: {:?}", e)))?;
        builder.set_brate(bitrate_from_kbps(mp3_config.bitrate_kbps)?)
            .map_err(|e| AudioError::DeviceError(format!("set bitrate: {:?}", e)))?;
        builder.set_quality(Quality::Best)
            .map_err(|e| AudioError::DeviceError(format!("set quality: {:?}", e)))?;

        let encoder = builder.build()
            .map_err(|e| AudioError::DeviceError(format!("build MP3 encoder: {:?}", e)))?;

        let file = std::fs::File::create(path)
            .map_err(|e| AudioError::DeviceError(format!("failed to create file: {}", e)))?;

        Ok(Self {
            encoder,
            file: BufWriter::new(file),
            channels,
        })
    }
}

impl AudioWriter for Mp3AudioWriter {
    fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError> {
        use std::io::Write;

        let mut mp3_out = Vec::new();
        let input = &chunk.samples;
        mp3_out.reserve(mp3lame_encoder::max_required_buffer_size(input.len()));

        let result = match self.channels {
            1 => self.encoder.encode_to_vec(MonoPcm(input), &mut mp3_out),
            2 => self.encoder.encode_to_vec(InterleavedPcm(input), &mut mp3_out),
            _ => self.encoder.encode_to_vec(InterleavedPcm(input), &mut mp3_out),
        };
        result.map_err(|e| AudioError::StreamError(format!("MP3 encode error: {:?}", e)))?;

        self.file.write_all(&mp3_out)
            .map_err(|e| AudioError::StreamError(format!("MP3 write error: {}", e)))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioError> {
        use std::io::Write;
        self.file.flush()
            .map_err(|e| AudioError::StreamError(format!("MP3 flush error: {}", e)))?;
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> Result<(), AudioError> {
        use std::io::Write;

        // LAME flush needs at least 7200 bytes of output buffer capacity
        let mut mp3_out = Vec::with_capacity(7200);
        self.encoder.flush_to_vec::<FlushGap>(&mut mp3_out)
            .map_err(|e| AudioError::StreamError(format!("MP3 flush error: {:?}", e)))?;
        self.file.write_all(&mp3_out)
            .map_err(|e| AudioError::StreamError(format!("MP3 write error: {}", e)))?;
        self.file.flush()
            .map_err(|e| AudioError::StreamError(format!("MP3 final flush error: {}", e)))?;
        Ok(())
    }
}

// -- Factory + threaded writer handle --

pub fn create_writer(
    format: AudioFormat,
    path: &PathBuf,
    channels: u16,
    sample_rate: u32,
    mp3_config: &Mp3Config,
) -> Result<Box<dyn AudioWriter>, AudioError> {
    match format {
        AudioFormat::Wav => Ok(Box::new(WavAudioWriter::new(path, channels, sample_rate)?)),
        AudioFormat::Mp3 => Ok(Box::new(Mp3AudioWriter::new(path, channels, sample_rate, mp3_config)?)),
    }
}

pub struct AudioWriterHandle {
    thread: Option<JoinHandle<Result<(), AudioError>>>,
}

impl AudioWriterHandle {
    pub fn start(
        format: AudioFormat,
        path: PathBuf,
        sample_rate: u32,
        mp3_config: Mp3Config,
        receiver: Receiver<AudioChunk>,
    ) -> Result<Self, AudioError> {
        info!("Audio writer started ({}): \"{}\"", format, path.display());

        let thread = thread::spawn(move || {
            // Create writer lazily on first chunk to detect actual channel count
            let mut writer: Option<Box<dyn AudioWriter>> = None;
            let mut chunk_count: u64 = 0;

            for chunk in receiver {
                if writer.is_none() {
                    let channels = chunk.channels;
                    let actual_sample_rate = chunk.sample_rate;
                    info!("Writer for \"{}\": {}ch {}Hz", path.display(), channels, actual_sample_rate);
                    writer = Some(create_writer(format, &path, channels, actual_sample_rate, &mp3_config)?);
                }

                writer.as_mut().unwrap().write_chunk(&chunk)?;
                chunk_count += 1;
                if chunk_count % FLUSH_INTERVAL_CHUNKS == 0 {
                    writer.as_mut().unwrap().flush()?;
                }
            }

            if let Some(w) = writer {
                w.finalize()?;
                info!("Audio writer finalized: \"{}\"", path.display());
            } else {
                // No chunks received, create an empty file with fallback params
                let w = create_writer(format, &path, 1, sample_rate, &mp3_config)?;
                w.finalize()?;
                info!("Audio writer finalized (empty): \"{}\"", path.display());
            }
            Ok(())
        });

        Ok(AudioWriterHandle {
            thread: Some(thread),
        })
    }

    pub fn finish(mut self) -> Result<(), AudioError> {
        if let Some(handle) = self.thread.take() {
            match handle.join() {
                Ok(result) => result,
                Err(e) => {
                    error!("Audio writer thread panicked: {:?}", e);
                    Err(AudioError::StreamError("writer thread panicked".into()))
                }
            }
        } else {
            Ok(())
        }
    }
}
