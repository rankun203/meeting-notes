# Swift macOS client

Repository-root instructions still apply.

## Documentation

Read the relevant documents before changing the app:

- [README.md](README.md): app overview, prerequisites, building, installing, and running.
- [Audio design](docs/AUDIO_DESIGN.md): capture architecture, permissions, encoding, and audio lifecycle.
- [UI design](docs/UI_DESIGN.md): Liquid Glass defaults, accessibility, interaction, and layout rules.
- [UI Preview](docs/UI_PREVIEW.md): isolated UI validation, synthetic fixtures, and preview versus full-app behavior.
- [Audio dependencies](ThirdParty/README.md): pinned offline source builds, licenses, and dependency upgrades.
- [Repository worklogs](../../docs/worklogs/): implementation decisions, validation results, technical debt, and outstanding issues; consult recent Swift entries for ongoing work.

Keep this index complete: whenever app documentation is added, moved, or renamed, update its link here in the same change. Every document under `docs/` must have a direct link from this file.

## Swift formatting

- Run `make format-macos` after Swift edits and `make lint-macos` before committing. Follow the checked-in `.swift-format`; do not hand-format against it. Keep broad formatting changes separate from behavioral changes. See README.md for toolchain and scope details.

## UI design default

- **Liquid Glass is the default design direction for this app.** Read and follow [UI_DESIGN.md](docs/UI_DESIGN.md) before creating or changing UI, including UI Preview, Settings, sheets, navigation, and playback controls.
- Prefer current public SwiftUI/AppKit controls and Apple's Human Interface Guidelines. Use Apple Music's capsule tabs and soft sidebar selection as visual references, not private APIs or a promise of identical system-app rendering.
- Preserve the declared minimum macOS version with availability-checked native fallbacks. Do not raise the minimum OS version or introduce deprecated APIs just to obtain a visual effect.
- Validate changed UI in isolated UI Preview, including appearance, keyboard access, and layout stability. Document any untested behavior or compatibility compromise; building successfully does not establish visual correctness.
