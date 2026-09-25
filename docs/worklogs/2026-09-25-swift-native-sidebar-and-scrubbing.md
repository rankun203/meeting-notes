---
date: 2026-09-25
title: Native sidebar, shared scrubbing, and Space playback
status: implemented
---

## Problem

The custom sidebar workaround moved the toggle between columns and removed desired animation. Each waveform held its drag position locally, leaving other timelines and time labels behind until release. Space did not toggle playback.

## Implemented solution

Restored the native sidebar toolbar item and animation, and added SidebarCommands. Shared transient scrub position now lives in PlaybackProgress: all waveform views and timestamps display it, while actual audio seeks once on release. Playback ticks cannot override that preview, and switching/clearing the recording resets it. Frequent updates remain isolated from menus and meeting editors.

Added a window-local Space handler for selected, available playback. Editable text fields/views retain spaces; modified keys, sheets, other windows, loading, and capture are excluded. Repeated keydown events are consumed without repeatedly toggling.

## Reasoning

The user requested a research sub-agent. Its primary-source finding: [macOS 14 release notes, 107148514](https://developer.apple.com/documentation/macos-release-notes/macos-14-release-notes) explicitly place the native sidebar button over the sidebar; a custom navigation item restores the older content-column placement. The workaround caused the reported button movement. Restore Apple's control and [SidebarCommands](https://developer.apple.com/documentation/swiftui/sidebarcommands), consistent with [HIG Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars) and the supplied Notes reference.

All timelines represent the same meeting time. Their widths differ, so equal times need not have identical screen x coordinates. Shared drag feedback fixes temporal synchronization without issuing repeated decoder seeks. Space is scoped to the library window rather than a global hotkey or an unconditional menu shortcut, preserving text entry.

## Validation

61 automated tests passed, including shared scrub position surviving playback ticks, rejecting nonfinite input, clearing on selection removal, and not invalidating playback controls. `make build-macos-preview` passed. Native UI checks verified Space starts/pauses, a space typed in the meeting title does not start playback (fixture title restored), an accessibility seek on the microphone advances all three timelines from 0:10 to 0:15, and the native toggle stays at the same screen position in open/closed screenshots. Preview left paused. Pointer dragging and frame-level sidebar animation were not revalidated; no live capture or services used.

## Technical debt

Native sidebar reveal clipping from the earlier video remains a known unresolved limitation: restored animation intentionally supersedes the nonanimated workaround. Native toolbar placement is corrected, but frame-level animation quality needs further validation rather than claiming a framework fix. Space requires a small AppKit event bridge because an unconditional shortcut could consume text-entry spaces; retain focused-editor and window-isolation checks when changing playback UI. Automated pointer drags remain suspended after the earlier misdirected drop; shared scrubbing is covered in model tests, with manual drag verification still needed.
