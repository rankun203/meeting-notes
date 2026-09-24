---
date: 2026-09-24
title: Match voice-processing client formats on built-in Mac audio
status: fix-implemented-awaiting-live-retest
---

## Problem

The first user-authorized built-in speaker/microphone test recorded unprocessed audio successfully, but the voice-processing mode failed to start with `com.apple.coreaudio.avfaudio -10875` (`kAudioUnitErr_FailedInitialization`). The app's Core Audio log explicitly reported that the voice processor's client input and output formats did not match. Read-only device inspection confirmed the selected built-in microphone was mono 48 kHz and built-in speakers were stereo 48 kHz.

## Implemented solution

`Core/AudioCapture.swift` now connects its silent output source directly to the engine output node using the exact microphone client format. It verifies both client formats match before initializing the engine. The previous automatic main-mixer output connection inherited the speakers' stereo format while the microphone client remained mono.

Errors now distinguish enabling voice processing from starting the microphone engine and include the Core Audio domain/code and relevant negotiated client formats. The app still never routes captured microphone or system audio into its output.

## Reasoning

[Apple's voice-processing API](https://developer.apple.com/documentation/avfaudio/avaudioionode/setvoiceprocessingenabled(_:)) and its SDK header require the input node's output format and output node's input format to match while the engine is stopped. Hardware channel counts can differ; the voice-processing Audio Unit manages the device format. The silent source needs no main mixer, so a direct connection removes an unnecessary implicit format negotiation.

## Technical debt

No new audio-format shortcut. Acoustic-quality validation remains required; graph initialization alone does not establish cancellation of unrelated applications' playback.

Retained: separate app copies can open the same meetings directory concurrently. This was observed during the live retry and is outside this audio-format correction. Besides confusing UI automation, independent in-memory libraries risk stale writes. Use one app instance during this validation; a future directory-scoped process lock should prevent a second writer before loading the library.

## Validation

The root agent supplied the decisive app log message and coordinates rebuilding and the approved live retry. Device/log inspection performed for diagnosis was read-only. All 39 tests in 13 suites passed after the explicit mono correction. `make install-macos` completed using Command Line Tools, verified the signed bundle, and opened the Finder installer. Single-instance live retest remains pending at this worklog revision.

## Follow-up: negotiate the processed-speech client

The first live retry started after matching the two client formats, but the post-voice-processing input node exposed nine channels on the built-in route. Copying that aggregate default into the capture graph produced a nine-channel WAV; the Opus encoder correctly refused to silently downmix it and preserved the original. Two app instances with the same bundle identifier were also running, so the retest must use one instance; the precise cause of the aggregate default is not assumed.

The capture graph now reads the microphone's original sample rate before enabling voice processing and explicitly negotiates a **mono processed-speech client** at that rate. It supplies the same format to the input tap and silent hardware output, then verifies both negotiated formats match before starting. It does not average nine channels or assume an undocumented channel index contains processed speech. Unprocessed capture retains the device's original channel layout.

[Apple's input-tap documentation](https://developer.apple.com/documentation/avfaudio/avaudionode/installtap(onbus:buffersize:format:block:)) states that a non-nil tap format is applied to the unconnected output bus. This is used together with the voice-processing I/O matching-format requirement. Live single-instance verification of the negotiated mono stream and Opus finalization is pending; no acoustic quality conclusion follows from format negotiation alone.
