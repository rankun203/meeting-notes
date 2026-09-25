---
date: 2026-09-25
title: Stable sidebar background and trailing toolbar actions
status: implemented
---

## Problem

Hiding the whole sidebar List also hid its native background, causing a color flash when rows appeared. Toolbar actions sat too close to the title.

## Implemented solution

Keep the native sidebar List visible throughout expansion and hide only its row labels. Suppress its selection highlight during motion without changing the selected destination. Restore labels and selection after the existing animation completion. Put actions and a bounded search field in one toolbar group after flexible space. Search retains filtering, a clear action, an accessible label, and a Command-F focus shortcut.

## Reasoning

Preserving the native background avoids approximating its appearance with a different material. Standard toolbar spacing retains the fixed navigation controls and moves actions toward search, consistent with [Apple HIG Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars). Automatic searchable placement inserted flexible space after the actions regardless of action placement; explicit search grouping removes that unwanted gap.

## Validation

Passed: preview build, native screenshot of right-aligned actions adjacent to search, filtering to the single-track meeting and clearing to restore both meetings. Sidebar collapse/expand restored selection with the persistent native background implementation. No new tests for this presentation-only change.

Untested: frame-by-frame background consistency and reliable Command-F focus verification (the preview was inactive during attempted key input). Further final-build toggles were interrupted by native automation's user-interaction guard; stopped input to avoid contention. No audio recording or upload performed.

## Technical debt

The bounded SwiftUI search field replaces automatic searchable chrome to control grouping. This retains plain-text filtering but requires maintaining focus/clear behavior and does not inherit future native search features automatically. Consider a native NSSearchToolbarItem when adopting an explicitly managed NSToolbar. The prior fixed sidebar width and missing View-menu sidebar command remain documented in the independent-toolbar worklog.
