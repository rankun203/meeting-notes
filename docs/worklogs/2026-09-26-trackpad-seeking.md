---
title: Seek waveforms with horizontal trackpad scrolling
date: 2026-09-26
status: complete
scope: client-macos-swift
---

**Problem:** The main waveform and individual audio tracks supported clicking, dragging, and keyboard seeking but did not handle horizontal trackpad scrolling.

**Implemented solution:** Added `WaveformScrollInput` to the shared timeline. It receives native horizontal scroll deltas, previews the shared playhead while accumulating movement, and seeks once after touch/momentum ends. Movement scales to the waveform’s width and recording duration and stays within the recording. Vertical gestures pass through. A gesture stays with its originating waveform; unrelated momentum cannot start a seek. Cancellation, disabled controls, removal, clicks, and keyboard input clear pending scrubs. Updated the waveform help text.

**Reasoning:** Keep the existing SwiftUI click/drag and accessibility controls. Use supported AppKit scroll events and an app-local event monitor to reach the native receiver even when the SwiftUI gesture layer owns hit testing. Intersect the native visible region with its bounds so a background receiver cannot consume scrolling elsewhere. Local monitoring does not capture input from other apps or require Input Monitoring permission. Delay completion by 150 ms for the touch-to-momentum handoff and phase-less wheel events, avoiding repeated transport restarts while panning.

**Technical debt:** The 150 ms handoff interval is a timing heuristic for hardware that omits momentum or gesture phases. Keep it until a physical Magic Trackpad/Magic Mouse matrix can establish a better completion policy; excessively delayed momentum would begin a separate gesture and is ignored. Existing CLT linker search-path warnings remain unrelated to this change.

**Notes:** Formatting and lint passed. All 100 tests in 28 suites passed. The build retained CLT linker warnings for missing `Developer/Library/Frameworks` and `Developer/usr/lib` paths. Regression coverage includes accumulation independent of playback ticks, direction reversal, bounds, invalid input, vertical-axis locking, momentum completion, cancellation, and disabling during a scrub. In the separate Preview app, click seeking still moved playback to the midpoint and the updated help text appeared. The automation tool’s horizontal-scroll calls delivered zero horizontal deltas, so they could not validate a physical trackpad pan. Temporary diagnostic logging was removed. Real trackpad direction, momentum feel, and playback continuity still need physical verification. The other agent’s Preview app was untouched.
