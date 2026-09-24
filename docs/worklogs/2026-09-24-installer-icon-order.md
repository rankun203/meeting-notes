---
date: 2026-09-24
title: Polish the Finder installer layout
status: completed
---

## Problem

Finder's alphabetical arrangement placed Applications to the left of Gday Meetings. The installer also lacked drag instructions, automatic app preview selection, and enough height for version information.

## Implemented solution

Updated `apps/client-macos-rust/scripts/open-installer.applescript` with explicit left-to-right icon positions, Snap to Grid, a 640 × 504 point window, and app selection using Finder's `select` command so its preview refreshes. Existing windows for this installer are closed before reopening to prevent stale layouts.

Added a 3× TIFF background with a directional arrow and installation instructions, plus a reproducible Pillow renderer. The install script stages the artwork beside the installer folder so it does not appear alongside the two installation icons, even with hidden files visible.

## Reasoning

Explicit positions preserve the familiar left-to-right drag-to-install layout; Snap to Grid permits later manual rearrangement without alphabetical sorting. TIFF resolution metadata preserves a 400 × 472 point canvas while rendering text sharply on Retina displays. Reopening after setting the background avoids Finder displaying its previous cached image. The preview pane uses Finder's existing visibility preference.

## Technical debt

Finder restores folder settings asynchronously, so the script waits 0.5 seconds after opening each window. This bounded timing workaround adds one second to opening and may be insufficient on unusually slow systems; replace it with a reliable readiness signal if Finder exposes one or increase/retry the wait if delayed restoration is observed.

## Notes

Compiled and executed the AppleScript on macOS, checked the shell script with `bash -n`, regenerated the TIFF with `uv run --no-project --with pillow`, and inspected the resulting Finder window. Confirmed app on the left, Applications on the right, sharp background instructions and arrow, automatic selection, and visible preview version. Initial sandbox compilation could not load Finder's dictionary; compilation and execution succeeded outside the sandbox. No application rebuild was necessary.
