---
date: 2026-09-23
title: Root Makefile and independent component packaging
status: implemented
---

## Problem

The root still contained client Cargo metadata, packaging, scripts and build output, plus a combined Docker stack that assumed colocated services. The user requested a thin Makefile, macOS build-and-launch behavior for make start, and a simple Finder installation workflow.

## Implemented solution

Moved the remaining native files into apps/client-macos-rust, made it an independent Cargo package, and delegated root Make targets to component scripts. Separated macOS build, development launch and Finder installer preparation. Bundle launches without CLI arguments now start the local UI and open the browser; explicit CLI behavior remains available. The listener binds before library loading/background recovery, so a port conflict cannot start duplicate background jobs. App versions come from Cargo metadata. Server and worker each own their Compose configuration and environment template, with no root Docker files or implicit shared network. Documentation and import-tool references follow the component paths.

Moved the old root build cache under apps/client-macos-rust/target/previous-layout without stopping its still-running executable or touching user data. Moved pnpm's generated root store under apps/server/.pnpm-store, set the component's storeDir and refreshed its frozen installation. The store is excluded from Git and Docker build contexts. Root contains only shared documentation, Makefile, repository/editor/agent configuration, apps, integrations and tools.

## Reasoning

make start builds and runs the host-native client without installing it or starting Docker. make install prepares a signed .app copy beside an Applications shortcut and opens Finder; the user completes the drag. This uses built-in macOS tools without a disk-image dependency. Server and worker commands are explicit because their deployment hosts are independent.

## Technical debt

Local builds retain an ad-hoc signature rather than a notarized distribution identity; rebuilding may require fresh recording permissions. Public binary distribution needs a signing/notarization release process. Existing generated build artifacts are preserved under the client target/previous-layout during the layout change to avoid disrupting a running older client; remove that cache after it exits to reclaim disk space. The current app still uses the browser UI and background daemon lifecycle; a native menu/quit UI belongs to the planned native frontend. Installed copies can be quit through Activity Monitor, while make stop manages the development bundle.

## Notes

No existing client was stopped, no recordings were moved, and no existing Applications installation was overwritten. A disposable test app was launched with a temporary data directory and unused port, then stopped with Ctrl+C.

Validation: Rust library tests passed 42 tests plus 3 CLI/Finder-default tests (the external provider test remains opt-in). Actual make build produced a signed ARM64 .app with Cargo version 0.2.1. make start opened the isolated UI through LaunchServices, served an empty library and shut down gracefully with Ctrl+C; a concurrent rebuild correctly refused to overwrite its running bundle. The sandbox initially returned kLSNoExecutableErr; the same launch succeeded outside the sandbox. make install opened Finder, where both the app and Applications shortcut were verified through the accessibility tree. Shell syntax, unsupported-platform handling, component Cargo metadata and four Compose configurations (server SQLite/PostgreSQL, worker CPU/GPU) passed. The moved import script's help command passed. pnpm frozen installation succeeded after store relocation; all 33 server regression tests passed. Docker images were not built or deployed.
