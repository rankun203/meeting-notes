---
title: Microphone selection, live Voice Processing, and echo detection
date: 2026-09-26
status: implemented; hardware validation pending
scope: swift-app-audio
---

## Problem

The Swift client always recorded the macOS default input. Voice processing could be chosen only before recording, per recording, in New Recording. Nothing noticed when speaker playback leaked into an unprocessed microphone track.

## Implemented solution

Code is under `apps/client-macos-swift/Sources/GdayMeetings/`. The Audio design sections “Microphone selection”, “Live Voice Processing switch”, and “Echo detection” describe the behavior.

- **Microphone menu.** `UI/RecordingWorkspaceView.swift` adds a menu to New Recording's Microphone row: “System Default (*name*)”, each input device, and “*Name* (Unavailable)” for a saved device that is not connected. `Core/RecordingAudioRoute.swift` lists input devices (no aggregates, hidden devices, or output-only devices) and `MicrophoneDeviceObserver` refreshes the list and default name from Core Audio listeners. The choice is saved as `AppSettings.microphoneDevice` (UID and name) when selected.
- **Pinned capture.** `Core/AudioCapture.swift` resolves the selected UID on every microphone build. A connected device is bound on input element 1 before the format is read and again after voice processing is enabled, then read back after start. A missing device records from the default input; a device-list listener rebuilds when it returns, and default-input changes are ignored while it is in use. The live view shows “*Name* disconnected · Using *Default*”.
- **Voice processing setting.** The New Recording toggle, its caption, and the sheet's output-route observer are removed. `AppSettings.automaticVoiceProcessing` (default on, new key) selects `automatic` or `off` at start. Settings → Recording has **Turn On Voice Processing Automatically**.
- **Live switch.** `RecordingVoiceProcessingControl` under the Microphone meter shows the running engine's state and calls `MeetingStore.setRecordingVoiceProcessing`, which sets an explicit policy and rebuilds only the microphone. It is disabled while a rebuild is pending or the microphone reconnects.
- **Fallback.** If processing cannot be enabled, capture records unprocessed under every policy and shows “Voice Processing is unavailable for *Device*”. Previously an explicit On retried indefinitely.
- **Echo detection.** New `Core/EchoDetector.swift` correlates 20 ms level envelopes over 6 seconds at lags 0–400 ms, once per second on the capture queue. `AudioCapture.measure` feeds it the existing per-buffer level. Automatic policy latches processing on and shows “Echo detected · Voice Processing turned on” for 10 seconds; an Off policy shows an “Echo detected” hint.
- **Metadata.** `RecordingRouteChange.reason` (optional text) records `route`, `voiceProcessingSwitched`, `echoDetected`, `voiceProcessingUnavailable`, `selectedMicrophoneUnavailable`, or `selectedMicrophoneReturned`. The route's `device` is now the bound microphone, not always the default input.
- **UI Preview** shows the switch with the echo hint (Off) and the automatic-change notice (On).
- **Docs:** `docs/AUDIO_DESIGN.md`, `docs/UI_PREVIEW.md`, and the README audio section. The README's stale “device changes finalize the recording” sentence was corrected.

## Reasoning

- **Device binding API.** `AUAudioUnit.setDeviceID` is current and not deprecated, but short probes on this Mac showed it is wrong for VoiceProcessingIO. Called after enabling processing, it moved the output element to the selected device, and the built-in microphone also ran. Called before, enabling processing reset the input to the default. `kAudioOutputUnitProperty_CurrentDevice` on input element 1 after enabling processing kept the default speakers as output and ran only the selected input. The production sequence was probed end to end: bind, enable, bind again, mono client format, start, and read back. It delivered buffers after about 1.2 seconds. The probes used a virtual input device (Microsoft Teams Audio) as the non-default microphone. iTerm already had microphone access, so no permission prompt appeared. Each probe ran for at most 1.5 seconds with minimum ducking. One element-1 path is used for both modes, rather than two APIs.
- **Read-back check.** Capture compares the bound device after start and fails the attempt on a mismatch, so it never silently records a different microphone than the one shown.
- **Fallback over retry.** A live switch that can leave the microphone silent until the route changes is worse than an unprocessed track with a visible notice.
- **Switch disabled during rebuild,** not queued: simplest correct behavior, and a rebuild lasts under 2 seconds.
- **Echo latch as policy `.on`.** It reuses the explicit-policy path: route changes keep processing on, and the reason in metadata records why.
- **Envelope correlation, not AEC-style filtering.** It reuses levels already measured, runs about 6,300 multiply-adds per second, and needs no FFT. Synthetic tuning: echo correlates at 0.98–0.99; independent speech-like envelopes peaked at 0.61 in one isolated evaluation, so the threshold was set to 0.6 with 3 consecutive matches. Lead review raised it to 0.75: a false report latches processing (and possible ducking) for the session, while a miss only leaves the speaker-route default; echo is still reported once near-end speech pauses.
- **Aggregates excluded** entirely, as requested. This also hides user-created aggregate devices; reading each aggregate's private flag was not worth the added code yet.

## Technical debt

- **Unverified non-default input with processing.** Routing was confirmed with a virtual input only. Echo cancellation quality with a non-default input, and USB, Bluetooth, or Continuity microphones, are unmeasured. Consequence: a selected device might route correctly but cancel poorly. Remediation: run the new hardware rows in the Audio design validation table.
- **Synthetic echo thresholds.** Threshold, activity limits, and lag range were tuned on generated envelopes. Consequence: real rooms could cause missed detection or, less likely, a false automatic switch (it latches on). Remediation: log correlation values from real speaker and headphone recordings, then retune.
- **User-created aggregates hidden** from the microphone menu. Remediation: read `kAudioAggregateDevicePropertyComposition` and exclude only private aggregates.
- **Switch shows “Reconnecting microphone…”** during its own rebuild, which is accurate but not specific. Remediation: a distinct status for deliberate rebuilds, if it confuses people.
- **Retained:** the fallback writer-format and in-process native call items from the device-recovery worklog still apply.

## Notes

Validation: `make format-macos`, `make lint-macos`, `make test-macos` (143 tests in 32 suites passed; baseline was 130), and `make build-macos-preview` passed with no compiler or deprecation warnings. The only build warnings are the existing Command Line Tools linker search-path warnings in debug builds. New tests cover the echo detector (delayed copy, delay estimate at 40, 200, and 360 ms, 60 ms buffers, independent signals, silent/steady/missing system audio, double-talk, and clearing after misses). They also cover settings decoding (new keys, legacy key ignored, malformed device), device filtering, the microphone menu, and route-change reasons, including unknown future values.

Not validated: the UI was not launched or screenshotted because the screen was in use. The AudioCapture integration (switch, echo latch, pinned device switching) has no automated test because it needs real engines. Nothing was tested on USB or Bluetooth hardware.
