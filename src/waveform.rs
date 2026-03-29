use std::io::BufReader;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::info;

/// Waveform data: alternating min/max pairs in a flat array.
/// Format: [min0, max0, min1, max1, ...] where values are in [-1.0, 1.0].
#[derive(Serialize, Deserialize)]
pub struct WaveformData {
    pub version: u32,
    pub length: usize,
    pub sample_rate: u32,
    pub duration_secs: f64,
    /// Flat array of alternating [min, max] pairs.
    pub data: Vec<f32>,
}

/// Number of bins for the overview waveform (matches SoundCloud standard).
const NUM_BINS: usize = 1800;

/// Compute min/max bins from a mono PCM sample buffer.
fn compute_bins(samples: &[f32], sample_rate: u32) -> WaveformData {
    if samples.is_empty() {
        return WaveformData {
            version: 1, length: 0, sample_rate, duration_secs: 0.0, data: vec![],
        };
    }

    let duration_secs = samples.len() as f64 / sample_rate as f64;
    let num_bins = NUM_BINS.min(samples.len());
    let samples_per_bin = samples.len() as f64 / num_bins as f64;

    let mut data = Vec::with_capacity(num_bins * 2);

    for i in 0..num_bins {
        let start = (i as f64 * samples_per_bin) as usize;
        let end = (((i + 1) as f64 * samples_per_bin) as usize).min(samples.len());
        let chunk = &samples[start..end];

        let mut min_val = 0.0f32;
        let mut max_val = 0.0f32;
        for &s in chunk {
            if s < min_val { min_val = s; }
            if s > max_val { max_val = s; }
        }

        data.push(min_val);
        data.push(max_val);
    }

    WaveformData { version: 1, length: num_bins, sample_rate, duration_secs, data }
}

/// Decode a WAV file to mono f32 samples using hound.
fn decode_wav(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| format!("open WAV: {}", e))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.into_samples::<f32>()
                .map(|s| s.unwrap_or(0.0))
                .collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader.into_samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / max_val)
                .collect()
        }
    };

    // Mono-mix if multi-channel
    if channels <= 1 {
        Ok((samples, sample_rate))
    } else {
        let mono: Vec<f32> = samples.chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        Ok((mono, sample_rate))
    }
}

/// Decode an Ogg Opus file to mono f32 samples.
fn decode_opus(path: &Path) -> Result<(Vec<f32>, u32), String> {
    use ogg::reading::PacketReader;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("open Opus: {}", e))?;
    let mut reader = PacketReader::new(BufReader::new(file));

    // Read OpusHead to get channel count
    let head_pkt = reader.read_packet_expected()
        .map_err(|e| format!("read OpusHead: {}", e))?;

    let channels = if head_pkt.data.len() >= 10 && &head_pkt.data[..8] == b"OpusHead" {
        head_pkt.data[9] as usize
    } else {
        1
    };

    // Skip OpusTags
    reader.read_packet_expected()
        .map_err(|e| format!("read OpusTags: {}", e))?;

    // Create decoder
    let opus_channels = match channels {
        1 => opus::Channels::Mono,
        _ => opus::Channels::Stereo,
    };
    let mut decoder = opus::Decoder::new(48000, opus_channels)
        .map_err(|e| format!("create Opus decoder: {}", e))?;

    let frame_size = 48000 / 50; // 20ms at 48kHz = 960 samples
    let mut decode_buf = vec![0.0f32; frame_size * channels];
    let mut all_samples: Vec<f32> = Vec::new();

    while let Some(pkt) = reader.read_packet()
        .map_err(|e| format!("read Opus packet: {}", e))?
    {
        let decoded = decoder.decode_float(&pkt.data, &mut decode_buf, false)
            .map_err(|e| format!("decode Opus frame: {}", e))?;

        let frame_samples = &decode_buf[..decoded * channels];
        if channels == 1 {
            all_samples.extend_from_slice(frame_samples);
        } else {
            // Mono-mix
            for frame in frame_samples.chunks(channels) {
                let sum: f32 = frame.iter().sum();
                all_samples.push(sum / channels as f32);
            }
        }
    }

    Ok((all_samples, 48000))
}

/// Decode an MP3 file to mono f32 samples using symphonia.
fn decode_mp3(path: &Path) -> Result<(Vec<f32>, u32), String> {
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("open MP3: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("probe MP3: {}", e))?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or("no audio track in MP3")?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(16000);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("create MP3 decoder: {}", e))?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(format!("read MP3 packet: {}", e)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("decode MP3: {}", e)),
        };

        let spec = *decoded.spec();
        let num_ch = spec.channels.count();
        let num_frames = decoded.frames();

        let mut sample_buf = symphonia::core::audio::SampleBuffer::<f32>::new(
            num_frames as u64, spec,
        );
        sample_buf.copy_interleaved_ref(decoded);
        let samples = sample_buf.samples();

        if num_ch == 1 {
            all_samples.extend_from_slice(samples);
        } else {
            for frame in 0..num_frames {
                let mut sum = 0.0f32;
                for ch in 0..num_ch {
                    sum += samples[frame * num_ch + ch];
                }
                all_samples.push(sum / num_ch as f32);
            }
        }
    }

    Ok((all_samples, sample_rate))
}

/// Generate waveform data from an audio file.
pub fn generate_waveform(audio_path: &Path) -> Result<WaveformData, String> {
    let ext = audio_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let (samples, sample_rate) = match ext {
        "wav" => decode_wav(audio_path)?,
        "opus" => decode_opus(audio_path)?,
        "mp3" => decode_mp3(audio_path)?,
        _ => return Err(format!("unsupported format: {}", ext)),
    };

    let waveform = compute_bins(&samples, sample_rate);
    info!(
        "Waveform generated: {} bins, {:.1}s from \"{}\"",
        waveform.length, waveform.duration_secs, audio_path.display()
    );
    Ok(waveform)
}

/// Get or generate waveform JSON for a session audio file.
/// Caches the result as `{filename}.waveform.json` alongside the audio file.
pub fn get_or_generate_waveform(session_dir: &Path, filename: &str) -> Result<WaveformData, String> {
    let audio_path = session_dir.join(filename);
    let waveform_path = session_dir.join(format!("{}.waveform.json", filename));

    // Return cached if it exists and is newer than the audio file
    if waveform_path.exists() {
        let audio_mtime = std::fs::metadata(&audio_path)
            .and_then(|m| m.modified())
            .ok();
        let wf_mtime = std::fs::metadata(&waveform_path)
            .and_then(|m| m.modified())
            .ok();

        if let (Some(a), Some(w)) = (audio_mtime, wf_mtime) {
            if w >= a {
                let json = std::fs::read_to_string(&waveform_path)
                    .map_err(|e| format!("read cached waveform: {}", e))?;
                let data: WaveformData = serde_json::from_str(&json)
                    .map_err(|e| format!("parse cached waveform: {}", e))?;
                return Ok(data);
            }
        }
    }

    let data = generate_waveform(&audio_path)?;

    let json = serde_json::to_string(&data)
        .map_err(|e| format!("serialize waveform: {}", e))?;
    if let Err(e) = std::fs::write(&waveform_path, &json) {
        tracing::warn!("Failed to cache waveform: {}", e);
    }

    Ok(data)
}
