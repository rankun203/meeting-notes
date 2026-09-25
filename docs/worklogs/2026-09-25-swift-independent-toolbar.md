---
date: 2026-09-25
title: Independent toolbar and completed sidebar reveal
status: implemented
---

## Problem

NavigationSplitView still moved toolbar/title content as its sidebar animated. The 400 ms row delay did not reliably hide early layout changes. The user requested a fixed toolbar, rows shown only after full expansion, top row spacing, and a focused empty selection action.

## Implemented solution

Separated the sidebar animation from the window toolbar: a fixed-width sidebar viewport sits beside native HSplitView list/detail columns. Toolbar toggle and destination title belong to the window, so column changes cannot reposition them. Rows remain mounted at 180 points and hidden during motion; withAnimation's removed completion reveals them only after the owned width animation completes. A generation token ignores stale completions after interrupted toggles. Reduce Motion skips animation. Added 10 points above the sidebar list.

The empty detail now centers a large record.circle.fill action with an accessible New Recording label and one short instruction. Removed the empty-detail import action, redundant heading, and waveform decoration. Import remains available in the toolbar/menu.

## Reasoning

The user explicitly wants independent chrome rather than NavigationSplitView's integrated toolbar behavior. Public [withAnimation completion](https://developer.apple.com/documentation/swiftui/withAnimation(_:completionCriteria:_:completion:)) replaces the timer workaround. Native [HSplitView](https://developer.apple.com/documentation/swiftui/hsplitview) retains adjustable list/detail columns. [HIG Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars) informs familiar labelled navigation and its toggle; [HIG Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons) informs a recognizable icon with an accessibility label and help.

## Validation

Passed: 61 automated tests and preview packaging. Native UI Preview checks confirmed identical toolbar toggle/title positions with the sidebar open and closed, restored rows after repeated keyboard toggles, 10-point top/left row spacing, People/Meetings navigation, and the recording icon opening setup (then cancelled). Dark and System/light appearances rendered consistently without changing windows. Preview remains paused in System appearance; real recordings were untouched.

Untested: frame-by-frame animation capture, Reduce Motion on a changed system setting, and live pointer dragging. The reveal now follows the owned animation completion rather than a guessed delay; settled screenshots alone do not prove every intermediate frame. No failures observed in the checks above.

## Technical debt

The sidebar is now fixed at 180 points rather than user-resizable; list/detail sizing remains native. Accepted to keep stable sidebar row geometry and own the exact animation lifecycle. If wider/localized labels require it, add a persisted adjustable sidebar width with the same completion-driven reveal. The custom window-level toggle retains Command-Control-S but replaces the default SidebarCommands menu contribution. Add a focused scene command if a View-menu entry is needed. No remaining timer-based reveal.
