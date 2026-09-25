---
date: 2026-09-25
title: Actionable control feedback audit
status: implemented
---

## Problem

Custom plain controls lacked consistent pointer feedback. User requested an app-wide pass with no changes to layout or dimensions.

## Implemented solution

Extracted the player feedback into shared `ActionButtonStyle` and `ActionHover`. Custom buttons use their full existing rectangular label bounds, hover and pressed backgrounds; increased contrast adds an outline. Disabled controls receive no feedback. No frames, padding, fonts, spacing, scaling, or layout animation changed.

| Audited area | Result |
| --- | --- |
| Player, title playback, return-to-meeting | Shared full-bounds button feedback; existing sizes retained |
| Waveform seeking | Hover/scrub feedback inside existing 24 pt surface; keyboard/focus/adjustable actions retained |
| Capsule tabs, recording disclosures | Shared feedback; keyboard selection and disclosure semantics retained |
| Library search/clear, empty-state recording | Shared feedback; explicit search accessibility label and clear tooltip |
| People, Tags, to-dos | Native add/delete controls retained; delete hover and missing icon tooltips added |
| Associated meetings and People & Tags menu | Row button feedback and menu hover/help |
| Recording setup, Settings, Server Library, alerts, toolbar, app/status menus | Native buttons, menus, pickers, switches, fields and keyboard behavior retained |
| Preview banner | Native appearance picker and existing sample drag help retained |

## Reasoning

[Apple Buttons HIG](https://developer.apple.com/design/human-interface-guidelines/buttons) calls for clear interaction feedback and usable targets. [Accessibility HIG](https://developer.apple.com/design/human-interface-guidelines/accessibility) distinguishes macOS sizing from general touch guidance. Backgrounds, overlays and content shapes do not request extra layout space. Native controls keep platform behavior. We do not expand hit regions over adjacent fields or buttons.

## Validation

- Passed: release compile, preview bundle build, `make install-macos`. Initial incorrect environment-key spelling was corrected to public `colorSchemeContrast` before successful build.
- Passed in rebuilt isolated preview: track disclosure, meeting selection, Summary then Right Arrow selects To-Dos, silent play/pause, speed menu opens during playback, Dark appearance screenshot with readable controls and intact layout. System/light screenshot also inspected.
- Source diff confirms unchanged geometry declarations. This is not exhaustive runtime pixel comparison.
- Untested: every hover/pressed appearance, every edge hit, small windows, increased contrast, older macOS fallback and VoiceOver end-to-end. No live audio, credentials, uploads, destructive actions or service calls exercised.
- Existing two Command Line Tools linker search-path warnings persist; no new deprecated API introduced.

## Technical debt

Retained compact targets to honor the explicit unchanged-dimensions constraint: title play remains 32 pt, waveform 24 pt, disclosure 28 pt and capsule tabs 30 pt; search/clear and text links retain intrinsic sizes. These do not all meet the app's larger preferred playback targets. Native compact controls also retain system sizing. Future remediation: measure edge hit areas across window sizes and allocate non-overlapping interaction space in a separately approved layout pass. This audit is not a blanket minimum-size compliance claim.
