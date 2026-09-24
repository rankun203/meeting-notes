---
date: 2026-09-24
title: Isolate playback progress and simplify meeting controls
status: implemented-live-ui-blocked
---

## Problem

User reported the speed menu closing during playback, the detail panel shifting, missing double-click playback, and unnecessarily elaborate UI copy. Every 250 ms, currentTime published through the app-wide MeetingPlayback object, invalidating all its observers including navigation, menus, and the meeting editor.

## Implemented solution

- Moved time publication into PlaybackProgress. Only PlaybackPosition views observe it, updating waveforms and timestamps; transport state continues through MeetingPlayback. Repeated identical times do not publish.
- Added the selectable meeting list's native primary action: double-click starts playback, single-click remains selection. Context-menu Play, export, and delete actions remain available. Capture blocks playback and meetings without available audio do nothing.
- Simplified the detail and context-menu action to Play. Replaced “Your conversation, in words” with “Recording transcript” and “No transcript yet.” Also made the unselected-meeting message direct.

## Reasoning

Isolating the clock removes frequent invalidation at its source rather than delaying updates or replacing native menus. [Apple's list primary-action API](https://developer.apple.com/documentation/swiftui/view/contextmenu(forselectiontype:menu:primaryaction:)) specifies double-click activation on macOS. Copy follows [Apple HIG Writing](https://developer.apple.com/design/human-interface-guidelines/writing): prioritize clarity over clever labels. Existing [HIG Playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio) transport and position controls remain available.

## Validation

- **Passed:** 55 tests / 18 suites; regression checks 40 clock updates emit 40 clock notifications and zero main playback notifications, and an identical final time emits none. Existing playback, seeking, waveform, and cleanup tests pass.
- **Passed:** release production/preview builds, plist/signature verification, and diff whitespace checks.
- **Blocked/untested live:** speed menu remaining open while time advances, choosing a speed, double-click playback, title focus/position stability, and final copy/layout. Native CUA returns cgWindowNotFound for preview and Finder. Read-only system inspection confirms IOConsoleLocked=Yes and CGSSessionScreenIsLocked=Yes. Restarted only the isolated preview process and retried; same result. Did not unlock the screen or request credentials.
- Normal running app and installer staging bundle were not replaced/relaunched. Updated production bundle is in apps/client-macos-swift/.build/macos; updated preview is in .build/preview. No real audio, transcription, or upload was initiated.

## Technical debt

No new runtime workaround or schema debt. Retained live validation gap: once the desktop is unlocked, verify speed selection through multiple clock ticks, double-click activation, single-click selection, unchanged editor focus, and stable geometry. Existing long-file waveform caching and signing limitations remain documented in the waveform-preview worklog.
