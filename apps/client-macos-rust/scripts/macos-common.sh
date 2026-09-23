#!/bin/bash
# Shared paths for the component's macOS build, launch and installer scripts.
if [[ "$(uname -s)" != Darwin ]]; then
    echo "The native client currently requires macOS." >&2
    exit 1
fi

client_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="$client_dir/target"
bundle_dir="${MEETING_NOTES_BUNDLE_DIR:-$target_dir/macos}"
app_path="$bundle_dir/Meeting Notes.app"
executable="$app_path/Contents/MacOS/meeting-notes-daemon"
log_path="$bundle_dir/meeting-notes.log"

require_stopped_app() {
    local candidate="$1/Contents/MacOS/meeting-notes-daemon"
    if [[ -f "$candidate" ]] && /usr/sbin/lsof -t "$candidate" >/dev/null 2>&1; then
        echo "Meeting Notes is running from $1. Stop recordings and quit it before rebuilding." >&2
        exit 1
    fi
}
