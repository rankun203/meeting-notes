---
title: Merge Transcription and Summaries settings into Defaults
date: 2026-09-26
status: complete
scope: client-macos-swift
---

**Problem:** Settings had separate **Transcription** and **Summaries** tabs, each holding one provider picker and little else. More per-capability defaults, such as Live Transcription, would each add another sparse tab.

**Implemented solution:** Replaced both tabs with one **Defaults** tab (`slider.horizontal.3`), placed after **Service Providers** because its pickers list providers configured there. The order is Recording, Service Providers, Defaults, Data Privacy. `UI/DefaultsSettingsView.swift` holds a grouped form with one `CapabilityDefaultSection` per capability: Transcription (provider and its caption) and Summaries (provider, **Summary Instructions**, and its caption). A section takes the capability, its `AppSettings` key path, a caption, and optional extra controls, so adding Live Transcription is one more section. When no provider qualifies, a section shows “Add a provider and turn on <capability> to choose it here.” with **Open Service Providers**; the picker stays while a saved choice exists so it can be cleared. Setting keys are unchanged.

Error messages now point to **Settings → Defaults**. **Set Up Transcription…** opens Defaults when a qualifying provider exists but none is chosen, and Service Providers otherwise. A saved `settingsTab` value of `transcription` or `summaries` opens Defaults. Updated the Swift README and the meeting-experience design. Added a test for the summary-provider error and tightened the transcription-provider error test.

**Reasoning:** **Automatically Transcribe Recordings** and **Default Language** stay under Recording → Transcription, because they were never in the Transcription tab and describe new recordings rather than a provider choice. A generic section view was chosen over a data table of capabilities because Summaries needs an extra control; a `@ViewBuilder` slot handles that without special cases.

**Technical debt:** The mapping of the legacy `transcription` and `summaries` tab values is a compatibility bridge for a saved UI selection. Without it, people whose last tab was one of those would open Settings with no tab shown. Remove it after a release in which those values can no longer be stored. Tab tags remain string literals shared by three views, as before.

**Notes:** `make format-macos`, `make lint-macos`, `make test-macos` (187 tests in 37 suites), and `make build-macos-preview` passed. The build kept the existing Command Line Tools linker search-path warnings. The Defaults form was checked with offscreen `NSHostingView` renders in Light and Dark appearance, with and without qualifying providers, using a temporary test that was then deleted. Offscreen rendering does not draw the Settings toolbar tabs, so the tab label, symbol, and keyboard navigation were not checked on screen. `DataPrivacy.swift` and `ServiceProvidersView.swift` contained no references to the old tabs and were not changed.
