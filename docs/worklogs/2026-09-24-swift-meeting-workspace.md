---
date: 2026-09-24
task: swift-meeting-workspace
owner: native-ui-agent
status: implemented-tested-and-rendered
---

## Problem

Playback belonged to each meeting-detail view, so navigating away could stop listening and selecting another meeting could replace the playback source. The workspace also gave recording, metadata, and secondary actions similar visual weight rather than prioritizing note-taking during a meeting.

## Implemented solution

- Removed all `AVPlayer`, asset preparation, temporary playback file ownership, and playback lifecycle handlers from `MeetingDetailView`.
- Explicit Play Recording and transcript timestamp actions delegate to the app-scoped `MeetingPlayback` environment object. Ordinary selection, editing, and navigation never select, replace, or stop audio.
- Added a clear title/date hierarchy and a compact recording overview with one primary listening action. People/tag assignment is grouped into a metadata menu; transcription, export, and archive occupy one secondary toolbar menu.
- Active meetings embed the core agent's `RecordingWorkspaceView` and initially open Notes. Notes remain editable while capture progresses. Summary, transcript/speaker editing, to-dos, and meeting chat remain available through the standard segmented control.
- Empty transcripts provide a contextual transcription action once audio is saved. Playback respects the shared player's recording block.

## Reasoning

[Apple Music's library instructions](https://support.apple.com/guide/music/mus36265ad9/mac) distinguish browsing the library from deliberately playing a selected item. [HIG Playing Audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio) favors familiar controls and appropriate audio ownership; [HIG Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility) calls for user control of playback. Those principles support the app-scoped transport and explicit playback actions. HIG links are cited beside the relevant code.

Native typography, adaptive colors, segmented navigation, menus, button styles, and standard text editing preserve platform behavior. The detail view adds no animation, decorative waveform, or sound, so it does not need a separate reduced-motion effect. The recording card and persistent transport remain owned by their respective agents to avoid duplicate controls and conflicting lifetimes.

## Technical debt

None added in this view. Playback lifetime and decoded-file ownership are now centralized in `MeetingPlayback`, removing the previous view-lifecycle coupling. The app must inject the shared playback object into every meeting-detail presentation, including sheets; root owns that integration.

## Validation

- Inspected the rewritten file to confirm no `AVPlayer`, preparation task, or disappearance cleanup remains.
- Existing edit actions and transcription/export/archive entry points are retained.
- `git diff --check` passes for this change.
- Root's integrated suite passed 45 tests in 15 suites, and the release build succeeded. Offscreen component checks are recorded below. No real app, microphone, or playback was operated by this agent.

## Integration refinements

- Inspected the generated concept and retained its understated title, inline recording card above notes, and persistent-player separation without adding invented counts or rich-text controls.
- Updated the card call to `RecordingWorkspaceView(meetingID:)`.
- The meeting's listen button reflects shared state: Pause while playing, Resume when paused, Play Again after completion, and Loading while preparation runs. It toggles the existing source instead of restarting from zero; a different meeting remains an explicit play action.
- Added `ViewThatFits` to stack metadata and association controls when the detail column narrows. Used explicit adaptive accent/separator `Color` values to avoid ambiguous shape-style overloads.
- No additional technical debt. Final compact-width and integrated playback verification remains with root; this agent did not operate app audio.

## Offscreen component validation

Rendered the actual SwiftUI setup, active workspace at 600/760-point detail widths, and paused player at 900 points using a temporary standalone `NSHostingView` artifact harness. Output: `/tmp/gday-ui-previews/recording-setup.png`, `meeting-active-600.png`, `meeting-active-760.png`, and `player-paused-900.png`.

The harness snapshots repository source into `/tmp`, replaces only the authentication initializer in that temporary snapshot with a no-op, and uses an explicit temporary `MeetingStore` directory and synthetic meeting/source levels. It selects a generated one-second silent file in paused state; it never starts playback or recording. Hosting windows are never ordered onscreen and app activation is prohibited. This is artifact rendering, not desktop interaction.

Visual inspection found no clipped controls in the requested sizes: the active card, metadata, segmented control, and Notes editor fit, and the 900-point transport has room for title, skip/play controls, timeline, speed, and track menu. Inactive offscreen windows render standard controls in inactive gray, so these artifacts do not establish active-window color/focus behavior. CUA interaction remained unavailable to root; these renders do not replace keyboard/VoiceOver or real audio-device validation. The isolated source snapshot compiled successfully with standalone CLT `swiftc`; no SwiftPM build was run concurrently.

## Minimum-size and dark-mode follow-up

The 440×550-point detail render (representing the 900×600-point app minimum after navigation columns/chrome) exposed truncated microphone/system source headings. Root corrected the meter header with a horizontal-to-vertical `ViewThatFits` fallback. Refreshed only that source file in the frozen fixture snapshot, rebuilt the standalone renderer, and inspected fresh 440×550 light/dark images: both source names and status labels now display in full, Stop & Save and all content tabs fit, and Notes retains an approximately 110-point scrollable editor. The 600×740 light/dark renders retain the wider horizontal meter labels and ample note space. The 760×780 workspace, 510×480 setup, and 900×98 paused player artifacts were also refreshed by the same harness run.

Latest artifacts remain in `/tmp/gday-ui-previews/`, including `meeting-active-440x550.png`, `meeting-active-440x550-dark.png`, and `meeting-active-600-dark.png`. This concludes targeted offscreen layout validation; source and fixtures are frozen for root's final build/commit.
