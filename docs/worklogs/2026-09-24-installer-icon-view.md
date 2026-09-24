---
date: 2026-09-24
title: Open the native installer in Finder icon view
status: completed
---

## Problem

Finder's default column view could obscure the staged app and Applications shortcut when opening the installer folder.

## Implemented solution

Added a Finder AppleScript that opens a new window targeted directly at the installer folder, selects icon view, hides the toolbar, sets a compact window size and uses 96-pixel icons arranged by name. The install script passes the path as an argument, without interpolating it into AppleScript source. If automation fails or is declined, it opens the folder normally and prints the Command-1 shortcut. Updated setup documentation.

## Reasoning

Per-window Finder scripting avoids changing the user's global view preference or modifying unrelated existing windows. A normal-open fallback keeps a denied automation prompt from invalidating a successful app build.

## Technical debt

None.

## Notes

Shell syntax and AppleScript compilation passed. Ran the helper against a temporary installer directory and confirmed Finder reported icon view with its toolbar hidden; the accessibility tree also identified icon view. A separate Applications window retained column view. No application was launched or running app stopped by validation.
