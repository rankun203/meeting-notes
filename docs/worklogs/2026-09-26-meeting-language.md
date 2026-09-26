---
title: Meeting language
date: 2026-09-26
status: complete
scope: swift-app-and-protocols
---

## Problem

Language appeared in the provider capability panel even though it describes a meeting. Provider guidance also repeated information already shown by the controls.

## Implemented solution

- Added Default Language to Recording settings, a Language choice beneath the title in recording setup, and an editable Language picker in meeting details. Names and codes match the Rust app, including Simplified and Traditional Chinese.
- Stored language with each meeting. New recordings, blank meetings, and imported audio use the configured default unless supplied a language. Rust imports retain their recorded language. Existing data without language uses English.
- Explicit language is required for both RunPod and website transcription before upload. Removed automatic-detection UI text and provider-specific auto recovery. Already submitted jobs remain resumable.
- Removed provider language configuration and repeated footer guidance. New transcription attempts snapshot the meeting language; retries preserve it.
- Updated the design and protocol documents to distinguish provider operations from meeting configuration.

## Reasoning

Capabilities describe operations a provider performs. Language is input to transcription and can differ between meetings using the same provider. A default reduces setup work without changing existing meetings. Pending jobs keep their submitted language when the meeting changes.

## Technical debt

The initial fixed language menu was replaced by [provider language discovery](2026-09-26-provider-languages.md). The selected provider now reports every available choice through the transcription protocol. Existing worker limitations remain documented in the service-provider worklog; this change adds no provider fallback or migration bridge.

## Notes

- All 105 Swift tests passed, including rejection of empty and automatic language values before either provider uploads. Formatting, lint, and whitespace checks passed. Release and Preview builds passed with the existing Command Line Tools search-path warnings; no API deprecation warnings were observed.
- Preview verified Default Language → New Recording inheritance, keyboard override, existing-meeting isolation, per-meeting editing retained across selection, light/system and dark layout, and removal of the provider field and repeated footer. Real audio capture was not exercised.
- Worker source review confirms one Chinese recognition code (`zh`), followed by OpenCC output conversion for `zh-cn` and `zh-tw`. The current menu's base languages have WhisperX 3.8.6 alignment models. Deployed Chinese conversion was not tested; no additional recordings were uploaded.
- Existing unrelated edits remain in place. No commit or deployment was performed.
