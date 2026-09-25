---
date: 2026-09-25
title: Use Finder for manual sample imports
status: implemented
---

## Problem

Preview sample labels looked like unexplained app features. User requested real files in Downloads for manual drag/drop, and explicitly retained Appearance in the preview bar.

## Implemented solution

Copied existing synthetic fixtures to `~/Downloads/Gday Drag and Drop Samples/` as `Gday Sample - Microphone.wav` and `Gday Sample - System Audio.wav`, checking hashes and refusing to overwrite different files. Removed sample drag controls and their unused store dependency from PreviewContainer. Kept Appearance unchanged. Updated preview/design documentation.

## Reasoning

Use normal Finder files with explicit names/extensions for manual import testing. Keep the development banner focused on mode identification and appearance testing. No automatic Downloads export on app startup.

## Validation

Both copied files match source hashes. afinfo reads each as 60-second, mono, 8 kHz Float32 WAV. Installer build passed; existing CLT linker-path warnings remain. The running preview has a user-imported test meeting, so it was deliberately left open without repackaging/relaunching; removal of sample controls is source/build-verified, not visually verified in a refreshed preview. Manual drag/drop is left to the user as requested.

## Technical debt

None added. Preview still uses temporary libraries; these Downloads copies remain independently available across relaunches. Refresh the preview bundle after the user finishes the current manual test session.
