---
date: 2026-09-26
title: Miniature recent recording activity waveforms
status: implemented
---

## Problem

The recording header's tiny status icons showed only current activity. The proposed ten-second waveform had not yet been implemented, so the installed app still showed those icons.

## Implemented solution

RecordingActivityHistory retains at most 100 scalar microphone/system meter snapshots, pruning by monotonic time to ten seconds. MeetingStore updates history on the existing 10 Hz delivery before publishing levels and resets it for each new session. History survives navigation. RecordingSourceMeter renders 50 rounded bars in a flexible 40–110 × 24 pt slot, blue for microphone and teal for system, using a fixed −60 to 0 dB RMS scale. The instantaneous level bars remain; exceptional waiting/stale/disabled/saving status is overlaid and retained in tooltips/accessibility.

UI Preview has an explicitly labeled, collapsible synthetic recording visualization using the production meter component; no microphone, recording, credentials or services are used.

## Reasoning

Reuse scalar metering instead of reading audio, adding sampling timers or display-rate redraws. A fixed scale distinguishes silence from activity without amplifying quiet noise. No interpolation or animated transitions are used, including with Reduce Motion. Only 50 small bars per source are drawn on existing meter updates. This is recent sampled level history rather than an exact PCM waveform; short sounds between meter snapshots may be missed.

## Technical debt

Existing metering samples the most recent audio-buffer level rather than the maximum over the complete 100 ms delivery interval. Very brief transients can be absent; accepted for a lightweight activity overview. If transient fidelity is needed, aggregate maxima on the capture side before delivery. No new audio storage, dependencies or schema changes. Existing CLT linker search-path warnings remain tracked in the prior worklogs.

## Validation

69 tests in 19 suites passed, including source separation, fixed amplitude, ten-second expiry, bounded capacity, invalid time and clock-reset checks. Release and isolated preview builds/signing passed. Light/System and Dark preview screenshots verified compact blue/teal histories, silent baselines and stable labels alongside the instantaneous meters. Accessible source values remained present. Formatting/lint and diff checks passed. Existing CLT missing linker search-path warnings remain; no new deprecated API warnings.

Updated `/Applications/Gday Meetings Swift.app` after confirming it was idle and quitting it. Preserved previous app at `/private/tmp/gday-before-recording-waveform.2AAJfC/Gday Meetings Swift.app`. Installed executable SHA-256 matched the tested release and code-signature verification passed. Reopened the installed app with its library intact. No live recording was started; physical capture, narrow-window and live Reduce Motion switching were not separately tested.
