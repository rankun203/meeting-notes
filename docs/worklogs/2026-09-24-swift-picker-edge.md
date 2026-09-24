---
date: 2026-09-24
task: recording-format-picker-edge
status: packaged-awaiting-active-appearance-check
---

## Problem

The user reported that the recording-format picker's bottom edge was missing in the active recording setup sheet. It is the final child of a DisclosureGroup with no bottom inset. The native control's visible region ended exactly at its lower bound, leaving no room for native drawing outside that bound.

Ancestor inspection confirmed that the disclosure's graphics view has both `clipsToBounds` and `layer.masksToBounds` enabled; the picker and its immediate native host do not. The four-point inset moves this actual clipping boundary below the control while leaving the control's frame unchanged.

## Implemented solution

Added a four-point bottom inset inside the Recording options disclosure. The system Picker and its keyboard/accessibility behavior remain intact; no custom border or control was introduced. The HIG pop-up-button reference is cited in code.

## Reasoning

An isolated render comparison at zero/four/eight points confirmed that the control's position and size do not change while its visible region gains the inset. Four points reserve room for the native bezel/shadow and focus ring at the disclosure boundary without adding a large gap. Inactive fixture renders do not reproduce the user's active-window bezel clearly. A simulated key-window fixture changed the primary button's tint but did not change the picker; zero/four/eight-point picker renders remained pixel-identical. Clipping remains the supported hypothesis rather than a fully reproduced visual diagnosis.

## Technical debt

The native UI automation connection still cannot start. Active-window confirmation is deferred until it is restored or the user checks the refreshed app; offscreen inactive renders cannot establish the final active bezel appearance. No additional production workaround beyond the normal layout inset, dependency, or schema debt.

## Validation

The independent fixture comparison used isolated data and no audio, permissions, credentials, or displayed windows. `make install-macos` succeeded, including release compilation, plist validation, strict ad-hoc signature verification, staging refresh, and the Finder-open command. Only the staging/build copies were changed; the Applications copy was not overwritten. Active app confirmation remains outstanding.
