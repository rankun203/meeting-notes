---
date: 2026-09-24
title: Native Opus recording encoder and interoperable Ogg container
status: implementation-complete-validation-in-progress
---

## Problem

The Swift recorder initially retained large PCM WAV tracks. The user requested Opus as the primary recording format, with native alternatives where supported, while retaining the Command Line Tools-only build and installation flow. A CAF file containing Opus is not an interoperable `.opus` file and must not be renamed to pretend otherwise.

## Implemented solution

- `Core/RecordingEncoder.swift` encodes each finalized source track with Apple's AVAudioConverter Opus encoder and writes a real RFC 7845 Ogg Opus container. It does not require libopus, FFmpeg, Homebrew, downloaded frameworks, or full Xcode.
- Mono uses 48 kbps and stereo uses 96 kbps, with a 48 kHz Opus output timeline. The original sample-rate field is preserved in OpusHead. Source channel identities remain separate files.
- The streaming muxer writes OpusHead and OpusTags, bounded groups of 20 packets per page, little-endian fields, complete packet lacing, page sequence numbers, CRCs, and a final EOS page. Encoder priming supplies pre-skip; final granule position trims trailing padding to the original audible duration. Packet durations are parsed from their Opus TOC because native packet descriptions report zero variable frames.
- M4A uses native AAC through AVAudioFile. Runtime codec probing confirms Opus encoding is available on the development Mac and MP3 encoding is not; MP3 is not advertised as an available recording option.
- Encoding runs in a cancellable detached utility task after capture. It streams bounded PCM/packet buffers, creates private temporary files beside the destination, removes failed temporary outputs, and refuses to overwrite an existing destination or its source. The root integrates metadata-first finalization before deleting PCM spools.

## Research and reasoning

- Native runtime probes distinguish the codec from the container: `afconvert -f caff -d opus` succeeds on a synthetic tone, but native Ogg output fails at close with `pck?`. Therefore the implementation uses Apple's codec with a small standards-based muxer rather than relying on the native Ogg file writer.
- [Apple AVAudioConverter](https://developer.apple.com/documentation/avfaudio/avaudioconverter) provides packet conversion and priming information. [Apple kAudioFormatOpus](https://developer.apple.com/documentation/coreaudiotypes/kaudioformatopus) identifies the native codec.
- [RFC 7845](https://www.rfc-editor.org/rfc/rfc7845.html) specifies OpusHead, channel mapping family 0, pre-skip, 48 kHz granules, and final-duration trimming. [RFC 6716](https://www.rfc-editor.org/rfc/rfc6716.html) specifies Opus TOC packet durations.
- [Xiph Ogg framing](https://www.xiph.org/ogg/doc/framing.html) specifies lacing and page CRCs.
- Native codec probes must run in the same authorized environment as the app: the tool sandbox initially returned a misleading unsupported-format error, while the approved probe successfully encoded Opus. Codec absence still returns an actionable error and preserves the original recording.

## Technical debt

- Encoding uses a finalized PCM spool instead of compressing inside capture callbacks. This keeps codec work and final-granule bookkeeping out of capture, but temporarily requires enough disk space for the PCM and compressed copy and adds stop latency. Remediation: introduce tested bounded background packet encoding during capture with crash-safe journals and equivalent finalization recovery.
- Opus channel mapping family 0 currently supports mono and stereo. A device with more channels retains its original WAV and receives an explicit error instead of silently downmixing. Remediation: add RFC 7845 multistream mapping with independent channel-layout fixtures if multichannel Opus capture is needed.
- The muxer is maintained project code. Its scope is deliberately limited to encoder-generated mono/stereo packets with bounded page sizes. Remediation: retain RFC-focused tests, fuzz page construction, and cross-check output with independent decoders when changing packet or container logic.
- Encoder availability varies with the installed OS. Unavailable Opus returns a clear fallback instruction rather than installing dependencies. Remediation: add supported-OS/device coverage and only consider vendored codec source if a verified target lacks native encoding.

## Validation

- Standalone Swift compilation of the encoder succeeded using Command Line Tools.
- Native runtime capability probe: Opus available, MP3 unavailable on this Mac.
- Independent FFmpeg validation (development verification only, not an app/build dependency): a one-second mono 48 kHz fixture decodes to exactly 48,000 samples; a stereo 44.1 kHz fixture with 22,073 source frames decodes to exactly 24,025 stereo frames at 48 kHz, matching rounded duration. Both decoded without errors.
- Added automated native round-trip tests for mono, stereo resampling, a 100-frame clip shorter than encoder pre-skip, nonzero signal, exact channel count and duration, M4A encoding, existing destination protection, and Opus TOC validation. The root agent coordinates the full build/test run and shared-index commit.
