#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
bash "$client_dir/scripts/build-macos.sh"
/usr/bin/open "$app_path"
