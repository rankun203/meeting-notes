---
date: 2026-09-25
title: Consistent preview appearance transitions
status: complete
---

## Problem

Reproduced Dark → System (light) leaving dark list/player backgrounds and white detail text on a light background until window activation changed. The preview used SwiftUI preferredColorScheme with a nil reset.

## Implemented solution

The preview picker sets NSApplication.appearance to Aqua, Dark Aqua, or nil for system inheritance. Native controls and SwiftUI now share AppKit's appearance source. This is guarded by UI Preview mode and does not change macOS settings. Playback observation and view identities are unchanged.

## Reasoning

[Apple's appearance documentation](https://developer.apple.com/documentation/appkit/nsapplication/appearance) specifies nil restores system appearance across windows, views, panels, and popovers. This follows [HIG Dark Mode](https://developer.apple.com/design/human-interface-guidelines/dark-mode) guidance to adapt consistently to the chosen appearance. Avoid forcing view reconstruction, which could lose selection, editing, or playback UI state.

## Validation

Reproduced the failure through native automation before the fix. `make build-macos-preview` passed. Native screenshots after System → Dark → System confirmed the entire window updates immediately, without switching windows. Checked while synthetic silent playback advanced from 0:05 to 0:14; selection and playback were preserved. Left preview paused in System mode. System-wide OS appearance changes and additional windows were not tested. No new unit test for this visual-only setting; verified the actual transition instead.

## Technical debt

None. Uses the public AppKit appearance API; no forced redraw timers or view identity resets.
