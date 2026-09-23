---
date: 2026-09-23
task: client-workspace
status: implemented
---

**Problem:** The current macOS Rust client occupied the repository root, and its web UI and server integration fixture depended on the old repository layout.

**Implemented solution:** Moved the Rust package, build script, source, tests, and embedded UI into `apps/client-macos-rust/`. The root Cargo workspace defaults to that package and keeps the existing root lockfile and target directory. Updated the embedded asset path, UI test imports, Python test command examples, and root macOS launcher. The OAuth fixture resolves `apps/server/` within the monorepo instead of requiring an external checkout. Active Settings, login, migration labels, and Rust user-facing authentication, upload, task, and migration errors now say Meeting Notes Server or server. Protocol names and synthetic test fixtures remain unchanged.

**Reasoning:** Keep `meeting-notes-daemon` as the package and binary name and preserve macOS bundle identity, persisted paths, and protocol identifiers. This avoids breaking the installed client while distinguishing the current implementation from a future `client-macos-app`. Root Cargo commands continue to select the client automatically; `scripts/run-macos.sh` selects the package explicitly.

**Technical debt:** Internal `gday_*` module, endpoint, fixture, and persisted credential names remain compatibility identifiers. Renaming them is deferred because it would require a separate protocol/storage migration; product-facing Settings labels no longer depend on those names. The optional file-drop utility remains an excluded independent Cargo package under `tools/file-drop`.

**Validation:** `cargo test --lib --locked` passed 42 tests (rerun after the final error-message branding sweep; one provider fixture test remains ignored by default); the separately invoked ignored provider test also passed against the moved in-repository server fixture. `cargo check --bins --locked` passed. UI analytics tests passed 2/2, 23 UI modules passed syntax checks, and the macOS launcher passed `bash -n`. Cargo metadata confirms the root workspace/target directory and unchanged default package identity. No production credentials, recordings, or audio devices were used.

**Commands:** From the root, run `cargo test --lib --locked`, `node --test apps/client-macos-rust/tests/analytics.test.mjs`, and `uv run --no-project apps/client-macos-rust/tests/filesystem_e2e.py`. The benchmark is now `apps/client-macos-rust/tests/filesystem_benchmark.py`. Start the optional provider fixture with `node apps/server/node_modules/tsx/dist/cli.mjs apps/client-macos-rust/tests/fixtures/gday-provider.mts`; pass its printed origin as `GDAY_TEST_ORIGIN` to the ignored Rust provider test.
