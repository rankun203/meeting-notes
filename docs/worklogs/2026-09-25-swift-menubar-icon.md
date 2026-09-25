---
date: 2026-09-25
title: Match the existing menu bar waveform
status: implemented
---

## Problem

The Swift client's SF Symbol waveform appeared smaller than the existing Rust client's custom menu bar icon.

## Implemented solution

Use the same 18-point canvas, five bar positions, widths and heights as desktop.rs waveform_icon(). Draw using NSImage's drawing handler for resolution-independent rendering and mark it as a template so the system controls appearance. Keep the active recording indicator and accessible labels.

## Reasoning

Reuse the user's preferred artwork rather than approximating it by scaling a different symbol. Retain MenuBarExtra and standard template-image behavior, consistent with [Apple menu bar guidance](https://developer.apple.com/design/human-interface-guidelines/the-menu-bar).

## Validation

Full app build passed. Geometry matches the existing icon source exactly. Menu bar pixel comparison and active-recording appearance were not tested. Existing Keychain ACL deprecations and toolchain linker-path warnings remain as recorded in the list-insets worklog; this change adds no deprecated API.

## Technical debt

The five-bar geometry is duplicated across Rust and Swift to avoid adding a resource pipeline for a tiny vector. If the artwork changes, update both implementations; a shared vector asset would remove this duplication when branding is consolidated.
