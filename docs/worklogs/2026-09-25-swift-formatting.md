---
date: 2026-09-25
title: Enforce shared Swift formatting
status: implemented
---

## Problem

Swift files used inconsistent wrapping and many semicolon-separated statements. No shared formatter configuration, development command, or CI formatting check existed.

## Implemented solution

Added an explicit `.swift-format` configuration with four-space indentation and a 120-column target. `make format-macos` applies formatting; `make lint-macos` checks it with strict failure status. Both use Apple's installed swift-format through xcrun, target only the manifest/Sources/Tests, and leave C and vendored libraries alone. A macOS GitHub Actions workflow checks changes on master and pull requests. README and agent instructions describe the workflow. Reformatted Swift code separately from functional development.

## Reasoning

Use the Apple toolchain already installed by developers instead of adding Homebrew or another formatter dependency. Keep formatting out of normal build/install commands. Disable rules that impose API naming, access-control, or structural refactoring policies; enforce layout without broadening this task into behavioral changes. A blank line protects the package manifest's first-line tools-version directive from import sorting.

## Technical debt

CI uses the macos-26 runner's selected Apple toolchain; hosted images and local toolchains can evolve independently. The configuration explicitly records current rules, but future formatter changes can still affect output. CI prints tool versions; align toolchains and review future baseline changes in dedicated formatting commits when needed. C formatting is not automated in this Swift-only workflow; a future C formatter must preserve the CLT-only installation requirement and leave upstream archives untouched.

## Validation

Follow-up: removed `.github/workflows/swift-format.yml` at the user's request and updated README. Local formatting commands and shared rules remain. Release/build CI is being discussed separately; no release workflow was added.

Passed: `make format-macos`, `make lint-macos`, repeat-run diff comparison (identical), shell syntax, workflow YAML parsing, and `git diff --check`. A temporary badly spaced Swift fixture caused the lint command to fail as expected and was removed. All 67 regression tests in 19 suites passed after formatting. Initial import sorting moved the package tools directive; separating it with a blank line fixed the manifest and subsequent runs preserve it.

Reviewed token differences across all 51 changed Swift files: layout, import ordering, statement separation, trailing commas, numeric separators, splitting multi-variable declarations, and redundant-parenthesis removal. One block comment became a line comment. No functional changes or C changes intended. The existing CLT linker search-path warnings remain as documented in the keychain/deprecations worklog; no new deprecation warnings. CI is configured but has not yet run on GitHub. UI screenshots were not repeated for source-formatting-only changes.
