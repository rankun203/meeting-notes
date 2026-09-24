#!/bin/bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/../macos-common.sh"
output="$(mktemp)"
trap 'rm -f "$output"' EXIT
# Simulate tools that are on PATH but unusable (including rustup shims with
# no installed toolchain). Both failures must be reported in the same pass.
cargo() { return 127; }
cmake() { return 127; }
if check_build_environment >"$output" 2>&1; then
    echo 'Expected missing prerequisites to fail' >&2
    exit 1
fi
grep -q 'cargo is unavailable' "$output"
grep -q 'CMake is required' "$output"
grep -q 'make doctor again' "$output"
fail_step() { echo 'original tool diagnostic' >&2; return 42; }
status=0
run_step 'Sign app locally' 'Check signing tool' fail_step >"$output" 2>&1 || status=$?
[[ "$status" == 42 ]]
grep -q 'original tool diagnostic' "$output"
grep -q 'FAILED: Sign app locally (exit 42)' "$output"
grep -q 'Check signing tool' "$output"
echo 'Setup diagnostic failure tests passed'
