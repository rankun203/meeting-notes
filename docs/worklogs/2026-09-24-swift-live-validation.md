---
date: 2026-09-24
title: Native live audio and recording UI validation
status: available-checks-passed-hardware-gaps-documented
---

## Problem

Recent audio-only capture, recording-sheet, disclosure, and Opus changes needed live UI and hardware validation. Earlier automation could not start its native pipe.

## Implemented solution

- Native automation now works. The supplied installer bundle was absent initially; the build bundle and Applications executable matched. Signature-stripped temporary copies confirmed the build bundle matched the release executable. No native-pipe/authentication failure recurred. Intermittent stale-state errors and startup observation timeouts required fresh app observations.
- Corrected Settings' obsolete screen-sharing-picker instructions to explain audio-source consent and screen-free system capture. Rebuilt with `make install-macos`; the running installer bundle and build bundle have SHA-256 `b7ed83a99f8456267e33303277c31579880e4f79e11d207e18fab54ac70bfffb`. Applications was not replaced with this copy correction.
- Created clearly named local validation meetings for microphone-only, system-only, combined processing OFF/ON, and a planned output-route test. Automatic transcription was verified off. No transcription/upload was initiated.

## Reasoning

Accurate permission descriptions follow [Apple HIG Privacy](https://developer.apple.com/design/human-interface-guidelines/privacy): explain the resources requested in context. Existing native controls were retained. Live observations are distinguished from synthetic checks and user-reported consent/acoustic observations.

## Validation to date

- **Passed:** all 52 tests in 16 suites before the Settings text-only correction. Release build, plist and signature validation passed after that correction. Live Settings showed the corrected text and automatic transcription off.
- **Passed:** app resized from 1200×800 toward its minimum (roughly 900×600 content). Setup's full expanded form, format-picker bottom border, Cancel and Start remained visible in light and dark appearance. Scroll input was accepted, but this live content fit the 510×540 sheet, so actual overflow scrolling remains untested live. Prior shorter offscreen fixtures are separate evidence. Original system Auto appearance was restored.
- **Passed:** clicking the disclosure label and empty header area toggled expansion. Tab reached source toggles, disclosure, processing checkbox and format picker; Space expanded disclosure and opened the format menu. Escape dismissed setup. Full VoiceOver behavior remains untested.
- **Passed:** microphone-only (18.664 s metadata), system-only (56.751 s), combined OFF (120.972 s), combined ON (130.432 s) captured and saved. Independent ffprobe/FFmpeg decoding confirmed readable 48 kHz Opus, mono microphone and stereo system tracks. Combined decoded track endpoints differed by 43.5 ms OFF and 30.75 ms ON; largest track-versus-metadata duration difference was about 146 ms. This is short-take alignment evidence, not a long-duration drift guarantee.
- **User-observed:** an audio-only permission prompt appeared and was accepted. Automation returned after startup and did not inspect the prompt itself; exact first-use wording and denial/retry remain untested. No permissions were reset and Privacy settings were not opened.
- **Passed:** live Opus playback continued through People, Tags, and selection of a different meeting. Player-title navigation returned to the playing meeting. Forward/backward 15-second seeks and pause worked. Switching paused combined playback to Microphone preserved its 24.915 s position. Recording blocked the player; after stopping it remained paused.
- **Preservation:** SHA-256 checks confirmed all nine pre-existing audio files unchanged after the four main captures. No existing meeting was deleted or re-encoded.

## Controlled speech comparison (preliminary measurements)

Built-in MacBook Pro microphone and speakers were confirmed through Sound settings. Output volume was 0.375, input volume 0.5843138; neither was changed by this task. A locally generated 26.0505-second speech file was replayed twice per combined take. The first pass was intended to be far-end only; the second had cued local speech. User confirmed speaking once during the second local-voice take and reported quieter speakers with processing ON. Identical near-end speech/timing in both takes is not established.

Analysis decoded only validation files to 16 kHz mono, with an explicit 0.5L + 0.5R stereo downmix (corrected on resume from FFmpeg’s default +3.01 dB correlated-stereo mix). The reference was located by normalized correlation (0.925–0.931). Ten 2-second windows in the first playback excluded its first/last two seconds; each used best-delay normalized projection within ±500 ms. Silent correlation candidates were excluded by an energy floor.

| Measurement | OFF | ON |
| --- | ---: | ---: |
| Matched system reference RMS, dBFS | -18.99 | -27.02 |
| Median mic/reference projection gain, dB | -21.11 | -51.31 |
| Median absolute correlation | 0.477 | 0.058 |
| Median mic RMS during far-end windows, dBFS | -33.70 | -54.34 |
| Last-five-second mic RMS, dBFS | -45.57 | -45.40 |

The matched digital system signal fell 8.03 dB, consistent with user-perceived ducking despite unchanged volume controls. Reference-normalized leakage fell 30.21 dB, but this is **not an ERLE/AEC guarantee**: ducking changes the acoustic signal and voice processing also includes suppression/AGC. Do not subtract 8.03 dB from the normalized projection result. The tail interval does not demonstrate background-noise improvement; room noise was not a calibrated repeatable stimulus. Near-end intelligibility and double-talk quality need listening and better matched near-end speech. Strong later microphone activity confirms capture, not intelligibility.

The initial apparent OFF system peak above full scale was an analysis downmix artifact. Independent 48 kHz per-channel decoding on resume found peaks of -1.15 dBFS for combined OFF and -1.30 dBFS for system-only, with zero samples at the 0.999 full-scale diagnostic threshold. ON microphone peak was about -0.06 dBFS. These peak checks do not establish subjective intelligibility.

## Pause checkpoint / remaining work

User requested pause during the planned route test. Immediately stopped and saved that system-only take (about 47 s); **no output route was changed**. Playback is paused, no recording remains active, appearance is Auto, built-in routes remain selected. Recording preferences currently retain system-only from this last take; original source preferences were both on with voice processing on. Restore when resuming if appropriate.

- Pending: route-change recovery, quit during capture, recording persistence/readback after relaunch, actual live overflow scrolling, detailed listening/intelligibility, final analysis review, and commit.
- Unavailable/unexercised: wired/USB and Bluetooth device connection/disconnection, sleep, disk failure, long meetings, older macOS codec compatibility.
- Local measurement artifacts: `/tmp/gday-validation-20260924/` contains generated `reference.aiff`, `analyze.py`, `results.json`, and original audio hashes. These are not uploaded or committed; preserve/copy locally if temporary storage is cleared before resuming. Validation recordings remain in the app's normal local library.
- Changed source and this worklog are **uncommitted** at pause. Inspect complete diff, perform appropriate remaining checks, then use a Conventional Commit on master. Do not rebuild over the running installer bundle.

## Technical debt

No new runtime dependency or schema debt from the Settings copy correction. Retained validation gaps are listed above and should be completed before claiming broad hardware coverage. Prior cross-process library-writer risk remains; only one Swift instance was used, while the separate Rust client was already running and left untouched. Future directory-scoped locking remains the remediation documented in the earlier worklog. Acoustic measurement scripts are currently temporary; retain a reviewed reproducible version if these measurements become release acceptance criteria.

## Resumed follow-up

User resumed validation and requested short credential-purpose text in the native Keychain dialog. See [Keychain prompt worklog](2026-09-24-swift-keychain-prompt.md). The stable meter icon change is committed as `055aee1`; renewed capture, route change and quit-during-capture are awaiting the user's readiness reply. An idle app inspection still showed all five new validation meetings and paused playback. The app then quit before rebuilding; no capture was active.

The user confirmed readiness for the remaining capture checks. The refreshed installer (including stable meter icons and Keychain prompt names) built, passed signature verification, and relaunched. All five saved validation meetings appeared after relaunch; this passes library persistence, while playback readback after relaunch remains pending. Automatic transcription was rechecked off.

Starting `Validation 2026-09-24 — Route recovery retry` blocked during Settings persistence, before a meeting/capture was created. The UI observation timed out twice, and SecurityAgent was running. Native computer use explicitly refused access to `com.apple.SecurityAgent` for safety reasons. Asked the user to handle the system prompt directly and report the name/result. No route was changed. This is a protected-system-dialog blocker, not a recurrence of the old native pipe startup failure.

All 53 tests in 17 suites passed after the Keychain change; `make install-macos` passed. Existing nine audio-file hashes remain unchanged. The pause-era silent system-only route placeholder independently decoded to 47.817 seconds of stereo Opus. No uploads/transcription occurred. Original outstanding hardware/listening/overflow checks remain untested, not failed.

## Prompt resolved and quit finalization

User reported the native prompt handled. Automation then observed active system-only capture in `Route recovery retry`; the exact Keychain item wording and Allow/Deny choice were not reported, so native wording remains unverified. The stable meter icons were visible and exposed full status through accessibility Help.

Attempted selecting the available ScreenShare Audio virtual output through Sound settings, but refreshed state still showed MacBook Pro Speakers selected. No actual output transition was established; route recovery remains untested rather than failed. Output volume was now 0.5 when read, different from the earlier acoustic comparison; this resumed take was not included in that comparison and this task did not change the volume.

User requested removal of redundant Recording details text during this take; see [recording-copy worklog](2026-09-24-swift-recording-copy.md). Used the active take for the authorized quit-during-capture check: Cmd-Q exited cleanly, persisted duration 136.183 s, and finalized `system.opus` (stereo 48 kHz, 136.164 s container duration). Independent full-file decode succeeded. All nine original audio hashes remain unchanged. No upload or transcription ran.

Final resumed readback: rebuilt/relaunched app listed the quit-finalized take as 2:16, loaded its Opus in the native player, advanced playback, and successfully sought forward before pausing at 23.575 s. Library persistence and native playback after relaunch therefore pass. The app is left idle with playback paused, built-in output selected, and no recording active. Route changes, external-device tests, live overflow scrolling, full VoiceOver, and matched near-end intelligibility remain explicitly untested. No current reproduced product failure remains from the completed checks.
