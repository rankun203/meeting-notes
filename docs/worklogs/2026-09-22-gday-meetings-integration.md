---
date: 2026-09-22
title: Durable GdayMeetings transcription results
status: complete
---

## Problem

RunPod results can expire while the desktop client is asleep. File-drop only
stores audio and cannot recover a completed transcription independently.

## Implemented solution

The daemon detects GdayMeetings capabilities, creates a task containing input
audio URLs, and sends the worker a task-scoped result sink. Session metadata
persists the task location without its callback credential. Initial and resumed
polling read durable transcript output before asking RunPod, including recovery
without RunPod credentials. Task references are saved before submission, and
lookup errors preserve them. Acknowledged job IDs and terminal worker failures
update the platform task. Upload URLs are
encoded correctly and capability download URLs are no longer logged.

## Reasoning

Worker-side persistence closes the gap when the desktop is offline. Each attempt
gets a fresh task so an older worker cannot overwrite a retried transcription.
Only a capabilities 404 selects the legacy flow; authentication and server errors
are surfaced rather than silently disabling persistence.

## Technical debt

Legacy file-drop remains supported during migration, so tasks submitted to that
service still depend on RunPod response retention. Replace deployed file-drop
with GdayMeetings and update existing file-drop URL/key settings to retire this
compatibility path. Service-key rotation or switching platforms requires keeping
the credential valid for outstanding tasks; per-platform credentials can be
introduced if multiple simultaneous backends become necessary.

A lost submission acknowledgement leaves a recoverable task with no RunPod job
ID. It can remain pending if RunPod never accepted the request; this is accepted
to avoid automatically duplicating GPU work. Inspect the task before manual retry;
a future provider-supported idempotent submission/reconciliation protocol can
resolve that ambiguity. Exhausted service lookup retries require a daemon restart
for automatic recovery; a dedicated resume action can remove that limitation.

## Notes

Validation: all 35 daemon library tests pass, including authenticated task contract,
legacy detection, output parsing, and recovery without credentials or submission
acknowledgement. GdayMeetings is maintained in a separate public repository. Live
GPU inference/deployment is not part of local verification.
