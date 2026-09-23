#!/bin/bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/macos-common.sh"

require_stopped_app "$app_path"
cargo build --manifest-path "$client_dir/Cargo.toml" --target-dir "$target_dir" --release --locked
version="$(cargo metadata --manifest-path "$client_dir/Cargo.toml" --format-version 1 --no-deps --locked \
    | /usr/bin/plutil -extract packages.0.version raw -o - -)"

# Recheck after compilation in case the app was started while Cargo was running.
require_stopped_app "$app_path"
mkdir -p "$app_path/Contents/MacOS"
cp "$target_dir/release/gday-meetings-client" "$executable"
cp "$client_dir/packaging/macos/Info.plist" "$app_path/Contents/Info.plist"
/usr/bin/plutil -replace CFBundleShortVersionString -string "$version" "$app_path/Contents/Info.plist"
/usr/bin/plutil -replace CFBundleVersion -string "$version" "$app_path/Contents/Info.plist"
/usr/bin/plutil -lint "$app_path/Contents/Info.plist"
# Local ad-hoc signature preserves bundle identity for OS recording permissions.
/usr/bin/codesign --force --sign - "$app_path"
/usr/bin/codesign --verify --strict "$app_path"
echo "Built $app_path"
