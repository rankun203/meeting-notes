#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
check_tools
# Some standalone Command Line Tools releases omit the Swift Testing macro
# plugin from swiftbuild's module-emission step. Supply the installed plugin
# explicitly when available; Xcode/older toolchains retain normal discovery.
toolchain_usr="$(cd "$(dirname "$(/usr/bin/xcrun --find swift)")/.." && pwd)"
testing_plugin="$toolchain_usr/lib/swift/host/plugins/testing/libTestingMacros.dylib"
if [[ -f "$testing_plugin" ]]; then
    swift_package test -Xswiftc -load-plugin-library -Xswiftc "$testing_plugin"
else
    swift_package test
fi
