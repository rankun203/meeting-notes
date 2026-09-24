---
date: 2026-09-24
title: Audio-only Core Audio system capture
status: implemented-awaiting-runtime-validation
---

## Problem

The ScreenCaptureKit display picker showed “Share Entire Screen” for an audio-only meeting recording. Although no video output was registered, this asked people to authorize a resource broader than their intended task.

## Implemented solution

- Replaced the Swift system-audio path with a private Core Audio global stereo process tap excluding the app's own HAL process object. A private tap-only aggregate stays unmuted and leaves default devices untouched. Removed the display picker, ScreenCaptureKit authorization types, and screen-purpose plist entry; retained `NSAudioCaptureUsageDescription`.
- Added the dependency-free C11 `AudioCaptureBridge` SwiftPM target: lock-free single-producer/single-consumer ring, fixed 64 slots of at most 8192 stereo Float32 frames (4 MiB), preallocated and prefaulted outside the hardware callback. The C IOProc copies samples and acquisition host timestamps only. A serial Swift consumer drains at approximately 10 ms into the existing timeline-aware writer and true meters.
- Validated actual tap ASBD, supported planar/interleaved stereo Float32, rejected invalid timestamps/layouts/oversized buffers, and surfaced ring overflow rather than silently losing audio. Missing source callbacks remain distinct from silence and permission denial.
- Preserved the microphone/mono VoiceProcessingIO graph and separate WAV-spool → Opus/AAC transaction. Start the system backend before microphone capture; discard system samples until the shared host epoch/writer is established. Stop and unregister the IOProc before draining and freeing its storage; observe tap format, aggregate liveness, and subsequent output-route changes.
- Renamed the pending-start cancellation hook to remove the obsolete picker terminology and rechecked cancellation after microphone consent, including microphone-only recordings. Warmed both sample and slot-metadata pages before starting IO; this avoids initial page faults without claiming the memory is pinned.

## Reasoning

Apple's [tap sample](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps) supports macOS 14.2 and documents first-use system-audio consent when recording starts from a tap aggregate. [NSAudioCaptureUsageDescription](https://developer.apple.com/documentation/bundleresources/information-property-list/nsaudiocaptureusagedescription) is the audio-specific purpose key. This matches the [HIG privacy principle](https://developer.apple.com/design/human-interface-guidelines/privacy) of requesting access in context and avoids an unrelated screen-sharing interaction. There is no screen-capture fallback.

The SDK's `AudioHardware.h` documents that IOProc block queues are synchronous, `inInputTime` describes acquisition time, PID translation can return an unknown object without error, and `tapautostart=true` waits for an application to play audio. Therefore the implementation uses a C callback, validates process identity, timestamps with `inInputTime.mHostTime`, and explicitly sets auto-start false. [Apple's realtime guidance](https://developer.apple.com/documentation/audiotoolbox/analyzing-audio-performance-with-instruments) rules out locks, allocations, and filesystem work in this callback. Existing `TimedAudioWriter` belongs on the consumer queue.

## Technical debt

- Public tap APIs do not expose a standalone authorization query/request or guarantee exact permission-prompt wording/completion timing. Accepted to avoid private TCC APIs; startup errors and missing callbacks are reported honestly. Validate first grant, denial, retry and Quit during the OS prompt on supported macOS versions; keep behavior gated on observed public API outcomes rather than guessing denial from silence.
- Supported capture format is stereo Float32 with callback blocks up to 8192 frames. This bounds realtime memory/work; unusual device formats fail visibly and preserve available audio. Extend only with documented format mappings and synthetic tests if device evidence requires it.
- If HAL refuses IOProc destruction, its raw context allocation is deliberately retained until process exit instead of risking use-after-free. The cleanup error is surfaced. Add fault-injected lifecycle tests and investigate any observed OSStatus before considering bounded recovery; normal teardown frees the allocation.
- Core Audio lifecycle and ring ownership add maintenance complexity. Synthetic tests cover buffer correctness; real device/route/permission behavior still requires the authorized manual matrix, including Bluetooth/USB, restart and stalls.

## Validation

- C11 `clang -fsyntax-only -Wall -Wextra`: passed.
- Swift debug build using Command Line Tools: passed.
- Initial direct Swift test invocation hit the current CLT's missing Testing macro discovery; the repository test script explicitly loads that installed plugin. `make test-macos` passed all 52 tests in 16 suites, including the new ring layout/wrap/ownership/overflow/timestamp tests, and passed again after final review changes. The stop order unregisters system route listeners before VoiceProcessingIO teardown; release compilation passed with this adjustment.
- `make install-macos` passed: release build, plist validation, ad-hoc signing and installer signature verification; Finder opened the refreshed installer folder. Existing CLT optional linker-search-path warnings remain. The Applications copy was not replaced or launched.
- A fresh unsandboxed C process successfully resolved its own HAL process ID on its first Core Audio call, without creating an engine/device/tap or playing/recording audio. The tool sandbox returned an unknown ID for the same probe; no workaround was added to the unsandboxed app. The SDK permits an unknown result, so the explicit guard remains. This checks local startup identity only, not capture or other macOS versions.
- No hardware devices started, permissions requested, user recordings read, or app UI operated by this implementation task. Live prompt, source receipt and route validation remain pending coordinated user-authorized testing.
