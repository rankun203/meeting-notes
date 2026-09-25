---
date: 2026-09-25
title: Diagnose changing native list top margins
status: implemented
---

## Problem

Sidebar rows moved down after selecting People, and the user reported similar movement during playback and window activation. The previous 10-point content-margin mitigation did not fix the issue.

## Evidence

Reproduced People navigation in isolated UI Preview on macOS 26.6.2. A temporary, read-only NSViewRepresentable sampled native NSScrollView geometry. Before navigation the sidebar frame was `(0, 88, 180, 483)`, safe-area insets zero, content inset top zero, and clip origin y zero. After navigation/activation the frame and safe area were unchanged, but content inset top became 10 and clip origin y became -10. This directly explains the extra gap: an additional scroll-content margin was applied, rather than the sidebar container moving. The trace does not establish the internal framework reason for its inconsistent application. Diagnostics were removed before delivery; no runtime logging or view introspection remains.

## Implemented solution

Set the extra top scroll-content margin to zero for sidebar and meeting lists. Preserve native list styles, their existing internal row spacing, background, selection, and scrolling behavior. An experiment using external padding was rejected because it duplicated the native row gap; hiding the native scroll background also unnecessarily changed the sidebar appearance.

## Reasoning

`contentMargins` changes scroll content insets; it is not ordinary view padding. Rely on the native list's existing spacing instead of adding an unstable inset or intercepting AppKit internals. This follows Apple's recommendation to prefer standard spacing and native components: [contentMargins](https://developer.apple.com/documentation/swiftui/view/contentmargins(_:for:)), [Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass).

## Validation

Passed: diagnostic preview build and native UI checks of People/Meetings navigation, synthetic playback, system-track mute, track expansion, and inactive-window state. Sidebar/list top content insets remained zero; the sidebar retained its native background and normal top row spacing. No microphone, system capture, credentials, transcription, or upload was involved.

Remaining: a diagnostic sample during track-panel expansion caught the HSplitView meeting scroll view retaining its old height and briefly reporting y=-16 before settling at y=88 with the new height. This is separate from the reproduced 10-point inset change; frame-by-frame visible impact and full-app/long-library regression remain unverified. Do not claim all possible layout shifts fixed.

## Technical debt

Retained: investigate the transient HSplitView resize during track-panel expansion, ideally with frame captures and layout timing before replacing split-view structure. Its possible consequence is a brief content jump during panel resizing. The existing fixed-width sidebar/custom toolbar compromises remain unchanged. Build toolchain missing-search-path warnings remain tracked in the Keychain deprecations worklog; no warning suppression or deprecated API introduced.

## Design follow-up

User likes Music's Liquid Glass capsule tabs and neutral rounded sidebar selection with accent-colored text. Adopt this as the visual direction. Current app already uses native `.segmented` Picker and `.sidebar` List; Music's exact appearance is not exposed as one reusable style. Apple documents `.tabs` Picker as macOS 27+, while this machine runs 26.6.2. Do not promise that selecting that API reproduces Music on macOS 26, or use private APIs. Any custom treatment must retain keyboard/VoiceOver behavior and older-macOS fallback.

Final delivery: diagnostics removed, `make build-macos-preview` and `make install-macos` passed; native final-preview People navigation retained the normal gap. Installer refreshed. Presentation-only change; no new automated tests or unrelated suite rerun. Two pre-existing CLT linker search-path warnings remain.
