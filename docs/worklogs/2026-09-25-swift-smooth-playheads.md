---
date: 2026-09-25
title: Smooth shared playback cursors
status: implemented
---

## Problem

The mix and individual-track waveform cursors visibly jumped because the AVPlayer clock was sampled every 250 milliseconds.

## Implemented solution

MeetingPlayback requests a 1/60-second media-time observer interval instead. All waveforms continue to consume the same isolated PlaybackProgress object and actual AVPlayer time.

## Reasoning

More frequent real-clock samples preserve pause, seek, rate, buffering, and end behavior without an animation that trails behind audio or slides across seeks. Only timeline views observe progress; menus and editors remain isolated. AVPlayer schedules callbacks, so this is a requested cadence rather than a guaranteed display refresh rate.

## Technical debt

None added. Existing waveform canvases redraw on each progress update; inspect rendering cost if large track counts become supported.

## Validation

Passed: all 60 tests in 18 suites, full/preview build and signing, and git diff --check. Isolated silent preview: expanded both source tracks, played, paused, and used the microphone timeline's accessible increment action. All three timeline values advanced, paused at 0:06, and sought together to 0:11; screenshot confirms intact layout. Actual frame cadence/perceptual smoothness cannot be established from discrete UI automation snapshots. Dark appearance, alternate playback rates, and physical audio were not separately exercised. Two existing CLT linker search-path warnings remain, with cause and remediation recorded in 2026-09-25-swift-keychain-deprecations.md; no deprecation warnings.

## Follow-up

[Streaming playback](2026-09-25-swift-opus-streaming.md) replaces AVPlayer callbacks with 60 Hz snapshots from the shared engine source cursor, retaining isolated timeline observations.
