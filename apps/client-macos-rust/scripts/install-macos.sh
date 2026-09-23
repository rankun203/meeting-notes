#!/bin/bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/macos-common.sh"

bash "$client_dir/scripts/build-macos.sh"
installer_dir="$target_dir/installer"
staged_app="$installer_dir/Gday Meetings.app"
require_stopped_app "$staged_app"
mkdir -p "$installer_dir"
# A separate copy keeps development launches working after the installer app is dragged away.
if [[ -e "$staged_app" ]]; then rm -rf "$staged_app"; fi
/usr/bin/ditto "$app_path" "$staged_app"
if [[ ! -e "$installer_dir/Applications" && ! -L "$installer_dir/Applications" ]]; then
    ln -s /Applications "$installer_dir/Applications"
fi
/usr/bin/codesign --verify --strict "$staged_app"
echo "Drag Gday Meetings.app onto Applications in the Finder window."
echo "Then open Gday Meetings from Applications to start the client and its browser UI."
echo "Installer folder: $installer_dir"
/usr/bin/open "$installer_dir"
