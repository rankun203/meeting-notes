---
date: 2026-09-26
title: Restore pointer seeking over native playback rendering
status: implemented
---

## Problem

The native layer-backed waveform introduced in the adaptive cursor change broke pointer dragging. The native representable received the same progress object without a changing time value, so paused/scrub changes could skip native updates. The SwiftUI gesture was also attached directly to NSViewRepresentable; keyboard/accessibility seeking had been tested but pointer dragging had not.

## Implemented solution

A displayed-time value now explicitly invalidates the native representable on seek/scrub updates. A transparent SwiftUI rectangle above the non-interactive native surface owns the drag gesture. Existing focus, hover, shared scrubbing and release-to-seek behavior remain. Display-link rendering is unchanged.

## Reasoning

Separate the pointer target from native rendering so AppKit hit testing cannot bypass the SwiftUI gesture. Keep frame-rate updates confined to layer changes.

## Technical debt

None added. Existing bounded clock extrapolation and hardware frame-rate validation limitations remain documented in 2026-09-25-swift-adaptive-playheads.md.

## Validation

69 tests in 19 suites, release/preview builds, formatting/lint and diff checks passed. Real pointer drags in silent preview: main waveform 0 → 40 seconds, microphone backward 40 → 14 seconds, and system track to 9 seconds during playback; playback continued after release. Screenshots confirmed cursor positions and coloring match the time across all tracks. Existing CLT linker search-path warnings remain; no new deprecation warnings.

Installed the verified release after confirming the app idle; signature and executable SHA-256 matched. Previous bundle retained at `/private/tmp/gday-before-seeking-fix.NpyxrF/Gday Meetings Swift.app`. Reopened successfully with library intact. Actual ProMotion callback frequency was not measured.

 CADisplayLink retains the default frame-rate range, documented by Apple as the display maximum; actual cadence is system selected, not a promise of constant 120 FPS: https://developer.apple.com/documentation/quartzcore/cadisplaylink/preferredframeraterange .
