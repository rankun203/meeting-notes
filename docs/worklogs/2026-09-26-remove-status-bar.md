---
title: Remove the persistent status bar notices
date: 2026-09-26
status: complete
scope: client-macos-swift
---

**Problem:** After a recording stopped, the main window showed “Recording saved” in a bar below the player, and the bar never went away. The bar in `LibraryView` appeared whenever `MeetingStore.statusMessage` was non-empty, and nothing cleared that text after work finished. Other tasks left similar lasting notices, such as “Summary complete,” “Imported 3 meetings,” and “Archive incomplete; local files are preserved.”

**Implemented solution:**

- `LibraryView` now shows the bar only while `store.isBusy` is true. It shows a spinner and the current progress text, such as “Transcribing with Office Server…” or “Uploading archive audio 1 of 2…”.
- `MeetingStore.isBusy` clears `statusMessage` when work ends, so old progress text cannot reappear.
- Removed success notices: “Recording,” “Recording cancelled,” “Recording saved,” “Transcription complete,” “Summary complete,” the archive-verified notice, and the meeting and track import counts. The result appears in the meeting, list, or transcript. “Saving OPUS audio…” was also removed. During saving the bar was hidden, and the recording view and strip show “Saving Recording.”
- Failures now use `errorMessage`, which appears in an alert. Each failure message says what happened and what was kept: recording stop failure (captured audio is kept), audio conversion failure (original WAV audio is kept), chat failure (the message is saved), and archive failure (local meeting and audio are kept; choose Archive to Server to resume). The removed status text for transcription and summary failures only repeated the error already shown.
- When transcription polling timed out, the app used to set a status message. It now throws, so the alert says the provider is still transcribing and tells the person to choose Resume Transcription later.
- The error alert no longer uses the generic title “Unable to Complete Action,” which `docs/writing.md` says to avoid. The error message is now the alert title. This follows the macOS `NSAlert(error:)` convention.
- Updated `docs/design/meeting-experience.md` to remove the planned “Recording saved on this Mac” confirmation.
- Added tests: progress text clears when work ends, and a failed recording finalization sets `errorMessage` and resets the recording state.

**Reasoning:** Saving is expected, so a confirmation adds no information. A failure needs attention, so an alert is the right level of urgency. Transcription, summaries, chat, and archiving can take minutes and have no other progress indicator, so the bar remains, but only while that work runs. Removing it entirely would hide long network work. `statusMessage` keeps its name to avoid churn in files another agent is editing.

**Technical debt:** None added. Successful archives no longer show a confirmation. Archive status in the meeting would be clearer than a temporary notice. Capture failures reported through `AudioCapture.onFailure` still show the capture error text as-is. Most of those messages already say that audio was saved, but the text is not checked centrally.

**Notes:** `make format-macos` and `make lint-macos` passed. `make test-macos`: all 132 tests in 31 suites passed. `make build-macos-preview` succeeded. Existing Command Line Tools linker search-path warnings remain and are unrelated. The UI was not launched because the machine was in use, so the bar, alert layout, and long alert titles have not been checked visually.

After review, the error alert uses the message's first sentence as its title and shows the rest (what was kept and technical detail) as the message. A long, multi-sentence title in bold did not follow the macOS alert pattern. `LibraryView.alertParts` performs the split; `errorAlertUsesFirstSentenceAsTitle` covers it.
