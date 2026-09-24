---
date: 2026-09-24
title: Source waveforms and independent UI preview
status: validated
---

## Problem

The persistent player had only a slider, making activity and gaps hard to locate. Multiple recordings needed a shared timeline and independent source mute controls. Repeated Keychain prompts interrupted UI validation.

## Implemented solution

- Added bounded, cancellable, background peak-envelope extraction from prepared audio. Peak magnitude is taken across channels, avoiding stereo phase cancellation. Opus uses the existing temporary CAF preparation. No originals are modified.
- Added a compact activity waveform, expandable aligned source waveforms, shared playhead, click/drag seeking, arrow-key seeking, and accessibility adjustable actions. Muting changes the AVPlayer audio mix without replacing its item or shortening the common timeline.
- Added an explicit UI Preview app: fresh temporary synthetic library, silent AVPlayer output, no Keychain operations, blocked capture and network service calls, and an appearance selector. See the client UI_PREVIEW.md for usage.
- Production retains its bundle ID. Inspection found ad-hoc signing with a cdhash-only designated requirement and zero valid installed signing identities. Added optional GDAY_CODESIGN_IDENTITY support; no certificate or permission changes were made. Settings saves avoid rewriting unchanged API keys.

## Reasoning

[HIG Playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio) supports persistent, familiar transport controls and direct position control. [HIG Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility) informs labeled controls, keyboard operation, semantic colors, and adjustable actions. Waveforms indicate source activity, not recognized speech or speaker identity. Rows are visually normalized for navigation; they are not calibrated loudness comparisons. The compact view uses maximum activity across audible sources, not phase-summed output.

[Apple TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements) explains why changing ad-hoc builds lose their prior designated requirement. Reusing the bundle identifier alone cannot fix that. Preview isolation avoids requesting more user passwords and does not relax production credential trust.

## Validation

- Passed: 54 tests in 18 suites, including bounded envelopes, silence, opposite-phase channels, common duration, mute selection, and existing stale-preparation cleanup.
- Passed live in preview: prompt-free launch and Settings; explicit rejection of recording before capture; compact/expanded waveforms; pointer, drag, arrow-key and accessibility-action seeking; aligned positions across rows; mute/unmute; persistent playback through People and Tags; light/dark appearance at ~903×652 points.
- An initial all-muted compact waveform disappeared; corrected to retain dimmed activity. Muted row contrast increased after visual inspection. Final rebuilt verification passed, including single-track layout and retained all-muted activity.
- Untested: certificate-signed update trust (no certificate installed), full VoiceOver narration, real audible mute/gain behavior, long-file latency, video-container envelope extraction, and hardware/permission tests already listed in the live-validation log. Silent preview is not acoustic evidence.

- Fixed a small-window issue found during validation: the transcript empty-state overlay could clip its action behind expanded playback. The player now reserves actual VStack layout height instead of relying on a safe-area inset, and the empty state scrolls within the remaining region. Final live scrolling reached the action button above the expanded player.

## Technical debt

- Envelopes are regenerated on selection and currently complete before playback preparation finishes. Bounded memory is maintained, but long files can add startup latency. Profile long recordings and add a versioned local envelope cache or independent post-load extraction.
- Preview blocks current service call sites; future direct network paths must maintain this guard. Consolidate all transport calls behind one injected transport when services are refactored.
- Production still eagerly reads credentials at startup and local default signing remains ad hoc. A stable installed certificate and deferred credential loading remain the production remedies. No certificate is available in this environment.
- Synthetic preview directories remain in system temporary storage for inspection; OS cleanup owns their lifetime. Add explicit lifecycle cleanup if preview usage causes material accumulation.

## Delivery

Release production and preview bundles built and signatures verified. Final tests passed (54 tests / 18 suites). Preview is open and paused; the already running normal installer app was not replaced or relaunched, avoiding another Keychain prompt. The updated normal build is available at `apps/client-macos-swift/.build/macos/Gday Meetings Swift.app`; the installer staging copy remains the previous version until the normal app is quit and `make install-macos` is run. No real recording, transcription, or upload occurred in this task. No unresolved failure remains in the exercised preview checks; limitations above remain untested.
