---
date: 2026-09-25
title: System-paced playback cursors with cheaper waveform rendering
status: implemented
---

## Problem

Playback used a fixed 60 Hz dispatch timer for decoding maintenance and UI publication. Every tick recalculated waveform bars and redundantly published isPlaying, invalidating transport controls as well as timelines.

## Implemented solution

- PlaybackWaveformSurface uses NSView.displayLink with the default frame-rate preference, synchronized to the view's current display (including moving between screens). Hidden/off-display views receive no callbacks. Smooth animation pauses when playback stops, while scrubbing, with Reduce Motion, or when the scene is inactive.
- Progress interpolates from the shared source position using monotonic uptime and playback speed, capped at one 50 ms sample interval to bound drift during stalls. Seeks, scrubbing and pause use the actual position.
- A native layer-backed surface caches a single waveform path until data or bounds change. Display ticks update only the reveal mask and cursor with implicit animations disabled; SwiftUI remains responsible for gestures, keyboard focus and accessibility. A weak callback target and explicit invalidation prevent display-link lifetime leaks.
- Audio refill and position sampling run independently every 50 ms with 5 ms leeway (20 Hz); the ring holds about 170 ms at maximum 2× speed. Maintenance still stops on pause/end. isPlaying publishes only when its value changes in transport snapshots.

## Reasoning

Apple documents view display links as synchronized to the current display and suspended when hidden: https://developer.apple.com/documentation/appkit/nsview/displaylink(target:selector:). SDK availability is macOS 14.0. No assumed 60/120 Hz setting or display polling is needed. An initial SwiftUI TimelineView approach measured 28–55% of one CPU core during short expanded-track preview samples, so per-frame updates were moved out of SwiftUI into layer properties. Audio must keep filling even when visual animation is paused. Existing minimum macOS 14.2 remains supported.

## Technical debt

Bounded extrapolation may lead the sampled source by up to 50 ms × playback speed during a stall, in addition to existing engine/output latency. Accepted to separate display cadence from decoding work; follow-up is hardware latency measurement and compensation if perceptible. Background progress still publishes at 20 Hz to keep displayed time correct, but has no extra animation ticks. Existing CLT missing linker search-path warnings remain tracked in 2026-09-25-swift-keychain-deprecations.md; toolchain update/remediation remains outstanding.

## Validation

- Final `make test-macos`: 68 tests in 19 suites passed, including deterministic speed, paused/seeking/scrub position, stalled sample and monotonic time checks, plus existing offline 1×/2× audio rendering tests.
- Release/preview build and signing passed. Formatting, lint and complete source/diff review passed; `git diff --check` clean. Two existing CLT linker search-path warnings remain; no new deprecated API warnings.
- Isolated silent preview: expanded microphone/system waveforms advance together; pause held all positions at 0:47; accessible decrement moved all to 0:42. System/dark screenshot confirms waveform coloring, cursor placement and layout. No real recordings or installed app were modified.
- Short `top` samples with all three waveforms visible: native renderer 5.7–12.1% of one CPU core during playback; paused 0.0% across subsequent samples. This is an indicative local preview measurement, not a controlled benchmark against the original release. UI automation runs windows in the background, so active-window/high-refresh performance still needs hardware validation.
- Not separately tested: alternate refresh-rate displays, moving between displays, live Reduce Motion changes, Light appearance, resized windows, pointer dragging, and real audible output. The public display-link behavior and explicit lifecycle/Reduce Motion guards implement these policies, but snapshots do not establish perceptual frame smoothness.
