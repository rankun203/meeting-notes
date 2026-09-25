---
date: 2026-09-25
title: Replace recording card with a simple playback button
status: complete
---

## Problem

The meeting detail recording card repeated audio information already available in the persistent player and consumed space needed for meeting content.

## Implemented solution

Replaced the card with one standard bordered Play/Pause button below the meeting header. Removed the decorative waveform tile, recording heading, source summary, background, and card padding. Existing loading, recording, and missing-file guards remain.

## Reasoning

Follow the user's request for a simple action. A native labelled control retains clear intentional playback per [Apple HIG Playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio); source controls stay in the persistent player.

## Validation

`make build-macos-preview` passed. Native screenshot confirms only the compact button remains below the header, including with the track panel expanded. Clicking it starts silent playback, changes to Pause, then pauses and returns to Play. Preview left paused. No new unit tests for this presentation-only removal.

## Technical debt

None.
