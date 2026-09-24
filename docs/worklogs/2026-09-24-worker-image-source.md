---
date: 2026-09-24
title: Link worker images to their source repository
status: completed
---

## Problem

Worker images lacked repository-source metadata for GitHub Container Registry.

## Implemented solution

Added `org.opencontainers.image.source="https://github.com/rankun203/meeting-notes"` to both the CPU and RunPod worker Dockerfiles.

## Reasoning

The OCI source label identifies the repository when images are published, including manual pushes outside GitHub Actions.

## Technical debt

None.

## Notes

Inspected both single-stage Dockerfiles and checked the diff for whitespace errors. This metadata-only change requires no runtime tests. Existing published images are unchanged until rebuilt and pushed; no publication or RunPod deployment was performed.
