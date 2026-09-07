#!/bin/bash
# Launch through LaunchServices so TCC attributes permission to Meeting Notes,
# not the terminal. Directly executing Contents/MacOS/... does not do this.
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
    echo "This launcher requires macOS." >&2
    exit 1
fi

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
build_only=false
if [[ "${1:-}" == --build-only ]]; then
    build_only=true
    shift
fi

# Use a predictable local location regardless of Cargo's configured target dir.
bundle_dir="${MEETING_NOTES_BUNDLE_DIR:-$repo_dir/target/macos}"
app_path="$bundle_dir/Meeting Notes.app"
executable="$app_path/Contents/MacOS/meeting-notes-daemon"
log_path="$bundle_dir/meeting-notes.log"

if [[ "${1:-}" == --stop ]]; then
    app_pids="$(/usr/sbin/lsof -t "$executable" 2>/dev/null)" || true
    if [[ -z "$app_pids" ]]; then
        echo "Meeting Notes is not running from $app_path"
        exit 0
    fi
    while IFS= read -r app_pid; do
        kill -INT "$app_pid"
    done <<< "$app_pids"
    echo "Shutdown requested. Meeting Notes will stop and finalize active recordings."
    exit 0
fi

# Do not overwrite an executable that is currently recording.
if [[ -f "$executable" ]] && /usr/sbin/lsof -t "$executable" >/dev/null 2>&1; then
    echo "Meeting Notes is running. Stop recordings and quit the daemon before rebuilding." >&2
    exit 1
fi

cargo build --manifest-path "$repo_dir/Cargo.toml" --target-dir "$repo_dir/target" --release
mkdir -p "$app_path/Contents/MacOS"
cp "$repo_dir/target/release/meeting-notes-daemon" "$executable"
cp "$repo_dir/packaging/macos/Info.plist" "$app_path/Contents/Info.plist"
/usr/bin/plutil -lint "$app_path/Contents/Info.plist"
# Ad-hoc signing is sufficient for a local development bundle. Rebuilds may
# require granting permission again; distribution needs a proper signing identity.
/usr/bin/codesign --force --sign - "$app_path"
/usr/bin/codesign --verify --strict "$app_path"

echo "Built $app_path"
if "$build_only"; then
    exit 0
fi

# LaunchServices owns the app process for permission attribution. Keep this
# shell attached to it, stream its logs, and forward terminal shutdown signals.
daemon_pid=""
launch_pid=""
log_pid=""
pending_signal=""
run_dir=""

forward_signal() {
    pending_signal="$1"
    if [[ -n "$daemon_pid" ]]; then
        kill -s "$1" "$daemon_pid" 2>/dev/null || true
    fi
}

cleanup() {
    # An unexpected launcher error must not leave an orphan recorder behind.
    if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" 2>/dev/null; then
        kill -TERM "$daemon_pid" 2>/dev/null || true
        while kill -0 "$daemon_pid" 2>/dev/null; do sleep 0.2; done
    fi
    if [[ -n "$log_pid" ]]; then
        kill -TERM "$log_pid" 2>/dev/null || true
        wait "$log_pid" 2>/dev/null || true
    fi
    if [[ -n "$launch_pid" ]]; then
        kill -TERM "$launch_pid" 2>/dev/null || true
        wait "$launch_pid" 2>/dev/null || true
    fi
    if [[ -n "$run_dir" ]]; then
        rm -f "$run_dir/output"
        rmdir "$run_dir"
    fi
}

trap 'forward_signal INT' INT
trap 'forward_signal TERM' TERM
trap 'forward_signal TERM' HUP
trap cleanup EXIT

# A private FIFO gives immediate log output without tail's polling delay or
# replaying old log entries. Only the new app writes to this pipe.
run_dir="$(mktemp -d "$bundle_dir/run.XXXXXX")"
mkfifo "$run_dir/output"
(
    # Keep printing finalization logs after Ctrl+C reaches the terminal group.
    trap '' INT HUP
    exec tee -a "$log_path" < "$run_dir/output"
) &
log_pid=$!

echo "Starting Meeting Notes. Press Ctrl+C to stop and finalize recordings."
echo "Logs are also saved to $log_path"
# Preserve access to developer tools (e.g. the Claude CLI) when LaunchServices
# starts the daemon outside the terminal's process tree.
(
    trap '' INT HUP
    exec /usr/bin/open -n -W -a "$app_path" \
        --stdout "$run_dir/output" --stderr "$run_dir/output" \
        --env "PATH=$PATH" \
        --env "RUST_LOG=${RUST_LOG:-meeting_notes_daemon=info}" \
        --env "RUST_BACKTRACE=${RUST_BACKTRACE:-1}" \
        --args serve --web-ui "$@"
) &
launch_pid=$!

# The running-bundle guard above ensures this executable belongs to this launch.
# Resolve its PID without matching unrelated CLI instances by process name.
for ((attempt = 0; attempt < 100; attempt++)); do
    daemon_pid="$(/usr/sbin/lsof -t "$executable" 2>/dev/null | head -n 1)" || true
    if [[ -n "$daemon_pid" ]]; then
        if [[ -n "$pending_signal" ]]; then
            kill -s "$pending_signal" "$daemon_pid" 2>/dev/null || true
        fi
        break
    fi
    if ! kill -0 "$launch_pid" 2>/dev/null; then break; fi
    sleep 0.1
done

if [[ -z "$daemon_pid" ]]; then
    echo "Meeting Notes exited before startup completed. See the logs above." >&2
    kill -TERM "$launch_pid" 2>/dev/null || true
    exit 1
fi

# Bash wait is interrupted by traps; keep waiting while the app shuts down.
launch_status=0
while kill -0 "$launch_pid" 2>/dev/null; do
    wait "$launch_pid" || launch_status=$?
done
# Drain the FIFO so the final shutdown messages reach the terminal.
wait "$log_pid" || true
exit "$launch_status"
