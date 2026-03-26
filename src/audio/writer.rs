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
    Opus,
}

impl AudioFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Opus => "opus",
        }
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        AudioFormat::Opus
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OpusConfig {
    #[serde(default = "default_opus_bitrate")]
    pub bitrate_kbps: u32,
    #[serde(default = "default_opus_complexity")]
    pub complexity: u32,
}

fn default_opus_bitrate() -> u32 { 32 }
fn default_opus_complexity() -> u32 { 5 }

impl Default for OpusConfig {
    fn default() -> Self {
        Self {
            bitrate_kbps: default_opus_bitrate(),
            complexity: default_opus_complexity(),
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
        builder.set_quality(Quality::Decent)
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

// -- Opus implementation (Ogg Opus container) --

pub struct OpusAudioWriter {
    encoder: opus::Encoder,
    ogg_writer: ogg::writing::PacketWriter<'static, BufWriter<std::fs::File>>,
    serial: u32,
    channels: u16,
    /// Opus requires a fixed sample rate (must be 48000, 24000, 16000, 12000, or 8000).
    /// We use 48000 as it is the native Opus rate.
    opus_sample_rate: u32,
    /// Frame size in samples per channel (20ms at opus_sample_rate)
    frame_size: usize,
    /// Buffer to accumulate incoming samples into complete Opus frames
    pending: Vec<f32>,
    /// Cumulative granule position (sample count at 48kHz per the Ogg Opus spec)
    granule_pos: u64,
    /// Whether headers have been written
    headers_written: bool,
    /// Input sample rate (from audio source) for resampling if needed
    input_sample_rate: u32,
}

impl OpusAudioWriter {
    pub fn new(path: &PathBuf, channels: u16, input_sample_rate: u32, opus_config: &OpusConfig) -> Result<Self, AudioError> {
        // Opus only supports specific sample rates. Use 48kHz (native) and let the
        // encoder handle any internal resampling.
        let opus_sample_rate = 48000u32;

        let opus_channels = match channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err(AudioError::DeviceError(format!("Opus supports 1 or 2 channels, got {}", channels))),
        };

        let mut encoder = opus::Encoder::new(opus_sample_rate, opus_channels, opus::Application::Voip)
            .map_err(|e| AudioError::DeviceError(format!("failed to create Opus encoder: {}", e)))?;

        encoder.set_bitrate(opus::Bitrate::Bits(opus_config.bitrate_kbps as i32 * 1000))
            .map_err(|e| AudioError::DeviceError(format!("set Opus bitrate: {}", e)))?;
        encoder.set_complexity(opus_config.complexity as i32)
            .map_err(|e| AudioError::DeviceError(format!("set Opus complexity: {}", e)))?;
        // DTX: skip encoding during silence (saves CPU and file size)
        encoder.set_dtx(true)
            .map_err(|e| AudioError::DeviceError(format!("set Opus DTX: {}", e)))?;
        // Hint that input is speech
        encoder.set_signal(opus::Signal::Voice)
            .map_err(|e| AudioError::DeviceError(format!("set Opus signal: {}", e)))?;

        let file = std::fs::File::create(path)
            .map_err(|e| AudioError::DeviceError(format!("failed to create file: {}", e)))?;
        let ogg_writer = ogg::writing::PacketWriter::new(BufWriter::new(file));
        let serial: u32 = rand::random();

        // 20ms frames at opus_sample_rate
        let frame_size = opus_sample_rate as usize / 50;

        Ok(Self {
            encoder,
            ogg_writer,
            serial,
            channels,
            opus_sample_rate,
            frame_size,
            pending: Vec::new(),
            granule_pos: 0,
            headers_written: false,
            input_sample_rate,
        })
    }

    fn write_headers(&mut self) -> Result<(), AudioError> {
        use ogg::writing::PacketWriteEndInfo;

        let pre_skip = self.encoder.get_lookahead()
            .map_err(|e| AudioError::DeviceError(format!("get Opus lookahead: {}", e)))? as u16;

        // OpusHead packet
        let mut head = Vec::with_capacity(19);
        head.extend_from_slice(b"OpusHead");
        head.push(1); // version
        head.push(self.channels as u8);
        head.extend_from_slice(&pre_skip.to_le_bytes());
        head.extend_from_slice(&self.input_sample_rate.to_le_bytes()); // original sample rate (informational)
        head.extend_from_slice(&0u16.to_le_bytes()); // output gain
        head.push(0); // channel mapping family 0 (mono/stereo)

        self.ogg_writer.write_packet(head, self.serial, PacketWriteEndInfo::EndPage, 0)
            .map_err(|e| AudioError::StreamError(format!("write OpusHead: {}", e)))?;

        // OpusTags packet
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        let vendor = b"meeting-notes";
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes()); // 0 user comments

        self.ogg_writer.write_packet(tags, self.serial, PacketWriteEndInfo::EndPage, 0)
            .map_err(|e| AudioError::StreamError(format!("write OpusTags: {}", e)))?;

        self.headers_written = true;
        Ok(())
    }

    /// Resample samples from input_sample_rate to opus_sample_rate using linear interpolation.
    fn resample(&self, input: &[f32]) -> Vec<f32> {
        if self.input_sample_rate == self.opus_sample_rate {
            return input.to_vec();
        }

        let ratio = self.opus_sample_rate as f64 / self.input_sample_rate as f64;
        let channels = self.channels as usize;
        let input_frames = input.len() / channels;
        let output_frames = (input_frames as f64 * ratio).ceil() as usize;
        let mut output = Vec::with_capacity(output_frames * channels);

        for i in 0..output_frames {
            let src_pos = i as f64 / ratio;
            let src_idx = src_pos as usize;
            let frac = (src_pos - src_idx as f64) as f32;

            for ch in 0..channels {
                let s0 = input.get(src_idx * channels + ch).copied().unwrap_or(0.0);
                let s1 = input.get((src_idx + 1) * channels + ch).copied().unwrap_or(s0);
                output.push(s0 + (s1 - s0) * frac);
            }
        }

        output
    }

    fn encode_pending_frames(&mut self) -> Result<(), AudioError> {
        use ogg::writing::PacketWriteEndInfo;

        let samples_per_frame = self.frame_size * self.channels as usize;

        while self.pending.len() >= samples_per_frame {
            let frame: Vec<f32> = self.pending.drain(..samples_per_frame).collect();

            let packet = self.encoder.encode_vec_float(&frame, 4000)
                .map_err(|e| AudioError::StreamError(format!("Opus encode error: {}", e)))?;

            self.granule_pos += self.frame_size as u64;

            self.ogg_writer.write_packet(
                packet,
                self.serial,
                PacketWriteEndInfo::NormalPacket,
                self.granule_pos,
            ).map_err(|e| AudioError::StreamError(format!("Ogg write error: {}", e)))?;
        }

        Ok(())
    }
}

impl AudioWriter for OpusAudioWriter {
    fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError> {
        if !self.headers_written {
            self.write_headers()?;
        }

        let resampled = self.resample(&chunk.samples);
        self.pending.extend_from_slice(&resampled);
        self.encode_pending_frames()?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioError> {
        // Ogg writer is buffered via BufWriter, but we don't need explicit flushing
        // mid-stream — the ogg crate handles page boundaries internally.
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> Result<(), AudioError> {
        use ogg::writing::PacketWriteEndInfo;

        if !self.headers_written {
            self.write_headers()?;
        }

        // Encode remaining samples (pad to frame boundary)
        let samples_per_frame = self.frame_size * self.channels as usize;
        if !self.pending.is_empty() {
            self.pending.resize(samples_per_frame, 0.0);

            let packet = self.encoder.encode_vec_float(&self.pending, 4000)
                .map_err(|e| AudioError::StreamError(format!("Opus encode error: {}", e)))?;

            self.granule_pos += self.frame_size as u64;

            self.ogg_writer.write_packet(
                packet,
                self.serial,
                PacketWriteEndInfo::EndStream,
                self.granule_pos,
            ).map_err(|e| AudioError::StreamError(format!("Ogg write error: {}", e)))?;
        } else {
            // Write an empty end-of-stream packet
            let silence = vec![0.0f32; samples_per_frame];
            let packet = self.encoder.encode_vec_float(&silence, 4000)
                .map_err(|e| AudioError::StreamError(format!("Opus encode error: {}", e)))?;

            self.granule_pos += self.frame_size as u64;

            self.ogg_writer.write_packet(
                packet,
                self.serial,
                PacketWriteEndInfo::EndStream,
                self.granule_pos,
            ).map_err(|e| AudioError::StreamError(format!("Ogg write error: {}", e)))?;
        }

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
    opus_config: &OpusConfig,
) -> Result<Box<dyn AudioWriter>, AudioError> {
    match format {
        AudioFormat::Wav => Ok(Box::new(WavAudioWriter::new(path, channels, sample_rate)?)),
        AudioFormat::Mp3 => Ok(Box::new(Mp3AudioWriter::new(path, channels, sample_rate, mp3_config)?)),
        AudioFormat::Opus => Ok(Box::new(OpusAudioWriter::new(path, channels, sample_rate, opus_config)?)),
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
        opus_config: OpusConfig,
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
                    writer = Some(create_writer(format, &path, channels, actual_sample_rate, &mp3_config, &opus_config)?);
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
                let w = create_writer(format, &path, 1, sample_rate, &mp3_config, &opus_config)?;
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
