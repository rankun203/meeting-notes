---
date: 2026-09-25
title: Bounded waveform overviews and independent cached loading
status: implemented
---

## Problem

Selecting audio scanned every decoded sample to build 1,200 peak buckets and awaited every waveform before creating AVPlayerItem. Envelopes were never persisted. Long recordings therefore paid full-file read/decoding cost repeatedly, and the UI incorrectly said unavailable while loading.

## Implemented solution

AudioWaveform reads at most 1,024 frames at the center of each of 1,200 time buckets, reducing across channels with Accelerate. Buckets smaller than that are read completely. A default overview examines at most 1,228,800 frames per channel, independent of duration. WaveformCache stores small versioned JSON envelopes under the user's Caches directory, validates path/size/modification/creation dates and envelope contents, writes atomically, and keeps at most 512 entries. Corruption or cache-write failure never prevents playback.

MeetingPlayback loads cached envelopes independently of media preparation; misses generate in a separate cancellable task after playback setup. A generation token prevents old tasks publishing into a different selection. Playback and seeking no longer await waveform generation. The UI distinguishes loading from unavailable.

## Reasoning

This is an approximate overview of activity, not sample-accurate editing. Fixed peak windows bound application-level analysis and preserve opposite-phase channels; a full scan would preserve every transient but cannot offer bounded work for ten-hour audio. Cache identities deliberately avoid whole-file hashes on the fast path. Native AVPlayer file-backed playback remains in use for supported formats.

## Technical debt

- Sparse sampling may miss short sounds between windows, especially in long recordings. This is accepted for the overview, not analysis/transcription. Future exact envelopes should accumulate peaks while capturing/encoding and merge them into display buckets, with indexed levels for zooming.
- Codec seeking can decode preroll or scan metadata; the frame budget does not bound decoder I/O or guarantee latency on slow disks. Benchmark real long compressed assets before promising a latency SLA.
- Metadata identity cannot detect an external rewrite preserving path, size, and both timestamps. Accepted to avoid a full-file hash on each selection. Future app-owned revision IDs can cover managed edits; external verification would require hashing.
- Ogg Opus playback still decodes the complete source into temporary PCM CAF before AVPlayer can use it. Waveform caching makes its overview available during preparation, but does not make Opus playback stream. A future indexed incremental Opus renderer must preserve seek/pre-skip/final-granule accuracy, multitrack synchronization, cancellation and route handling. No unbounded persistent PCM cache was introduced to mask that limitation.
- Existing CLT linker-path warnings remain; remediation is tracked in 2026-09-25-swift-keychain-deprecations.md.

## Validation

Passed: 63 tests in 18 suites, production/preview build and signature verification, and git diff --check. New tests cover bounded ten-hour sampling, disk-cache hits without opening readable audio, corrupt-cache recovery, changed-source invalidation, playback readiness before waveform completion, stale-work rejection, and cached waveforms appearing while media preparation remains blocked. The invalidation test exposed cached URL resource metadata; identity now reads fresh filesystem attributes.

The ten-hour 8 kHz mono sparse PCM test took approximately 36 ms cold and 1 ms cached. This measures bounded access on this Mac, not real HDD or compressed-codec throughput. Native silent preview displayed mix/microphone/system envelopes and synchronized advancing playback positions. The user began interacting with preview during inspection, so further UI mutations were stopped. Dark mode and slow physical storage were not separately tested. Two known CLT linker-path warnings remain, with no deprecation warnings. Ogg Opus streaming is explicitly not implemented by this waveform change.

## Follow-up

The full-file Opus playback limitation and AVPlayer path described above are superseded by [libopusfile streaming playback](2026-09-25-swift-opus-streaming.md). Opus waveform sampling now seeks directly through libopusfile.
