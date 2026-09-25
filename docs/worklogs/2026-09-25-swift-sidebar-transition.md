---
date: 2026-09-25
title: Avoid clipped sidebar reveal
status: superseded
---

## Problem

The user's 5.49-second screen recording shows the sidebar reveal clipping the left side of labels and moving toolbar content in stages around 3.9–4.2 seconds. Earlier automation observed only settled layouts and missed the transition artifact.

## Implemented solution

Superseded by [native sidebar and shared scrubbing](2026-09-25-swift-native-sidebar-and-scrubbing.md): user preferred animation and reported the custom button moving between columns. Restored the native toggle after primary-source research confirmed its correct sidebar placement.

LibraryView owns NavigationSplitView column visibility and supplies a labelled sidebar toolbar button with Command-Control-S. The button switches between all three columns and the list/detail columns using a transaction with animations disabled. This scopes the mitigation to the toggle, retaining native split columns, selection, resizing, and the persistent player.

## Reasoning

Prefer a clean immediate transition over the observed broken slide. Uses Apple's public [column visibility](https://developer.apple.com/documentation/swiftui/navigationsplitviewvisibility) and [toolbar removal](https://developer.apple.com/documentation/swiftui/view/toolbar(removing:)) APIs. The familiar sidebar symbol, descriptive label, and keyboard toggle retain the navigation affordance described in [HIG Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars). No global animation or playback-rendering changes.

## Validation

Inspected extracted frames from the user-supplied recording at 30 frames per second. `make build-macos-preview` passed. Native UI checks passed for a single labelled toggle, collapse/expand with no meeting selected, Command-Control-S in both directions with a meeting selected, and preservation of meeting selection and paused player state. Expanded and collapsed screenshots show complete layouts. No post-fix high-frame-rate recording was captured; native automation observes settled states, so frame-level verification remains limited. No unit tests added for the visual-only transition.

## Technical debt

Sidebar toggle animation is deliberately suppressed as a workaround for the observed native transition. This loses animated spatial continuity and adds a small custom toolbar action. Restore the native toggle when the supported macOS/SwiftUI versions can reveal it without clipping or toolbar jumps, verified with a frame-level recording.
