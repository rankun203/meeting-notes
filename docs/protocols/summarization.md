---
title: Summarization protocol
date: 2026-09-26
status: active
scope: capability-contract
---

# Summarization

## Purpose

Produce a summary from a specified transcript and optional notes. The UI calls this capability **Summaries**.

## Contract

Input contains the transcript text, explicitly included notes, summary instructions, and the selected model. The app must know which meeting and transcript version supplied that text. Audio and unrelated meetings are excluded.

The result is summary text associated with that input. Generated text is editable and must not be treated as verified meeting facts. Preserve user edits if the source changes while a request is running; require a deliberate replacement rather than silently overwriting a newer summary.

Follow the [shared provider rules](README.md). Sending text for a summary does not enable [Search](search.md) or upload original audio for [Playback](playback.md).

## Operations and results

| Operation | Result |
| --- | --- |
| Check connection | An authenticated response without meeting content. |
| Generate summary | Summary text or a task-specific error. |
| Cancel, when supported | Cancellation acknowledgement; otherwise stop waiting without claiming remote processing stopped. |

Empty responses, missing completion content, and malformed result objects are failures. Explain rejected credentials, unavailable models, input limits, and service failures separately where the transport provides enough information.

## Swift interface

`SummarizationProvider.summarize(transcript:instructions:)` returns text. The language-model adapter also accepts message arrays for the app's summaries and chat. The initial interface does not carry a persisted transcript version; the caller owns meeting selection and protection of edits. The app rejects a generated result if the summary was edited during processing. That rejected result is not retained, and changes to the input transcript do not yet produce a persisted revision link.

## OpenAI-compatible adapter

The configured endpoint is an API base URL. The adapter checks `GET /models` with the configured Bearer API key and submits messages to `POST /chat/completions`. Requests specify the configured model and include only the chosen context and instructions. The adapter extracts the assistant's text from the completion response.

Compatibility with this API supports this adapter's language-model operations. It does not imply support for transcription, diarization, meeting indexes, or audio storage. A model-list response confirms access to that route; it does not prove the configured model will accept a completion request.

The completion API does not provide the app with a durable job identifier or a general remote cancellation guarantee. Do not automatically resubmit after an ambiguous submission failure. The provider's retention and billing policies apply to submitted text.
