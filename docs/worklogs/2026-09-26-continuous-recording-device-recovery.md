---
title: Continue recording through audio device changes
date: 2026-09-26
status: implemented; hardware validation pending
scope: swift-app-audio
---

## Problem

The Swift client treats audio device and format changes as fatal recording errors. Connecting AirPods, disconnecting a headset, or changing the default input or output can stop and finalize the meeting. The expected behavior is to keep the same recording active, reconnect the affected audio sources to the latest default devices, and resume capture automatically.

The new route observer only updates the voice-processing default in New Recording. It does not recover an active recording. This task supersedes the existing stop-on-route-change policy in the Swift audio design.

## Implemented solution

Implemented in `apps/client-macos-swift/Sources/GdayMeetings/`. The plan below remains the reference; the Audio design section “Device changes during recording” describes the resulting behavior.

- `Core/CaptureSourceRecovery.swift` (new): per-source controller with running, reconnecting, failed, and stopped states; generation numbers; a 0.3-second debounce; backoff from 0.25 seconds to a 5-second cap; and stop with a deadline.
- `Core/AudioCapture.swift`: microphone and system audio each have a recovery controller. Engine setup is shared by first start and rebuilds. Default input and output listeners compare device IDs. A 1-second supervisor pads reconnecting tracks and runs the 3-second delivery watchdog. Stop waits at most 3 seconds (`AudioCapture.stopBound`) and ends both tracks at one host time. `onFailure` now fires only for writer errors or when every selected source fails permanently.
- `Core/SystemAudioCapture.swift`: tap format, aggregate, and ring failures report a recoverable interruption instead of a fatal error.
- `Core/TimedAudioWriter.swift`: fixed track format with channel mapping and `AVAudioConverter` resampling; no 30-second gap limit; `padSilence(throughHostSeconds:)`; trailing padding in `finish`; gap metadata. `RecordingProfile` adds `voiceProcessingPolicy` and `routeChanges`; older recordings still decode.
- `UI/RecordingWorkspaceView.swift`, `Core/UIPreview.swift`: “Reconnecting microphone…” or “Reconnecting system audio…” in the recording header, and a reset meter. UI Preview simulates a system-audio reconnect.
- Docs: `docs/AUDIO_DESIGN.md` and `docs/UI_PREVIEW.md`.

### Required behavior

- Keep the same meeting, recording identity, start time, elapsed timer, notes, and logical microphone/system tracks throughout recovery. Do not require another Start Recording action or create another meeting.
- Follow the latest macOS default input and output devices for the sources the user enabled. Switch only when macOS makes the new device the default input or output; do not switch to every newly connected device or enable a source the user disabled.
- Recover affected sources independently where possible. Continue writing a healthy source while another reconnects. If voice processing requires rebuilding coupled input/output resources, preserve the unaffected capture path wherever supported.
- Re-read the current devices on each attempt. Debounce bursts of route notifications and abandon stale attempts when a newer route replaces them.
- Retry temporary device loss with capped backoff for as long as the recording remains active, including when no window is visible. Recover after extended absence, not just the first few failures.
- In automatic mode, recompute microphone voice processing for the new output: recognized speakers on, headphones off, unknown routes off. Preserve an explicit on/off choice for the session. Carry the automatic-versus-manual policy into capture; the initial effective Boolean alone is insufficient.
- Reconfigure voice processing while the affected engine is stopped. Resume without monitoring the microphone or replaying captured system audio to the output.
- Keep **Stop & Save** responsive during recovery. Cancel pending retries, reject late callbacks, drain accepted audio, and finalize once. A route notification must not restart capture after stop or app shutdown.
- Show source-specific status such as “Reconnecting microphone…” or “Reconnecting system audio…”. Keep recording controls available, reset stale meters, and clear the status after audio delivery resumes. Do not claim audio was captured during an outage.
- Separate recoverable route failures from failures such as permission revocation, disk errors, or invalid capture data. Define explicit handling for each; do not retry every error indefinitely or discard already captured audio.

### Implementation plan

1. Introduce a serialized capture lifecycle with per-source states for running, reconnecting, stopping, and stopped. Use attempt generations to reject stale callbacks and coordinate route events, user stop, and startup. Move route changes out of the generic fatal-error path.
2. Observe default input/output, device availability, and relevant format/configuration changes. Rebuild the microphone engine and system tap/aggregate as required using fresh device identities and formats. Remove old listeners and callbacks before releasing their resources; distinguish capture's own aggregate changes from external route changes.
3. Keep writers and the recording timeline independent of replaceable device engines. Convert incoming formats into stable per-track PCM formats before appending. Document channel-mapping rules and verify sample-rate/channel changes, including Bluetooth call profiles. Never append newly formatted samples as if they still had the previous format.
4. Preserve one host-clock epoch across reconnects. Represent missing capture intervals as gaps padded with silence, without compressing time, duplicating samples, or drifting microphone and system tracks. Handle trailing gaps when the user stops before a source returns. Replace the writer's current 30-second gap failure with bounded, cancellable handling for prolonged outages; avoid allocating a buffer proportional to outage duration or blocking the UI while padding.
5. Make each recovery attempt time-bounded without accumulating blocked Core Audio calls. Evaluate a disposable native helper process, as used by Rust, for capture operations that can hang. A task or thread timeout alone does not cancel a stuck native call. Resolve process ownership, signing, permissions, audio IPC, and teardown before claiming bounded recovery. Keep the implementation native and compatible with Command Line Tools builds.
6. Persist effective device/format/voice-processing changes and outage intervals with timestamps. The current single recording profile cannot describe a route that changes mid-session. Define a backward-compatible metadata extension and ensure older recordings still load; keep this history separate from the user's session processing policy.
7. Update the audio design, user-facing errors, and recording status to describe recovery. Replace the Audio design hardware-validation row that expects a retained partial recording and recovery instructions after a route change. Run formatting, relevant tests, and isolated UI checks before hardware validation.

## Reasoning

The Rust client provides a useful recovery model, particularly for the microphone:

- `apps/client-macos-rust/src/audio/mic_native.rs` creates a fresh engine using the current default input after device loss.
- `apps/client-macos-rust/src/audio/mic.rs` isolates microphone capture in a disposable child, requires audio delivery within a deadline, and uses capped retry backoff. Terminating and reaping the child bounds native-call failures.
- `apps/client-macos-rust/src/audio/recorder.rs` restarts lost sources while retaining their writer channel.
- `apps/client-macos-rust/src/session/mod.rs` runs recovery without a connected browser and retries isolated sources beyond the finite budget used for non-isolated sources. Its regression test covers recovery after more than three failures.

Use these lifecycle and isolation principles as references. They do not establish that Rust already implements every system-audio, voice-processing, or timeline requirement above. Swift must preserve its own separate-track and host-clock guarantees.

Primary Swift integration points are `Core/AudioCapture.swift`, `Core/SystemAudioCapture.swift`, `Core/TimedAudioWriter.swift`, `Core/MeetingStore.swift`, `Core/RecordingAudioRoute.swift`, and `UI/RecordingWorkspaceView.swift`, under `apps/client-macos-swift/Sources/GdayMeetings/`. Currently, engine/tap route events call `onFailure`, the store stops recording, and the writer rejects changed formats and gaps over 30 seconds. Fixing only the notification handler would leave these other failure paths intact.

## Validation and acceptance

- Automated fault-injection tests: microphone-only loss, system-only loss, simultaneous changes, repeated reconnect failures, recovery after more than 30 seconds, rapidly superseded routes, and stop during every recovery stage. Verify bounded resources, no duplicate finalization, and no capture restart after stop.
- Synthetic audio tests: different sample rates/channel counts before and after recovery; one meeting and stable logical tracks; aligned timestamps, explicit gaps, preserved pre/post-recovery samples, trailing-gap duration, and valid final Opus/M4A/WAV output.
- Processing-policy tests: speakers → headphones → speakers updates automatic processing; explicit on/off overrides survive every route change. Unknown routes remain conservative.
- Failure tests: hung native start/stop, missing callbacks despite a live engine, revoked permission, disk failure, and app shutdown. Confirm other sources and Stop & Save remain responsive and saved audio stays readable.
- UI Preview: source-specific reconnect status, meters, elapsed timer, notes, and stop controls across appearances and window sizes. Preview uses simulated recovery; it cannot establish hardware behavior.
- Hardware: built-in speakers/mic → AirPods → built-in devices during one recording; separate default-input changes; Bluetooth call-profile changes; USB headset removal/reconnection; prolonged device absence; and another calling app changing devices. Verify actual microphone/system capture, echo reduction, ducking, track alignment, and absence of duplicate playback.
- Acceptance: a normal route change never finalizes the meeting by itself. Capture resumes on the latest devices without user intervention, the outage is represented honestly, and user stop always completes within a defined, tested bound.

## Technical debt

- **In-process native calls.** A Core Audio call that hangs during recovery or stop is abandoned after 3 seconds, not cancelled; its thread stays blocked until the call returns. Initial `start()` is still unbounded. Accepted because a helper process needs its own signing, permission, audio IPC, and teardown design. Consequence: a hung driver can leak a thread and its device session until the call returns. Remediation: move capture into a disposable helper process, as the Rust client does for the microphone.
- **Paced silence writes.** `ExtAudioFileWriteAsync` overflows (-66570) and corrupts the file when a large silence burst is queued at once. The writer enlarges its buffer to 512 KiB and sleeps 1 ms per 4096-frame chunk beyond 1 second of padding, while holding the writer lock. Normal outages are padded once per second, so this only applies if the supervisor falls behind. Accepted as a timing-based safeguard; it has not been stress-tested under heavy disk load. Remediation: write padding from a dedicated writer thread with backpressure.
- **Same-device output changes (resolved).** Previously only default device ID changes were observed, so headphones on a built-in jack did not re-decide automatic voice processing. Resolved by the follow-up below: capture observes the default output's data source, stream list, and stream terminal types.
- **Voice-processing fallback metadata.** If automatic mode falls back to unprocessed capture after the microphone writer was created, the track keeps the mono processed format and the writer converts to it.

## Notes

Automated validation: `make format-macos`, `make lint-macos`, `make test-macos` (130 tests in 31 suites passed; baseline was 113), and `make build-macos-preview` passed. The only warnings are the existing Command Line Tools linker search-path warnings; there are no compiler or deprecation warnings. New tests cover recovery after more than 30 seconds of simulated failures, the backoff cap, superseded attempts, stop during debounce, backoff, and a stuck attempt, per-source permanent failure, the automatic voice-processing policy, format conversion, long and trailing gaps, and decoding older profiles.

Not validated: UI Preview screenshots were skipped because the screen was in use during validation, and nothing was tested on hardware. The hardware rows in the Audio design validation table remain open. In particular, confirm that the system-audio tap keeps delivering during silence. If it stops, the 3-second watchdog would rebuild system audio repeatedly. An earlier silent system-only recording decoded to its full length, which suggests it keeps delivering.

After review, a default output change no longer rebuilds an unprocessed microphone unless automatic mode now selects voice processing. This avoids a microphone gap when switching between headphones.

## Follow-up: device names in status and same-device output changes

**Problem.** The header said “Reconnecting microphone…” even when capture already knew the new device. Same-device output changes, such as headphones on a built-in jack, did not re-decide automatic voice processing (the technical debt above).

**Implemented solution.**

- `Core/AudioCapture.swift`: `SourceDelivery` records each installed session's device, the device of the last session that delivered audio, and the reconnect target. The supervisor refreshes the target each second while a source is reconnecting. The microphone uses the selected device when it is connected and has not failed, otherwise the default input. System audio uses the default output. `SourceDelivery.switchingTo(state:)` returns a name only when the target differs from the device that last delivered audio. A rebuild on the same device, or a source that has not delivered yet, still shows “Reconnecting…”. `RecordingSourceLevel.switchingTo` carries the name to the UI; the meter's help text and accessibility value read “Switching to *Name*…”.
- `UI/RecordingWorkspaceView.swift`: `reconnectingStatus` shows “Switching microphone to *Name*…”, “Switching system audio to *Name*…”, or “Switching audio devices…” when both sources switch. It keeps “Reconnecting microphone…”, “Reconnecting system audio…”, and “Reconnecting microphone and system audio…” when no different device is known. `Core/UIPreview.swift` simulates two seconds of each state.
- `Core/RecordingAudioRoute.swift`: `OutputSpeakerRoute` holds the speaker classification and decides, through the injectable `PropertyReader`, whether a notification changes it and whether the microphone must rebuild. `rebuildsMicrophone` is shared with the default-output path.
- `Core/AudioCapture.swift`: while the microphone records, listeners on the default output's data source (output scope), output stream list, and each stream's terminal type call `outputRouteChanged`. They are replaced when the default output or its stream list changes and removed at stop. A lock guards the listener lists, so a late notification cannot register listeners after Stop & Save removed them. Unchanged classifications are logged at debug level and ignored; changes are logged at notice level with the policy and decision.

**Reasoning.**

- Both sources switching usually comes from one event, such as connecting AirPods. Two device names would not fit the one-line header, so it shows the short “Switching audio devices…” and the meters keep the names. When only one of two reconnecting sources has a known device, “Reconnecting microphone and system audio…” is accurate for both.
- The name comes from the same device-name reads as the fallback notices, so “Switching microphone to *Name*…” and “… · Using *Name*” name the same device.
- The same-device path applies only to the automatic policy. An explicit On or Off, including the echo latch, holds for the session. Rebuilding a processed engine for an explicit On is left to AVAudioEngine's configuration-change notification, as before.
- Classification uses the existing terminal-type rules instead of data source codes, whose values are driver-specific.

**Technical debt.** None added. The same-device output debt above is resolved.

**Notes.** `make format-macos` and `make lint-macos` passed. In a copy of `HEAD` with only this change applied, `make test-macos` passed 169 tests in 35 suites (baseline 164; new tests cover classification changes, explicit-policy precedence, the shared rebuild rule, header wording, and switch-target selection). In the shared working tree, which also contains other tasks' changes, it passed 182 tests in 37 suites. `make build-macos-preview` passed in both. The only warnings were the existing Command Line Tools linker search-path warnings. Not validated on hardware: the listeners' firing on a built-in jack, whether AirPods expose a named default input before the tap rebuilds, and the header wording on screen. UI Preview was not opened.
