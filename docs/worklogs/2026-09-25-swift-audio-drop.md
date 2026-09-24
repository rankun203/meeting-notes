---
date: 2026-09-25
title: Compact waveform and audio file drop destinations
status: implemented-live-drag-unverified
---

## Problem

The waveform was too tall and its timestamps displaced it above the transport controls. File drops needed distinct semantics: one meeting per file in the list, and additional tracks in an existing meeting detail.

## Implemented solution

Reduced waveform height from 36 to 24 points, aligned its center with transport controls, and placed timestamps below without changing the waveform's alignment bounds. User review caught asymmetric bar padding lifting the whole control row; changed to equal 18-point top/bottom padding, preserving total bar height while centering the controls. Added highlighted file-URL destinations to the meetings list and meeting detail, with ordered asynchronous provider loading. The file-open panel uses the same batch importer.

The importer copies files off the main actor, checks for audio tracks and positive duration (including native Opus metadata), avoids filename collisions, and commits the whole batch once. Failures remove only new copies; existing recordings and notes remain intact. Added tracks share time zero, and meeting duration is the maximum track duration. Recording/busy states and pending server transcription block import rather than modifying active audio/checkpoints. No automatic transcription/upload is performed. Duplicate source names retain their filename suffix in playback labels. UI Preview offers draggable synthetic samples for manual checks.

## Reasoning

[Apple HIG Drag and drop](https://developer.apple.com/design/human-interface-guidelines/drag-and-drop) informs visible destination feedback and copying rather than moving source files. [Playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio) informs aligned persistent controls. No automatic alignment is inferred from arbitrary files; an offset editor is outside this change.

## Validation

Passed: all 60 tests in 19 suites (`make test-macos`) and `make build-macos-preview`. Coverage includes batch imports, duplicate/repeated-dot filenames, duration, persistence, source preservation, corrupt-file rollback, failed-metadata-save rollback, capture/pending-transcription guards, and file-URL provider compatibility.

Passed visually: rebuilt native preview screenshot at roughly 900 points wide shows a shorter waveform centered on Play and speed/track labels, with timestamps below. Final screenshot confirms the main row is centered within the bar after the padding correction. Preview was refreshed without accessing Keychain or real recordings.

Live drag validation is **not passed**. Native automation drag attempts did not import into the preview. User reported temporary SwiftUI.Drag microphone.wav file paths appearing in Codex chat instead. These were generated synthetic sample files, not user recordings; the drag was misdirected. Stopped automated drag attempts and did not repeat them. Finder/multi-file drops and hover feedback remain unverified live; backend multi-file behavior is covered by tests.

## Technical debt

- Imported tracks start at time zero; no arbitrary offset editing or automatic synchronization. Accepted because timestamps of unrelated files cannot establish shared recording time. Add explicit track offsets with timeline tests when needed.
- Batch copying is off the main actor but has no per-file progress/cancel UI; the existing busy state prevents overlapping operations. Add cancellable progress for large imports if required. Temporary rollback is exception-safe, but a process crash can leave unreferenced copied files; future startup reconciliation should identify these without deleting referenced audio.
- Pending server transcription must finish before adding tracks. This preserves checkpoint validity; a future explicit abandon/restart workflow can permit intentional replacement.
- Actual native drag routing still needs manual verification; the automation incident prevents claiming end-to-end drag success.
