#!/bin/bash
# Shared paths for the component's macOS build, launch and installer scripts.
if [[ "$(uname -s)" != Darwin ]]; then
    echo "The native client currently requires macOS." >&2
    exit 1
fi

client_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="$client_dir/target"
bundle_dir="${GDAY_MEETINGS_BUNDLE_DIR:-$target_dir/macos}"
app_path="$bundle_dir/Gday Meetings.app"
executable="$app_path/Contents/MacOS/gday-meetings-client"
log_dir="${GDAY_MEETINGS_LOG_DIR:-$HOME/Library/Logs/Gday Meetings}"

require_stopped_app() {
    local candidate="$1/Contents/MacOS/gday-meetings-client"
    if [[ -f "$candidate" ]] && /usr/sbin/lsof -t "$candidate" >/dev/null 2>&1; then
        echo "Gday Meetings is running from $1. Stop recordings and quit it before rebuilding." >&2
        exit 1
    fi
}
