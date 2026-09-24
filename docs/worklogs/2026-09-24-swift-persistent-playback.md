---
date: 2026-09-24
scope: client-macos-swift persistent-playback
status: implemented-tests-passed
owner: native-services-agent
---

## Problem

Playback belonged to `MeetingDetailView`, so navigation destroyed its player, position and temporary Opus decode files. The requested persistent player needs shared transport state that remains useful while browsing meetings or opening another window.

## Implemented solution

Added app-owned `Core/MeetingPlayback.swift`, a main-actor observable transport with selection, track choice, play/pause, time/relative seeking, speed, elapsed/duration, loading, ended and error state. The app supplies one instance to its views. Explicit selection prepares paused audio; explicit Play or transcript-time activation may start it. Navigating away does not change playback.

Track switching preserves the timeline and intended play state. All Tracks creates an AVFoundation composition with every source aligned to zero, using the recorder's existing host-clock padding. Mixed tracks receive equal attenuation to avoid summed clipping; the app never changes system volume. Original Opus recordings use the existing bounded decoder and remain unchanged.

Preparation and seeking have independent generation/cancellation checks. Obsolete work cannot replace the current item. Successful temporary files transfer to the transport; errors/cancellation delete unowned files. Replacing/clearing playback detaches AVPlayer's old item before deleting owned decode files. KVO, end/failure notifications and the periodic observer update controls and have explicit teardown.

Recording-state input pauses playback and prevents starting it; ending recording does not resume unexpectedly. Library reconciliation updates titles, clears deleted meetings and releases outdated audio revisions. The parent agent wires those events and the persistent player bar; the UI agent removes detail-owned transport state.

## Reasoning

Keep audio lifetime independent of view lifetime and use one AVPlayer shared by the app, so windows cannot accidentally play different meetings simultaneously. Main-actor state is coherent for SwiftUI; native decoding remains in the existing nonisolated async helper. Generation checks complement cancellation because framework/preparation work may finish after cancellation.

Apple's [Playing audio HIG](https://developer.apple.com/design/human-interface-guidelines/playing-audio) supports predictable transport controls and preserving system volume. The [Accessibility HIG](https://developer.apple.com/design/human-interface-guidelines/accessibility) requires discoverable start/stop control rather than unexpected audio. [AVPlayer observer guidance](https://developer.apple.com/documentation/avfoundation/avplayer/removetimeobserver(_:)) requires explicitly removing time observers. These principles are cited in code.

## Technical debt

- Retained Opus playback's temporary full-duration PCM CAF conversion. Memory is bounded and files are deleted after ownership ends, but disk use grows with recording duration. A seekable packet index/resource loader is the concrete future optimization if large-library playback makes preparation too costly.
- All Tracks uses equal per-track gain instead of loudness analysis or a limiter. This prevents ordinary summed clipping with a simple deterministic mix but makes a lone voice quieter when another track is silent. Future measured loudness/limiting can improve consistency while keeping system volume unchanged.
- Transport state persists across navigation and windows for this app session, not across relaunches; system Now Playing/media-key integration is not included. This keeps the change focused on the requested in-app experience and avoids claiming global media controls when no meeting is active. Future work can persist a validated meeting/time bookmark and register media commands only while this transport owns playback.

## Validation

Added meaningful tests using only generated silent audio and controlled preparation callbacks: paused selection, mixed duration, preserving position when selecting a source, bounded seeks, recording prevention, metadata updates/deletion, stale cancelled preparation and temporary-file cleanup, and decode errors. Tests do not play sound or use real meeting recordings. Root's integrated run passed all 44 tests in 15 suites. Native visual and interactive playback checks remain pending. Git commits are serialized by root.

## Retry follow-up

Integration review found that the detail's same-meeting Play/Resume action called the general transport toggle, which returned early for a failed item. Updated `play()` to rebuild the retained meeting, source-track choice and position when an error exists; recording still prevents this retry. Added a counted failing-preparer regression: the explicit retry makes a second preparation attempt and surfaces its new error, while an attempt during recording performs no new work. Both fixture attempts fail before creating a player item, so the test cannot emit sound. Root's final integrated run passed all 45 tests in 15 suites. No new technical debt.
