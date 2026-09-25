#!/bin/bash
# Static, checksum-pinned Xiph libraries; only CLT tools are required.
set -euo pipefail
client_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
root="${GDAY_AUDIO_BUILD_ROOT:-$client_dir/.build/native-audio-$(uname -m)}"
prefix="$root/install"
signature="$(shasum -a 256 "$0" | cut -d' ' -f1)-$(xcrun clang --version | head -1)-$(xcrun --sdk macosx --show-sdk-version)"
if [[ -f "$root/ready" && "$(cat "$root/ready")" == "$signature" && -f "$prefix/lib/libopusfile.a" ]]; then exit 0; fi
mkdir -p "$root"
if ! mkdir "$root/lock" 2>/dev/null; then
    echo "Audio dependency build already running (or stale lock: $root/lock)." >&2
    exit 1
fi
trap 'rmdir "$root/lock"' EXIT
export CC="$(xcrun --find clang)"
export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
export MACOSX_DEPLOYMENT_TARGET=14.2
export CFLAGS="-O2 -isysroot $SDKROOT -mmacosx-version-min=14.2"
export LDFLAGS="-isysroot $SDKROOT -mmacosx-version-min=14.2"
build_library() {
    local name="$1" checksum="$2"
    shift 2
    local archive="$client_dir/ThirdParty/archives/$name.tar.gz"
    if [[ "$(shasum -a 256 "$archive" | cut -d' ' -f1)" != "$checksum" ]]; then
        echo "Checksum mismatch: $archive. Restore the checked-in archive before retrying." >&2; exit 1
    fi
    # A changed compiler/SDK/script must not reuse objects from an older build.
    /bin/rm -rf "$root/build-$name" "$root/sources/$name"
    mkdir -p "$root/sources"
    tar -xzf "$archive" -C "$root/sources"
    mkdir -p "$root/build-$name"
    (
        cd "$root/build-$name"
        if [[ "$name" == opusfile-* ]]; then
            # Upstream's four-file local decoder target; avoid the old libtool
            # wrapper and unused opusurl target. macOS provides lrintf in libSystem.
            for unit in info internal opusfile stream; do
                "$CC" -O2 -isysroot "$SDKROOT" -mmacosx-version-min=14.2 -DOP_HAVE_LRINTF=1 \
                    -I"$prefix/include" -I"$prefix/include/opus" -I"$root/sources/$name/include" \
                    -c "$root/sources/$name/src/$unit.c" -o "$unit.o"
            done
            /usr/bin/ar -crs libopusfile-local.a info.o internal.o opusfile.o stream.o
            mkdir -p "$prefix/lib" "$prefix/include/opus"
            cp libopusfile-local.a "$prefix/lib/libopusfile.a"
            cp "$root/sources/$name/include/opusfile.h" "$prefix/include/opus/"
        else
            "$root/sources/$name/configure" --prefix="$prefix" --disable-shared --enable-static "$@"
            /usr/bin/make -j4
            /usr/bin/make install
        fi
    )
    mkdir -p "$prefix/licenses"
    cp "$root/sources/$name/COPYING" "$prefix/licenses/$name.txt"
}
build_library libogg-1.3.6 83e6704730683d004d20e21b8f7f55dcb3383cdf84c0daedf30bde175f774638
build_library opus-1.6.1 6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1 --disable-doc --disable-extra-programs
build_library opusfile-0.12 118d8601c12dd6a44f52423e68ca9083cc9f2bfe72da7a8c1acb22a80ae3550b
printf '%s' "$signature" > "$root/ready"
