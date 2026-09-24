---
date: 2026-09-24
title: Owned-domain Rust client identity and exclusive library migration
status: complete
---

## Problem

The Rust client still used the historical `org.rankun.meeting-notes` bundle and library identity. The user owns `gdaymeetings.com` and requested owned-domain identities, with the Swift app at `com.gdaymeetings.macos`. The clients have incompatible local schemas and must not share a default library.

## Implemented solution

- Rust bundle ID and Core Audio aggregate-device prefix now use `com.gdaymeetings.macos.rust`.
- The default library is `~/.local/share/com.gdaymeetings.macos.rust`.
- `src/data_dir.rs` detects the historical Rust location only when the destination is absent and moves the entire directory with Darwin's atomic `renamex_np(RENAME_EXCL)`. Existing destinations, including dangling symlinks, are never replaced or merged. A legacy symlink is rejected with an explicit `--data-dir` recovery instruction.
- Migration is a **move**: contents are preserved, but the historical pathname stops existing after success. This differs from the Swift client's copy/import path. Users must quit older Rust app versions first; running old versions can continue addressing the old pathname.
- Explicit `--data-dir` continues to bypass default-path migration.
- The Teams transcript helper defaults to the new identity and falls back to the historical location only before migration, avoiding an accidentally empty new library. Updated its usage documentation, the Rust README, and the repository's Teams-import reference.

## Reasoning

An exclusive same-directory rename preserves all recordings, credentials, settings, and sidecars without a costly recursive copy. The kernel enforces nonreplacement even if another process creates the destination after the initial existence check. Keeping `.rust` separate prevents either client from interpreting the other's schema. Historical strings remain only as migration/fallback constants and explanatory documentation.

## Technical debt

- Automatic migration is macOS-specific because it relies on Darwin's exclusive rename primitive. Other platforms with an old library receive an actionable error and can explicitly choose the old directory. This is accepted for the macOS client; add an equivalent platform-specific atomic nonreplacement primitive if cross-platform default-path migration becomes supported.
- Existing macOS microphone/system-audio grants belong to the old bundle identifier and must be granted again. No attempt is made to edit macOS privacy databases. Installed old bundles are not removed automatically; users must quit them before migration and replace the app through the normal installer workflow.
- The transcript helper retains a read/write historical fallback until the Rust app performs migration. This prevents split libraries before first app launch, but means an old path may still receive imports during that transition. Remove the fallback only after a documented migration window, or expose an explicit migration command shared by helpers.

## Validation

- `cargo test --manifest-path apps/client-macos-rust/Cargo.toml data_dir::tests --lib`: five tests passed, covering fresh path selection, complete migration, existing destination preservation, symlink behavior, and kernel-level nonreplacement of an empty destination.
- Standalone `rustc --test src/data_dir.rs`: the same five migration tests passed.
- Four Python helper path-selection checks passed using `uv run --no-project`.
- Rust Info.plist passed `plutil -lint`.
- Root agent coordinates the shared-index commit after complete diff review.
- `cargo check --manifest-path apps/client-macos-rust/Cargo.toml --bin gday-meetings-client`: passed.
- Server settings placeholder now uses `https://gdaymeetings.com`; `node --check` passed for the edited module.
