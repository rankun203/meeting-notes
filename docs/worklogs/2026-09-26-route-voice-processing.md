---
title: Choose microphone processing from the output route
date: 2026-09-26
status: complete
scope: client-macos-swift
---

**Problem:** A saved global voice-processing preference applied to later recordings even when the output changed between speakers and headphones.

**Implemented solution:** Removed Microphone Processing from Recording settings and its persisted preference. New Recording uses the default output’s Core Audio stream terminal types to choose its initial value. Speakers and low-frequency speakers enable processing; headphones and unidentified endpoints leave it off. A manual choice applies only to that recording. Capture rechecks the route after microphone permission, before creating aggregate devices, unless the user made an explicit choice. Capture health uses the effective recording profile. Updated startup recovery text and audio documentation.

**Reasoning:** Stream terminal types identify endpoints without guessing from localized names or treating all Bluetooth/USB devices as headphones. Keep a per-recording override for devices that cannot be classified and calls that select a different output from the Mac’s default. Ignore legacy preferences while retaining processing metadata on existing recordings. Use Core Audio property APIs supported by the macOS 14.2 deployment target; checked Apple documentation and installed SDK headers.

**Technical debt:** Core Audio cannot reliably identify the physical destination of unknown, line, digital, or aggregate outputs, or another app’s independently selected output. These routes default off and retain a manual override. Further automatic classification requires a physical-device test matrix and reliable endpoint descriptors; do not guess from transport or device names. Processing is selected at startup, not changed during a take; existing route-failure handling remains in place.

**Notes:** `make format-macos` and `make lint-macos` passed. `make test-macos` passed 91 tests in 24 suites, including route changes, headphone/unknown fallbacks, mixed endpoints, property failures, and both legacy preference values. Initial sandboxed SwiftPM execution failed; the authorized unsandboxed run passed. The installed Command Line Tools emitted linker warnings for missing `Developer/Library/Frameworks` and `Developer/usr/lib` search paths; these are toolchain paths, not deprecated APIs, and remain unresolved. Follow up by validating an updated CLT installation. UI checked in a separately identified `/tmp/Gday Route Preview.app` using synthetic data: Settings has no processing section, expanded New Recording options fit, and the manual checkbox works. System/light appearance and keyboard close/navigation were checked; dark appearance, window-size matrix, and physical speaker/headset capture remain untested. The other agent’s Preview app was not changed.
