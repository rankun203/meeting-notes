---
title: Library format version check and meeting archive status
date: 2026-09-26
status: complete
scope: client-macos-swift
---

**Problem:**

- `MeetingStore` opened `library.json` only when `version` equaled 1. A newer library got a short, generic failure (“Could not open the local library…”), and older versions could not be migrated. The notes migration and version 1.0.0 both need a clear version contract. See [notes-markdown.md](../design/notes-markdown.md#migration).
- The status-bar cleanup removed the “Archive verified on the server” notice. After that change, the app no longer showed whether a meeting was archived. The archive checkpoint did not record verification, so a finished archive looked the same as one whose import or verification had failed.

**Implemented solution:**

- `MeetingLibrary.currentVersion` (1) and `MeetingLibrary.migrations` (empty) in `Core/Models.swift`, with a comment stating the contract.
- `Core/LibraryFormat.swift`: `MeetingLibrary.load(from:supportedVersion:migrations:)` reads only the version first. For a newer version, it throws `NewerLibraryVersionError` before the full decode, so a future layout that no longer decodes still gets the right message. It opens the current version as before. For an older version, it copies the original to `library-v<N>-backup.json` (the first backup is kept), runs each step, and returns `migrated: true`, so `MeetingStore` saves once. A version with no migration step is refused. A missing `version` key still means version 1.
- `MeetingStore`: a newer library sets `newerLibraryVersion`, keeps `canSave` false, and shows the alert **“This library was saved by a newer version of Gday Meetings.”** with the message “Install the latest version of Gday Meetings to open it. This version doesn’t change the library.” `save()` and `saveSettings()` repeat that text instead of the “restore library.json” text. Every library write path already checked `canSave` or `libraryWritable`. `save()` always writes `currentVersion`; with newer libraries refused and older ones migrated, this cannot lower the version.
- `LibraryView`: for a newer library, the empty Meetings list shows the same title and recovery text instead of “No Meetings.” **New Recording**, **Import**, and **Library Actions** are disabled whenever the library is read-only. Before, they appeared to work but saved nothing.
- `ServerArchive.swift`: `ArchiveCheckpoint` gains optional `verifiedAt`, written after server verification succeeds. Checkpoints without the key decode as before. `MeetingArchiveStatus` (`archived(host:date:)`, `incomplete(host:)`) is derived from `server-archive.json`. The store holds `archiveStatuses` in memory, loads them when the library opens, and updates them when `archiveToServer` finishes or fails. `library.json` is unchanged.
- `UI/MeetingArchiveStatusView.swift`: the meeting header shows **“Archived on meetings.example.com · 26 Sep 2026”** (`checkmark.icloud`), or **“Archive incomplete”** (`exclamationmark.icloud`) followed by an **Archive to Server** link. The link is tinted only when it can run. Its tooltip says “Resume archiving this meeting,” or “Sign in to the Gday Meetings website in Service Providers to resume” when no website is signed in. Nothing is shown for a meeting that was never archived. The meetings list shows the same symbol after the duration. VoiceOver reads “Archived on <host>, <long date>” or “Archive to <host> incomplete. Choose Archive to Server to resume.”
- UI Preview writes synthetic checkpoints for the `.invalid` host `meetings.example.invalid`: “Synthetic single track” shows as archived and “Synthetic conversation” as incomplete. Preview cannot sign in, so the resume link stays disabled.
- Docs: [notes-markdown.md](../design/notes-markdown.md) now records the implemented policy in Current state, Decisions, and Migration. The notes migration now increases the version to 2. The versioning follow-up was removed. The [Swift README](../../apps/client-macos-swift/README.md) describes the version behavior and archive labels, and [UI Preview](../../apps/client-macos-swift/docs/UI_PREVIEW.md) describes the fixtures.
- Tests (`LibraryFormatTests`, 4 tests): an unversioned file and a current file load, and saves keep the current version. A newer library whose layout can't be decoded is refused. Creating, adding people and tags, chat, settings, and imports all fail, `library.json` keeps its original bytes, and no other file appears. An older library is backed up and migrated with an injected step, and a missing step is refused. Archive status follows the checkpoint: verified, legacy without `verifiedAt`, and absent. This test also checks label text.

**Reasoning:**

- Archive status is derived from the existing checkpoint instead of a new `Meeting` field. This keeps `library.json` unchanged, so no version increase is needed. The new optional `verifiedAt` key lives in a per-meeting file that older builds decode without error.
- Reading the version before the full decode separates “newer version” from “corrupt file.” A full decode that fails would otherwise show the generic error for a legitimate newer library.
- The migration loop and backup are in place now, though no step exists yet, so the notes migration only adds a step and increases `currentVersion`. The injected-steps test covers that path now.
- The resume action in the header uses the existing label **Archive to Server**. `docs/writing.md` asks for the same label wherever the same action appears. A separate “Resume Archive” label was rejected.

**Technical debt:**

- Checkpoints written before this change have no `verifiedAt`, so a meeting archived earlier shows **Archive incomplete** until **Archive to Server** runs again. This was accepted because re-running is idempotent: the server receives the same import key and the app verifies the snapshot again. The consequence is a one-time false “incomplete” for existing archives. Remediation: none needed beyond one re-run per meeting; this app has one user before 1.0.0.
- **Archived** describes the immutable snapshot. Edits made after archiving are not reflected, and the label does not warn about them. This matches the README's “immutable snapshots” statement. Future remediation: show “Changed since archive” if server sync is added.
- Another agent's in-progress provider metadata caches (`provider-languages.json`, `provider-models.json`) write into the library folder without checking `libraryWritable`. With a newer library, settings are not loaded, so no provider fetch is expected. These files are disposable caches, not library data. That agent should decide whether they should respect the read-only state.
- The menu bar **Start Recording** command is not disabled for a read-only library. `startRecording` returns without recording, as before.

**Notes:**

- `make format-macos` and `make lint-macos` passed. `make test-macos` ran 182 tests in 37 suites, and all passed; this includes other agents' concurrent tests. At one point, other agents' in-progress `ProviderLanguageTests` did not compile. During that time, the suite ran in a scratch copy without that file (166 passed). The final run was in the real tree. `make build-macos-preview` succeeded; the only warnings were the existing Command Line Tools linker search-path warnings.
- The header and list indicator were rendered offscreen with `NSHostingView` bitmaps in a throwaway scratch test, in Light and Dark. Both states lay out on one line under the date, and the disabled resume link is dimmed. Not checked: the running app, keyboard focus on the resume link, VoiceOver output, the newer-library empty state and alert, and narrow window widths.
