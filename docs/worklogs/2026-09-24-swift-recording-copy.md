---
date: 2026-09-24
title: Remove redundant recording details
status: implemented-build-verified
---

## Problem

The active recording card repeated source information and told people they could take notes even though both the source meters and Notes editor were already visible. The user identified this as unnecessary clutter.

## Implemented solution

Removed both explanatory lines and their now-empty Recording details disclosure from RecordingWorkspaceView. Removed its unused expansion state. The timer, Stop & Save, source meters, status tooltips/accessibility values, and error reporting remain available. Recording options in the setup sheet are unchanged.

## Reasoning

Keep the active recording interface focused on useful status and controls. This follows [Apple HIG Writing](https://developer.apple.com/design/human-interface-guidelines/writing): keep interface language clear and concise rather than explaining self-evident interactions.

## Validation

Release compilation, bundle plist validation and signature verification passed through make install-macos. No new behavioral test was added for this view-only removal. Existing audio validation was used to verify safe quit before rebuilding: the active system-only take finalized to a readable stereo 48 kHz Opus file (136.164 seconds), and original audio hashes remained unchanged.

## Technical debt

None. The store's existing captureHealth state remains available to other recording surfaces; this change only removes redundant presentation from the workspace card.
