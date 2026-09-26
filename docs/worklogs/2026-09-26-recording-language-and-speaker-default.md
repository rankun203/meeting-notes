---
title: Compact meeting language and correct speaker defaults
date: 2026-09-26
status: completed
scope: swift-app-recording
---

## Problem

The language picker and its persistent description made the meeting header unnecessarily tall. New Recording left microphone voice processing off on this Mac's built-in speakers, and its descriptions needed clearer wording.

## Implemented solution

- `MeetingDetailView` places a compact language picker beside the date and duration, with a vertical fallback when space is limited. People & Tags has its own row.
- `MeetingLanguagePicker` moves guidance and retry controls into an accessible information popover. A failed language request changes the information icon to an exclamation mark. New Recording and Settings use the same information control.
- Shortened New Recording's audio-source and voice-processing descriptions.
- `RecordingAudioRoute` recognizes numeric IOAudioFamily speaker terminal codes as well as Core Audio's four-character codes. Added regression cases for numeric speakers, headphones, head-mounted displays, headsets, and unknown endpoints.
- A sheet-scoped Core Audio observer refreshes the suggested processing state on default-output, data-source, stream-list, and terminal changes. A manual override wins. Capture retains its independent route check after permission prompts.

## Reasoning

A read-only probe of this Mac returned built-in transport, internal-speaker data source, and stream terminal `769` (`0x0301`). The previous comparison against `kAudioStreamTerminalTypeSpeaker` could never match it. Checked Apple's current Core Audio documentation, installed SDK headers, and [IOAudioFamily terminal definitions](https://github.com/apple-oss-distributions/IOAudioFamily/blob/main/IOAudioTypes.h). Native macOS cannot use AVAudioSession's iOS route API; the installed SDK explicitly marks it unavailable on macOS. Recognizing explicit endpoint metadata and observing changes is more reliable than guessing from device names, transport, or jack presence.

## Technical debt

- Retained driver-format compatibility: numeric IOAudioFamily codes are required by the observed built-in driver. Keep the documented mapping and regression cases until supported drivers consistently return Core Audio constants.
- Retained conservative handling of unknown, virtual, and independently routed outputs. They default to processing off because system-default metadata does not establish another app's acoustic path. The session toggle is the fallback. Extend classification only with documented endpoint evidence and hardware validation.
- Some drivers omit properties or reject property listeners. The initial and pre-capture checks remain the fallback; test such hardware before promising live updates for it.

## Validation

- `make format-macos`, `make lint-macos`, and `git diff --check` passed.
- All 113 tests in 30 suites passed after the observer change.
- Built and launched isolated UI Preview. Verified inline date/duration/language, the information popover, the revised New Recording copy, processing enabled on this Mac's speakers, and the manual off toggle. No recording or external service job was started.
- System appearance (currently light) was visually checked. Automatic approval review blocked selecting Dark because it required explicit permission for an appearance-setting change; requested permission. Explicit Light/Dark checks, narrow-window layout, physical headphone switching, and acoustic echo-reduction quality remain unverified.
- Builds and tests retain Command Line Tools linker warnings for missing `Developer/usr/lib` and `Developer/Library/Frameworks` paths. Linking and signing passed; no deprecation warning was emitted. Follow up by repairing the selected toolchain installation or its linker search paths.
