---
date: 2026-09-24
task: native-recording-formats
status: implemented-awaiting-acoustic-test
---

**Problem:** Native capture saved only WAV; the user requires Opus as primary storage, with native alternatives, and wants a real echo/noise comparison rather than an untested quality claim.

**Implemented solution:** Adding default Opus and selectable M4A/AAC or WAV, bounded native codec conversion after capture, standard Ogg Opus wrapping, playback/transcription compatibility, and Opus imports. Finalization persists compressed-file metadata before removing generated PCM spools; conversion or metadata failure retains the recoverable WAV files. Existing settings decode to Opus. Recording format is fixed at capture start.

**Reasoning:** Preserve a stable bounded capture path and make compression a visible finishing stage. Apple's voice processing already requests AEC/noise suppression/AGC; no acoustic improvement is claimed without speaker/microphone comparison. Native codec probing avoids imposing Homebrew, full Xcode, FFmpeg, or downloaded binaries on users.

**Technical debt:** PCM spooling consumes uncompressed disk space during recording, and abrupt termination before finalization leaves WAV files. Accepted to preserve recoverability and avoid codec latency in capture callbacks. Future remediation: bounded compressed segment writing and an explicit resumable finalization queue. Crash after compressed files are published but before metadata is saved can leave orphan compressed files; originals remain the canonical recoverable recording. Future remediation: journal conversion checkpoints and clean only verified unreferenced outputs. Echo rejection for external-app audio remains route-dependent and unverified; a user-assisted off/on acoustic test is pending.

**Notes:** All 39 tests across 13 suites passed, including native Opus/M4A conversion, independent variable-frame fixtures, malformed Ogg rejection, and failed metadata-commit recovery. CLT release/signing/installer passed; native Settings visually shows Opus (Recommended) as default. Native MP3 encoder probe failed, so MP3 output is not offered; MP3 import remains supported. The user agreed to a built-in speaker/microphone acoustic comparison; readiness is pending.
