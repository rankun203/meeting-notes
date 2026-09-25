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

## Follow-up: menu spacing

User requested more left/right padding and less gap. Increased speed/track label horizontal padding from 6 to 12 pt and reduced option-group spacing from 10 to 4 pt. Kept 44 pt height, symbol sizes and existing track-menu width limit. Installer and preview builds passed; rebuilt preview screenshot confirms centered controls and readable labels/chevrons. No new technical debt beyond the narrow-window validation already recorded. Existing linker-path warnings remain.

## Follow-up: compact preview bar

User prefers 20 pt sample height because 28 pt made the preview banner taller. Reduced only sample minimum height to 20 pt, retaining horizontal padding, drag hit shape and hover. Updated UI_DESIGN to record this explicit compact preview exception. Installer build passed with the existing linker-path warnings. No new technical debt; this intentionally supersedes the earlier 28 pt sample target policy.

Preview packaging also passed; rebuilt screenshot confirms the banner returned from 44 to 36 pt overall height, with sample labels and Appearance picker aligned and readable. Sample hover/drag at the new height was not separately exercised.
