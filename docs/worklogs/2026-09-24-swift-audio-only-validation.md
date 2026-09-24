---
date: 2026-09-24
area: client-macos-swift
status: automated-validation-passed-hardware-pending
owner: native-services
---

## Problem

System-audio recording used a screen-content picker, asking users to share a display for an audio-only task. Replacing it with Core Audio process taps introduces a real-time callback whose channel, timestamp, memory ownership, and bounded-queue behavior require independent verification.

## Implemented solution

- Added synthetic `AudioCaptureBridgeTests` for planar/interleaved stereo, mono, ring wrap, exact host timestamps, bounded overflow with FIFO draining, incompatible channel layout, oversized input, invalid timestamps, disabled HAL buffers, copied sample ownership, and undersized destination recovery.
- Updated the native README and audio design document for private Core Audio taps, audio-only permission, separate microphone/system files, real-time handoff, and retained format/codec behavior.
- No hardware capture, permission interaction, or real recording files were used by this work.

## Reasoning

Apple's Core Audio tap sample explicitly supports macOS 14.2 and `NSAudioCaptureUsageDescription`. The installed SDK confirms that IOProc input timestamps describe the first acquired frame, whereas callback arrival includes scheduling latency. Its IOProc block queue dispatch is synchronous; moving heavy work into a block on a queue does not remove the real-time obligation. Tests therefore exercise the C queue independently from HAL and keep file processing outside the callback.

## Technical debt

- Hardware and TCC behavior cannot be established by synthetic samples. Accepted to keep automated checks silent and permission-free; consequence is that route changes, first consent, denial/retry, Bluetooth, long sessions, and microphone/system acoustic separation still require explicit hardware validation. Remediation: run the documented audio-design matrix with user-authorized input and record actual results before claiming route coverage.
- The queue has no reset or stop primitive: each recording owns a fresh ring, and its owner must stop/destroy IOProc before freeing it. Queue tests verify draining after failure but cannot prove HAL lifecycle ordering. Remediation: retain lifecycle review and add injectable HAL lifecycle tests if this backend grows more complex.

## Validation

- Source reviewed against `AudioCaptureBridge.h` and its C implementation.
- `make test-macos`: 52 tests in 16 suites passed, including the bridge tests; repeated successfully after the final cancellation-boundary and sample/metadata page initialization changes.
- C11 `clang -std=c11 -Wall -Wextra -Werror -fsyntax-only`: passed.
- Release build, bundle plist validation, ad-hoc signature verification, and installer staging passed. The installed CLT still emits missing optional linker-search-path warnings; they did not prevent building or linking.
- Hardware permission, source receipt, and route behavior remain unverified for the new backend. Documentation retains codec and voice-processing limitations rather than claiming all macOS releases and device routes are validated.
