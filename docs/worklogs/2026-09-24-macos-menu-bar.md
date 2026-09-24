---
date: 2026-09-24
title: Native menu bar for the Rust macOS client
status: implemented
---

**Problem:** The background app had no visible lifecycle controls; reopening the UI or quitting required a terminal or Activity Monitor.

**Implemented solution:** Added `src/desktop.rs` with a native waveform status item and Open Gday Meetings, status, Show Logs and Quit actions. Bundle launches run a macOS accessory event loop on the main thread, with the existing server/recording runtime in the background. Open uses the actual bound URL; recording counts refresh each second and add a dot to the icon. Quit cancels through the existing audio-finalization and connection-drain shutdown path, disables repeat actions while stopping, and exits the menu loop after the backend finishes. Ordinary CLI execution remains terminal-only. Shared log-directory resolution supports Show Logs, including configured overrides.

**Reasoning:** macOS-only tray-icon/winit dependencies provide native menus and main-thread event delivery without a new app/window or duplicated recording logic. Menu creation happens after event-loop startup. Existing Ctrl+C/SIGTERM behavior remains; backend panics terminate the shell rather than leaving a stale menu running.

**Technical debt:** Single-instance/reopen routing remains deferred; a future library lock should redirect repeated launches to the existing app. The original audio bindings and winit use objc2 0.5 while tray-icon uses 0.6; both remain isolated behind their own wrappers. Consolidate versions when upstream event-loop bindings align, instead of mixing object types. Status polling can lag by one second; replace with recording lifecycle notifications if more immediate feedback is needed. Existing shutdown timeouts remain and may force exit if audio/request finalization hangs.

**Notes:** Cargo check, release bundle build and signature/plist checks passed. 47 client tests passed; one external-fixture test ignored. New integration test verifies a real assigned URL, HTTP readiness, cancellation-triggered stopping and listener closure. LaunchServices ran the bundle at port 54464 with `/private/tmp/gday-menubar-smoke`; that isolated process was stopped gracefully. Native UI automation repeatedly timed out when selecting the windowless app, so visual menu/click verification remains unconfirmed. The user's recording client at port 33487 must remain untouched; future tests must use separate ports and data directories. No installation, deployment or release performed.
