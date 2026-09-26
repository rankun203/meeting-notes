---
title: Share the macOS installer style
date: 2026-09-26
status: completed
scope: macos-packaging
---

## Problem

`make install-macos` opened a plain Swift installer folder instead of the Rust client's illustrated drag-to-Applications layout.

## Implemented solution

Moved the Finder helper, Retina TIFF, and artwork renderer to `apps/packaging/macos/`. Both install scripts use these shared files and pass their app's filename to the helper. The Swift installer now stages the background beside its folder, sets icon positions, and selects the app for Finder's preview. It falls back to opening the folder normally if automation fails. Updated both client READMEs and shortened the background instructions to follow the writing guide.

## Reasoning

Sharing the assets keeps the two installers consistent without adding build dependencies. Pillow is needed only to regenerate the checked-in artwork. Visual validation found that Finder restored the Swift folder's old settings after reopening; the helper now reapplies the layout after reopening and hides the status bar.

## Technical debt

Retained the existing two 0.5-second waits for Finder to restore folder settings. Layout is deliberately applied before and after reopening to handle cached artwork and saved view options. Slow Finder restoration may still require a retry; replace the waits and repeated application with a readiness check if Finder exposes a reliable signal.

## Notes

- Regenerated the TIFF with `uv run --no-project --with pillow`; checked both shell scripts with `bash -n` and compiled the AppleScript.
- Ran `make install-macos` successfully, including release build, staging, signing verification, and Finder opening. Inspected the live window: background, arrow, icon positions, selected Swift app, and preview match the intended layout.
- Initial sandbox attempts could not access SwiftPM services or Finder's scripting dictionary; validation succeeded outside the sandbox.
- The release link reported two missing Command Line Tools search paths: `Developer/usr/lib` and `Developer/Library/Frameworks` under `/Library/Developer/CommandLineTools`. These are toolchain path warnings, not deprecation warnings; linking and signing succeeded. Follow up by checking the selected Command Line Tools installation and Swift linker search paths. The subsequent incremental build emitted no warnings.
- Rust shell syntax and shared helper compilation passed; a full Rust rebuild and denied-automation fallback were not exercised.
