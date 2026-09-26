---
title: Service providers in the Swift app
date: 2026-09-26
status: complete
scope: swift-app-worker-and-protocols
---

## Problem

The app used global transcription and language-model settings. Website sign-in changed the transcription destination implicitly. Provider setup needed explicit capabilities, editable credentials, and a current connection status.

## Implemented solution

- Added provider models, capability interfaces, and RunPod, Filedrop, OpenAI-compatible summary, and website search adapters. Provider API keys use Keychain entries bound to provider identity and endpoint, and are excluded from settings JSON. Failed settings writes restore prior credentials; destination binding prevents a new key from being paired with an old endpoint even if restoration fails.
- Transcribe starts with one click. Brief upload and charge information appears in provider panels. Filedrop reads “Temporary audio links expire automatically.” Connection-check details appear in an info popover. Successful checks show a green icon and “Healthy.” Confirmation remains for replacing an edited transcript or discarding a pending request. Progress remains visible beside the audio player.
- Added a two-column Service Providers panel, capability switches, explicit task defaults, endpoint tooltips, and status icons. Save, panel entry, and Settings reopening check connections without sending meeting content.
- RunPod requires an explicitly selected, enabled Filedrop provider. Filedrop streams audio and returns temporary download URLs; the RunPod request contains URLs, not audio bytes. Format/size checks precede upload. Unsupported input formats can be converted to temporary Opus without replacing originals.
- Replaced implicit transcription routing and the old global transcription/LLM settings. Durable requests retain provider and upload destinations, uploaded inputs, expiry, language, and job ID. Ambiguous submissions require explicit recovery rather than an automatic duplicate request. Completed results preserve edits made during processing.
- Added capability documents in [protocols](../protocols/README.md), including file transfer as a supporting capability. Updated app, worker, Filedrop, and design references.
- UI Preview permits real online requests. It retains temporary libraries, silent playback, disabled capture, and no Keychain prompts. Explicit `--provider-test-env PATH` supplies temporary RunPod/Filedrop credentials in memory for automated testing; launch does not upload audio.
- Removed transcript/embedding response logging from the worker and download-link logging from Filedrop.

## Reasoning

Provider setup, feature enablement, and task selection are separate. Recording stays local and independent. RunPod's JSON request size limit makes its existing URL-based worker contract appropriate for meeting recordings. The user supplied Filedrop details and requested it as an explicit dependency. No inline-audio change remains.

Filedrop connection checks read health and upload limits, then make an empty request without a filename. Authentication runs before filename validation; the expected rejection proves key acceptance without creating a file. A Rust regression checks this order for valid and invalid keys.

Three agents implemented core adapters, settings UI, and protocols/worker verification. The primary agent integrated routing, reviewed errors and privacy boundaries, and ran live tests.

## Progress and results

- The final Swift suite passed outside the sandbox: 98 tests in 27 suites, including credential rollback and destination binding. Formatting, lint, and diff whitespace checks passed. The release app and Preview built successfully.
- Initial sandboxed audio tests failed to create system audio components. Elevated execution resolved those failures. Builds still report missing Command Line Tools framework/library search paths; these are toolchain warnings, not API deprecations. No deprecation warnings were observed.
- Preview light/dark layout, keyboard field navigation and Save, provider dependency selection, and Settings reopening were checked. Save and panel reopening performed real connection checks. The final single-click flow completed a live RunPod transcription with speaker labels in Preview; progress remained visible while the player was open.
- Live RunPod health and Filedrop health, limits, and credential probes passed. Filedrop reports MP3/Opus, a 100 MB limit, and 600-second expiry.
- First live synthetic-speech test uploaded Opus successfully but failed in the deployed worker because it rejects `auto` as a language. Repository worker code already handles that value. Added an explicit language value and a sanitized recovery message; the live integration test and two UI transcription runs then passed with explicit English.
- Protocol metadata, relative links, fences, and whitespace passed validation. Worker tests: 13 passed, one optional CPU inference test skipped. Filedrop tests: two passed.
- No new test credentials were added to tracked files. A credential scan matched an existing constant in the unchanged Rust settings file, already present in HEAD. No private meeting audio was uploaded. Existing unrelated playback/recording edits are preserved.

## Technical debt

- Website authentication currently supports one account at a time. The UI permits one website provider; multiple website accounts require per-provider OAuth storage and service instances.
- Website Search supports querying its existing library. Index mutation and remote Playback are defined extension points, not implemented adapters. Add those operations before exposing their controls.
- RunPod does not guarantee idempotent submission. An uncertain response retains a checkpoint and requires checking job history before discarding it; automatic recovery needs a durable intermediary or a provider-supported idempotency mechanism.
- RunPod retains completed results for 30 minutes. Local checkpointing cannot extend that period. A durable result sink is needed for reliable recovery after long offline periods.
- Filedrop links grant access by possession until expiry and cannot be revoked through the existing API. The repository service retains files for retry downloads until expiry; cleanup is best effort, and restart can leave files on disk. Add startup cleanup and retry failed deletions. It does not provide end-to-end encryption or hide recordings from service operators. Add scoped deletion/revocation and validate confidential processing before making stronger privacy claims.
- The deployed worker needs an update to support automatic language detection. Language now belongs to each meeting's recording configuration and is copied into the transcription attempt. **Settings → Recording → Default Language** initializes new meetings, starts as English, and leaves existing meetings unchanged when edited. Provider settings do not own it. Explicit meeting language selection supports the current endpoint. Deployed worker and Filedrop instances also retain old logging until updated.

## Notes

No commit or deployment has been performed. Builds and live tests use synthetic recordings and isolated libraries. Only synthetic speech was submitted to the configured RunPod and Filedrop services.
