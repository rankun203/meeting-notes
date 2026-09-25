#!/bin/bash
set -euo pipefail
client_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mode="${1:-format}"
case "$mode" in
    format) options=(format --in-place) ;;
    lint) options=(lint --strict) ;;
    *) echo 'Usage: format-macos.sh [format|lint]' >&2; exit 2 ;;
esac
if ! /usr/bin/xcrun --find swift-format >/dev/null 2>&1; then
    echo 'Formatting needs Apple Command Line Tools with swift-format. Update Command Line Tools; no Homebrew installation is needed. Building the app does not require this formatter.' >&2
    exit 1
fi
# Explicit roots exclude vendored sources, generated code, and build caches.
exec /usr/bin/xcrun swift-format "${options[@]}" --recursive \
    --configuration "$client_dir/.swift-format" \
    "$client_dir/Package.swift" "$client_dir/Sources" "$client_dir/Tests"
