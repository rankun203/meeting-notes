---
date: 2026-09-25
title: Preserve track focus-ring overflow
status: implemented
---

## Problem

The first track's mute-button focus ring was clipped at the top of the newly animated track viewport.

## Implemented solution

Disable the inner ScrollView's content clipping and allow 6 pt drawing overflow around the outer animated viewport when rows are visible. Keep the zero-height collapsed viewport clipped. No layout dimensions, padding, or hit targets changed.

## Reasoning

Focus rings draw outside control bounds; clipping must account for this visual overflow rather than adding row spacing. Preserves visible keyboard focus required by the app UI design accessibility policy. Uses supported SwiftUI scrollClipDisabled (compatible with the existing minimum OS).

## Validation

Production, preview and installer builds passed; preview expansion exposed both tracks. After refreshing a concurrent-input guard, clicking the microphone waveform then Shift-Tab focused Mute Microphone. The screenshot confirms the complete top edge and corners of its focus ring are visible. Dark appearance and many-track scrolling remain untested. Existing CLT linker-path warnings remain. No audio capture or network actions.

## Technical debt

The explicit 6 pt visual allowance accommodates current native focus rings without layout changes. A future platform focus-ring metric change requires rechecking this allowance; retain keyboard-focus screenshots in visual regression coverage when available.
