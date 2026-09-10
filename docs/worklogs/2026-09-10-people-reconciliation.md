---
title: Avoid repeated people-library reads
status: complete
started: 2026-09-10
release: v0.2.1
---

## Problem

v0.2.0 reparses every profile and embedding summary on each people-list request.
Transcript and attribution components both fetch that list on mounting, even
though only their unopened pickers use it. Correctness and memory tests passed
but did not measure redundant content reads or browser request counts.

## Implemented solution

- Added per-file revision validation for compact people projections. Unchanged
  files are not parsed; reads use a bounded blocking worker. Reconciliation is
  serialized with daemon mutations to prevent older scans replacing fresh data.
- Speaker pickers request people only while open and refresh on person changes.
  Session navigation no longer fetches people. Chat mention resources refresh
  independently, so conversation saves do not trigger unrelated reads.
- Added exact projection-read counts and browser network-request assertions.

## Reasoning

File metadata checks preserve immediate detection of external edits. Unchanged
profiles and embedding summaries should reuse compact derived data. Neither a
time-based stale cache nor suppressing log messages fixes unnecessary parsing.

## Technical debt

Retain per-request directory/revision checks for freshness; these still perform
metadata I/O. If metadata polling becomes material at larger scale, introduce
watcher hints with reconciliation fallback. Never cache complete embedding stores
or person notes for list/recognition operations.

## Notes

- Preserve the existing AGENTS.md edit and running production daemon.
- Projection-read regression passed: 40 warm list/matching operations read zero
  additional content files; 12 concurrent queries after a profile edit read one
  file. Embedding edits, import/delete, malformed JSON recovery and daemon edits
  remain current. Compact catalogs do not retain notes or sample vectors.
- Real Chrome/API E2E passed: zero people requests during session display and
  navigation, one on picker open, one on external person edit, one on chat open.
  A conversation edit refetched none of people/tags/settings/session mentions.
  Existing API, recording failure/lifecycle, chat, source freshness, notes conflict,
  reconnect, desktop/mobile and deep-link checks remain in the suite.
- Final v0.2.1 validation passed: 28 Rust tests, 2 JavaScript tests, changed-module
  syntax checks, Clippy (existing warnings), release build, complete API/Chrome
  E2E. Browser load 1.56 s; API max 65 ms. Evidence is in local
  `target/people-reconciliation-e2e/` artifacts.
- Disposable-library benchmark: 335 sessions / 323 transcripts, 12 MiB after
  catalog load and 41 MiB after browsing all transcripts; transcript p95 26 ms.
  Source hashes unchanged. Evidence: `target/people-reconciliation-benchmark/`.
- Reviewed changes and prepared patch release v0.2.1 for commit/tag/push. Existing
  files need no migration; restart with the new binary and hard-refresh web tabs.
  No implementation or validation blockers remain.
