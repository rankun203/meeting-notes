---
title: Data Privacy settings panel and network transmission log
date: 2026-09-26
status: implemented; not yet checked on screen in UI Preview
scope: swift-app
---

## Problem

The Swift client gave no single answer to “Is any data leaving my device?”. Destinations were described only in scattered provider captions. Outbound requests were not logged, so a person could not check which transmissions happened.

## Implemented solution

Code is under `apps/client-macos-swift/Sources/GdayMeetings/`.

- **Model.** New `Core/DataPrivacy.swift`. `DataPrivacy.rows(_:)` is a pure function from `PrivacyContext` (settings, the signed-in website origin, and unsent transcription attempts) to one `PrivacyRow` per `PrivacyDataType`. Each rule produces a `PrivacyRoute`: data types, a trigger, and receiving providers. Each rule mirrors the guard that allows the matching request:
  - Transcription follows `transcriptionProvider(for:)` and `uploadProvider`. RunPod needs a usable Filedrop provider. The website needs sign-in. **Automatically Transcribe Recordings** adds “after each recording”.
  - Summaries and chat follow `summaryProvider()`.
  - Archive follows `archiveToServer`: an enabled, signed-in website, with no capability gate.
  - Search follows `ServerLibraryView`.
  - Credentials cover every provider with a key, including disabled ones, because opening a provider panel runs a connection check. Website sign-in tokens are included.
  
  Rows merge triggers per provider into one sentence: “Sent to *Provider* (*host*) when you …”. With no route, a row shows “Stays on this Mac”.
- **UI.** New `UI/DataPrivacyView.swift` adds the **Data Privacy** tab (`hand.raised`) to Settings. It contains a short introduction, a **Data** section with one row per type, and a **Logs** section with **Export Logs**. `DataPrivacyForm` takes rows directly so layout can be rendered without the store. Each row is one VoiceOver element. Symbols (`laptopcomputer` or `arrow.up.forward.circle`) distinguish local and sent data without relying on color.
- **Network log.** New `Services/NetworkLog.swift` uses the `network` category of the app subsystem. `ServiceHTTP.json`, `.data(for:trace:)`, and `.upload(for:fromFile:trace:)` now require a `NetworkTrace` (provider and data category). All former direct `ServiceHTTP.session` calls go through them. Each request logs one `notice` line, or an `error` line for a failure or non-2xx status, in this format: `Provider · data category · METHOD host[:port]/path · N bytes sent · HTTP 200`. Query strings, fragments, user info, bodies, headers, and error descriptions are not logged; errors are reduced to `URLError` codes or type names.
- **Export.** `RecordingLogExport` is renamed `LogExport`, and `exportRecordingLogs` is renamed `exportLogs`. The menu item is **Help → Export Logs**, the same label used in Data Privacy. Files are named `gday-meetings-log-<time>.txt`. The export already includes every category of the subsystem, so network entries are included.
- **UI Preview.** `--synthetic-providers` seeds RunPod, Filedrop, and OpenAI-compatible providers with `.invalid` hosts and turns on automatic transcription.
- **Tests.** `DataPrivacyTests` covers no providers, RunPod with Filedrop (including automatic transcription), unusable upload providers, pending attempts, the signed-in website, OpenAI-compatible summaries, server archive, phrase joining, and synthetic preview hosts. `LogExportTests` (renamed) checks that network entries are exported without query strings, and that message and outcome formatting redacts secrets. 164 tests pass; the baseline was 152.
- **Docs.** README gains “Data privacy and logs” with a transmission table. AUDIO_DESIGN and UI_PREVIEW are updated.

### Data inventory

| Data | Stored | Leaves the Mac | Code |
| --- | --- | --- | --- |
| Recorded audio | Library folder | Transcribe (RunPod: Filedrop upload, then RunPod downloads the link; website: upload). Archive to Server. | `ProviderTranscription.swift`, `ServerArchive.swift` |
| Meeting details | `library.json` | Language to RunPod. Title and language to the website on Transcribe. Title to the LLM. Full `meeting.json`, including the recording profile with device names, on Archive. | same, plus `MeetingIntelligence.context` |
| Notes, transcripts, summaries, chat | `library.json` | LLM on Generate Summary or Send (context chats include several meetings). Archive. | `MeetingIntelligence.swift` |
| To-dos | `library.json` | Archive only | `ServerArchive.swift` |
| People and tags | `library.json` | Archive only (linked people's name, email, and notes) | `ServerArchive.swift` |
| Server Library searches | Not stored | Website search query | `GdayPlatform.search` |
| Credentials | Keychain | Bearer header to their own provider; website tokens to its token and revocation endpoints | `ServiceSupport.swift`, `GdayAuthentication.swift` |
| Settings | `settings.json` | Only Summary Instructions, to the LLM | `MeetingIntelligence.summarize` |
| Logs | Unified log; exports in `~/Library/Logs/Gday Meetings` | Never sent by the app | `CaptureLog.swift` |
| Legacy and meeting-archive imports, text export | Local copies | Never | `LegacyImport.swift`, `MeetingStore.importArchive` |
| Archive checkpoint and converted archive audio | Meeting folder | Only as part of Archive | `ServerArchive.swift` |
| Filedrop copies | Filedrop host until expiry | Download link to RunPod | `FiledropProvider.upload` |

## Reasoning

- **Mirroring guards rather than capability flags alone.** A selected but unusable provider (no model, key, or upload provider, or not signed in) cannot send data. Showing it as a destination would be inaccurate. The cost is duplicating guard conditions in one place, where tests pin them.
- **Required `trace:` parameter** instead of a default, so the compiler finds every call site, and a new request cannot skip the log silently.
- **Host plus path in the log.** Paths are needed to tell operations apart (for example, `/run`, `/status`, `/upload`). They contain job IDs and meeting UUIDs but no secrets. Queries carry filenames and search text, so they are dropped.
- **Log values are `.public`.** They are reduced to categories, hosts, and counts first. Private values would appear as `<private>` in the in-app export.
- **Rejected:** a per-request transmission list inside the panel. The unified log already records transmissions, and a second store would add persistence and retention decisions.

## Technical debt

- **Duplicated eligibility rules.** `DataPrivacy.routes` restates the guards in the transcription, summary, archive, and search paths. Accepted to keep the panel pure and testable. If a guard changes without the matching rule, the panel becomes inaccurate. Remediation: move each guard into a shared `canSend` predicate that both the action and the panel call.
- **Website rows can't be seen in UI Preview.** Sign-in cannot be simulated. Unit tests cover them. Remediation: inject a website session into `GdayServerService` for Preview.

## Notes

- **Validation.** `make format-macos`, `make lint-macos`, `make test-macos` (164 passed, no Swift warnings; the linker search-path warnings existed before this change), and `make build-macos-preview`. Layout was checked by rendering `DataPrivacyForm` offscreen with an `NSHostingView` bitmap in Light and Dark, using synthetic providers, in a temporary test that was then removed. The panel was not opened on screen, and keyboard and VoiceOver behavior was not exercised interactively.
- **Automatic transmissions that carry no meeting content:** connection checks when a provider panel opens or saves; language discovery when a language picker appears (RunPod runs a `runsync` capabilities job); token refresh. Each is logged, and the Credentials row covers them.
- **Future capabilities** add a `PrivacyRoute`. An on-device provider adds a route with no receivers, so its data stays local.
