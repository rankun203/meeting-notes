#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
preview_app="$build_dir/preview/Gday Meetings UI Preview.app"
require_stopped_app "$preview_app"
/bin/bash "$client_dir/scripts/build-macos.sh"
mkdir -p "$(dirname "$preview_app")"
/usr/bin/ditto "$app_path" "$preview_app"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier com.gdaymeetings.macos.preview' "$preview_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName Gday Meetings UI Preview' "$preview_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :GdayUIPreview bool true' "$preview_app/Contents/Info.plist"
/usr/bin/codesign --force --sign "${GDAY_CODESIGN_IDENTITY:--}" "$preview_app"
/usr/bin/codesign --verify --strict "$preview_app"
printf 'UI preview ready: %s\n' "$preview_app"
