---
date: 2026-09-22
title: Remove shared-key Gday integration
status: complete
---

## Problem

The initial integration invented a shared-key compatibility path for Gday, including
capability-driven fallback and client-owned RunPod orchestration. Gday is a user platform
and should consistently authorize users and own transcription execution.

## Implemented solution

- PlatformClient now requires the user's OAuth session; removed the shared-key
  constructor, capability 404 fallback, external-job creation, result-sink handling and
  task status PATCH. Task creation always requests the platform's server-owned workflow
  and no longer sends an `execute` switch.
- Removed Gday probing and durable-task logic from the separate standalone file-drop +
  direct RunPod workflow. Removed its unused Rust result-sink submission interface.
- Saved Gday task references always resume using OAuth. The old `user_auth` field is
  ignored on read and no longer written. Signing out cannot overwrite a pending Gday
  task with standalone work; it reports the required login and preserves the reference.
- Updated integration guidance and regression tests. The Python worker retains the
  per-task callback capability used by Gday's server-owned execution.

## Reasoning

Authentication and execution ownership are platform rules, not capability fallbacks.
Keep standalone recording/transcription as its existing independent feature without
turning its file-drop key into a Gday credential. Existing metadata can still identify
an output location without preserving an alternative authentication mechanism.

## Technical debt

None added or retained by this refactor. The shared-key Gday bridge and auth-mode flag
were removed. Unrelated token-storage/client-registration limitations remain recorded
in the preceding OIDC worklog.

## Notes

Validation includes OAuth-required platform requests, 404 rejection after real local
OIDC login, ignored old auth-mode metadata, and a standalone HTTP upload/RunPod test
that rejects any Gday route call. The existing signed-provider test still covers task
submission replay, restart recovery and local transcript creation. No production
credentials or deployed worker jobs are used.

Results: 37 default Rust tests passed (one opt-in test ignored); the opt-in real
Better Auth provider test also passed against the current Gday source, including
initial and refreshed OAuth access to the platform API. `git diff --check` passed.
