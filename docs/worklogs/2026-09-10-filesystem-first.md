---
title: Filesystem-first storage refactor
status: complete
started: 2026-09-10
release: v0.2.0
---

## Problem

The daemon retains every parsed transcript: 320 files (446 MiB on disk) require
2,525 MiB of allocations, matching nearly all of its 2,598 MiB footprint. External
file edits do not invalidate cached transcripts or metadata; chat context also
reuses obsolete snapshots.

## Plan and progress

- [x] Reread AGENTS.md; work directly on master and preserve the user's existing AGENTS.md edit.
- [x] Replace full transcript caching with on-demand reads and compact, revision-validated speaker indexes.
- [x] Reconcile filesystem changes, refresh visible UI, and reduce session-list loading work.
- [x] Make file updates atomic and prevent stale app writes; preserve existing formats.
- [x] Reduce conversation-loading allocations and refresh chat source context.
- [x] Remove unnecessary people/embedding retention and preserve active recording/job state.
- [x] Add and run storage/API/browser end-to-end tests, compatibility checks, and memory/latency benchmarks.
- [x] Inspect the complete diff, run release checks, and prepare minor release v0.2.0 for commit/tag/push.

## Implemented solution

- `FilesDb` now reads content on demand and retains only revision-checked speaker
  projections. Raw transcript GET validates without constructing a JSON tree.
- Shared storage helpers provide bounded blocking work, atomic replacement,
  revision fingerprints, and conflict-aware document merging.
- Metadata reconciliation preserves live operations; session pagination occurs
  before detailed file inspection. WebSocket init prompts a paged API fetch.
- A portable one-second JSON metadata poll sends targeted invalidation events.
  Visible panels and off-page session deep links refetch with request guards.
- Conversation list parsing skips context payloads; display/context parsing skips
  word arrays. Every new chat turn resolves remembered criteria against disk.
- People indexes retain centroids/counts/timestamps rather than sample vectors.
- Session and person notes are read on demand. Reconciliation preserves selected
  audio sources as well as recording and processing state. Tags are file-backed;
  serialized read/modify/write operations preserve extensions and concurrent edits.

## Reasoning

Plain files remain authoritative and portable. Speaker badges and person lookup
need derived summaries, not entire transcript trees. Start without a full-content
cache. External edits must be visible without restarting. Prefer bounded work and
rebuildable indexes over mandatory data migration.

## Technical debt

- Use a metadata poll rather than an OS watcher: predictable atomic-save/import
  detection and no new platform dependency. Consequence: up to roughly one second
  for automatic UI refresh; direct content reads remain current. If directory
  stat work becomes material at much larger scale, add watcher hints while
  retaining reconciliation as the fallback.
- Speaker projections are memory-only and built lazily. There is no persistent
  derived-index format to migrate or repair. A cold person query scans source
  bytes once with bounded memory. Add a disposable persisted projection only if
  cold-query benchmarks justify its invalidation/versioning complexity.
- Historical conversation snapshots remain in existing files to preserve history
  without a mandatory migration. New snapshots omit word arrays and are appended
  only when context changes, but old files can remain large. Future opt-in history
  compaction can reduce disk size after defining what historical context to keep.
- Atomicity is per file; related JSON/Markdown/embedding updates are not a single
  transaction. A crash or companion-write failure can leave derived outputs out
  of sync. This preserves the existing directory format; use a small recovery
  journal if cross-file transaction guarantees become necessary. Uncooperative
  external writers can still race the final revision check and rename.
- The typed session catalog and optional runtime handles remain in one map to
  limit recording-lifecycle changes. Large notes/content are no longer retained,
  and reconciliation guards active transitions. Separate the runtime registry if
  future lifecycle work needs independent ownership; avoid caching full session
  documents in the meantime.

## Notes

- Baseline daemon PID 95115 is the installed binary; do not disturb its recordings
  or production library while testing. Use isolated data and a separate port,
  without production credentials or external-service calls.
- Initial acceptance target: under 150 MiB idle on the existing library, bounded
  memory during repeated browsing, correct APIs, responsive pages, and no source
  writes during read-only startup/browsing.
- Release metadata is bumped to 0.2.0; CLI now supports `--version`. Remote master
  matches the starting commit; no existing release tags or GitHub releases.
- Validation: 27 Rust tests and 2 JavaScript tests pass; all web modules pass
  syntax checks. Clippy completes with warnings (not a warnings-as-errors gate).
  Native lifecycle tests send synthetic PCM through real WAV/MP3/Opus writers,
  reconcile external edits while active, and finalize recordings. Hardware audio
  capture is not opened by the isolated tests.
- Isolated 75-session API/Chrome E2E passes, including imports/deletions, stale
  notes, concurrent tags/todos, malformed transcript recovery, local SSE chat
  with refreshed follow-up context, desktop/mobile, live panel refresh, reconnect,
  rapid navigation and off-page deep links. Final v0.2.0 run: startup 956 ms,
  browser load 1.89 s, slowest API 68.3 ms; no browser JavaScript errors.
- Full-library isolated benchmark: 333 session metadata files, 320 transcripts,
  960 transcript GETs across three browsing cycles. Final v0.2.0 footprint 12 MiB
  after initial full listing; 37 / 21 / 19 MiB after cycles (original daemon
  2,598 MiB). Initial full listing took 1.46 s. Transcript GET median 8.5 ms,
  p95 26.4 ms; largest conversation GET max 78.4 ms. Source JSON
  hashes unchanged. Evidence: `target/filesystem-benchmark/results.json` and
  `target/filesystem-e2e/` (local artifacts; reproducible test scripts tracked).
- Ten-times library benchmark: 3,320 sessions / 3,200 transcripts; footprint
  51 MiB after the full catalog and 52 MiB after reading every transcript. Initial
  full catalog took 12.8 s; transcript median 8.5 ms / p95 26.7 ms. This deliberately
  requests all sessions; normal web pages request 50. Source hashes unchanged.
- Upgrade requires no JSON migration. Reload existing web tabs after updating the
  daemon because WebSocket init now signals a paged fetch. Release publication
  does not replace or restart the running production daemon; install/restart at a
  suitable recording boundary. No implementation or validation blockers remain.
