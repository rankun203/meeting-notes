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

Layout fix, New Recording source rows: the Microphone explanation wrapped onto two lines in a wide sheet. Each row put its text column beside a `Spacer` in an `HStack`. The stack offered the less flexible text column only part of the free width and gave the rest to the spacer. The microphone menu's `fixedSize()` still reported its full title width, so the column looked wide while the explanation wrapped at about half the row. System Audio's shorter explanation happened to fit. Long device names also pushed the switch outside the card. The rows are now `RecordingSourceRow`: the text column fills the width left of the switch (`frame(maxWidth: .infinity)`, no spacer), and the menu is capped at 280 pt instead of sized to its title. The row stays stable when the selected name changes; long names truncate in the button and appear in full in the menu. `RecordingSourceRowLayoutTests` hosts the row in an `NSHostingView` at the sheet's 430 pt row width. It checks that the explanation keeps one-line height for short, typical, and long device names, and that it wraps rather than truncates at 200 pt. Before the fix, the typical and long cases failed. Offscreen PNG renders before and after the fix confirmed the result; the app was not launched. After the fix, `make format-macos`, `make lint-macos`, `make test-macos` (145 tests in 33 suites), and `make build-macos-preview` passed with only the existing linker search-path warnings.

## Fix: selected microphone rebuild loop (AirPods as default devices)

### Problem

With AirPods as the default input and output, a recording with **MacBook Pro Microphone** selected showed “Recording · Reconnecting microphone…” continuously and saved no microphone audio. Stop & Save reported “A selected audio source delivered no samples…”. The app had no logs to explain it.

Evidence from the saved meeting (read only): the microphone track's single gap covered all 17.66 seconds, and `routeChanges` held 34 microphone entries with reason `route`, about 0.51 seconds apart (the 0.3-second debounce plus the rebuild). System audio recorded normally: its only gap was 0.23 seconds at the end, so the tap delivered buffers throughout, including silence. The live meter showed nothing because nothing was playing. The unified log for the app process (`log show … processID == 95123`) repeated one cycle per rebuild: the engine's input unit opened the default-device aggregate, the bind moved it to the built-in microphone, AVAudioEngine logged `Format mismatch: input hw 1 ch, 48000 Hz, client format 1 ch, 24000 Hz` and `Failed to create tap, config change pending!`, the engine started, then logged `iounit configuration changed > posting notification`.

### Root cause

Selecting an input device with `kAudioOutputUnitProperty_CurrentDevice` leaves AVAudioEngine a pending configuration change. The engine starts on a default-device aggregate, so any bind changes its device. Capture then read `outputFormat(forBus: 0)`, which still described the previous default input (AirPods at 24 kHz), and installed the tap at that rate. The tap was rejected, so the engine delivered nothing. The pending change then posted `AVAudioEngineConfigurationChange`; capture treated it as a route change and rebuilt, and each rebuild repeated the bind and the notification. Because no session ever delivered, the 3-second watchdog, which only watched sources that had delivered, never ran. Every route request reset the backoff, so the loop ran at full speed. Short probes on this Mac (a virtual input as the selected device; defaults untouched) showed the loop does not need AirPods. Bound from a background queue, as capture does, the engine stopped itself on the pending change in 5 of 5 runs. On the main thread it kept running and posted the change about 0.1 seconds after start. Earlier device-selection probes ran on the main thread and did not rebuild on the notification, so they missed this.

### Implemented solution

In `Core/AudioCapture.swift`:

- **Settle the bind.** `PendingConfigurationChange` observes before binding. After a bind that changes the device, capture calls `prepare()` and waits up to 0.5 seconds for the notification before it reads formats. With voice processing, it stops the prepared engine, enables processing, binds again, and waits after the graph is connected but before `start()`. Preparing before `connect` makes AVAudioEngine raise an assertion.
- **Hardware rate.** Tap formats take their sample rate from `inputFormat(forBus: 0)` (the hardware side), not from the node's possibly stale output format. A mismatch is logged.
- **Ignore own notifications.** A configuration change that leaves the engine running on the same device with the same hardware format is logged and ignored (`MicrophoneConfigurationChange`).
- **Fall back once.** `SelectedMicrophoneFallback` moves capture to the default input after the selected device fails to bind, start, read back, or deliver audio within 3 seconds. The live view shows “*Name* unavailable · Using *Default*”. The selected device is tried again only after it disconnects and reconnects. Device-list notifications now rebuild only when the selected device's connection changes, so aggregate churn from voice processing cannot trigger rebuilds.
- **Startup watchdog.** A microphone session that never delivers is rebuilt after 3 seconds. System audio keeps the delivered-then-stopped rule.

In `Core/CaptureSourceRecovery.swift`, a **loop guard** handles a second rebuild in a row whose session never delivered. From then on, requests use the doubling backoff (0.3, 0.5, 1, 2, 4, 5 seconds) instead of the debounce, and route changes stop resetting it. `sessionDelivered()`, reported once per session from `AudioCapture.measure`, clears it.

**Logging.** New `Core/CaptureLog.swift`: `os.Logger` with subsystem = bundle identifier, categories `capture` and `recovery`. It records source states, rebuild triggers and causes, device IDs and names bound and read back, settle timing, formats, voice-processing decisions, watchdog firings, fallbacks, errors, and frames at stop. Nothing is logged per buffer. **Help → Export Recording Logs** uses `OSLogStore(scope: .currentProcessIdentifier)` to save the last hour of this app run's entries, plus `com.apple.avfaudio` entries, to `~/Library/Logs/Gday Meetings/`, and reveals the file in Finder. `log show` and `log stream` commands are in Audio design → Recording diagnostics and the README.

**Stop alert.** An empty source now throws `CaptureSourceError.noAudio`, which names the source and says which other track was saved, for example “Microphone recorded no audio. The System Audio track was saved. Check the microphone selected in New Recording and microphone access in System Settings → Privacy & Security.” `MeetingStore` shows it without the “Couldn’t finish the recording” prefix, so the alert title is the first sentence. The claim that system silence can produce no samples was removed, because this recording showed the tap delivering through silence.

### Reasoning

- Settling was chosen over only ignoring the notification. From a background queue the engine had already stopped itself, so ignoring the notification would have left a stopped engine. The notification filter remains for changes that do not affect the track.
- The loop guard is in `CaptureSourceRecovery` rather than `AudioCapture`, so every trigger (notification, watchdog, device list) is bounded the same way and is tested with the virtual scheduler.
- The fallback latches until reconnection instead of retrying the selected device on each rebuild. A device that failed once is likely to fail again, and a working default microphone with a visible notice is better than a silent track.
- A Help menu export was simple and reliable because the process-scoped `OSLogStore` needs no entitlement. Its limit is the current app run, which the docs state.

### Technical debt

- **0.5-second settle wait on the attempt thread.** It is accepted because the notification arrived in 64–107 ms in every probe, and it only runs when a bind changes the device. Consequence: a start or rebuild with a selected device takes up to 0.5 seconds longer if the notification never posts. Remediation: measure on USB and Bluetooth devices, and shorten the timeout if it never runs out.
- **Watchdog source check by name.** `superviseSources` distinguishes the microphone by its label string. Remediation: use a source enum if a third source is added.
- **Retained:** the items above (unverified non-default input with processing, synthetic echo thresholds, hidden user aggregates) still apply.

### Notes

Validation: `make format-macos`, `make lint-macos`, `make test-macos` (152 tests in 34 suites), and `make build-macos-preview` passed with no compiler or deprecation warnings. New tests cover the loop guard (backoff 0.3, 0.3, 0.5, 1, 2, 4, 5, 5, 5 seconds and reset on delivery), prompt debounce for delivering sessions, ignored delivery reports from replaced sessions, fallback and reconnection, configuration-change classification, stop-alert wording and titles, and log export through `OSLogStore`.

A temporary, environment-gated test (deleted afterwards) ran the real `AudioCapture` with the microphone only, with iTerm's existing microphone access. Each case ran about 3 seconds. Microsoft Teams Audio selected, unprocessed and with voice processing, recorded 3 seconds each with exactly one microphone route and no rebuilds. A live Voice Processing switch on the selected device rebuilt once and resumed. A missing selected device used the built-in microphone with “Gone Mic unavailable · Using MacBook Pro Microphone”. Starting a default-input recording within milliseconds of a voice-processed engine's teardown once failed with AVAudioEngine's `Error setting device on iounit ('!dev')`. That check is unchanged from before this fix. System audio was not exercised in the probe, to avoid a permission prompt.

Evidence extracts are in the task scratchpad (`app-log-raw.txt`, `app-log-first-cycles.txt`, `meeting-metadata.json`, `fixed-probe-log.txt`); they are not committed.

Remaining hardware checks: AirPods connected as default input and output, **MacBook Pro Microphone** selected, both sources on, automatic voice processing. The meter should show audio within about a second, with no repeated “Reconnecting microphone…”. The recording profile should have one microphone route, and the log should show “configuration settled”. Repeat with built-in speakers as output (voice processing on), then disconnect and reconnect AirPods during a recording.
