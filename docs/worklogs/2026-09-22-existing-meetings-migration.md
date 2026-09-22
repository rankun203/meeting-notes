---
date: 2026-09-22
title: Preserve existing local meetings in GdayMeetings
status: implemented-awaiting-deployment
---

## Problem

Migrating audio records already on the CMS does not transfer the existing local meeting library. The client needs a separate authenticated, resumable import that preserves completed work without paying for transcription again.

## Implemented solution

Added migration preview/start/progress routes and Settings controls. The daemon inventories all meetings including audio-less transcript imports and failed/untranscribed sessions, streams existing audio, imports all regular JSON/Markdown/text artifacts, and preserves metadata plus referenced speaker profiles/embeddings and tag definitions. Chats are excluded by explicit user choice. Original IDs and edited transcripts are retained.

Per-session private `.gday-migration.json` checkpoints record acknowledged uploads and verified remote meeting IDs. SHA256 and byte counts are checked on the CMS and on readback. Stable snapshot hashes prevent duplicate meeting imports, and repeated source checks detect edits during copying. Existing conflicting CMS content is never overwritten. No local recording/result file is deleted or retranscribed. The server must advertise `meetingImports` before any bulk upload begins.

## Reasoning

One-time archival import is separate from transcription jobs and worker callbacks. A same-origin local start request and the existing user OAuth grant authorize transfer; neither shared service keys nor credentials inside meeting metadata are needed. Sequential streaming bounds memory and server load. Retaining local data allows users to verify the remote archive before adopting it.

## Technical debt

A lost upload acknowledgement can leave a native CMS audio record unattached, because the upload API does not yet accept an idempotency key. Acknowledged uploads are checkpointed and reused. Automatic orphan cleanup is deferred to avoid deleting recordings in flight; future content-addressed upload claims can close this gap. Migration progress is in memory while durable per-meeting checkpoints survive restart; after restart users explicitly start the migration again. This is a snapshot migration, not continuous two-way editing synchronization; future CMS-first editing should use remote revision checks.

## Notes

Read-only local inventory found 350 meetings, 691 audio files and about 5.73 GB of meeting artifacts. Largest audio45.1 MB, largest JSON/Markdown bundle13.04 MB; no file needs compression to fit500 MB. Three meetings have no audio. Existing source data has two missing referenced profiles and three missing embedding files; migration preserves references and records missing files rather than discarding meetings. Chat files and global settings/secrets are not included.

Validation:42 Rust tests passed plus the opt-in actual Better Auth/Gday provider test, extended to stream a real WAV into the native CMS upload collection, import artifacts, retry idempotently and verify readback. Synthetic tests cover interruption/retry, conflict rejection, audio-less meetings, source preservation, size bounds and symlink denial. No real meetings transferred: deployment URL and user login are still required.
