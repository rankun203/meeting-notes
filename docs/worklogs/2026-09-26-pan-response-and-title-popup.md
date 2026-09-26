---
title: Trackpad response and recording title popup
date: 2026-09-26
status: completed-with-platform-limitation
scope: macos-playback-and-recording
---

## Problem

Horizontal trackpad seeking moved opposite to the intended direction. Playback waited 150 ms after fingers lifted, even when no momentum followed. The user also reported a blank popup below the New Recording title field.

## Implemented solution

- Reverse the horizontal delta mapping so positive deltas advance the playhead.
- Seek immediately when the touch phase ends. Keep the gesture target during the 150 ms momentum handoff, without delaying playback or repeating an unchanged seek. Commit the final momentum target when momentum ends. Phase-less wheel events remain coalesced.
- Match the Meeting Title label and accessibility label to the writing guide.

## Reasoning

Immediate finger-lift commitment removes avoidable latency without issuing expensive transport seeks for every scroll event. Live scrub previews still update during the gesture. Pointer seeking remains proportional to the full waveform duration.

Temporary, isolated Preview diagnostics identified the blank box as `SPRoundedWindow` in Apple's `SafariPlatformSupport.framework`, with a 332 × 265 point frame matching the screenshot. A window-ordering stack traced it to `SPSafariPlatformSupport displayOTPAutoFillRelativeToRect:ofView:oneTimeCodeMode:completionHandler:`. The title field's content type was nil; the app has no meeting-name suggestions or one-time-code completion implementation. The system invoked this path on field focus and subsequently ordered the window out.

Explicit nil content type, disabled automatic text completion, and disabled inline prediction (including before field-editor focus) did not prevent the system AutoFill window from being created. The experimental AppKit field and all diagnostic instrumentation were removed. No private framework calls, window hiding, or global AutoFill preference changes ship in the app. The precise reason the system occasionally leaves the empty surface visible remains unverified.

## Technical debt

The system AutoFill popup has no verified app-side workaround. Revisit when a supported field-level opt-out or an OS fix is available; validate focus, typing, and reopening New Recording. A custom field that failed to prevent the window was rejected rather than retained as a compatibility layer.

## Validation

- Offline, read-only playback of two real two-track Opus recordings: 3 h 24 m 35 s and 15 h 29 m 58 s. Ten seeks per recording covered 1–99% of duration; each measurement included seek, playback restart, and rendering the first 4,096 frames. Median/max were 9.0/12.5 ms and 4.4/11.3 ms respectively. Preparation took 130 ms and 12 ms. Originals were unchanged; no hardware playback or upload occurred.
- Regression coverage checks direction, accumulated deltas, bounds, vertical pass-through, immediate finger-lift seeking, momentum, cancellation, and no duplicate seek after the handoff timeout.
- Formatting and lint passed. Final validation after diagnostic removal passed all 113 tests in 30 suites.
- Preview title focus, editing, and Tab navigation were inspected in a separate app bundle. CUA's horizontal scroll action did not provide usable nonzero native horizontal deltas, so physical trackpad feel and full input-to-display latency remain user validation items. Offline decoder timings are not end-to-end UI latency measurements.
- Existing linker warnings reference missing Command Line Tools Frameworks and usr/lib search directories. No deprecation warnings were emitted for this change.
