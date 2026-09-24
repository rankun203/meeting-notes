#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
bash "$client_dir/scripts/build-macos.sh"
installer_dir="$build_dir/installer"
staged_app="$installer_dir/Gday Meetings Swift.app"
require_stopped_app "$staged_app"
mkdir -p "$installer_dir"
# The disposable staging copy can be dragged away without removing the build.
if [[ -e "$staged_app" ]]; then /bin/rm -rf "$staged_app"; fi
/usr/bin/ditto "$app_path" "$staged_app"
if [[ ! -e "$installer_dir/Applications" && ! -L "$installer_dir/Applications" ]]; then
    /bin/ln -s /Applications "$installer_dir/Applications"
fi
/usr/bin/codesign --verify --strict "$staged_app"
printf 'Drag Gday Meetings Swift.app onto Applications, then open it.\nInstaller folder: %s\n' "$installer_dir"
# Opening a folder needs no Finder Automation permission.
/usr/bin/open "$installer_dir"
