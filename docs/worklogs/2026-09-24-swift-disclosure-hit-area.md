---
date: 2026-09-24
task: recording-disclosure-header-activation
status: implemented-build-checked
---

## Problem

The native DisclosureGroup only responded to its small chevron; clicking the Recording options label did not expand or collapse it.

## Implemented solution

A scoped `RecordingDisclosureStyle` uses one full-width native Button for the chevron, label, and remaining header space. Both Recording options and Recording details use it. The button exposes its expanded/collapsed state, inherits disabled behavior, and animates only when Reduce Motion is off. Expanded controls remain outside the button.

## Reasoning

A real Button supports normal keyboard and accessibility activation. A tap gesture attached to the entire group would also intercept controls inside the expanded form; attaching one just to the text would leave two separate interactive targets. A single header action is clearer and easier to hit. HIG disclosure-control guidance is cited beside the style.

## Technical debt

Native desktop automation remains unavailable. Isolated component activation and layout checks can cover header behavior but cannot establish live VoiceOver or system keyboard-navigation behavior. Confirm those when the connection is restored. No added dependency or schema debt.

## Validation

Command Line Tools release build and strict ad-hoc signature verification passed. Independent source review confirmed a single toggle handler, full-width rectangular header hit region, a native Button, and separate expanded controls. Collapsed/expanded component renders at 400/540-point heights remained readable; root inspected the expanded 540-point result.

Offscreen fixture event/accessibility delivery could not reliably activate SwiftUI's button, so label/whitespace clicks, disabled activation, and live keyboard/VoiceOver behavior are not claimed as exercised. Resetting the native automation connection and retrying both app selection and desktop inventory still returned “Sky Computer Use native pipe startup failed.” No real recording or playback was started.
