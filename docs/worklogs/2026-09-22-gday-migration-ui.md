---
date: 2026-09-22
title: Existing meeting migration controls
status: complete
---

## Problem

Users need to review and transfer their existing local meetings to their authenticated
Gday account, including visibility into blocked inputs and individual failures.

## Implemented solution

Added signed-in migration controls to `apps/webui/gday-settings.mjs`: initial dry-run
preview and existing job status, explicit start, periodic progress polling, per-meeting
results, refresh, and retry after completion. Start is disabled until a preview and
status load and at least one meeting is ready. Blocked entries remain visible without
preventing ready entries from being copied. Polling stops on completion/unmount and
retries transient status errors without overlapping requests.

Documented the deployed URL/OAuth prerequisite, retained local backups, verification
in CMS, and retry flow in `docs/gday-meetings.md`.

## Reasoning

Migration is an explicit authenticated data transfer. The UI exposes readiness and
errors before and during transfer, and does not imply that source changes have moved
any real meetings. The daemon owns execution so reopening settings can recover status.

## Technical debt

None. This UI uses the dedicated migration API; transfer validation and idempotency
belong to the backend implementation being developed alongside it.

## Notes

`node --check apps/webui/gday-settings.mjs` and scoped `git diff --check` pass. Backend
and end-to-end migration checks are owned by the coordinating implementation. No real
meeting data was transferred during this UI work.

Added five Rust regression tests in `src/server/gday_migration_tests.rs`: audio-less
snapshots retain notes/documents/speaker profiles and embeddings; active recording,
oversized files, and symlinks are blocked; a mock OAuth-authorized import verifies
SHA-256 and local file preservation; failed imports reuse uploaded checkpoints; lost
acknowledgements recover the existing remote meeting without duplicate upload/import.
Remote snapshot mismatch is rejected. These tests exposed an extra slash in the
import lookup URL, corrected in the coordinating backend change.

Final validation: coordinating run of `cargo test --lib` passed 42 tests (one opt-in
provider test ignored), including all five migration regressions after the URL fix.
