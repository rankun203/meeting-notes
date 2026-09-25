---
date: 2026-09-25
title: Player button hit areas and pointer feedback
status: implemented-validation-incomplete
---

## Problem

Player icon controls were difficult to click and had no visible hover feedback. Plain styling left the track disclosure without an explicit full-area content shape.

## Implemented solution

Use 36-point secondary player buttons and a 44-point main Play/Pause target. Explicit rounded hit shapes cover the complete button label. A shared style shows semantic hover/pressed fills without changing geometry and suppresses feedback while disabled. Speed/track menu labels gain 36-point minimum height and hover feedback; menu interaction remains native. Documented the sizing policy in the app UI design guide.

## Reasoning

Apple's [Accessibility HIG](https://developer.apple.com/design/human-interface-guidelines/accessibility) lists macOS default 28×28 and minimum 20×20 pt controls; general [Buttons HIG](https://developer.apple.com/design/human-interface-guidelines/buttons) recommends a 44×44 pt hit region and requires a pressed state for custom buttons. Choose larger-than-default secondary targets and a generous primary target while preserving transport spacing and modest icon sizes. Do not confuse visionOS sizing tables with macOS guidance.

## Validation

Passed: production/preview build, final code diff and whitespace checks. Native preview launched and a read-only screenshot showed the enlarged player layout.

Untested: hover/pressed visual feedback, edge-of-target click behavior, dark appearance and minimum window width. An initial native pipe closure recovered on retry; two subsequent interaction attempts were rejected by the tool's concurrent-user-input guard. Stopped input to avoid interfering. No audio capture or service requests. Presentation-only change; no new automated tests. Existing CLT linker missing-search-path warnings remain.

## Technical debt

The small shared custom ButtonStyle owns pointer/pressed feedback rather than inheriting a system bezel. Retained to keep the transport visually quiet while adding clear interaction feedback. Reevaluate a native borderless style if it supplies equivalent full-area hover behavior on supported macOS releases; keep disabled/focus/contrast regression checks for this style.
