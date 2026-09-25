# Gday Meetings — SwiftUI client

A native macOS meeting app built entirely with SwiftUI, AppKit, AVFoundation, Core Audio, Security, and Foundation. The package has no third-party dependencies and does not launch the Rust client or a browser UI.

## Choose a run mode

The same SwiftUI client supports two explicit modes. Commands below run from the repository root.

| | Full app (default) | UI Preview |
| --- | --- | --- |
| Build only | `make build-macos` | `make build-macos-preview` |
| Build and launch | `make start-macos` | `make start-macos-preview` |
| Library | Your persistent meetings library | Fresh temporary library with synthetic recordings |
| Credentials | Reads saved API keys and sign-in tokens from Keychain; macOS may prompt | No Keychain access |
| Audio | Real playback and recording | Silent playback; recording disabled |
| Online services | Configured transcription, AI, and server services available | Service network requests blocked |
| Purpose | Normal use and coordinated hardware/service testing | Independent layout and interaction testing |

UI Preview displays a visible banner and offers System/Light/Dark appearance controls. Use it for UI validation without passwords or real audio. It does not validate capture permissions, audible output, or server behavior. See [UI Preview details and signing](UI_PREVIEW.md).

Build outputs:

- Full app: `.build/macos/Gday Meetings Swift.app`
- UI Preview: `.build/preview/Gday Meetings UI Preview.app`

These paths are relative to this client directory. Quit the bundle being rebuilt first. Preview packaging currently also rebuilds the full `.build/macos` bundle, so that development copy must be stopped too. A full app running from Applications or `.build/installer` can remain open while building Preview.

## Build and install

Requires macOS 14.2 or later and Apple's Command Line Tools with Swift 5.9 or later. Install current Command Line Tools for your macOS version; tests use Swift Testing and require Swift 6 or later.

```sh
xcode-select --install
# Wait for Apple's installer to finish, then from the repository root:
make install-macos
```

Finder opens `apps/client-macos-swift/.build/installer/`, containing **Gday Meetings Swift.app** and an **Applications** shortcut. Drag the app onto Applications and open it. Installation does not overwrite Applications automatically. No Rust, CMake, Homebrew, Node, Python, Docker, Apple Developer account, or full Xcode is needed. The app is locally ad-hoc signed; these scripts do not produce a notarized public release.

Other commands:

```sh
make doctor-macos    # Show the selected developer tools, Swift and SDK
make build-macos     # Build and sign the full app without opening Finder
make start-macos     # Build and launch the full app
make test-macos      # Run persistence, import and service contract tests
```

Builds target the current Mac's architecture. Quit the development or staged app before rebuilding its bundle. The original Rust client retains `make install`, `make start`, and `make test-client`.

### With Xcode

Open `Package.swift` in Xcode and select the **GdayMeetings** executable scheme to build and debug. No generated `.xcodeproj` is needed. To run with the microphone/system-audio purpose strings and stable app identity, use `make start-macos` to launch the packaged app; Xcode can attach to its `GdayMeetings` process. The same Make commands work with Xcode selected through `xcode-select` or `DEVELOPER_DIR`. For UI Preview when running the executable from Xcode, add `--ui-preview` to the scheme’s launch arguments; remove it to return to full mode. The packaged Preview target additionally uses a separate bundle identifier to isolate window/preferences state.

## Native workflows

- Search your local meeting titles, notes, summaries and transcripts.
- Choose **New Recording** to name the meeting and select microphone/system sources. During capture, take notes beside real source meters, an elapsed timer, and **Stop & Save**. Recording controls remain available when browsing elsewhere.
- Drop audio/video files onto the meetings list to create one meeting per file. Drop files onto a meeting detail to add separate tracks; imported tracks start together at time zero. Originals are copied, and a failed batch is rolled back. Finish recording or a pending transcription before changing tracks. Importing never automatically transcribes or uploads.
- Play all tracks together or individual tracks in the persistent player. Continue browsing, searching, and editing other meetings while listening; use 15-second skips, speed selection, the scrubber, or transcript timestamps. Starting a recording pauses playback; it resumes only when you choose Play.
- Space toggles playback in the library window, except while editing text or using a sheet. Dragging any waveform previews the same time across the mix and individual tracks; release to seek all tracks together.
- Edit transcripts, rename speakers, write notes, and generate/edit summaries and action items.
- Organize meetings with people and tags, and chat using meeting, person or tag context.
- Configure a Gday Meetings server with browser-based OAuth sign-in, or use an OpenAI-compatible transcription endpoint. Summaries and chat use a separately configured OpenAI-compatible language model.
- Export meeting text as JSON or Markdown, import text archives, or copy recordings from the Rust client's library using **File → Import Existing Gday Library**. JSON text exports do not embed audio.
- Search server meetings and import their transcript text, or use **Meeting Actions → Archive to Server** to retain a verified server snapshot of a local meeting and its audio.
- Use the menu bar recording controls, **Command-N** for a meeting, **Command-O** for audio import, **Command-Shift-R** for recording, and **Command-comma** for Settings.

Server transcription checkpoints its upload inputs, stable attempt key, and task ID locally. If the app exits or a request fails, choose **Resume Transcription** to check the same durable job. The server and worker run separately; installing this client does not install them. A working server must have a worker configured before it can transcribe.

Direct transcription follows the compatible `/audio/transcriptions` API. Tracks are split into ten-minute AAC excerpts, with timestamps restored to the original timeline; each request is limited to 25 MB. Server uploads accept up to 500 MB, subject to the server's configured lower limit. The client converts unsupported native audio containers and large PCM tracks to M4A for server upload. AI requests send the selected context to the provider configured in Settings.

Server archives are immutable snapshots. Repeating an archive resumes or verifies the original snapshot; it does not synchronize later edits. Local audio is retained. Search imports contain transcript text because the search API does not return the original audio or editable segment structure.

### Differences from the Rust client

The native client currently supports one active recording with the default microphone and system audio. It saves separate source tracks as Opus (default), M4A/AAC, or WAV. It does not offer MP3 recording or concurrent recording sessions. It has no local REST/WebSocket server or Claude Code subprocess provider. Existing-library import copies the supported meeting content and associations, but does not migrate voice embeddings, legacy chat history, or arbitrary sidecars. The Rust client remains available for those workflows.

## Audio quality

Settings → Recording offers optional **Microphone voice processing** using Apple's echo cancellation, noise suppression, and automatic gain control. It is off by default because processing can affect other apps' playback volume. The implementation requests minimum ducking, never monitors the microphone through speakers, and saves system audio separately. Echo removal depends on the device route; headphones provide the most reliable acoustic separation. Voice processing cannot guarantee echo-free recordings from every third-party calling app.

The audio pipeline aligns track timestamps to a shared host-clock timeline, preserves gaps with silence, and uses bounded asynchronous PCM file writes during capture. Settings → Recording selects Opus (default), M4A/AAC, or WAV. After stopping, each track is encoded separately; compressed-file metadata is saved before temporary WAV sources are removed. Conversion failure preserves the WAV recording. Opus uses native Apple codecs and standard Ogg wrapping without FFmpeg or third-party runtime dependencies. Playback prepares temporary decoded audio because AVKit does not directly read Ogg Opus on the tested macOS release. Device/format changes finalize the partial recording with a visible error. Effective processing, sample rates, and channel counts are retained in the meeting's recording profile. Transcription uses compatible copies when needed; it does not replace the saved recording. See [audio research, design decisions, and hardware validation matrix](AUDIO_DESIGN.md).

## Recording permissions and storage

When recording begins, macOS requests the microphone and system-audio permissions needed by the selected sources. System audio uses an audio-only Core Audio process tap: there is no display selection, screen-sharing session, or video capture. The packaged app includes `NSAudioCaptureUsageDescription` and `NSMicrophoneUsageDescription`. Permission decisions belong to macOS; previously denied access may require the user to change the existing decision in Privacy & Security. A recording needs at least one enabled audio source. Each session creates and tears down its own private tap and aggregate device, and saves microphone and system tracks separately.

The meetings library lives in `~/.local/share/com.gdaymeetings.macos/`, and the app identity is `com.gdaymeetings.macos`, based on our domain `gdaymeetings.com`. The toolbar folder button opens this directory. On first launch, if the new directory does not exist, the app copies the former `~/Library/Application Support/Gday Meetings Swift/` library into it, preserving the original. Existing destination libraries are never merged or overwritten. Quit older app versions before migration; changes subsequently made in an older version are not synchronized. The Rust client uses its own directory and format. Changing the bundle identity may require granting recording permissions again. OAuth credentials and provider keys are stored in Keychain. Keep a backup of the library to retain audio as well as text.

For independent UI checks, use `make start-macos-preview`. `GDAY_SWIFT_DATA_DIR` is only a library-location override for development: it does not enable UI Preview, disable recording/network access, or suppress all credential access (server authentication can still read Keychain). Tests use temporary directories and synthetic data.

## Human Interface Guidelines

**Liquid Glass is the default design direction for all future UI changes.** Follow [UI_DESIGN.md](UI_DESIGN.md) for appearance, interaction, accessibility, compatibility, and validation requirements. Apple Music's capsule tabs and soft sidebar selection are visual references; use supported native APIs and preserve older-macOS fallbacks. Existing views have not all been migrated yet.

The source cites the relevant Apple HIG principles beside the controls implementing them:

| Principle | Implementation |
| --- | --- |
| [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars) | Native navigation split view with library, content selection, and detail panes. |
| [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars) | Recording and contextual actions at the top of the window, labeled SF Symbols. |
| [Settings](https://developer.apple.com/design/human-interface-guidelines/settings) | Standard Settings scene with grouped recording, transcription, and intelligence options. |
| [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility) | Native controls, semantic fonts/colors, accessible labels, keyboard navigation, and text selection. |
| [Privacy](https://developer.apple.com/design/human-interface-guidelines/privacy) | Just-in-time recording permission requests and browser authentication. |
| [Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets) | Focused recording setup with draft choices, explicit start/cancel, and inline retry errors. |
| [Feedback](https://developer.apple.com/design/human-interface-guidelines/feedback) | Actual source levels and distinct recording/saving states without invented progress. |
| [Playing audio](https://developer.apple.com/design/human-interface-guidelines/playing-audio) | App-owned persistent transport; browsing never implicitly starts or replaces playback. |
| [Designing for macOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos) | Resizable windows, standard file panels, menu commands, and familiar keyboard shortcuts. |

`ViewState` is an alias of the original `SwiftUI.State` property wrapper. SDK 27 also declares a `State` macro whose plugin is absent from this Command Line Tools installation; the alias avoids that optional macro dependency without changing SwiftUI state behavior.
