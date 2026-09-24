#!/bin/bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/macos-common.sh"

check_build_environment
# The bundled Opus CMake project declares 3.1; CMake 4 removed policies older
# than 3.5. Preserve an explicitly selected policy floor.
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"
require_stopped_app "$app_path"
run_step "Compile native client" "Read Cargo's error above. For an old compiler, run rustup update stable. For download failures, check network/proxy access to crates.io. Re-run make doctor for native tools." \
    cargo build --manifest-path "$client_dir/Cargo.toml" --target-dir "$target_dir" --release --locked
version="$(cargo metadata --manifest-path "$client_dir/Cargo.toml" --format-version 1 --no-deps --locked \
    | /usr/bin/plutil -extract packages.0.version raw -o - -)"

# Recheck after compilation in case the app was started while Cargo was running.
require_stopped_app "$app_path"
mkdir -p "$app_path/Contents/MacOS"
mkdir -p "$app_path/Contents/Resources"
cp "$client_dir/packaging/macos/GdayMeetings.icns" "$app_path/Contents/Resources/GdayMeetings.icns"
cp "$target_dir/release/gday-meetings-client" "$executable"
cp "$client_dir/packaging/macos/Info.plist" "$app_path/Contents/Info.plist"
/usr/bin/plutil -replace CFBundleShortVersionString -string "$version" "$app_path/Contents/Info.plist"
/usr/bin/plutil -replace CFBundleVersion -string "$version" "$app_path/Contents/Info.plist"
/usr/bin/plutil -lint "$app_path/Contents/Info.plist"
# Local ad-hoc signature preserves bundle identity for OS recording permissions.
run_step "Sign app locally" "No certificate is needed. Check the codesign error above and that the build directory is writable. Do not use sudo make." \
    /usr/bin/codesign --force --sign - "$app_path"
run_step "Verify app signature" "The bundle may be incomplete or modified. Run make build again; do not launch a bundle that fails verification." \
    /usr/bin/codesign --verify --strict "$app_path"
echo "Built $app_path"
