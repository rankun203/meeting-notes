---
title: Provider language discovery
date: 2026-09-26
status: complete
scope: swift-worker-server-and-protocols
---

## Problem

The app used a fixed language menu copied from the Rust client. Different transcription providers and models support different languages, so that menu could offer unsupported choices or omit available ones.

## Implemented solution

- Defined versioned language discovery in the transcription protocol: a provider reports language codes and display names. The app contains no fallback catalog.
- Added worker metadata through authenticated local HTTP and an audio-free RunPod operation. The worker intersects installed recognition and alignment metadata, limits English-only models, and adds its Chinese output conversions. Metadata does not initialize speech models.
- The website returns its configured worker's catalog. Both website and app cache successful discovery for five minutes, share concurrent lookups, and poll existing RunPod metadata jobs when workers are starting.
- All app language pickers use the selected provider. Cache identity includes provider configuration and credentials. Unsupported saved choices remain visible; new jobs validate language before upload. Existing submitted jobs remain resumable without discovery.
- Unavailable metadata has a Retry action. With no provider, the hint reads “Language selection is available when a transcription provider is selected.” The picker is disabled, but recording remains available.

## Reasoning

The provider owns availability; the meeting owns its chosen language. Provider changes must not silently change that choice. Missing metadata must not prevent local recording. Automatic language detection remains unsupported in the app.

## Technical debt

- RunPod metadata can start a worker and incur infrastructure cost. Caching reduces repeated requests; a future control-plane metadata endpoint could avoid cold starts entirely.
- The worker derives language support from its installed recognition/alignment stack and recognizes standard English-only model names. A custom checkpoint with narrower support needs corresponding worker-side metadata before being advertised; the app must not guess those restrictions.

## Notes

- Swift: the integrated suite passed 114 tests. A prior run encountered concurrently changing waveform tests; the settled suite passed. Final language/schema tests passed after aligning field bounds. Formatting, lint, and whitespace checks passed.
- Worker: 17 tests passed; one optional CPU inference test skipped. Server: five metadata tests and eight platform tests passed; TypeScript checks passed. Existing server warnings concern email and trusted-IP configuration. Builds retain the known Command Line Tools search-path warnings.
- Preview verified an English/Japanese response, switching to a French-only response, retention of Japanese as unsupported, and rejection before transcription. Fixtures use localhost and synthetic credentials; no recording was uploaded.
- The final preview build passed. With no providers configured, the recording sheet displays the revised language hint without clipping and keeps Start Recording enabled.
- No deployment is included. Existing remote services must implement the metadata operation before the app can discover their languages. No legacy fallback was added.
- Before committing all accumulated changes, reran validation against the final working tree: 113 Swift tests, 38 server tests, 17 worker tests, and two Filedrop tests passed; one optional worker inference test skipped. TypeScript and Swift lint passed. Existing ignore rules exclude credentials and build outputs; no local secret values were found in changed files.
