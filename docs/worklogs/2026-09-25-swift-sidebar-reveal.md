---
date: 2026-09-25
title: Consistent toolbar and delayed sidebar rows
status: implemented
---

## Problem

The sidebar material extends into the toolbar, changing its left-side color during toggling. The user wants a continuous toolbar and animated sidebar column with its rows hidden until expansion settles.

## Implemented solution

Set an explicit semantic window-background toolbar style. Retain the native animated toggle, bind its visibility, and mask sidebar list content during transitions without removing the list or changing selection. A cancellable reveal task shows rows after a 400 ms settling interval; hidden rows are also excluded from interaction and accessibility.

## Reasoning

Use public [toolbar background](https://developer.apple.com/documentation/swiftui/view/toolbarbackground(_:for:)-5ybst) and split-view visibility APIs. Semantic colors follow light/dark appearance; keeping native navigation follows [HIG Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars). Preserve the native button's position and column animation.

## Validation

`make build-macos-preview` passed. Native screenshots confirm a uniform toolbar across the sidebar boundary in light and dark appearances. Collapse and keyboard expansion restore visible, accessible rows. Frame-level reveal timing remains unverified because tool observations settle after the animation; the delay is explicitly a workaround, not a proven animation-completion signal. A repeated-toggle check was interrupted by user interaction and is not counted as passed. No unit tests added for this visual-only change.

## Technical debt

SwiftUI does not expose completion of the native split-view toolbar toggle. Row reveal uses a conservative 400 ms interval, based on the roughly 300 ms transition in the supplied video, rather than a guaranteed animation-completion event. A slow or future OS animation may exceed it; Reduce Motion still incurs the delay. Replace with an explicit AppKit animation completion bridge if testing reveals timing mismatches. Repeated toggles cancel the previous reveal task.
