---
title: Playback protocol
date: 2026-09-26
status: proposed
scope: capability-contract
---

# Playback

## Purpose

Store selected original audio with a provider and retrieve it for listening. Playing local recordings does not require a provider.

## Contract

Upload input contains selected audio tracks, stable meeting and track identifiers, media type, size, duration, and timeline offsets. Uploading audio for remote playback is a separate choice from sending it for [transcription](transcription.md).

Results describe stored tracks and authorized access to them. Access may use authenticated byte-range requests or short-lived download capabilities. A permanent public URL is not an acceptable private playback result.

Follow the [shared provider rules](README.md). Show the audio destination before uploading. Recording files remain local unless the person separately chooses to remove them after verifying the remote copies.

## Operations and results

| Operation | Result |
| --- | --- |
| Upload selected tracks | Stored object identifiers and verified metadata. |
| Inspect availability | Available, uploading, failed, or deletion pending. |
| Retrieve or stream | Authorized media bytes with supported seeking behavior. |
| Refresh access | Renewed authorization without creating another audio copy. |
| Delete audio | Confirmed deletion or a pending state. |

Uploads must tolerate retries without creating duplicate tracks. Verify complete transfers before reporting success. Seeking must use the track's original timeline. Expired authorization is recoverable; deleted audio is not reported as temporarily offline. Removing a provider or turning Playback off does not delete stored recordings.

## Swift interface

`PlaybackProvider` declares `upload`, `playback`, and `remove`. A playback resource contains the meeting ID, duration, and an authorized request. Track metadata, availability checks, and access renewal require further interface work. No remote playback adapter is implemented yet.

## Current website integration

The website stores audio during archive and transcription workflows and supports authorized file retrieval. See its [API reference](../../apps/server/docs/api.md). The Swift client's existing archive action creates an immutable snapshot; it is not continuous audio synchronization.

The full remote playback contract above remains a target. Existing upload support alone does not establish a complete streaming, deletion, and access-renewal adapter. The app must not expose unsupported operations as working capabilities. Connection checks use account or service metadata and never upload or fetch a meeting recording.
