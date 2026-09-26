---
title: Diarization protocol
date: 2026-09-26
status: active
scope: capability-contract
---

# Diarization

## Purpose

Diarization assigns speaker labels to audio time ranges. The UI calls this capability **Speaker Labels**. A label identifies a voice within the result; it does not establish a person's identity.

## Contract

Input contains identified audio tracks, their timeline offsets, and optional minimum and maximum speaker counts. An adapter may accept existing timed text to associate labels with transcript segments. Validate that speaker-count bounds are positive and ordered.

Output associates each track with speaker ranges. Each range has a start time, end time, and speaker identifier. Speaker identifiers are scoped to their track and job unless the provider explicitly supplies a broader identity contract. Word or segment assignments may accompany the ranges. Voice embeddings are optional sensitive output, not names or verified identities.

Follow the [shared provider rules](README.md). Audio goes to the chosen provider and any disclosed transfer service. Enabling this capability does not grant ongoing access to the library.

## Operations and results

| Operation | Result |
| --- | --- |
| Submit audio | A job identifier, or a combined transcription job identifier. |
| Inspect progress | Queued, running, completed, failed, or canceled. |
| Retrieve labels | Speaker ranges or speaker assignments tied to the input tracks. |
| Cancel, when supported | Confirmed cancellation or an explanation that work may continue. |

An implementation can combine diarization with [transcription](transcription.md) to reuse the same upload. It must request labels only when enabled. A transcription-only result must not be described as completed speaker labeling.

## RunPod adapter

The audio worker accepts `diarize`, `min_speakers`, and `max_speakers` in the transcription input. Returned segments can contain `speaker`; tracks can contain `speaker_embeddings`. RunPod submission, polling, authentication, and cancellation use the [transcription transport](transcription.md).

The worker needs configured Hugging Face model access for diarization. The current worker can skip diarization when that access is missing. A successful health check cannot establish that these models are available. Missing labels must not be presented as verified speaker identities or fabricated by the client.

The Swift `DiarizationProvider` currently extends `TranscriptionProvider`; `diarize` selects combined processing. Separate speaker-range output and speaker-count controls are not yet part of that Swift interface.

Standalone diarization without transcription is not implemented by this adapter. A future adapter may implement it independently under the same capability contract.
