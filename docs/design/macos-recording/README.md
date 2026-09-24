# Recording and listening design

The [generated concept](recording-and-player-concept.png) and [exact prompt](imagegen-prompt.md) explore two states using the built-in image generation tool. Native implementation follows their hierarchy rather than embedding the image in the app.

Actual SwiftUI component renders:

- [Recording setup](implemented-setup.png)
- [Recording with notes](implemented-recording.png)
- [Compact recording workspace in dark appearance](implemented-recording-compact-dark.png)
- [Persistent player](implemented-player.png)

These are offscreen renders with synthetic meeting data/source levels and a paused one-second silent audio fixture, not screenshots of a live meeting. Standard controls render with inactive-window gray styling; the active recording button uses the native red tint. The harness did not display windows, request permissions, play or record audio, use the real library, access credentials, or make network requests. It validated component layout, including a 440×550-point detail and 900-point player. Interactive keyboard/VoiceOver and full-window behavior remain to be verified when native UI automation is available.
