---
date: 2026-09-25
title: Compact menu bar actions and Option recording setup
status: implemented
---

## Problem

The menu repeated the app name in Show and exposed recording setup as a separate action; actions lacked icons.

## Implemented solution

RecordingMenuView uses SF Symbol labels for recording, Show app, and Quit. Removed the separate setup row. On macOS 15+, the native modifierKeyAlternate API replaces Start Recording with New Recording… while Option is held, opening the existing setup sheet. Normal activation starts immediately; Stop keeps its existing behavior. Busy/start/save guards also cover the alternate.

## Reasoning

Use Apple's native alternate menu behavior and semantic labels, preserving native layout and keyboard access. SDK availability confirms modifierKeyAlternate requires macOS 15, while the app supports 14.2.

## Technical debt

Retained compatibility bridge: macOS 14 checks NSEvent.modifierFlags at activation to support Option-click but cannot display the alternate title. Remove this fallback when the minimum OS reaches 15; macOS 14 runtime validation remains outstanding.

## Validation

Passed: final full/preview build, bundle signing/verification, and git diff --check. Preview launches with its isolated synthetic library. The UI automation surface exposes the main window and application menus, but not the status menu; keyboard focus did not expose it either. Menu icon rendering, Option alternate activation, light/dark menu appearance, and macOS 14 fallback remain unverified at runtime. No real capture started. Existing two CLT linker search-path warnings remain; cause and remediation are documented in 2026-09-25-swift-keychain-deprecations.md. No deprecated API introduced.
