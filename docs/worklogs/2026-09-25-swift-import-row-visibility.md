---
date: 2026-09-25
title: Reveal newly imported meeting rows
---

## Problem
The first meeting appeared clipped beneath UI Preview after import. In the existing affected session, scrolling the meeting list upward restored the full row while the banner and detail stayed stationary. PreviewContainer uses a normal VStack, not a floating overlay. This supports a retained list scroll position; the framework's exact insertion/offset sequence was not instrumented.

## Implemented solution
LibraryView wraps the meeting List in ScrollViewReader and reveals the first visible newly added meeting when the store's ID collection gains entries. Selection and playback are preserved. Track additions, edits, deletions and search changes do not trigger scrolling. No dimensions, padding, insets, or preview banner layout changed.

## Reasoning
Make the insertion's intended scroll destination explicit through the public List-compatible scrolling API instead of compensating with offsets or recreating the list. Observing store IDs covers both file-picker and drop imports without duplicating import completion plumbing.

## Validation
- Observed the clipped row in the existing preview and restored it with upward list scrolling.
- `make build-macos-preview` passed; existing CommandLineTools missing library/framework search-path linker warnings remain. No new deprecated API use.
- Launched rebuilt preview, selected Synthetic conversation, imported the Downloads synthetic microphone WAV through the native picker. Screenshot confirmed full new first row below the banner, existing selection retained, playback still paused at zero.
- Finder drop, multi-file batch, long-library scrolling, appearance variants and older macOS were not revalidated in this pass. No capture, credentials, transcription or upload used.

## Technical debt
Retained: exact native insertion/scroll-offset cause remains uninstrumented, and the earlier HSplitView resize transient is separate. This change defines insertion visibility rather than replacing native list layout. If clipping recurs after a completed import, capture scroll bounds/insets and split-view frames around that event before further layout changes. Existing linker search-path warnings require toolchain configuration investigation; no suppression added.
