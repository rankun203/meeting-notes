---
date: 2026-09-22
title: Recover interrupted audio downloads and persist extraction results
status: complete
---

## Problem

The supplied RunPod trace fails while reading an HTTP audio response, before decoding
or inference. The worker did not retry streamed-body errors. Legacy file-drop marked
and deleted files when constructing the first download response, so an interrupted
consumer could never retry. Failed decode/parallel tasks could leak temporary files.
RunPod polling also abandoned jobs on one transient HTTP failure and polled TIMED_OUT
forever.

## Implemented solution

- Added GPU-independent transfer helpers with four complete download attempts, bounded
  connect/read timeouts, exponential backoff, response closing and partial-file cleanup.
- Scoped every parallel track to a job temporary directory, cleaned after all workers exit.
- Retained legacy file-drop downloads until their TTL (default increased from ten minutes to 24 hours for queued jobs); rejected expired entries.
- Added bounded status GET retries and explicit terminal/unknown RunPod status handling.
- Added optional task-scoped result sink to submission and worker: persist typed output
  before success, retry idempotent callbacks, redact callback credentials from logs/errors.

## Reasoning

Stream iteration fails after response headers, outside ordinary HTTP-adapter retry
coverage, so the entire transfer must retry. Restarting at byte zero avoids unsafe
partial audio decoding. Retention is essential for those retries. Submission POST is
not retried automatically because a lost acknowledgement could create duplicate GPU
jobs. A sink acknowledgement means storage succeeded before ephemeral RunPod output
can disappear. The original connection interruption's infrastructure cause is unknown.

## Technical debt

Legacy file-drop still indexes files only in memory and expires after 24 hours by
default; exceptionally long queues and service restarts can still invalidate input URLs. This is retained
for compatibility while GdayMeetings replaces it with durable task/input/output records.
Deploy the platform and migrate clients to remove that legacy lifecycle limitation.

## Notes

Validation: six Python transfer tests, including a real HTTP server truncating a declared
Content-Length; file-drop retry/expiry regression; three Rust polling tests, including
typed terminal-versus-retrieval failure classification. GPU inference
and production deployment are not exercised locally. Existing failed jobs require retry
after rebuilding/deploying the worker and upgrading file-drop or migrating to the platform.
