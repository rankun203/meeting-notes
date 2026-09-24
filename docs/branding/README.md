# Gday Meetings artwork

The approved logo is the winking koala holding ivory and orange chat bubbles on a teal tile. `gday-meetings-koala.png` is the original 1254 × 1254 PNG, including transparency. Preserve this master; do not regenerate the character when exporting icons.

`gday-meetings-koala-macos.png` is the approved-design adaptation for macOS: opaque square artwork with teal extending to every edge, without an inset rounded tile or transparent padding. Use this master for macOS exports so the system's icon treatment does not surround a smaller tile with a gray plate. Keep the original master for web exports.

Exports are committed inside each component so they remain independently buildable:

- `apps/client-macos-rust/packaging/macos/GdayMeetings.icns`: macOS icon containing standard 16–512 point sizes and their 2× variants.
- `apps/client-macos-rust/webui/gday-meetings.png`: 256px browser/sidebar icon.
- `apps/server/public/gday-meetings.png`: identical 256px server/admin icon.

To refresh the web exports from the repository root:

```sh
sips -z 256 256 docs/branding/gday-meetings-koala.png --out apps/client-macos-rust/webui/gday-meetings.png
cp apps/client-macos-rust/webui/gday-meetings.png apps/server/public/gday-meetings.png
```

To refresh the macOS icon, create a temporary `.iconset` folder. Use `sips -z` with `docs/branding/gday-meetings-koala-macos.png` to export `icon_NxN.png` for N = 16, 32, 128, 256, 512, plus `icon_NxN@2x.png` at twice each size. Package with `iconutil -c icns <folder>.iconset -o apps/client-macos-rust/packaging/macos/GdayMeetings.icns`. The app build copies the icon into its Resources directory before signing.

The menu bar keeps its monochrome waveform status symbol: the detailed, opaque tile is unsuitable as a macOS template image. A dedicated simplified koala silhouette can be designed separately.
