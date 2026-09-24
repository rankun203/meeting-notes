---
date: 2026-09-24
title: Stable recording meter status layout
status: implemented-build-and-fixture-verified
---

## Problem

Each source meter used ViewThatFits to choose an inline or stacked status label. Different status lengths made the microphone and system meters use different heights, and status changes could move the bars during capture.

## Implemented solution

Replaced the adaptive status text in RecordingSourceMeter with a fixed 18×18-point SF Symbol slot beside the source label. Disabled, waiting, quiet, receiving, stale and finalizing states have distinct symbols. Full status text remains in hover help and the combined accessibility value; disabled sources now correctly announce Not recorded during finalization. Both headers remain a single row.

## Reasoning

Stable geometry prevents changing audio activity from moving adjacent controls. Symbols distinguish state without relying only on color; hover explanations and spoken values preserve the status meaning. This follows [Apple HIG Feedback](https://developer.apple.com/design/human-interface-guidelines/feedback) and [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility). Recording details continue to provide fuller source explanations.

## Validation

Rendered the actual meter component offscreen at 404-point total width, with two 171-point meter columns, in light and dark appearance. Inspected disabled/waiting/quiet/receiving/stale/saving pairs: titles fit, bars align, and all state rows keep the same height. Release compilation passed. No hardware recording resumed; live VoiceOver and hover verification remain pending with the paused validation task. The running app bundle was not replaced; this change needs packaging/relaunch before live use.

## Technical debt

None added. Retained live accessibility-validation gap is explicit above; complete hover and VoiceOver checks when the user resumes desktop validation.
