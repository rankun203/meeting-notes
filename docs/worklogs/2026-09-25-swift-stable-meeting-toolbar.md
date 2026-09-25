---
date: 2026-09-25
title: Stable meeting actions toolbar ownership
---

## Problem
Switching recordings briefly showed two meeting action menus and shifted the toolbar. MeetingDetailView contributed an anonymous toolbar menu while LibraryView replaced the entire detail using .id(meetingID), allowing toolbar contributions from changing detail identities.

## Implemented solution
Moved MeetingActionsMenu into LibraryView's toolbar, in a single ToolbarItem with stable ID `meeting-actions`. The selected meeting is input to the menu; the detail no longer contributes toolbar items. Existing labels, actions, busy/recording restrictions and server connectivity observation are retained.

## Reasoning
Toolbar lifetime should match its library/window owner. Keep detail identity resets for per-meeting editor state, but avoid tying toolbar insertion/removal to those resets. No timing delays, animation suppression or geometry changes.

## Validation
`make build-macos-preview` passed. In native preview, selected Synthetic conversation then Synthetic single track. Detail accessibility elements were replaced, while the existing meeting actions menu retained element 67 with no additional toolbar element. This confirms stable ownership at sampled states; sub-frame visual behavior was not captured. A subsequent interaction hit the automation user-change guard and was not treated as a passed check. No real audio, credentials or service actions used.

## Technical debt
None introduced. Existing toolchain linker warnings for missing CommandLineTools Developer library/framework search paths remain; resolving the toolchain configuration is separate. No deprecated APIs introduced. Full-app and older macOS visual regression remain untested.
