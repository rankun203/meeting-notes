# UI design: Liquid Glass by default

Liquid Glass is the design standard for all future UI work in the Swift macOS client. Apply it when adding or revising screens and controls. This is a design policy, not a claim that every existing view has already been migrated.

## Appearance and hierarchy

- Use Apple's native Liquid Glass presentation for navigation and controls where supported. Keep meeting text, notes, transcripts, and waveform content on quiet, readable surfaces. Do not put glass behind every card or stack translucent layers unnecessarily.
- Apple Music is the reference for capsule-shaped tabs and a soft rounded sidebar selection with accent-colored text/icons. Use Gday's accent color and consistent selection treatment. Retain a visible selected state when the window is inactive.
- Prefer standard SwiftUI/AppKit controls, SF Symbols, semantic colors, system typography, and system spacing. Use supported customization only when the native component cannot express the required interaction or visual hierarchy.
- Keep labels direct and concise: “Play,” “Recording transcript,” and meaningful source names. Avoid decorative explanatory text that repeats what the controls already communicate.
- Make recording actions easy to find and distinguish recording, saving, playing, and paused states through symbols and accessible labels as well as color.
- Give icon buttons a full-area hit shape, visible hover/pressed feedback, and a tooltip. Use at least 44×44 pt for player controls (including menu triggers and track mute buttons); keep symbols smaller inside the target. Apple's macOS accessibility table lists 28×28 pt default and 20×20 pt minimum, while general Buttons guidance recommends 44×44 pt hit regions. The minimum is not our target for frequently used controls. Hover feedback must not resize or move controls.

## Interaction and stable layout

- Keep the window toolbar independent of sidebar expansion. Its title and toggle must remain stationary; reveal sidebar rows after expansion completes and preserve the sidebar background throughout.
- Preserve native list spacing. Do not add compensating scroll insets or offsets to hide a layout issue; reproduce the cause and verify navigation, playback changes, and window activation.
- Keep playback updates local to the transport. Menus, text editing, selection, and the main content layout must remain stable while time advances.
- Center waveform and primary playback controls together, with time labels below. Every track shares one playback/scrubbing timeline.
- Support keyboard navigation, visible focus, Space for playback outside text editing, and VoiceOver labels and selection state. Respect Reduce Motion, Reduce Transparency, and increased contrast; do not override system accessibility preferences to preserve an effect.

## API and compatibility policy

- Use the latest stable public APIs supported by the app's toolchain and target OS. Check Apple documentation and SDK availability before adoption; a newer SDK does not make an API available on an older running system.
- Preserve the deployment target declared in `Package.swift` (currently macOS 14.2). Use availability checks and native older-system appearances where Liquid Glass APIs are unavailable. Do not imitate glass with private APIs or require a developer account for local builds.
- There is no single public “Apple Music style.” Prefer native behavior; document any necessary custom control and its keyboard/accessibility obligations. A newer tab-picker API must not be presented as a way to change the appearance on an OS that cannot run it.
- Follow the repository's deprecation policy. Record necessary compatibility fallbacks and a concrete future removal condition in the implementation worklog.

## Validation for UI changes

Use `make start-macos-preview` for independent UI checks without credentials, real capture, or uploads. Verify the changed interaction in Light, Dark, and System appearance; small and large windows; active and inactive window states; and keyboard navigation. For navigation or player changes, also check sidebar transitions, persistent playback, menus, and multi-track scrubbing as applicable. Include accessibility preference checks when introducing custom material or animation.

Report passed, failed, and untested checks accurately. Preserve existing recordings and coordinate separately before testing real audio. See [UI Preview](UI_PREVIEW.md) and [audio design](AUDIO_DESIGN.md).

## Apple references

- [Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass)
- [Build a SwiftUI app with the new design](https://developer.apple.com/videos/play/wwdc2025/323/)
- [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars), [toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars), and [segmented controls](https://developer.apple.com/design/human-interface-guidelines/segmented-controls)
- [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility)
