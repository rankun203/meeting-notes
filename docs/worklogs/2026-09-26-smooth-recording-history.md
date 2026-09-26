---
date: 2026-09-26
title: Display-paced scrolling of immutable recording history
status: implemented
---

## Problem

Completed recording bars preserved their heights but scrolled in visible 200 ms steps.

## Implemented solution

RecordingActivitySurface uses a view-associated CADisplayLink with the default system frame-rate range. It caches the signal and silence paths, rebuilding only for changed bars/bounds. Each frame updates one container-layer translation with implicit animations disabled. Translation uses monotonic time relative to the fixed bucket boundary, so rollover changes indices and origin together without changing a bar's position. New bars enter from the right through clipping. No meter resampling or per-frame SwiftUI publication is added.

The meter's existing Reduce Motion and scene lifecycle gates stop smooth scrolling when inactive, saving, stale, disabled, or waiting. AppKit suspends hidden/off-display callbacks; weak targets and dismantle invalidation release clocks. Preview uses the same monotonic time domain as production.

## Reasoning

Decouple data production (200 ms immutable bars) from visual motion (display cadence). Cache geometry and translate layers to avoid rebuilding waveform paths at high refresh rates. Source: https://developer.apple.com/documentation/appkit/nsview/displaylink(target:selector:).

## Technical debt

Motion is bounded to two bar widths beyond the latest bucket to avoid runaway scrolling if updates stall. Existing sampled-level transient limitations remain; no additional audio storage or schema changes. Actual 120 Hz cadence remains a hardware validation item.

## Validation

71 tests in 19 suites passed. Final release/preview builds, code signing, formatting/lint and diff checks passed. Preview screenshot verified signal paths, colors and clipping with both sources visible; snapshots cannot establish actual frame cadence or perceptual smoothness. Short preview CPU samples were 10.4–19.2% of one core (including synthetic meter generation), not a controlled production capture benchmark. Physical capture and live Reduce Motion switching were not separately tested. Installed after checking the app idle; signature verified and installed executable hash matched the release. Previous version: `/private/tmp/gday-before-smooth-history.N1pqIn/Gday Meetings Swift.app`. Reopened successfully. Existing CLT linker search-path warnings remain; no new deprecated API warnings. Added regression coverage for translation continuity across bucket rollover and bounded delayed updates, alongside immutable-height tests.
