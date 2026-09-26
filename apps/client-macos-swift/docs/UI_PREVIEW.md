---
title: UI Preview
date: 2026-09-26
status: active
scope: swift-app-testing
---

# UI Preview

From the repository root:

```sh
make build-macos-preview   # Build only
make start-macos-preview   # Build and launch
```

The bundle is `apps/client-macos-swift/.build/preview/Gday Meetings UI Preview.app`. The underlying build script remains `bash apps/client-macos-swift/scripts/preview-macos.sh`.

For full mode, use `make build-macos` or `make start-macos`; `make install-macos` stages the full app for installation. See the [mode comparison](../README.md#choose-a-run-mode). Quit Preview and the full `.build/macos` development copy before rebuilding Preview, because packaging reuses the full build. Copies running from Applications or `.build/installer` can remain open.

The preview banner identifies the mode and provides an Appearance selector for UI testing. Use audio files from Finder for manual drop checks: drop files into the meetings list to create separate meetings, or onto a meeting to add tracks. Generated `microphone.wav` and `system.wav` fixtures can be copied from the temporary preview library for this purpose.

This uses the production SwiftUI screens with a clearly marked preview banner, generated one- and two-track audio, a fresh temporary library on each launch, silent playback, and a local light/dark appearance selector. Keychain reads/writes and real recording are disabled; playback is silent. Online services remain available for connection checks and deliberately started provider jobs. Normal recordings and saved credentials are not loaded. Enter test credentials in Service Providers or load a test credential file explicitly. Temporary fixture libraries are left in the system temporary directory for inspection and normal OS cleanup.

The bundle flag `GdayUIPreview` enables this mode; developers can also launch the executable with `--ui-preview`. The separate preview bundle identifier isolates window/preferences state and lets the normal app remain open. Do not use preview results as evidence of real capture, permissions, or speaker output. A provider check validates its documented connection operation; a successful transcription test validates the tested service path. Neither establishes performance or accuracy for other recordings.

## Provider testing

UI Preview uses the real provider adapters. Saving a provider or opening its panel checks its connection without uploading meeting content. Transcription and other content operations use the same actions as the full app. **Transcribe** starts the configured upload and job directly; provider panels contain the brief destination and charge details. Preview fixtures are synthetic by default; importing another recording does not automatically upload it.

To seed test providers without typing credentials, build Preview, then launch its executable with an explicit credential-file path:

```sh
"apps/client-macos-swift/.build/preview/Gday Meetings UI Preview.app/Contents/MacOS/GdayMeetings" \
  --provider-test-env "$PWD/apps/client-macos-swift/.env"
```

This option is for UI Preview. The file supplies `RUNPOD_ENDPOINT_URL`, `RUNPOD_API_KEY`, `FILE_DROP_URL`, and `FILE_DROP_API_KEY`. Credentials remain in memory; the app does not write them to Keychain or load this file during normal launches. Keep the file untracked. The seed creates RunPod and Filedrop providers, enables Transcription, Speaker Labels, and File Transfer, links the upload provider, and selects RunPod for transcription. New meetings use **Settings → Recording → Default Language**, initially English (`en`); the provider seed does not set a language. Loading it makes no network requests and does not upload a recording. Saving provider settings later writes only non-secret settings into the temporary library.

Connection checks use real services. Starting a RunPod transcription uploads the selected audio to the configured Filedrop provider and can incur RunPod charges. Use generated speech when validating recognition; the default waveform fixtures exercise layout and playback controls. See the [file-transfer contract](../../../docs/protocols/file-transfer.md) and the [optional live test](../README.md#optional-live-provider-test).

## Signing and repeated Keychain prompts

The production identifier remains `com.gdaymeetings.macos`. Default local builds use ad-hoc signing, whose designated requirement is tied to one build's code hash. Merely keeping the bundle identifier does not preserve Keychain trust across changed ad-hoc executables. See [Apple TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements).

To use an already installed signing certificate consistently, set `GDAY_CODESIGN_IDENTITY` to its identity or SHA-1 when building/installing. No certificate is created or imported automatically. A transition from an existing ad-hoc build may still need authorization. Existing credential access controls are not weakened. UI Preview requires no signing certificate and never accesses Keychain.

Normal settings saves now write credentials only when their values changed. The normal app still reads saved credentials at startup; this is not a promise of prompt-free production launches.

The collapsible **Recording visualization preview · synthetic levels** section shows the production microphone/system meters with simulated ten-second histories. Use it to check miniature waveform layout and appearance without starting capture. It is absent from the full app.
