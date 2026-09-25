---
date: 2026-09-25
title: Bounded Opus playback through libopusfile and AVAudioEngine
status: implemented
---

## Problem

AVPlayer required complete Ogg Opus conversion to temporary PCM before playback. Long meetings incurred proportional startup time and temporary disk usage.

## Implemented solution

Pinned libogg 1.3.6, libopus 1.6.1, and libopusfile 0.12 source archives are checked in for offline CLT builds, checksum verified, and statically linked. Local-file libopusfile support only; no HTTP/OpenSSL/runtime package-manager dependency. Packaging includes license notices.

OpusFileDecoder reads/accurately seeks bounded PCM chunks. StreamingPlayback reads on a serial worker, feeding a 16,384-frame-per-track C SPSC ring. One AVAudioSourceNode mixes all tracks against the same read cursor; underruns produce shared silence without advancing the media cursor. Native sources use continuous AVAudioConverter resampling to the common 48 kHz stereo format. AVAudioUnitTimePitch provides playback speed. Waveform sampling now opens Opus directly without creating PCM files. Existing full conversion remains only for service/transcription compatibility.

## Reasoning

Use upstream container/codec handling for pre-skip, gain, seek preroll and end trimming. Static source builds avoid dynamic library installation and third-party binary dependencies. Checked-in archives remove download-host availability from normal builds. All file I/O and decoding stay off the render thread; mute masks and shared cursors use C atomics. One graph and cursor prevent independent-track drift.

## Technical debt

The transport supports at most 32 tracks and single-link mono/stereo Ogg Opus. Hardware route changes pause with an actionable restart rather than attempting an untested seamless engine rebuild. Upstream autotools configure probes for libogg/libopus emit an obsolete `-single_module` warning; these probes do not add that flag to our static-library compile/link. Updating upstream generated configure/libtool when available is the remediation. libopusfile uses its upstream four-source local target directly, avoiding unused HTTP code and its older libtool wrapper. Existing Swift CLT linker search-path warnings remain tracked in 2026-09-25-swift-keychain-deprecations.md. The displayed clock follows source consumption and may lead audible output by engine/time-pitch latency; accepted for this transport, with latency compensation and hardware measurement as follow-up. The older full-decode service path remains for transcription compatibility until service extraction also adopts incremental readers. Intel and minimum-OS execution need release-matrix validation; this run exercises Apple silicon only.

## Validation

- `make test-macos`: 67 tests in 19 suites passed outside the workspace sandbox, explicitly approved by the user. Coverage includes mono/stereo variable-duration Opus packets, seek/restart/EOF, ring mixing/mute/wrap/starvation, stale selection cleanup, cache invalidation, and real AVAudioEngine offline rendering of Opus plus resampled native audio at 1×/2×. The tests caught and fixed repeated native AVAudioFile reads at EOF.
- Synthetic 10-hour Opus: open + first block + seek near end about 1.6 ms, reading 136,218 of 7,394,527 bytes; sampled waveform about 228 ms. Synthetic sparse PCM overview about 42 ms and cached lookup about 1 ms. Local warm-filesystem observations, not slow-disk latency guarantees; synthetic fixture creation excluded from these figures.
- Clean dependency build from the checked-in archives passed with no downloads. Compiler/SDK/script changes rebuild native objects; subsequent builds reuse the completed stamp.
- `make build-macos-preview` built both full and preview release bundles; code-signature checks passed. `otool -L` shows only Apple system libraries, and all three Xiph license notices are packaged.
- Rebuilt silent Preview: play/pause, shared keyboard-accessible seek (16 → 21 seconds), microphone mute, 2× playback, and automatic stop at 1:00 verified. All three cursor values stay aligned; dark/system screenshot shows intact layout. Physical audible output, device switching, Intel, and macOS 14 were not tested. Discrete UI snapshots do not establish perceptual frame smoothness.
- Complete source/diff review and `git diff --check` passed. Remaining warnings: upstream configure probes use obsolete `-single_module`; Swift CLT adds two nonexistent linker search paths. No new deprecated Swift API warnings.

Initial sandboxed engine tests could not load Apple's audio components. Two automatic approval checks timed out; the user's explicit approval enabled the successful full run. A nested SwiftPM sandbox also blocked the initial packaging attempt; the normal packaging command subsequently passed outside that sandbox.

## Source-only installation verification

Repository ignore rules exclude native object/static/dynamic libraries, debug-symbol bundles, and the local VS Code launch configuration. Vendored source archives remain tracked; generated dependency builds remain under ignored `.build` directories.

Confirmed the three vendored archives contain no compiled library/object/executable entries. Clarified in ThirdParty/README.md that archives are source distributions and the normal installation command automatically compiles them. A fresh dependency build in `/tmp/gday-clt-only-audio-20260925` passed with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, excluding Homebrew and other user-installed tools. App release build, packaging, and signing also passed with that restricted PATH and `/Library/Developer/CommandLineTools` selected. No extra tool installation was required. This validates the local CLT-only path, not every historical macOS/CLT combination; the documented minimum versions still apply.
