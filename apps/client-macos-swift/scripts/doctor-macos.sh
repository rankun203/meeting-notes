#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
check_tools
/usr/bin/xcode-select -p
/usr/bin/xcrun swift --version
printf 'macOS SDK: '
/usr/bin/xcrun --sdk macosx --show-sdk-version
echo 'Ready. No full Xcode, Rust, CMake, Homebrew, or Node installation is required.'
