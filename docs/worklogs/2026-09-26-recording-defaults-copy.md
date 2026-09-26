---
title: Clarify recording defaults and permissions
date: 2026-09-26
status: complete
scope: client-macos-swift
---

**Problem:** Recording settings did not identify the audio choices as defaults. Permission wording emphasized system audio, format help exposed capture internals, and labels differed from New Recording. Starting a recording also replaced saved defaults with that session’s choices.

**Implemented solution:** Added a concise explanation of defaults, stated that microphone and system audio each require permission, and shortened format help to describe saved audio and recovery. Matched source names, Audio Format, format choices, and Recording Options across both screens. New Recording passes source and format choices directly to capture; it no longer saves them as preferences. Capture snapshots source choices before asynchronous startup and retains the session format for finalization. Updated the no-source error to refer to New Recording.

**Reasoning:** Settings controls future defaults; a recording’s choices apply to that session. Shared labels and direct permission wording make the relationship clear without explaining PCM storage. The automatic-transcription preference remains under After Recording and keeps its existing behavior.

**Technical debt:** None introduced. Existing output-route classification limits and toolchain warnings remain documented in the preceding voice-processing worklog.

**Notes:** Reviewed the changed and surrounding wording against `docs/writing.md`. Formatting and lint passed. All 92 tests in 25 suites passed, including a regression test that overrides both sources and the format, fails before requesting permissions, and verifies in-memory and persisted defaults remain unchanged. An initial version of the test accessed private state; removed that assertion and reran successfully. CLT linker warnings for missing framework/library search paths remain. Checked Settings in system/light appearance and New Recording in dark appearance using the separate `/tmp/Gday Route Preview.app`; the other agent’s Preview app was untouched. No real capture or permission prompts were invoked. Full window-size and accessibility-preference matrices remain untested.

A blank floating panel appeared during the dark-preview check and in the user’s screenshot. The sheet defines no popup at that location. Reopening the separate Preview and focusing the title did not reproduce it. Its source remains unidentified; no popup fix is claimed.
