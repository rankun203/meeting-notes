# meeting-notes

**meeting-notes** is a local meeting recorder daemon with a built-in web UI. It captures microphone and system audio simultaneously, saves recordings per-session, and provides synced multi-track playback.

![demo](demo.png)

## Features

- **Multi-source recording** — captures microphone and system audio as separate tracks
- **WAV and MP3** — record in lossless WAV or compressed MP3 (CBR)
- **Web UI** — built-in single-page app with real-time updates via WebSocket
- **Synced playback** — play all tracks together with shared controls and per-track mute
- **Session management** — create, name, start/stop, delete sessions with persistent metadata
- **Crash recovery** — metadata.json written at every state transition; sessions restore on restart
- **macOS system audio** — uses Audio Process Tap API to capture all system output at full volume

## Installation

Requires Rust toolchain.

```bash
cargo build --release
```

## Usage

```bash
# Start the daemon with web UI
cargo run -- serve --web-ui

# Custom port and data directory
cargo run -- serve --port 8080 --data-dir ~/my-recordings --web-ui
```

Open `http://127.0.0.1:33487` in your browser.

## Architecture

- `src/audio/` — audio capture (mic via cpal, system audio via macOS Process Tap), WAV/MP3 writers
- `src/session/` — session lifecycle, metadata persistence, broadcast events
- `src/server/` — HTTP API (axum), WebSocket handler, embedded web UI
- `web/index.html` — single-file React frontend (no build step)

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/config` | Available sources and config options |
| `POST` | `/sessions` | Create a new session |
| `GET` | `/sessions` | List sessions |
| `GET` | `/sessions/:id` | Get session details |
| `PATCH` | `/sessions/:id` | Rename session |
| `DELETE` | `/sessions/:id` | Delete session and files |
| `POST` | `/sessions/:id/recording/start` | Start recording |
| `POST` | `/sessions/:id/recording/stop` | Stop recording |
| `GET` | `/sessions/:id/files/:name` | Download/stream a file |
| `WS` | `/ws` | Real-time session updates |

## Development

```bash
# Run with debug logging
RUST_LOG=meeting_notes_daemon=debug cargo run -- serve --web-ui
```

The web UI is embedded at compile time via `rust-embed`. Edit `web/index.html` and rebuild to see changes.
