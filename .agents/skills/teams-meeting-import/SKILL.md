---
name: teams-meeting-import
description: "Import an authorized Microsoft Teams meeting into meeting-notes using the best available source: directly download the video and retain its audio, reconstruct the streamed recording when download is unavailable, or fall back to the Teams transcript. Use when the user provides a Teams, SharePoint, OneDrive, Stream, or Clipchamp meeting link, HAR capture, or transcript file."
---

# Import a Teams meeting

Preserve the richest usable source available. Unless the user explicitly asks for a transcript only, use this priority order:

1. **Direct recording download:** use Microsoft's visible Download action, verify that the video contains readable audio, and import that media into meeting-notes so the app retains the audio.
2. **Stream reconstruction:** if direct download is unavailable but playback works, reconstruct the recording from the current player's observed media traffic, verify the result, and import its audio into meeting-notes.
3. **Transcript-only fallback:** if no usable recording can be obtained, recover or download the Teams transcript and import a transcript-only session.

Do not fall back merely because the first approach needs a signed-in browser or a fresh HAR. Fall back when the richer source is unavailable, the user declines the required capture, or current evidence shows it cannot be recovered. A user's explicit request for only a particular artifact overrides this ladder.

## Workflow routing

- Start with [references/recording-download.md](references/recording-download.md) for a meeting or recording link, including a missing Download button, view-only playback, or media recovery from a HAR.
- Read [references/transcript-import.md](references/transcript-import.md) only when the user explicitly wants a transcript, supplies only a transcript artifact, or recording acquisition reaches the fallback condition above.
- If a HAR may contain both assets, inspect it locally without printing secrets. Recording reconstruction and transcript extraction use different response data even though they share the capture.

## Shared constraints

- Confirm the user is signed in to an account that can access the meeting and is authorized to retain the requested assets.
- Prefer Microsoft's visible recording Download action before inspecting network traffic, and prefer a recording with audio over a transcript-only import.
- Treat HAR files, signed media URLs, cookies, tokens, and response bodies as sensitive authentication material. Never print or persist credentials beyond what the task requires.
- Do not ask the user to extract media URLs, cookies, encryption keys, or request headers manually.
- Microsoft private endpoints and payloads change. Inspect the current page, supplied files, and current network evidence rather than relying on fixed private endpoints.
- Ask before deleting a user-provided HAR or source transcript.

## Completion

Report the chosen acquisition tier and each created artifact with a clickable path. Say whether the meeting-notes session contains imported recording audio or only a supplied transcript. Include the recording verification details or transcript speaker-mapping caveats required by the relevant workflow.
