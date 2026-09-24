---
date: 2026-09-24
title: Document and diagnose native client build prerequisites
status: completed
---

## Problem

Clone-to-install documentation omitted CMake and minimum OS/SDK requirements. Build scripts mostly relied on raw tool failures, with no unified preflight or stage-specific remediation.

## Implemented solution

- Added `make doctor` and a standalone doctor script. Build/start/install automatically check macOS 14.2+, SDK 14.2+, usable Git/Make/Clang/Rust/Cargo/CMake, and required system packaging tools.
- Added shared stage diagnostics preserving original output and exit status for compilation, signing, verification, installer copying and Finder opening. Remaining shell failures report script/line; early-launch failures point to persistent logs, desktop-session requirements and port selection.
- Documented clone-to-install instructions, build versus runtime dependencies, ad-hoc signing without developer credentials, optional ffmpeg for media import, runtime permissions and troubleshooting. Linked from root and client READMEs.
- Set a default CMake policy floor of 3.5 for the bundled Opus project when building with CMake 4; explicit user overrides remain respected.

## Reasoning

Fail early with actionable instructions, retain native diagnostics, and avoid installing software or changing system settings automatically. Keep the native client setup independent of server and worker dependencies. No Rust toolchain version pin was introduced: stable is the documented supported setup, with Cargo reporting dependency-specific minimum compiler requirements.

## Technical debt

The bundled Opus dependency still declares CMake policy version 3.1. The environment policy floor bridges CMake 4 compatibility without vendoring/patching registry sources. It affects native CMake children of the build; remove it after upgrading the dependency to declare a supported minimum policy version itself.

## Notes

Validated shell syntax, real `make doctor`, simulated missing Cargo/CMake with aggregated guidance, and failed-stage diagnostic/exit-code preservation. Release bundle build, plist validation, ad-hoc signing and strict verification passed in `/private/tmp/gday-setup-build`. Fresh bundled Opus configuration succeeded with CMake 4.4.2 and the policy floor. Existing installed/running client was not replaced or restarted. This validates the current Apple Silicon host, not a clean Intel machine or every supported macOS release.
