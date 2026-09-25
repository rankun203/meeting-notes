---
date: 2026-09-25
title: Capsule meeting tabs and compact playback header
status: implemented
---

## Problem

The rectangular meeting-content picker did not match the requested Music-inspired Liquid Glass direction. Play occupied an entire row. Switching to To-Dos could clip the title as content types competed for vertical space. Supporting app documentation was scattered at the app root.

## Implemented solution

- Added capsule navigation with real SwiftUI Buttons, selected accessibility traits, explicit keyboard focus, and left/right arrow selection. One public glass surface surrounds the group on macOS 26+, with material fallback on older versions and an opaque surface for Reduce Transparency. Selection animation respects Reduce Motion.
- Moved Play/Pause into the title row as a labeled, tooltipped 32-point icon button. Existing playback blocking/loading guards remain.
- Preserve the header's intrinsic vertical size and layout priority; give the large title field sufficient minimum height so flexible tab content cannot compress it.
- Refresh only native glass surfaces on color-scheme changes to avoid retaining the previous appearance until window activation.
- Moved AUDIO_DESIGN.md, UI_DESIGN.md, and UI_PREVIEW.md under the app's docs/ directory. Updated README/AGENTS entry links and the Preview backlink. README and AGENTS remain discoverable at the app root.

## Reasoning

Native segmented pickers on the tested OS retain a rectangular bezel. A small Button-based group provides the requested capsule shape without private APIs or raising the deployment target. Follow [Apple segmented-control guidance](https://developer.apple.com/design/human-interface-guidelines/segmented-controls), [Liquid Glass adoption](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass), and [playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio). Header sizing expresses the intended layout priority instead of applying offsets to mask clipping.

## Validation

Passed: preview builds; native UI selection and accessible selected state; title Play/Pause toggles synthetic playback; click then Right moves Summary to To-Dos with visible focus; title remains fully visible in To-Dos and Chat; light/system and dark screenshots. Documentation relative links and diff whitespace checks passed. No real capture, credentials, transcription, or upload.

Untested: older-macOS fallback runtime, VoiceOver spoken output, accessibility preference overrides, and minimum-width window regression. No new automated tests for this presentation-only change. Existing CLT linker missing-search-path warnings remain; no deprecated APIs introduced.

## Technical debt

Custom capsule navigation owns keyboard/selection accessibility instead of inheriting Picker semantics. Buttons expose labels and selected state, but are announced as buttons rather than native tabs. Evaluate the native tabs picker when macOS 27 is supported and visually validated; replace this group when it meets the appearance and deployment requirements. The local material refresh resets control focus when appearance changes; meeting selection and editing state remain outside that scope.

Final build and `make install-macos` passed; installer refreshed. The final theme-refresh runtime check was interrupted by concurrent user interaction in Preview, so its effectiveness remains unverified. The earlier dark-mode screenshot initially retained light glass until activation; the local color-scheme identity refresh is the targeted fix pending confirmation.
