---
date: 2026-09-24
title: Isolate microphone capture and retain recovery
status: completed
---

## Problem

Installed client PID 83233 continued using about 38% CPU after a recording stopped. A native stack sample showed AVAudioEngine input-node initialization stuck in Core Audio property calls during microphone reconnection. The existing ten-second Tokio timeout abandoned the blocking thread without canceling it. The user authorized stopping the client; SIGTERM did not finish, so the exact affected process was killed and verified absent.

## Implemented solution

- Move AVAudioEngine ownership into a private same-executable capture helper (`audio/mic_native.rs`). The parent has no microphone engine objects to stop or destroy.
- `audio/mic.rs` supervises framed PCM over a pipe. Startup and complete-frame stalls have five-second deadlines. Stop, malformed data, timeout, crash, and drop all terminate and reap the child. The child also exits on parent-pipe EOF, including while its native capture thread is stuck.
- Keep the helper inside the signed native client bundle; it is not a server/transcription worker or container. Hidden entry point bypasses desktop, server, and async-runtime startup.
- Session recovery retains process-isolated microphone sources indefinitely while recording. Failed attempts back off for 2, 4, 8, 16, then 30 seconds. Existing source-health warnings remain active; recovery emits a retry notice and successful reconnection notice. Other source implementations retain their defensive retry limit.
- Bound IPC frame allocation, preserve writer ownership across retries, and keep stop independent of native teardown.

## Reasoning

Native calls cannot be canceled by timing out a Rust future or abandoning a thread. Process isolation gives the parent a real termination boundary. Retaining a source through backoff allows a microphone that returns much later to resume without restarting the recording. Waiting for actual PCM rather than a helper-started message also detects engines that initialize but never deliver audio.

## Technical debt

- Existing writers concatenate received PCM and do not pad missing-device time using timestamps. This predates the fix and is retained to avoid changing recording timeline semantics in this lifecycle repair. Long outages can shorten the microphone track relative to system audio; future work should normalize channel changes and persist/pad capture gaps consistently across sources and formats.
- The outer defensive timeout for non-isolated AudioSource implementations still cannot cancel arbitrary blocking native calls. The observed AVAudioEngine path is now isolated; audit/isolate other capture backends if they show the same failure mode.

## Notes

- `cargo test --locked`: 54 passed, one existing external-auth fixture ignored. Covers synthetic helper hangs, stalls, repeated crashes, recovery, parent EOF, and session recovery beyond the old three-attempt cap with no browser subscriber; audio before and after recovery is finalized in the same WAV.
- Release app built and ad-hoc signature verified. Installed into `/Applications/Gday Meetings.app`, verified the executable hash against the build, and left closed. Previous bundle retained at `/private/tmp/gday-recovery-backup-20260924/Gday Meetings.app`.
- No live device unplug/Core Audio reconfiguration has been induced. Hardware/TCC behavior still needs a real recording smoke test.
