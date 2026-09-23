#!/bin/bash
# Launch through LaunchServices so TCC attributes permission to Gday Meetings,
# not the terminal. Directly executing Contents/MacOS/... does not do this.
set -euo pipefail

source "$(dirname -- "${BASH_SOURCE[0]}")/macos-common.sh"
build_only=false
if [[ "${1:-}" == --build-only ]]; then
    build_only=true
    shift
fi

if [[ "${1:-}" == --stop ]]; then
    app_pids="$(/usr/sbin/lsof -t "$executable" 2>/dev/null)" || true
    if [[ -z "$app_pids" ]]; then
        echo "Gday Meetings is not running from $app_path"
        exit 0
    fi
    while IFS= read -r app_pid; do
        kill -INT "$app_pid"
    done <<< "$app_pids"
    echo "Shutdown requested. Gday Meetings will stop and finalize active recordings."
    exit 0
fi

bash "$client_dir/scripts/build-macos.sh"
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

echo "Starting Gday Meetings. Press Ctrl+C to stop and finalize recordings."
echo "Logs are also saved to $log_path"
# Preserve access to developer tools (e.g. the Claude CLI) when LaunchServices
# starts the daemon outside the terminal's process tree.
(
    trap '' INT HUP
    exec /usr/bin/open -n -W -a "$app_path" \
        --stdout "$run_dir/output" --stderr "$run_dir/output" \
        --env "PATH=$PATH" \
        --env "RUST_LOG=${RUST_LOG:-gday_meetings_client=info}" \
        --env "RUST_BACKTRACE=${RUST_BACKTRACE:-1}" \
        --args serve --web-ui --open "$@"
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
    echo "Gday Meetings exited before startup completed. See the logs above." >&2
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
