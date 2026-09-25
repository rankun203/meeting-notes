---
date: 2026-09-25
title: Actual padded interaction targets
status: implemented
---

## Problem

The previous pass strengthened hover decoration instead of providing the generous interaction area requested. User clarified player controls need 44 pt targets and preview sample sources at least 28 pt with padding.

## Implemented solution

Player skip, disclosure, mute and title play buttons now use 44 by 44 pt label frames. Speed and track menus use ButtonMenuStyle with the shared button style, padded 44 pt minimum-height labels and explicit chevrons; this avoids native borderless styling reducing the label to a compact text control. Sample labels have 6 pt horizontal padding and a 28 pt minimum height. Reverted stronger hover fill/outline to the same standard feedback as playback controls (increased-contrast outline retained). Updated UI_DESIGN sizing policy.

## Reasoning

A real framed label with contentShape creates interaction space; a darker background does not. Latest sizing instruction authorizes the necessary footprint adjustments: banner height follows samples, mute-row height grows 8 pt, track viewport calculation grows accordingly. Icons remain their original size and transport stays vertically centered. Follows the Buttons/Accessibility references in UI_DESIGN.

## Validation

Release, preview and installer builds passed. Preview screenshot shows padded Sample 1 hover with no outline, centered player controls and readable menu labels. Native edge-click test interrupted by concurrent-user-input guard; edge activation, final added chevrons, dark appearance and narrow windows remain unverified. Existing CLT linker-path warnings persist. No live audio, credentials, drag delivery or network actions tested.

## Technical debt

Existing narrow-window fit remains to be validated with the larger requested controls. If targets crowd at minimum window width, introduce an adaptive compact arrangement without shrinking hit regions. Do not substitute hover decoration for actual target sizing again.
