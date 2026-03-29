# Right-Trim Trailing Silence from Audio Sources

## Problem

When a recording source has no audio after a certain point (e.g., system audio stops at 54:00 but the recording runs to 60:00), the trailing silence wastes space. Each source may have a different actual audio length — the web UI already uses the longest for the timeline (`Math.max(...durations)`).

## Implementation Plan

### 1. Silence tracking in writer thread (`writer.rs`)

In `AudioWriterHandle::start()`, track the last non-silent chunk:

- Compute peak amplitude per chunk: `chunk.samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max)`
- Threshold: `0.001` (~-60 dBFS)
- Track `last_non_silent_frames` (cumulative frame count at the end of the last non-silent chunk)
- Return a `WriterTrimInfo { last_non_silent_secs, total_secs }` from `finish()`
- Only trim if trailing silence > 3 seconds; keep 0.5s tail after last non-silent audio

### 2. Format-specific trim (`trim.rs`)

**WAV:** Truncate file in-place, rewrite RIFF/data chunk size headers (offsets 4 and 40). Simple since WAV is raw PCM with a 44-byte header.

**MP3 (CBR):** Truncate at `trim_to_secs * bitrate_bps / 8` bytes. Works because CBR maps bytes to time linearly.

**Opus (Ogg container):** Work at the raw Ogg **page** level — do NOT re-mux individual packets (re-muxing changes page structure and corrupts the stream). Instead:
1. Parse all Ogg page boundaries from raw bytes (27-byte headers + segment tables)
2. Find the last page whose granule position <= target granule
3. Set the EOS flag (`byte[5] |= 0x04`) on that page's header
4. Recompute the page's CRC-32 (polynomial `0x04C11DB7`, MSB-first, no reflection)
5. Truncate everything after that page

Trim granularity is at page boundaries (~5s for typical Opus), which is fine for trimming minutes of trailing silence.

### 3. Wire into `Recorder::stop()` (`recorder.rs`)

After each writer finishes, check `WriterTrimInfo`. If trailing silence > 3s, call `right_trim_audio(path, format, last_non_silent_secs + 0.5)`. Failures are non-fatal (log warning, keep full-length file).

## Key Discovery: Opus DTX breaks playback of silent regions

**The Opus encoder has DTX (Discontinuous Transmission) enabled** (`encoder.set_dtx(true)`). DTX produces minimal comfort-noise frames during silence. Some players (e.g., macOS QuickTime) cannot decode these frames and stop playback when encountering silent regions.

**Fix:** Disable DTX (`set_dtx(false)`) before implementing right-trim. The file size increase is negligible (comfort noise frames are tiny). This must be done first, otherwise trimmed files with leading silence will appear corrupted even though the trim itself is correct.

## Status

Not yet implemented. DTX fix is a prerequisite.
