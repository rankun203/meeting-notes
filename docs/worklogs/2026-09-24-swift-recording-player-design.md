---
date: 2026-09-24
task: native-recording-and-player-design
status: implemented-tested-and-rendered
---

## Problem

Recording was a set of toolbar controls and status strings, with source choices hidden in Settings. Playback lived inside a meeting detail, so navigation interrupted listening. The user requested image-generated exploration, an intentional recording workflow, Apple Music-like persistent playback, and delegated implementation.

## Implemented solution

Three agents handled shared playback, real recording meters/setup, and meeting detail; root integrated the app shell, persistent player, recording strip, commands, settings locks, and validation. The built-in image generation tool produced the [two-state concept](../design/macos-recording/recording-and-player-concept.png); its [full prompt](../design/macos-recording/imagegen-prompt.md) is retained. The concept is a design reference, not a runtime asset. Product views use SwiftUI controls and SF Symbols.

The flow is New Recording → title/sources/options → native consent → recording and notes → Stop & Save. Setup never requests access merely by appearing. Secondary commands move to menus. The active card shows real meters and one primary completion action; when browsing elsewhere, a compact recording strip provides a return action and Stop & Save. Saving is a distinct indeterminate state.

One app-owned player survives view navigation. Its bottom bar exposes the meeting, play/pause, 15-second skips, scrubber/timestamps, playback speed, and source selection. Clicking its title returns to the playing meeting. Detail selection never changes audio. Recording state synchronously pauses/blocks playback through published-state subscriptions, and ending recording does not resume it unexpectedly. Deleting or changing the active audio releases the old player item. Settings cannot change sources/processing/format during startup, capture, or saving.

## Reasoning

The concept's useful hierarchy was adopted without its illustrative meeting data, invented tag counts, or rich-text toolbar. Native semantic colors, text, controls, focus behavior, and accessible transport/meter labels preserve macOS conventions. No random waveform or fake progress stands in for audio activity. HIG principles are cited beside their implementations: [playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio), [feedback](https://developer.apple.com/design/human-interface-guidelines/feedback), [toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars), and [sheets](https://developer.apple.com/design/human-interface-guidelines/sheets).

## Technical debt

Playback's retained PCM decode/mix-gain limitations and missing relaunch/media-key state are recorded in the [playback worklog](2026-09-24-swift-persistent-playback.md). The existing cross-process library-writer risk remains documented in the earlier voice-processing worklog. No new data schema or runtime image dependency was added. Live native UI validation is temporarily unavailable because the computer-use native pipe fails to start; fixture renders and test results must not be represented as an end-to-end interactive check.

## Validation

All 45 tests in 15 suites passed in the final integrated run, including generated-signal meter calculations, silent-file playback lifecycle/race checks, and retry after failed preparation. The final Command Line Tools release build and ad-hoc signature verification succeeded. A compiler warning about converting the preparation function to a sendable closure was corrected with an explicit capture-free closure.

Offscreen actual-view renders covered recording setup, 600/760-point details, a 900-point persistent player, and the minimum-size 440×550 detail in light/dark appearance. That narrow check found truncated meter headings; a `ViewThatFits` stacked-status fallback fixed them and was re-rendered successfully, preserving a scrollable Notes editor. Selected fixture renders and their limitations are saved in the [design directory](../design/macos-recording/README.md). Native UI automation failed twice, including after resetting its connection, so no interactive behavior/VoiceOver verification is claimed.

The previous installer app copy remained running. Root requested that the user quit it before refreshing that bundle; the updated signed app is already available under `.build/macos/Gday Meetings Swift.app`. No live microphone capture, real meeting playback, or cloud upload was performed during this redesign.
