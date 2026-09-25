# Swift macOS client

Repository-root instructions still apply.

## UI design default

- **Liquid Glass is the default design direction for this app.** Read and follow [UI_DESIGN.md](docs/UI_DESIGN.md) before creating or changing UI, including UI Preview, Settings, sheets, navigation, and playback controls.
- Prefer current public SwiftUI/AppKit controls and Apple's Human Interface Guidelines. Use Apple Music's capsule tabs and soft sidebar selection as visual references, not private APIs or a promise of identical system-app rendering.
- Preserve the declared minimum macOS version with availability-checked native fallbacks. Do not raise the minimum OS version or introduce deprecated APIs just to obtain a visual effect.
- Validate changed UI in isolated UI Preview, including appearance, keyboard access, and layout stability. Document any untested behavior or compatibility compromise; building successfully does not establish visual correctness.
