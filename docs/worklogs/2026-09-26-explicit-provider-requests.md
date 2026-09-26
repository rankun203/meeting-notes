---
title: Explicit provider requests and model list
date: 2026-09-26
status: implemented; not yet checked on screen in UI Preview
scope: swift-app-and-protocols
---

## Problem

The Data Privacy audit found two automatic request paths in the Swift client. Both were confirmed in code:

- **Language discovery on picker display.** `MeetingLanguagePicker` ran `.task(id:)` → `loadProviderLanguages` whenever a picker appeared (New Recording, meeting detail, Settings → Recording). For RunPod this posts `runsync` with the `capabilities` operation, which starts a serverless job and can incur charges. The in-memory cache expired after five minutes, so the job could repeat after every relaunch or once the cache expired.
- **Connection checks on panel open, including disabled providers.** `ServiceProviderPanel` ran `.task { startCheck() }` and rechecked whenever the Settings window became key. `startCheck` did not consider `isEnabled`.

A follow-up request asked for an OpenAI-compatible **Model** menu filled from `GET {endpoint}/models`. It also refined the rule: free, credential-only metadata may run automatically for enabled providers, billable work needs an explicit action, and disabled providers are never contacted automatically, except to list models while their panel is being edited.

## Implemented solution

Code is under `apps/client-macos-swift/Sources/GdayMeetings/`.

- **Saved metadata.** New `Services/ProviderMetadataCache.swift` is a generic per-provider cache: provider ID, a SHA-256 fingerprint of non-secret configuration, the value, and `fetchedAt`. It is stored as JSON in the library folder (`provider-languages.json` and `provider-models.json`) and entries for removed providers are pruned on write. `MeetingStore` gains two lazy properties and loses `providerLanguageFetchedAt`.
- **Languages.** `ProviderLanguageIdentity` now hashes kind, endpoint, and model; the API key, enablement, and website account are excluded. `languageState(for:)` only reads the saved list, plus transient loading or failure. `loadProviderLanguages(force:)` becomes `refreshProviderLanguages`, called only by **Load Languages**. `validateTranscriptionLanguage` uses the saved list and loads one only when none exists; that happens inside **Transcribe**, which already starts provider work. The five-minute expiry is gone.
- **Picker.** The `.task` is removed. The information popover shows state and **Load Languages**:
  - No list: “Languages for *Provider* aren’t loaded.”
  - Loaded: “Languages updated *date*.”
  - Failed: “Languages Unavailable” followed by the provider's message. The old **Retry** button is replaced by **Load Languages**, so the same action has one label.
  - Provider cannot load: “Turn on Transcription for *Provider* in Settings → Service Providers to load its languages.” or “*Provider* is disabled. Turn it on in Settings → Service Providers to load its languages.”
  - RunPod adds: “Loading languages starts a short RunPod job. RunPod charges apply.”
- **Provider panel.**
  - A **Transcription Languages** section appears for RunPod and website providers, with a status line and **Load Languages**. Status lines: “Not Loaded”, “Loading Languages…”, “*n* languages · Updated *date*”, “Save to load languages.”, “Turn on Enable This Provider to load languages.”, and “Turn on Transcription to load languages.”
  - Automatic checks on open and window focus remain, because they are free and send only credentials. `startCheck` now returns without a request for a disabled provider and shows “Not checked. This provider is disabled.” **Check Connection** is disabled for disabled providers. `ProviderConnectionChecker.check` rejects disabled providers before building a request: “Turn on Enable This Provider to check its connection.”
  - The information popover now reads: “Enabled providers are checked when this panel opens, when you save, and when you choose Check Connection. Checks send credentials, not recordings or meeting text.”
- **Model menu.** New `Services/ProviderModels.swift`:
  - `ProviderModelList` builds the `GET /models` request (logged as “model list”), parses `data[].id` and an optional `name`, skips malformed or duplicate entries, sorts the list, and filters it by ID or name.
  - `ProviderModelListPolicy` decides between none, immediate (an enabled provider's saved configuration), and debounced by 0.8 s (endpoint or key edited). It requires a key and a valid endpoint.
  - The connection check uses the same parser.
  - New `UI/ModelComboBox.swift` wraps `NSComboBox` in data-source mode. Typing filters its menu, and any typed name is kept.
  - The panel shows the saved list immediately, then refreshes in a `.task(id:)` keyed by endpoint, key, and enablement, so typing cancels the pending request. Captions: “Loading Models…”, “Couldn’t load the model list. *reason* Type a model name instead.”, “*model* (not listed)”, and “Enter the endpoint URL and API key to choose from the provider’s models.”
- **Data Privacy.** `PrivacyTrigger.authenticate` is replaced by `.openProvider` (“open the provider in Settings”), `.editProvider` (“edit the provider in Settings”, disabled OpenAI-compatible providers only), and `.loadLanguages` (“choose Load Languages”).
  - Credentials rows merge each provider's content triggers with these. Example: “Sent to RunPod (api.runpod.ai) to authenticate when you transcribe a meeting, open the provider in Settings, or choose Load Languages”.
  - Disabled RunPod, Filedrop, and website providers no longer appear.
  - Joins use a comma when an action already contains “or”.
- **Docs.** The protocol index states the automatic-request rule. The transcription protocol replaces the five-minute client cache with the saved list and explicit **Load Languages**. The summarization protocol describes the model menu. README (language, model, connection checks, and the privacy table) and UI_PREVIEW were updated.
- **Tests.** Coverage added or updated:
  - `ProviderLanguageTests`: saved lists survive a new store; reading state and validating with a saved list start no job; a key change keeps the list; endpoint and model changes invalidate it; Transcribe loads once when none is saved; the RunPod charge note.
  - New `ProviderModelTests`: parsing and fallback, request shape, filtering, listing policy (including disabled providers), cache persistence, pruning, and no endpoint or key in the file; disabled providers of every kind are rejected before any request.
  - `DataPrivacyTests`: new credential wording, and disabled providers.

## Reasoning

- **Explicit trigger for languages: Load Languages, not Save or Check Connection.** Save and Check Connection are understood as free checks. Attaching a billable RunPod job to either would surprise people. A separate button next to a charge note keeps the cost visible. The button appears in the panel and in each picker's popover, so a person who sees an empty picker can act without leaving the task. Website lists use the same action for consistency. Their discovery is free, so an automatic load would be allowed, but it would add a second behavior and another Data Privacy trigger.
- **Transcribe may load a missing list.** Transcribe is an explicit action that submits a billable job. Refusing it until languages are loaded would add a step without reducing surprise.
- **RunPod health for connection checks.** The check already uses `GET {endpoint}/health`. It returns queue and worker counts, starts no worker, and is not billed. Only language discovery needs a job, because the worker computes its list.
- **Checks on open kept for enabled providers.** Under the refined rule they are free and credential-only, and the panel popover and Data Privacy now describe them. Disabled providers are filtered in both `startCheck` and the checker, so neither a UI path nor a future caller can contact them.
- **Fingerprints exclude credentials.** The key does not change a worker's language or model list. Leaving it out avoids storing a derived secret on disk and keeps a list after key rotation.
- **NSComboBox for the model field.** It is the native macOS control for “type a value or choose one”, keeps free text for endpoints without `/models`, and handles arrow keys, Return, and Escape. The data-source mode filters by substring, which prefix completion cannot do for IDs like `openai/gpt-4o`. A SwiftUI popover with a search field and list was rejected: it needs custom focus and arrow-key handling, and it separates typing from the field that holds the value.
- **Rejected:** keeping language lists in `AppSettings`. That would change `Models.swift` and `settings.json`, both of which are being edited concurrently, and would mix fetched data with preferences.

## Technical debt

- **Menu does not open while typing.** `NSComboBox` has no public API to open its menu from code. Typing filters the menu, and the person opens it with the arrow button or the arrow keys. Opening it automatically would require a private selector. Remediation: if testing shows the filter is hard to discover, replace the field with a custom `NSTextField` and an `NSPopover` list.
- **A saved language list can be outdated.** Lists no longer expire. A worker redeployed with fewer languages is detected only when its job fails or the person chooses **Load Languages**. This was accepted to avoid unrequested RunPod jobs. Remediation: show the list's date more prominently if outdated lists cause failures, or use a free worker metadata endpoint if RunPod adds one.
- **Duplicated eligibility rules (retained).** `DataPrivacy.routes` restates the checks in `startCheck`, `ProviderModelListPolicy`, and the language panel. Remediation is unchanged from the Data Privacy worklog: shared `canSend` predicates.

## Notes

- **Read-only libraries.** `ProviderMetadataCache` takes a `canWrite` closure; `MeetingStore` passes `canSave`. With a read-only library, such as one saved by a newer app version, fetched lists stay in memory for the session and no cache file is written. `readOnlyLibraryGainsNoCacheFile` covers this.
- **Concurrent edits.** Only three stored-property lines in `MeetingStore.swift` were changed. Capture, archive, `Models.swift`, and library UI files were not touched.
- **Validation from the repository root.** `make format-macos`, `make lint-macos`, `make test-macos` (183 tests passed; the baseline was 164, and other agents' concurrent tests are included), and `make build-macos-preview` passed. No Swift compiler warnings appeared; the linker search-path warnings existed before this change.
- **Not validated.** No real network requests were made, and nothing was launched on screen. The combo box's keyboard behavior, filtering, and layout in the grouped Form, and the picker popover layout, still need checking in UI Preview.
