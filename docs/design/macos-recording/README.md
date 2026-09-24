# Recording and listening design

The [generated concept](recording-and-player-concept.png) and [exact prompt](imagegen-prompt.md) explore two states using the built-in image generation tool. Native implementation follows their hierarchy rather than embedding the image in the app.

Actual SwiftUI component renders:

- [Recording setup](implemented-setup.png)
- [Expanded setup scrolled to its options at 400-point height](implemented-setup-expanded-compact.png)
- [Recording with notes](implemented-recording.png)
- [Compact recording workspace in dark appearance](implemented-recording-compact-dark.png)
- [Persistent player](implemented-player.png)

These are offscreen renders with synthetic meeting data/source levels and a paused one-second silent audio fixture, not screenshots of a live meeting. Standard controls render with inactive-window gray styling; the active recording button uses the native red tint. The harness did not display windows, request permissions, play or record audio, use the real library, access credentials, or make network requests. It validated component layout, including a 440×550-point detail and 900-point player. Interactive keyboard/VoiceOver and full-window behavior remain to be verified when native UI automation is available.

Setup regression checks also cover expanded options and long errors at 400/480/600-point heights in light/dark appearance, plus automatic scrolling to a new error and pinned startup progress. The form scrolls independently of its header and action buttons.
