# Waveform Generation

## What the waveform data represents

A waveform display is a visual summary of an audio file's amplitude over time — the same visualization used by SoundCloud, Final Cut Pro, Audacity, and Apple Voice Memos. It answers: "how loud was the audio at each point in time?"

Raw audio is far too dense to display directly. A 1-hour meeting recorded at 48 kHz contains 172,800,000 samples. A screen is ~1800 pixels wide. We need to compress 172M data points into ~1800 visual columns.

## How it works

### Step 1: Decode to PCM

The audio file (Opus, WAV, or MP3) is decoded to raw PCM samples — a stream of floating-point numbers between -1.0 and 1.0 representing the sound pressure at each moment. Multi-channel audio is mixed down to mono (averaged across channels).

### Step 2: Divide into bins

The decoded samples are divided into 1800 equal-sized bins. For a 1-hour recording at 48 kHz:

```
172,800,000 samples / 1800 bins = 96,000 samples per bin
Each bin represents: 3600s / 1800 = 2 seconds of audio
```

For a 5-minute recording at 48 kHz:

```
14,400,000 samples / 1800 bins = 8,000 samples per bin
Each bin represents: 300s / 1800 = 0.167 seconds of audio
```

### Step 3: Compute min/max per bin

For each bin, we find the **minimum** and **maximum** sample value:

```
Bin 0:  samples[0..8000]      → min: -0.82,  max: 0.91
Bin 1:  samples[8000..16000]  → min: -0.45,  max: 0.67
Bin 2:  samples[16000..24000] → min: -0.01,  max: 0.02   ← silence
...
```

- **max** captures the loudest positive peak in that time window
- **min** captures the loudest negative peak
- Together they define a vertical bar from min to max at each horizontal position

### Why min/max pairs, not just peak?

Audio waveforms are **bipolar** — they swing above and below zero. Storing both min and max preserves the waveform's shape, including asymmetry between positive and negative excursions. This is the same approach used by Audacity, Final Cut Pro, and BBC's `audiowaveform` tool.

A simpler alternative (used by SoundCloud) stores only the absolute peak and mirrors it symmetrically. We use min/max for more accurate representation.

## Why 1800 bins?

1800 is SoundCloud's standard and maps well to common display widths:

| Display | Pixels | Bins/pixel |
|---------|--------|------------|
| 720p    | ~900   | 2:1 (downsample client-side) |
| 1080p   | ~1400  | ~1.3:1 |
| 1440p   | ~1800  | 1:1 |
| 4K      | ~2500  | client interpolates |

The total data size is small: 1800 bins x 2 values x 4 bytes = **14.4 KB** (binary), ~25 KB as JSON. Negligible compared to the audio file itself.

## Data format

Stored as `{filename}.waveform.json` alongside the audio file:

```json
{
  "version": 1,
  "length": 1800,
  "sample_rate": 48000,
  "duration_secs": 3542.5,
  "data": [-0.82, 0.91, -0.45, 0.67, -0.01, 0.02, ...]
}
```

The `data` array contains alternating `[min, max, min, max, ...]` pairs — 3600 floats total for 1800 bins.

## How it's rendered

On the frontend, each bin becomes one pixel column on a canvas:

```
For each pixel x (0..canvas_width):
  bin_index = x * num_bins / canvas_width
  min = data[bin_index * 2]
  max = data[bin_index * 2 + 1]

  Draw vertical bar from (canvas_mid + min * scale) to (canvas_mid + max * scale)
```

The result is the classic waveform shape: tall bars where audio is loud, flat line where it's silent.

- **Played region** is drawn in a brighter color (blue)
- **Unplayed region** is drawn in a dimmer color
- **Muted tracks** are drawn in gray at reduced opacity
- Clicking on the waveform seeks to that time position

## Generation pipeline

1. **Lazy generation**: Waveforms are computed on first HTTP request, not during recording
2. **Caching**: Saved as `.waveform.json` sidecar files; subsequent requests return cached data
3. **Cache invalidation**: If the audio file is newer than the cached waveform, it's regenerated
4. **Blocking thread**: Decoding is CPU-intensive, so it runs on `spawn_blocking` to avoid stalling the async runtime

### Decoder per format

| Format | Decoder | Notes |
|--------|---------|-------|
| WAV    | `hound` crate | Direct PCM access, fastest |
| Opus   | `ogg` + `opus` crates | Decodes at 48 kHz, handles DTX frames |
| MP3    | `symphonia` crate | Handles CBR and VBR |

## Comparison with other implementations

| App | Data points | Format | Storage |
|-----|------------|--------|---------|
| SoundCloud | 1800 | Single peak values (symmetric) | JSON or PNG |
| Audacity | Variable (256 samples/bin base) | Min/max pairs + RMS | Binary block files |
| Final Cut Pro | Variable by zoom | Min/max pairs | Internal cache |
| **This project** | **1800** | **Min/max pairs** | **JSON sidecar** |
