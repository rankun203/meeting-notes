---
date: 2026-09-26
title: Recording-first experience and optional service hierarchy
status: design-proposed
scope: documentation-only
---

## Problem

A user with no configured API key saw a transcription HTTP 401. The product needs a complete offline recording experience and clear Just recording / Self-deployment / Cloud approaches, including optional local/remote workers, a website, paid transcription, MCP, and an honest distributed-worker privacy model.

## Implemented solution

Created [the experience proposal](../design/meeting-experience.md): public positioning, first-run/record/stop flows, service chooser, settings hierarchy, connection readiness, actionable failures, consent and sync boundaries, example paid-job review, proposed Gday Points accounting, and sequenced acceptance gates. Recorded future website messaging and separated confirmed requirements from commercial/security proposals. No runtime fix or hosted feature is claimed.

## Reasoning

Recording is the independent foundation. Storage, processing, and integrations must be independent choices; login must not imply uploading or paying. Source inspection found an unconfigured direct-transcription route, preset external endpoint, and empty key; auto-transcription defaults off. The screenshot alone does not establish its initiating action. The worker currently requires authentication and URL-based input, so worker-only direct upload and optional auth need contract work.

Reviewed the prior live-transcription research and current NVIDIA/AWS primary documentation. Conventional container deployment cannot substantiate protection from the worker host operator. Proposed a fail-closed confidential-computing experiment, with explicit key, full-pipeline, recovery, and attestation boundaries. Server-side plaintext search/MCP conflicts with a server-blind encrypted library and requires a separate product decision.

## Progress and results

- Completed repository/source review and wrote the ideal journey, state hierarchy, billing proposal, and delivery gates.
- Rewrote the proposal using [Apple's writing guidance](https://developer.apple.com/design/human-interface-guidelines/writing): plain language, shorter paragraphs, consistent service names, action labels, and errors with clear next steps. Kept the product requirements, proposed billing rules, security limits, and open decisions. Checked the revised links, fences, whitespace, and text diff.
- Added YAML frontmatter and the Service Providers design using the supplied Calendar Accounts and Internet Accounts references. Defined a two-column panel, provider-specific settings, five independent capabilities, task defaults, upload consent, and removal behavior. Added proposed common contracts and adapters without claiming existing protocol compatibility.
- Made gradual delivery explicit: providers can ship one capability at a time, new capabilities start off, and unavailable features do not block other providers or local recording. Contract versioning and discovery remain future specifications. Aligned the transcription chooser, settings sections, and delivery plan with this model.
- Identified tenant isolation, worker upload protocol, local model readiness, points ledger, storage policy, and confidential computing as future work rather than existing features.
- Checked document links, code fences, whitespace, and the full new-file content. No app build or runtime tests apply to these documentation-only changes; no deprecation-warning-free build is claimed.
- Preserved existing unrelated Swift playback/recording changes and worklogs. No deployment, account creation, purchase, audio upload, commit, or push performed.

## Writing audit

- Added [the repository writing guide](../writing.md), with a sourced paraphrase of Apple's Writing guidance and separate Gday Meetings conventions. Added a root AGENTS.md requirement to read it before every editing task and apply it to UI text and documents across all apps.
- Reviewed the entire experience proposal, including tables, diagram labels, and interface examples. Replaced the vague recording introduction with an empty state and a specific recording action. Clarified upload destinations, reduced broad claims, aligned control labels, explained technical terms, and separated error messages from implementation behavior.
- Preserved the provider model, offline recording, separate upload permissions, proposed billing, and security research. This audit changes documentation only; existing app text still needs review during implementation.
- Validated YAML metadata, local links, code fences, and whitespace. Reviewed the changes against the writing guide. No runtime tests apply.
- No additional technical debt introduced by the writing audit.

## Technical debt

No implementation shortcut or schema debt added by this documentation task. Existing implicit provider routing remains until delivery steps 1–2; its consequence is requests/errors without explicit service setup. Existing URL-fetch/token-required worker transport blocks the desired simple worker-only connection; step 3 adds the contract. Existing transcript replacement can discard edits; preserve revisions before introducing new processing results. Existing shared-workspace semantics require an ownership/security audit before multi-account hosting. These are retained gaps, not completed fixes.

## Notes

Open decisions: whole-point rounding, actual price/markup, diarization inclusion, storage/retention policy, initial hosted trust boundary, and encrypted-library key recovery/MCP. The user asked to start with ideal UX, so this deliverable is a concrete design and implementation sequence, not an unrequested broad application rewrite.
