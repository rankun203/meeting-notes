---
date: 2026-09-24
scope: client-macos-swift services
status: implemented
owner: native-services-agent
---

## Problem

A dependency-free Swift client needed the existing server's public-client OAuth, durable transcription, search and immutable archive contracts, plus compatible AI providers. Recording uploads needed bounded memory use, correct channel metadata and codecs available in Apple's Command Line Tools environment.

## Implemented solution

- `Services/GdayAuthentication.swift`: browser authorization with S256 PKCE, random state/nonce, ephemeral loopback listener, dynamic native registration and scoped platform audience. Verify Ed25519/RS256 identity signatures, issuer, audience, nonce, expiry, authorized party and access-token hash. Credentials live in Keychain; refresh requests share an in-flight task to avoid refresh-token rotation races. Discovery endpoints remain same-origin and authenticated HTTP requests never follow redirects.
- `Services/GdayPlatform.swift` and `ServiceSupport.swift`: actual upload, capabilities, submit, poll, search, archive/readback and OpenAI-compatible chat requests. Uploads stream from files; malformed task/transcript responses fail explicitly.
- `Core/ServerTranscription.swift`: persist origin, immutable submission title, attempt key, uploaded track URLs and task ID before subsequent remote steps. Retry/relaunch resumes the saved attempt. Bounded foreground polling returns a resumable status; terminal server failures permit an explicit new attempt. Read-only local libraries cannot submit work without durable checkpoints.
- `Core/ServerArchive.swift`: capture an immutable meeting/notes/transcript/summary/todos/chat/people/tags snapshot, hash audio incrementally, checkpoint uploads and verify server import key/artifact/audio counts. Originals are never deleted. Checkpoints containing signed audio URLs use private file permissions. Hashing runs in a nonisolated async function away from the main actor with cancellation checks.
- Native audio preparation uses Apple frameworks, preserves separate source files and reports the encoded channel count. Bounded AAC encoding supports ordinary 44.1/48 kHz mono/stereo PCM at 64 kbps per channel; an Apple M4A preset handles other decodable formats. Converted upload copies are checked against the 500 MB server limit. Archive conversion copies persist locally so retries use identical bytes and hashes.
- Tests cover signed identity rejection, origin restrictions, form encoding, transcript parsing and checkpoint compatibility. Additional real loopback HTTP tests inspect JSON/multipart/auth requests, HTTP 401 failures and redirects to a second listener. Actual synthetic mono/stereo CAF-to-AAC tests inspect duration, sample rate, channel signal, compression and original-file preservation.

## Reasoning

Match the repository's existing server contracts instead of introducing a new authentication or worker protocol. Native public clients keep passwords in the browser and do not embed a client secret. Keychain and no-redirect transport reduce credential exposure. Stable persisted submissions prevent duplicate paid transcription tasks after a lost acknowledgement.

Keep mic and system audio as separate tracks: the worker calls `whisperx.load_audio`, whose upstream implementation downmixes each file to mono at 16 kHz. A stereo file does not preserve independent source identity through that pipeline. Retaining original local files also makes lossy upload conversion reversible.

Apple's M4A export preset promises the format and gapless metadata, not a particular bit rate. Explicit AAC settings give predictable upload sizes for ordinary captured PCM, with bounded 8192-frame buffers. Channel metadata is measured from the output rather than assuming the source format survived conversion unchanged.

Evidence:

- [Apple M4A export preset](https://developer.apple.com/documentation/avfoundation/avassetexportpresetapplem4a).
- [Apple audio settings](https://developer.apple.com/documentation/avfoundation/audio-settings).
- [Apple export guide](https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/AVFoundationPG/Articles/05_Export.html): explicit AAC settings and source-time handling; reader/writer control for custom reencoding.
- [WhisperX audio implementation](https://github.com/m-bain/whisperX/blob/main/whisperx/audio.py): per-file mono downmix and 16 kHz normalization.
- [WhisperX technical details](https://github.com/m-bain/whisperX#technical-details): VAD-based segmentation and alignment; this informs future silence-aware direct-provider chunk boundaries.

## Technical debt

- OAuth supports the server's Ed25519 and RS256 signing algorithms rather than a general-purpose OIDC library, accepted to keep the CLT-only build dependency-free. Other algorithms fail closed; maintainers must add validated algorithm/JWK implementations and contract fixtures if the provider changes.
- The browser authorization round trip, account revocation/rotation and real worker execution have not yet been exercised against a disposable live server in this task. Unit signature tests and synthetic HTTP tests cannot prove that integration. Add an automated disposable server OAuth fixture before claiming live-provider coverage.
- Archive snapshots are immutable and retain upload checkpoints and converted copies. This enables reliable retries but later local edits do not update the already captured archive, and interrupted uploads can leave unattached server audio. Future work should provide explicit versioned archive creation and verified orphan/converted-copy cleanup without deleting originals.
- Foreground server polling is bounded to five minutes and resumes through the Transcribe action; there is no background scheduler or remote cancellation API. This avoids inventing unsupported server mutation semantics. Add a durable background task monitor and a server-supported cancellation contract if needed.
- Exotic formats use Apple's preset rather than guaranteed fixed-rate encoding. Output size/channel validation protects requests, but an oversized result still needs user-directed splitting. Extend the bounded converter with tested channel layouts/resampling and a durable server chunking contract if these inputs become common.
- The worker downmixes each submitted source; arbitrary multichannel speaker separation is not implemented. Preserve source-separated recordings and add an explicit channel-aware worker contract before promising independent speaker channels.

## Validation and progress

- Root reported the initial integrated suite passed 16 tests, including the initial service security/parsing coverage.
- Final integrated CLT validation passed all 23 tests in eight suites, including loopback HTTP transport and actual mono/stereo AAC conversion. Release packaging and signature verification also passed. Live OAuth/server/worker checks remain outside this synthetic coverage.
- Reviewed fixture lifetimes: loopback-only bind, listener-start timeout of five seconds, per-connection timeout of ten seconds, 1 MiB request bound, complete Content-Length/chunked body parsing, connection-close responses and deferred listener/file cleanup. Tests do not replace the production URLSession or use real credentials.
- Root coordinates complete-diff review, integrated tests and serialized Conventional Commits on `master`; this agent has not modified the shared Git index or committed concurrently.
