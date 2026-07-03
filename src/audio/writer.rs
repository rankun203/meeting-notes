use std::fmt;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};
use hound::{SampleFormat, WavSpec, WavWriter};
use mp3lame_encoder::{Builder as LameBuilder, Bitrate, Quality, InterleavedPcm, MonoPcm, FlushGap};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use super::source::{AudioChunk, AudioError};

const FLUSH_INTERVAL_CHUNKS: u64 = 100;

/// RMS threshold below which audio is considered silence.
/// -60 dBFS ≈ 0.001 amplitude.
const SILENCE_RMS_THRESHOLD: f32 = 0.001;

/// Resample interleaved audio from `from_rate` to `to_rate` using linear interpolation.
/// Returns the input unchanged if rates match.
fn resample_linear(input: &[f32], channels: u16, from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let ch = channels as usize;
    let input_frames = input.len() / ch;
    let output_frames = (input_frames as f64 * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_frames * ch);

    for i in 0..output_frames {
        let src_pos = i as f64 / ratio;
        let src_idx = src_pos as usize;
        let frac = (src_pos - src_idx as f64) as f32;

        for c in 0..ch {
            let s0 = input.get(src_idx * ch + c).copied().unwrap_or(0.0);
            let s1 = input.get((src_idx + 1) * ch + c).copied().unwrap_or(s0);
            output.push(s0 + (s1 - s0) * frac);
        }
    }

    output
}

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
    sample_rate: u32,
    channels: u16,
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
        Ok(Self { writer, sample_rate, channels })
    }
}

impl AudioWriter for WavAudioWriter {
    fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError> {
        let samples = resample_linear(&chunk.samples, self.channels, chunk.sample_rate, self.sample_rate);
        for &sample in &samples {
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
    sample_rate: u32,
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
            sample_rate: input_sample_rate,
        })
    }
}

impl AudioWriter for Mp3AudioWriter {
    fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError> {
        use std::io::Write;

        let mut mp3_out = Vec::new();
        let resampled = resample_linear(&chunk.samples, self.channels, chunk.sample_rate, self.sample_rate);
        let input = &resampled;
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

        let resampled = resample_linear(&chunk.samples, self.channels, chunk.sample_rate, self.opus_sample_rate);
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

/// How long the writer thread may sleep between stop-flag checks. Bounds how
/// long `finish()` waits after setting the flag when senders have leaked.
const RECV_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Max chunks drained after the stop flag is set. A leaked source can keep
/// producing chunks forever; this caps the drain so finalize always runs.
const STOP_DRAIN_MAX_CHUNKS: usize = 1024;

/// How long `finish()` waits for the writer thread before detaching it.
const FINISH_TIMEOUT: Duration = Duration::from_secs(10);

/// Process one chunk: lazily create the writer, update the silence tracker,
/// encode, and periodically flush.
fn write_one(
    writer: &mut Option<Box<dyn AudioWriter>>,
    chunk: &AudioChunk,
    chunk_count: &mut u64,
    format: AudioFormat,
    path: &PathBuf,
    mp3_config: &Mp3Config,
    opus_config: &OpusConfig,
    last_active_ms: &AtomicU64,
) -> Result<(), AudioError> {
    if writer.is_none() {
        info!("Writer for \"{}\": {}ch {}Hz", path.display(), chunk.channels, chunk.sample_rate);
        *writer = Some(create_writer(format, path, chunk.channels, chunk.sample_rate, mp3_config, opus_config)?);
    }

    // Update last-active timestamp when audio is non-silent.
    if !chunk.samples.is_empty() {
        let mean_sq = chunk.samples.iter().map(|s| s * s).sum::<f32>() / chunk.samples.len() as f32;
        if mean_sq.sqrt() > SILENCE_RMS_THRESHOLD {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            last_active_ms.store(now_ms, Ordering::Relaxed);
        }
    }

    writer.as_mut().unwrap().write_chunk(chunk)?;
    *chunk_count += 1;
    if *chunk_count % FLUSH_INTERVAL_CHUNKS == 0 {
        writer.as_mut().unwrap().flush()?;
    }
    Ok(())
}

pub struct AudioWriterHandle {
    thread: Option<JoinHandle<Result<(), AudioError>>>,
    stop_flag: Arc<AtomicBool>,
}

impl AudioWriterHandle {
    pub fn start(
        format: AudioFormat,
        path: PathBuf,
        sample_rate: u32,
        mp3_config: Mp3Config,
        opus_config: OpusConfig,
        receiver: Receiver<AudioChunk>,
        last_active_ms: Arc<AtomicU64>,
    ) -> Result<Self, AudioError> {
        info!("Audio writer started ({}): \"{}\"", format, path.display());

        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop_flag = stop_flag.clone();

        let thread = thread::spawn(move || {
            // Create writer lazily on first chunk to detect actual channel count
            let mut writer: Option<Box<dyn AudioWriter>> = None;
            let mut chunk_count: u64 = 0;
            let mut write_error: Option<AudioError> = None;

            // Exit on channel disconnect (all senders dropped — the normal
            // path) OR on the stop flag. The flag is the safety net for
            // leaked senders: an orphaned Core Audio thread can hold a
            // Sender clone forever, and the writer must still finalize.
            loop {
                if thread_stop_flag.load(Ordering::Relaxed) {
                    let mut drained = 0;
                    while drained < STOP_DRAIN_MAX_CHUNKS {
                        match receiver.try_recv() {
                            Ok(chunk) => {
                                if let Err(e) = write_one(&mut writer, &chunk, &mut chunk_count, format, &path, &mp3_config, &opus_config, &last_active_ms) {
                                    write_error = Some(e);
                                    break;
                                }
                                drained += 1;
                            }
                            Err(_) => break,
                        }
                    }
                    break;
                }
                match receiver.recv_timeout(RECV_POLL_INTERVAL) {
                    Ok(chunk) => {
                        if let Err(e) = write_one(&mut writer, &chunk, &mut chunk_count, format, &path, &mp3_config, &opus_config, &last_active_ms) {
                            write_error = Some(e);
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }

            // Always attempt to finalize, even after a write error, so the
            // file on disk is as playable as possible.
            let finalize_result = if let Some(w) = writer.take() {
                w.finalize().map(|_| {
                    info!("Audio writer finalized: \"{}\"", path.display());
                })
            } else {
                // No chunks received, create an empty file with fallback params
                create_writer(format, &path, 1, sample_rate, &mp3_config, &opus_config)
                    .and_then(|w| w.finalize())
                    .map(|_| {
                        info!("Audio writer finalized (empty): \"{}\"", path.display());
                    })
            };

            match write_error {
                Some(e) => {
                    if let Err(fe) = finalize_result {
                        error!("Audio writer finalize also failed for \"{}\": {}", path.display(), fe);
                    }
                    Err(e)
                }
                None => finalize_result,
            }
        });

        Ok(AudioWriterHandle {
            thread: Some(thread),
            stop_flag,
        })
    }

    /// Signal the writer to stop and wait (bounded) for it to finalize.
    /// If the thread does not finish within `FINISH_TIMEOUT` (e.g. hung on
    /// disk IO), it is detached rather than hanging the caller forever.
    pub fn finish(mut self) -> Result<(), AudioError> {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let deadline = Instant::now() + FINISH_TIMEOUT;
            while !handle.is_finished() {
                if Instant::now() >= deadline {
                    error!("Audio writer thread did not finish within {:?} — detaching", FINISH_TIMEOUT);
                    return Err(AudioError::StreamError(
                        "writer thread hung; file may be missing trailing data".into(),
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;

    fn chunk(samples: usize) -> AudioChunk {
        AudioChunk {
            samples: vec![0.1f32; samples],
            channels: 1,
            sample_rate: 48000,
            timestamp_us: 0,
        }
    }

    fn start_writer(path: &PathBuf, receiver: Receiver<AudioChunk>) -> AudioWriterHandle {
        AudioWriterHandle::start(
            AudioFormat::Opus,
            path.clone(),
            48000,
            Mp3Config::default(),
            OpusConfig::default(),
            receiver,
            Arc::new(AtomicU64::new(0)),
        )
        .unwrap()
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mn-writer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn finish_completes_on_disconnect() {
        let path = temp_path("disconnect.opus");
        let (sender, receiver) = bounded(64);
        let handle = start_writer(&path, receiver);
        sender.send(chunk(960)).unwrap();
        drop(sender);
        handle.finish().unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }

    /// Regression test for the AirPods-disconnect hang: an orphaned Core
    /// Audio restart thread can hold a Sender clone forever. finish() must
    /// still finalize the file in bounded time instead of joining forever.
    #[test]
    fn finish_completes_with_leaked_sender() {
        let path = temp_path("leaked.opus");
        let (sender, receiver) = bounded(64);
        let handle = start_writer(&path, receiver);
        sender.send(chunk(960)).unwrap();

        let _leaked = sender.clone(); // simulates the orphaned thread's clone
        // (the recorder's own copy is dropped on stop; leaked one stays)
        drop(sender);

        let started = Instant::now();
        handle.finish().unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "finish() took {:?} with a leaked sender",
            started.elapsed()
        );
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }

    /// A source that never sends anything must still produce a valid
    /// (empty) file on finish.
    #[test]
    fn finish_writes_empty_file_when_no_chunks() {
        let path = temp_path("empty.opus");
        let (sender, receiver) = bounded::<AudioChunk>(64);
        let handle = start_writer(&path, receiver);
        let _leaked = sender; // keep the channel connected the whole time
        handle.finish().unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }
}
