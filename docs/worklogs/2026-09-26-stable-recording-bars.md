---
date: 2026-09-26
title: Preserve recording waveform bars as they scroll
status: implemented
---

## Problem

The recording history rebinned existing samples relative to each newest timestamp. Meter jitter and the half-bin update interval changed which values were combined, so old bars changed shape. Preview also regenerated samples on a shifting time grid.

## Implemented solution

Use fixed monotonic 200 ms buckets. Accumulate the two source maxima only in the pending bucket; on advancing to another bucket, publish it once. Completed bars remain immutable and move by integer positions through the fifty-bar window. Missing intervals stay blank, backward time clears history, and memory remains bounded. Preview samples use an absolute 100 ms grid, so overlapping historical values are identical between renders.

## Reasoning

A bar should depict a fixed interval. Finalizing before display adds up to 200 ms visual latency but avoids changing any displayed height. The instantaneous meter still updates immediately. Existing 10 Hz meter delivery remains the only production clock; no new timer, interpolation, or audio retention.

## Technical debt

Retains sampled-meter transient limitations documented in 2026-09-26-recording-activity-waveforms.md. The bounded completion delay is deliberate, not deferred work. No additional debt.

## Validation

Added a regression test asserting old bars are byte-for-byte unchanged during pending updates and exactly shift when a bucket closes, including jitter and missing intervals. Updated expiry/capacity/reset coverage. All 70 tests in 19 suites passed. Release/preview builds, signing, formatting/lint and diff checks passed. Preview screenshot verified intact layout and fixed-grid histories; deterministic regression checks establish height preservation more precisely than discrete screenshots. Installed and reopened the tested release after confirming no active recording; executable hashes matched and signature verified. Backup: `/private/tmp/gday-before-fixed-bars.JdAWrP/Gday Meetings Swift.app`. Existing CLT missing linker search-path warnings remain; no new deprecation warnings. Live capture was not separately tested.
