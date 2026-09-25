#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
check_tools
require_stopped_app "$app_path"
swift_package build -c release
binary_dir="$(swift_package build -c release --show-bin-path)"
require_stopped_app "$app_path"
mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources"
/bin/cp "$binary_dir/GdayMeetings" "$app_path/Contents/MacOS/GdayMeetings"
/bin/cp "$client_dir/packaging/macos/Info.plist" "$app_path/Contents/Info.plist"
/bin/cp "$client_dir/packaging/macos/GdayMeetings.icns" "$app_path/Contents/Resources/GdayMeetings.icns"
/usr/bin/ditto "$build_dir/native-audio-$(uname -m)/install/licenses" "$app_path/Contents/Resources/ThirdPartyLicenses"
/usr/bin/plutil -lint "$app_path/Contents/Info.plist"
/usr/bin/codesign --force --sign "${GDAY_CODESIGN_IDENTITY:--}" "$app_path"
/usr/bin/codesign --verify --strict "$app_path"
printf 'Built %s\n' "$app_path"
