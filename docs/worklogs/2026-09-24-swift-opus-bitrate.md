---
date: 2026-09-24
title: Speech-focused native Opus defaults
status: complete-live-validation-handed-off
---

## Problem

Opus used 48 kbps mono / 96 kbps stereo and left bitrate strategy implicit. The user requested 32 kbps mono / 64 kbps stereo, VBR where supported, and native encoder complexity.

## Implemented solution

- Updated `RecordingEncoder.encodeOpus` to the requested total track bitrates and explicitly request VBR when the encoder exposes bitrate-strategy control.
- Retained the 48 kHz encoding timeline, native complexity, original capture sample rates, separate source tracks, and transactional WAV retention. Existing recordings are not re-encoded.
- Documented the resulting policy in `AUDIO_DESIGN.md`.

## Reasoning

The Opus specification's full-band speech guidance includes 32 kbps. Stereo gets a higher total budget without changing channel layout. Apple's native encoder keeps Command Line Tools installation dependency-free; its public `AVAudioConverter` interface does not expose libopus's complexity 0–10 control. A silent converter-only probe on this Mac read back VBR and exactly 32000/64000 after setting them. Native bitrate strategy and actual output size must not be confused: VBR may depart from the nominal average.

## Technical debt

- Retained platform-dependent native codec behavior: older supported macOS releases have not been tested for this configuration. Accepted to keep the native, dependency-free codec path; an encoder without exposed strategy control retains its default, and encoding failure retains WAV. Consequence: VBR is verified on the current Mac only. Remediation: run codec round trips and read back strategy/bitrate on the macOS compatibility matrix before claiming uniform VBR support.
- No new schema or runtime dependency debt.

## Validation

- Native converter-only probe: mono and stereo both accept and report VBR with the requested bitrates. The sandbox could not load the native encoder; the same probe succeeded outside the tool sandbox. No recording or playback occurred.
- `make test-macos`: all 52 tests in 16 suites passed, including native mono/stereo Opus round trips, 44.1 → 48 kHz conversion, very short input, channel/duration/signal preservation, and transactional recording finalization.
- `make install-macos`: passed; release build (13.50 s), plist validation, ad-hoc signature verification, and installer staging completed. Finder opened the installer folder. Existing CLT optional linker-search-path warnings remain. No installed Applications bundle or user recording was replaced.
- Desktop review was retried after the user added Accessibility permissions, including a fresh automation session. Both attempts still returned `Sky Computer Use native pipe startup failed`; no live UI validation is claimed. User requested the remaining manual validations be handed to a separate chat.
