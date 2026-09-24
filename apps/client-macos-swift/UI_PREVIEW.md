# UI Preview

From the repository root:

```sh
make build-macos-preview   # Build only
make start-macos-preview   # Build and launch
```

The bundle is `apps/client-macos-swift/.build/preview/Gday Meetings UI Preview.app`. The underlying build script remains `bash apps/client-macos-swift/scripts/preview-macos.sh`.

For full mode, use `make build-macos` or `make start-macos`; `make install-macos` stages the full app for installation. See the [mode comparison](README.md#choose-a-run-mode). Quit Preview and the full `.build/macos` development copy before rebuilding Preview, because packaging reuses the full build. Copies running from Applications or `.build/installer` can remain open.

This uses the production SwiftUI screens with a clearly marked preview banner, generated one- and two-track audio, a fresh temporary library on each launch, silent playback, and a local light/dark appearance selector. Keychain reads/writes, recording, and service network requests are disabled. Normal recordings and credentials are not loaded. Temporary fixture libraries are left in the system temporary directory for inspection and normal OS cleanup.

The bundle flag `GdayUIPreview` enables this mode; developers can also launch the executable with `--ui-preview`. The separate preview bundle identifier isolates window/preferences state and lets the normal app remain open. Do not use preview results as evidence of real capture, permissions, speaker output, or server behavior.

## Signing and repeated Keychain prompts

The production identifier remains `com.gdaymeetings.macos`. Default local builds use ad-hoc signing, whose designated requirement is tied to one build's code hash. Merely keeping the bundle identifier does not preserve Keychain trust across changed ad-hoc executables. See [Apple TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements).

To use an already installed signing certificate consistently, set `GDAY_CODESIGN_IDENTITY` to its identity or SHA-1 when building/installing. No certificate is created or imported automatically. A transition from an existing ad-hoc build may still need authorization. Existing credential access controls are not weakened. UI Preview requires no signing certificate and never accesses Keychain.

Normal settings saves now write credentials only when their values changed. The normal app still reads saved credentials at startup; this is not a promise of prompt-free production launches.
