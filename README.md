# meeting-notes

**meeting-notes** is a local-first meeting recorder daemon. It captures microphone and system audio as separate tracks, manages recording sessions via a REST API, and serves a built-in web UI as one of its clients.

![demo](demo.png)

## Features

- **REST API** — full resource-based API for session and recording management
- **Recordings management** — create, name, start/stop, delete sessions with persistent metadata
- **Multi-source recording** — capture microphone and system audio as separate tracks
- **Multi-format** — WAV (lossless) and MP3 (CBR) output
- **Web UI** — built-in single-page client with real-time updates via WebSocket

## Installation

Requires [Rust toolchain](https://rustup.rs).

```bash
cargo install --git https://github.com/rankun203/meeting-notes
```

## Usage

```bash
# Start the daemon with web UI
meeting-notes-daemon serve --web-ui

# Custom port and data directory
meeting-notes-daemon serve --port 8080 --data-dir ~/my-recordings --web-ui
```

Open `http://127.0.0.1:33487` in your browser.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        meeting-notes daemon                      │
│                                                                  │
│  ┌───────────────────────┐    ┌───────────────────────────────┐  │
│  │   Audio Capture       │    │   REST API + WebSocket        │  │
│  │                       │    │                               │  │
│  │  macOS:               │    │  POST /sessions               │  │
│  │   Mic ── cpal         │    │  POST /sessions/:id/start     │  │
│  │   System ── ProcessTap│    │  POST /sessions/:id/stop      │  │
│  │                       │    │  GET  /sessions/:id/files/:f  │  │
│  │  Linux: (TBD)         │    │  WS   /ws (live updates)      │  │
│  │   Mic ── cpal         │    │                               │  │
│  │   System ── PipeWire  │    └──────────┬────────────────────┘  │
│  │                       │               │                       │
│  │  Windows: (TBD)       │               │                       │
│  │   Mic ── cpal         │    ┌──────────▼────────────────────┐  │
│  │   System ── WASAPI    │    │   Clients                     │  │
│  │                       │    │                               │  │
│  └──────────┬────────────┘    │   Web UI (built-in)           │  │
│             │                 │   Logseq Plugin (planned)     │  │
│             ▼                 │   Obsidian Plugin (planned)   │  │
│  ┌───────────────────────┐    │   CLI / custom clients        │  │
│  │  Writers              │    └───────────────────────────────┘  │
│  │  WAV (hound)          │                                       │
│  │  MP3 (LAME)           │                                       │
│  └──────────┬────────────┘                                       │
│             │                                                    │
│             ▼                                                    │
│  ┌───────────────────────┐    ┌───────────────────────────────┐  │
│  │  Session Storage      │    │   Transcription (planned)     │  │
│  │                       │    │                               │  │
│  │  recordings/          │───▶│   Cloud or local deployment   │  │
│  │    {id}/              │    │   Speech-to-text API          │  │
│  │      metadata.json    │    │          │                    │  │
│  │      mic.mp3          │    │          ▼                    │  │
│  │      system.mp3       │    │   Meeting summary + TODOs     │  │
│  └───────────────────────┘    └───────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

## API

| Resource | Method | Endpoint | Description |
|----------|--------|----------|-------------|
| Config | `GET` | `/config` | Available sources and config options |
| Sessions | `POST` | `/sessions` | Create a new session |
| Sessions | `GET` | `/sessions` | List sessions |
| Sessions | `GET` | `/sessions/:id` | Get session details |
| Sessions | `PATCH` | `/sessions/:id` | Rename session |
| Sessions | `DELETE` | `/sessions/:id` | Delete session and files |
| Recording | `POST` | `/sessions/:id/recording/start` | Start recording |
| Recording | `POST` | `/sessions/:id/recording/stop` | Stop recording |
| Files | `GET` | `/sessions/:id/files/:name` | Download/stream a file |
| Events | `WS` | `/ws` | Real-time session updates |

## Roadmap

- [ ] Full meeting transcription
- [ ] Speaker diarization
- [ ] Meeting summary and TODO extraction
- [ ] Logseq plugin
- [ ] Obsidian plugin
- [ ] Windows support
- [ ] Linux support

## Platform Support

| Platform | Microphone | System Audio | Status |
|----------|-----------|--------------|--------|
| macOS | cpal | Audio Process Tap | Available |
| Linux | cpal | PipeWire | Planned |
| Windows | cpal | WASAPI loopback | Planned |

## Development

```bash
# Run with debug logging
RUST_LOG=meeting_notes_daemon=debug cargo run -- serve --web-ui
```
