---
date: 2026-09-25
title: Make full and UI Preview modes discoverable
status: complete
---

## Problem

UI Preview existed, but the client README and Make help only exposed full-app builds and launches. The README also presented the library-directory override as suitable UI isolation despite its remaining server credential and hardware/service access.

## Implemented solution

Added build-macos-preview and start-macos-preview targets. Documented both modes together in the client README, including commands, bundle paths, data/credential/audio/network behavior, visible preview controls, Xcode launch arguments, and rebuild constraints. Linked the detailed preview/signing guide and corrected the directory-override description.

## Reasoning

Explicit target names make the mode clear before launching. Existing full-app commands retain their behavior; Preview reuses the existing packaging script. Full-app installation remains separate from preview launch.

## Validation

Passed Make dry runs for all four build/start targets, inspected make help, reviewed the complete diff, and passed git diff --check. No app was launched or rebuilt for this documentation/command-wiring change.

## Technical debt

Retained: preview packaging rebuilds the full development bundle before copying it. This avoids duplicate packaging logic, but requires that development copy to be stopped even when only Preview is wanted. The README documents this limitation. Future remediation: parameterize shared packaging so each mode writes directly to its own bundle.
