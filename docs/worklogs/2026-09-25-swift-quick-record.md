---
date: 2026-09-25
title: Direct recording from the menu bar
status: implemented
---

## Problem

The menu bar's Start Recording action opened setup instead of starting capture.

## Implemented solution

Start Recording now calls MeetingStore.startRecording directly with persisted settings and an automatic timestamp title. The Recording menu keyboard command uses the same path. Recording Setup… remains an explicit separate menu bar action. Show starting status, preserve duplicate-start/finalization guards, and open the main window only when a failed start needs error or permission guidance. The menu icon already changes from the custom waveform to record.circle.fill while recording (and remains there during save).

## Reasoning

A direct verb should perform the action; an ellipsis distinguishes the setup workflow. Required OS consent still occurs through the existing capture path. No bypass of permissions and no changes to auto-transcription preferences. UI Preview continues to prohibit real capture.

## Validation

61 automated tests and full app build passed. Live capture was deliberately not started; no audio or upload during validation. Updated full app is at apps/client-macos-swift/.build/macos/Gday Meetings Swift.app; the currently running UI Preview was not replaced for this capture-only behavior.

## Technical debt

None added. Existing Keychain ACL deprecations and linker search-path warnings remain documented in the list-insets worklog; this change does not add deprecated APIs.
