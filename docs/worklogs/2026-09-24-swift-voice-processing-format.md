---
date: 2026-09-24
title: Match voice-processing client formats on built-in Mac audio
status: implemented-and-live-capture-verified
---

## Problem

The first user-authorized built-in speaker/microphone test recorded unprocessed audio successfully, but the voice-processing mode failed to start with `com.apple.coreaudio.avfaudio -10875` (`kAudioUnitErr_FailedInitialization`). The app's Core Audio log explicitly reported that the voice processor's client input and output formats did not match. Read-only device inspection confirmed the selected built-in microphone was mono 48 kHz and built-in speakers were stereo 48 kHz.

## Implemented solution

`Core/AudioCapture.swift` now connects its silent output source directly to the engine output node using the exact microphone client format. It verifies both client formats match before initializing the engine. The previous main-mixer connection left its output format implicit; the log established a client-format mismatch, not the exact channel counts at that original failure.

Errors now distinguish enabling voice processing from starting the microphone engine and include the Core Audio domain/code and relevant negotiated client formats. The app still never routes captured microphone or system audio into its output.

## Reasoning

[Apple's voice-processing API](https://developer.apple.com/documentation/avfaudio/avaudioionode/setvoiceprocessingenabled(_:)) and its SDK header require the input node's output format and output node's input format to match while the engine is stopped. Hardware channel counts can differ; the voice-processing Audio Unit manages the device format. The silent source needs no main mixer, so a direct connection removes an unnecessary implicit format negotiation.

## Technical debt

No new audio-format shortcut. Acoustic-quality validation remains required; graph initialization alone does not establish cancellation of unrelated applications' playback.

Retained: separate app copies can open the same meetings directory concurrently. This was observed during the live retry and is outside this audio-format correction. Besides confusing UI automation, independent in-memory libraries risk stale writes. Use one app instance during this validation; a future directory-scoped process lock should prevent a second writer before loading the library.

## Validation

The root agent supplied the decisive app log message and coordinated the approved live retry. All 39 tests in 13 suites passed after the explicit mono correction. `make install-macos` completed using Command Line Tools, verified the signed bundle, and opened the Finder installer.

## Follow-up: negotiate the processed-speech client

The first live retry started after matching the two client formats, but the post-voice-processing input node exposed nine channels on the built-in route. Copying that aggregate default into the capture graph produced a nine-channel WAV; the Opus encoder correctly refused to silently downmix it and preserved the original. Two app instances with the same bundle identifier were also running, so the retest must use one instance; the precise cause of the aggregate default is not assumed.

The capture graph now reads the microphone's original sample rate before enabling voice processing and explicitly negotiates a **mono processed-speech client** at that rate. It supplies the same format to the input tap and silent hardware output, then verifies both negotiated formats match before starting. It does not average nine channels or assume an undocumented channel index contains processed speech. Unprocessed capture retains the device's original channel layout.

[Apple's input-tap documentation](https://developer.apple.com/documentation/avfaudio/avaudionode/installtap(onbus:buffersize:format:block:)) states that a non-nil tap format is applied to the unconnected output bus. This is used together with the voice-processing I/O matching-format requirement.

## Live result

With exactly one installer app instance running, the user approved capture through the app's native picker. The 16:29 take received both sources and stopped with **Recording saved**, without an alert. Metadata and independent `ffprobe` inspection confirmed a mono 48 kHz Opus microphone track with voice processing enabled and a separate stereo 48 kHz Opus system track. Capture lasted about 70 seconds. The two earlier multichannel recordings remain intact as WAV. No audio was uploaded or transcribed.

Local signal analysis decoded at most 90 seconds per track to 16 kHz mono, using two-second windows and a best-delay normalized reference projection within ±500 ms. Across playback-active windows, median reference projection gain decreased from −29.49 dB to −41.64 dB (12.15 dB), and median absolute correlation from 0.312 to 0.121. These are descriptive observations, **not a measured AEC/ERLE guarantee**: digital playback median levels differed by 6.63 dB, clip segments were not verified to match, the baseline had 45 active windows versus 12 processed windows, and ducking/AGC can affect the comparison. Do not subtract the level difference from the projection result; that metric already normalizes its digital reference.

The processed take had playback at roughly 14–38 seconds and strong later microphone activity around 48–68 seconds, with no decoded samples reaching the full-scale diagnostic threshold. This supports capture continuity, not a claim of preserved intelligibility. The intervals did not demonstrate simultaneous speech and playback, and the baseline had no controlled quiet interval. Double-talk handling and background-noise suppression therefore remain unverified. A future controlled comparison needs matched far-end content/levels, isolated quiet intervals, and a clearly timed simultaneous-speech section.

The user also reported Input Source Pro opening unexpectedly. Repository inspection found no launch integration with that app; intercepted automation shortcuts remain a possibility, not a confirmed cause. Subsequent UI control used direct clicks.
