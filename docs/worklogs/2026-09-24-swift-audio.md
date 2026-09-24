---
date: 2026-09-24
title: Native Swift capture quality and audio integrity
status: implemented
---

## Problem

The initial native client used independent microphone and system recordings without explicit clock alignment or selectable speech processing. Speakerphone meetings can duplicate remote voices in the microphone track; device changes, delayed samples, and disk failures must not silently produce plausible but incomplete recordings.

## Implemented solution

- `Core/AudioCapture.swift` now captures the microphone through AVAudioEngine and system audio through ScreenCaptureKit. The streams remain separate; the client never plays captured microphone or system audio back to speakers.
- Microphone voice processing is an explicit, persisted, default-off setting. Enabling it requests Apple's voice-processing I/O, automatic gain control, and minimum, non-advanced other-audio ducking. A silent output source keeps hardware output rendering active without monitoring the captured meeting.
- `Core/TimedAudioWriter.swift` uses `ExtAudioFileWriteAsync`, warmed before microphone delivery, to hand PCM to Core Audio's internal ring buffer. Finalization disposes the writer to drain pending writes; overflow, format changes, and I/O errors surface instead of silently dropping samples.
- Microphone host timestamps and ScreenCaptureKit timestamps converted through `synchronizationClock` share a host-clock timeline. Initial/remaining missing intervals are padded; overlapping samples are trimmed; sub-2 ms clock rounding is tolerated. Gaps over 30 seconds stop capture with a partial-recording error.
- Separate PCM16 WAV files preserve source identities and actual channel counts. Effective voice-processing state, rate, and channels are saved in `Meeting.recordingProfile`.
- Audio-engine configuration changes stop the recording and retain partial audio. Empty selected tracks generate a finalization warning, and a periodic status reports whether callbacks have delivered samples; a silent source is not treated as proof of permission denial.
- Direct transcription uses temporary 10-minute AAC exports and restores chunk timestamp offsets. Original separate recordings remain local.

## Reasoning and sources

- [Apple WWDC19: What's New in AVAudioEngine](https://developer.apple.com/videos/play/wwdc2019/510/) explains hardware input/output voice processing, enabling it only while stopped, and the distinction between ordinary taps and realtime sink/source render blocks. The silent source's render block only zeroes output memory; file work is outside that realtime callback.
- [Apple WWDC23: What's new in voice processing](https://developer.apple.com/videos/play/wwdc2023/10235/) documents noise suppression, echo processing, gain control, microphone modes, and ducking of other apps' audio. Default-off preserves the unprocessed recording path and avoids unexpectedly modifying another app's playback volume.
- [ScreenCaptureKit synchronization clock](https://developer.apple.com/documentation/screencapturekit/scstream/synchronizationclock) supplies the conversion between stream and host time.
- [ExtAudioFileWriteAsync](https://developer.apple.com/documentation/audiotoolbox/extaudiofilewriteasync(_:_:_:)) and the SDK's `ExtendedAudioFile.h` document the internal asynchronous ring buffer, prewarming, and dispose-to-flush behavior.
- [Apple's Core Audio taps sample](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps) offers an alternative on macOS 14.2, but a process tap itself provides no documented acoustic-echo-cancellation feature. It also adds aggregate-device lifecycle management. ScreenCaptureKit remains the existing system-output path.
- [WWDC24 ScreenCaptureKit](https://developer.apple.com/videos/play/wwdc2024/10088/) introduces microphone capture on newer macOS; adopting that API alone would not satisfy the macOS 14.2 baseline or establish voice-processing behavior.

## Technical debt

- External-app acoustic echo cancellation is **not guaranteed**. Apple's voice-processing documentation describes device-playback cancellation, but does not establish reliable reference coverage for this recorder's unrelated conferencing-app playback on every route. The optional processing path is accepted for documented speech processing; speaker bleed may remain. Remediation: measure built-in speaker/mic, USB speakerphone, and Bluetooth routes with repeatable far-end/double-talk fixtures before making stronger AEC claims; consider an explicit validated reference-aware offline processor if required. Headphones are the reliable operational recommendation.
- Route changes intentionally stop instead of stitching a changing device into the same files. This avoids rate/channel reinterpretation and hidden discontinuities. The user must start a new recording. Remediation: support explicit segmented capture with per-segment formats and verified timeline reconstruction.
- PCM16 WAV is interoperable and preserves channels but is larger than compressed audio and has a practical RIFF size limit for exceptionally long/multichannel sessions. Remediation: bounded-duration lossless segments or CAF originals with verified server conversion.
- Direct-transcription chunks can split an utterance at a ten-minute boundary. Bounded excerpts support ordinary long meetings without exceeding upload limits; contextual continuity can be reduced. Remediation: overlap chunks and reconcile words using timestamp-aware deduplication, backed by regression fixtures.
- Host-clock alignment corrects gaps and overlaps rather than implementing a dedicated adaptive sample-rate converter. Different hardware clocks can require small repeated adjustments. Remediation: measure long-duration drift and add a bounded resampling strategy if sample-level continuity is required.

## Validation

Implemented synthetic tests cover planar stereo WAV sample readback, startup padding, missing intervals, overlap trimming, invalid timestamps, format-change rejection with flushed partial files, and direct-transcription chunk boundaries. The final CLT release build and installer passed, and all 23 tests in eight suites passed; the stereo sample-readback fixture exercises the production asynchronous writer. See the integration worklog for UI coverage.

Hardware permission prompts, actual microphone/system capture, device switching, Bluetooth behavior, acoustic echo rejection, noise suppression quality, and long-duration drift remain manual verification requirements. No live acoustic-quality claim is inferred from synthetic PCM tests.
