---
date: 2026-09-23
title: Separate native client, CMS server and audio worker
status: implemented
---

## Problem

The Rust client was the repository root, audio extraction lived under a generic name, and the CMS had moved to a separate repository. Deployment instructions mixed native capture, durable storage, transient file parking and RunPod execution. The components have different OS, authentication and compute requirements.

## Implemented solution

Created a Cargo workspace containing apps/client-macos-rust; imported the clean server source from GdayMeetings commit 5f8e6f7 into apps/server; renamed audio extraction to apps/worker-audio-extraction. Kept independently installable manifests and runtime boundaries. The server import includes source, generated migrations, dependency lockfile/patch and historical worklogs, excluding .git, credentials, data and dependencies. The original repository is not deleted or archived automatically.

Move the optional direct-transfer file-drop helper to tools/file-drop and editor plugin placeholders to integrations. Root documentation describes the three primary components, local/cloud deployment and native client requirements. CPU worker mode and explicit local HTTP transport are implemented independently of the CMS collection redesign, which is recorded in a separate deferred worklog.

## Reasoning

One repository coordinates contracts and development; separate packages/images permit native OS capture, an ordinary CPU server, and an isolated CPU/GPU ML runtime. The client always runs on the host. The server owns authentication, stored meetings and durable results. The worker processes task-scoped inputs and returns outputs. Provider transport and machine credentials are distinct from user OAuth.

## Technical debt

Retain existing client binary/bundle/data identities and persisted server table names to preserve installed clients, permissions and data. This is intentional stable identity, not duplicate runtimes. Existing direct RunPod plus temporary file-drop configuration remains an optional path; document it outside the main three-component deployment. The collection-model compromises are tracked separately in the deferred modeling worklog. Detailed local-worker limits and validation belong to the component worklogs.

## Notes

No user recordings, databases, Docker volumes or running applications are moved by this repository restructuring. No real meeting migration is started. Release automation must use repository-root workflows and component build contexts; imported nested workflows cannot run in a monorepo. Independent future server releases use server-v* tags.

Validation: Rust library tests passed 42 tests plus the separately invoked real-provider integration test; binary compilation and UI checks passed. Server passed 33 tests, TypeScript and production build. Worker passed 13 lightweight tests and an actual CPU tiny/int8 transcription/alignment smoke against a public speech fixture. A synthetic cross-language HTTP round trip verified worker submission, internal audio download, scoped callback, polling and unauthorized rejection; model inference and server persistence were stubbed in that transport check and tested separately. Root CPU/GPU and standalone server SQLite/PostgreSQL Compose configurations validate. Review aligned the 32-track limit and shared CPU/GPU volume ownership, and fixed local callback exhaustion so completed output survives in worker SQLite for status recovery.

Unverified: Docker engine was unavailable, so CPU/GPU image builds and container runtime checks were not run locally. GPU inference and gated diarization were not exercised. No new images or releases were published as part of this restructuring.
