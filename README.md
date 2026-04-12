# meeting-notes · VoiceRecords (主簿)

**meeting-notes** is a local-first meeting recorder, transcriber, and summarizer. It captures microphone and system audio as separate tracks, transcribes them with speaker diarization, extracts TODOs, and lets you chat with your recordings — all backed by a single source of truth and two interchangeable runtimes:

- **`meeting-notes-daemon`** — headless Rust daemon with REST + WebSocket API and an embedded web UI. Ideal for remote access, Logseq/Obsidian integrations, and power users who want to keep a browser tab open.
- **`voicerecords`** — a native macOS desktop app built with Tauri 2. Same business logic, same data directory, native window chrome + vibrancy, proper `.app` bundle, microphone permission prompt, icon in the dock. Bilingual name: **VoiceRecords** in English, **主簿** (Zhǔbù, *"Master of the Register"*) in Chinese.

![demo](demo.png)

## Features

- **Zero setup** — no virtual audio devices or kernel extensions needed; just install and run
- **Multi-track recording** — microphone and system audio captured simultaneously as separate tracks
- **Concurrent sessions** — run multiple recording sessions in parallel, each with its own set of tracks
- **Multi-format output** — WAV (lossless), MP3 (CBR), Opus (VBR)
- **Speech-to-text + diarization** — via a pluggable RunPod serverless endpoint
- **Speaker library** — persistent "people" database with voice embeddings for cross-session recognition
- **Summaries and TODOs** — LLM-powered extraction with configurable provider / model / prompt
- **Chat with your recordings** — ask questions about a meeting, a person, or a tag; context is retrieved from the local index
- **Claude Code integration** — optional, with tool-approval flow (once / session / permanent)
- **Native desktop app** — macOS vibrancy, design-system tokens, i18n-ready, same data dir as the daemon
- **Low resource usage** — built with Rust; ~2% CPU for WAV, ~4% for Opus, ~6% for MP3

## Installation

Requires the [Rust toolchain](https://rustup.rs). macOS 11+ for system-audio capture.

### Headless daemon

```bash
cargo install --git https://github.com/rankun203/meeting-notes meeting-notes-daemon
```

### Desktop app (VoiceRecords / 主簿)

Clone the repo, then build the `.app` bundle with the Tauri CLI:

```bash
git clone https://github.com/rankun203/meeting-notes
cd meeting-notes

# One-time: install the Tauri 2 CLI
cargo install tauri-cli --version '^2.0' --locked

# Fast dev loop — opens the app with hot-reload on the Rust side
cd apps/desktop && cargo tauri dev

# Debug .app + .dmg (unsigned, ~54 MB)
cd apps/desktop && cargo tauri build --debug
# → target/debug/bundle/macos/VoiceRecords.app
# → target/debug/bundle/dmg/VoiceRecords_0.1.0_aarch64.dmg

# Release .app + .dmg (optimized, smaller binary)
cd apps/desktop && cargo tauri build
```

The debug build is unsigned, so the first launch from Finder will show a Gatekeeper warning. Right-click → Open to bypass, or `xattr -dr com.apple.quarantine VoiceRecords.app` on your own copy. Codesigning and notarization are on the roadmap.

## Usage

### Daemon

```bash
# Start on the default port (33487) with the built-in web UI
meeting-notes-daemon serve --web-ui

# Custom port, host, and data directory
meeting-notes-daemon serve --port 8080 --host 0.0.0.0 \
  --data-dir ~/my-recordings --web-ui
```

Open `http://127.0.0.1:33487` in your browser.

### Desktop app

Double-click `VoiceRecords.app` (or run `voicerecords` directly from `target/debug/`). On first launch, macOS will prompt for microphone and screen-audio permissions — these are declared in the bundled `Info.plist` (`NSMicrophoneUsageDescription`, `NSAudioCaptureUsageDescription`).

Both runtimes read and write the same data directory — `~/.local/share/org.rankun.meeting-notes/` — so sessions you record in one show up immediately in the other. You can even run them side-by-side.

## Architecture

The codebase is a Cargo workspace with three members:

```
meeting-notes/
├── src/                        ← meeting-notes-daemon (lib + bin)
│   ├── audio/                    mic + system-audio capture, encoders
│   ├── session/                  session manager, metadata, broadcast events
│   ├── people/                   speaker library + voice embeddings
│   ├── tags/                     tag CRUD + session cascading
│   ├── chat/                     conversation manager, summarize runner
│   ├── llm/                      LLM client, context retrieval, Claude Code runner
│   ├── filesdb/                  transcript cache + per-session file index
│   ├── markdown/                 auto-generated CLAUDE.md + index.md files
│   ├── services/                 ← TRANSPORT-AGNOSTIC BUSINESS LOGIC
│   │   ├── sessions.rs             create / list / start / stop / update
│   │   ├── transcripts.rs          transcript + attribution + pipeline
│   │   ├── summary.rs              summary + todos + summarize kickoff
│   │   ├── files.rs                list / serve / waveform
│   │   ├── people.rs               people CRUD
│   │   ├── tags.rs                 tag CRUD + session-tag assignment
│   │   ├── settings.rs             settings + secret routing
│   │   ├── config.rs               static app config schema
│   │   ├── chat.rs                 non-streaming chat + send_message_stream
│   │   ├── claude.rs               non-streaming claude + send_stream
│   │   ├── state.rs                AppState (shared by both transports)
│   │   └── error.rs                ServiceError (maps to HTTP + Tauri)
│   ├── server/                   axum REST + WebSocket (thin shims over services/)
│   └── main.rs                   CLI entry point — just builds AppState, hands to axum
│
├── apps/
│   ├── desktop/                ← voicerecords-desktop (Tauri 2 binary)
│   │   ├── src/
│   │   │   ├── main.rs           bootstraps AppState, registers commands, vibrancy
│   │   │   └── commands/         49 mn_* Tauri commands — thin shims over services/
│   │   ├── tauri.conf.json
│   │   ├── Info.plist            NSMicrophoneUsageDescription etc.
│   │   ├── capabilities/         Tauri permission manifests
│   │   └── icons/                quill-over-scroll placeholder set (.svg → .icns / .ico / .png)
│   │
│   ├── webui/                  ← shared SPA (no build step)
│   │   ├── index.html            imports Tailwind + tokens.css + tauri-bridge.mjs
│   │   ├── app.mjs               React root wrapped in LocaleProvider
│   │   ├── api-router.mjs        REST path → mn_* Tauri command map
│   │   ├── utils.mjs             api() dispatches to fetch OR invoke()
│   │   ├── tauri-bridge.mjs      no-op in browser; exposes window.__mn in desktop
│   │   ├── tokens.css            design system — CSS custom properties
│   │   ├── i18n.mjs              loadLocale / t() / LocaleProvider
│   │   ├── i18n/en.json          ~80 translation keys (ready for zh-CN follow-up)
│   │   ├── session.mjs           session detail view + recording controls
│   │   ├── transcript.mjs        transcript timeline + attribution UI
│   │   ├── people.mjs, tags.mjs, settings.mjs, sidebar.mjs, player.mjs, …
│   │   └── chat/                 chat panel, threads, composer, mentions
│   │
│   └── file-drop/              ← temporary file parking server (separate binary)
│
└── Cargo.toml                  ← [workspace] root
```

### Service layer = single source of truth

Every REST handler in `src/server/routes.rs` and every `#[tauri::command]` in `apps/desktop/src/commands/` is a thin shim — typically 3 to 8 lines — that forwards to a function in `src/services/`. Business logic exists in exactly one place:

```
                       ┌────────────────────────┐
POST /sessions ──────▶ │  axum handler          │
                       │  (src/server/routes.rs)│──┐
                       └────────────────────────┘  │
                                                   ▼
                                          ┌──────────────────┐
                                          │ services::       │
                                          │  sessions::      │
                                          │  create_session  │
                                          └──────────────────┘
                                                   ▲
                                                   │
                       ┌────────────────────────┐  │
invoke('mn_create_    │  #[tauri::command]     │  │
       session') ───▶ │  (apps/desktop/src/    │──┘
                       │   commands/sessions.rs)│
                       └────────────────────────┘
```

Adding a new endpoint means writing the service function once and two 5-line wrappers; extending an existing one means editing the service function and both transports get the change automatically.

### Streaming endpoints

`POST /conversations/:id/messages` (LLM chat) and `POST /claude/send` (Claude Code) are streaming. The service functions return `impl Stream<Item = ChatEvent>` / `impl Stream<Item = ClaudeStreamEvent>`, and each transport adapts the same stream:

- **REST / browser** — `Sse<Stream>` response, named SSE events matching the typed variants.
- **Tauri / desktop** — `tauri::ipc::Channel<T>` argument; the command pumps events through it until the stream is done.

The webui's `apiSendMessage()` and `apiClaudeSend()` helpers hide the difference — components always receive `{type: "delta", content: "..."}` regardless of transport.

### Event bridge

The daemon's `/api/ws` WebSocket forwards `SessionManager` broadcast events to the browser. In the desktop app, `main.rs` subscribes to the same broadcast channel at startup and re-emits each event as `app.emit("mn:server-event", ...)`. The `useWebSocket` hook in `utils.mjs` subscribes via `listen()` in Tauri mode and via `WebSocket` in browser mode — same `onEvent(evt)` callback, same payloads, same code.

## API

The REST API is the authoritative interface for the daemon. Every endpoint is mirrored by a `mn_*` Tauri command that accepts the same arguments.

### Sessions + recording

| Method | Endpoint | Tauri command |
|---|---|---|
| `POST` | `/api/sessions` | `mn_create_session` |
| `GET` | `/api/sessions` | `mn_list_sessions` |
| `GET` | `/api/sessions/:id` | `mn_get_session` |
| `PATCH` | `/api/sessions/:id` | `mn_update_session` |
| `DELETE` | `/api/sessions/:id` | `mn_delete_session` |
| `POST` | `/api/sessions/:id/recording/start` | `mn_start_recording` |
| `POST` | `/api/sessions/:id/recording/stop` | `mn_stop_recording` |
| `GET` | `/api/sessions/:id/files` | `mn_list_files` |
| `GET` | `/api/sessions/:id/files/:name` | `mn_resolve_session_file` |
| `GET` | `/api/sessions/:id/waveform/:name` | `mn_get_waveform` |

### Transcripts + summaries + todos

| Method | Endpoint | Tauri command |
|---|---|---|
| `GET` | `/api/sessions/:id/transcript` | `mn_get_transcript` |
| `DELETE` | `/api/sessions/:id/transcript` | `mn_delete_transcript` |
| `GET` | `/api/sessions/:id/attribution` | `mn_get_attribution` |
| `POST` | `/api/sessions/:id/attribution` | `mn_update_attribution` |
| `POST` | `/api/sessions/:id/transcribe` | `mn_transcribe_session` |
| `POST` | `/api/sessions/:id/summarize` | `mn_summarize_session` |
| `GET` | `/api/sessions/:id/summary` | `mn_get_summary` |
| `PATCH` | `/api/sessions/:id/summary` | `mn_update_summary` |
| `DELETE` | `/api/sessions/:id/summary` | `mn_delete_summary` |
| `GET` | `/api/sessions/:id/todos` | `mn_get_session_todos` |
| `PATCH` | `/api/sessions/:id/todos/:idx` | `mn_toggle_todo` |

### People + tags

| Method | Endpoint | Tauri command |
|---|---|---|
| `GET / POST` | `/api/people` | `mn_list_people` / `mn_create_person` |
| `GET / PATCH / DELETE` | `/api/people/:id` | `mn_{get,update,delete}_person` |
| `GET` | `/api/people/:id/sessions` | `mn_get_person_sessions` |
| `GET` | `/api/people/:id/todos` | `mn_get_person_todos` |
| `GET / POST` | `/api/tags` | `mn_list_tags` / `mn_create_tag` |
| `GET / PATCH / DELETE` | `/api/tags/:name` | `mn_{get_tag_sessions,update_tag,delete_tag}` |
| `PUT` | `/api/sessions/:id/tags` | `mn_set_session_tags` |

### Chat + Claude Code

| Method | Endpoint | Tauri command |
|---|---|---|
| `GET / POST` | `/api/conversations` | `mn_list_conversations` / `mn_create_conversation` |
| `GET / DELETE` | `/api/conversations/:id` | `mn_{get,delete}_conversation` |
| `POST` | `/api/conversations/:id/messages` **(streaming)** | `mn_send_message` (Channel) |
| `DELETE` | `/api/conversations/:id/messages/:msg_id` | `mn_delete_message` |
| `POST` | `/api/conversations/:id/claude-sync` | `mn_sync_claude_messages` |
| `GET` | `/api/conversations/:id/export-prompt` | `mn_export_prompt` |
| `GET` | `/api/llm/models` | `mn_list_models` |
| `GET` | `/api/claude/status` | `mn_claude_status` |
| `POST` | `/api/claude/stop` | `mn_claude_stop` |
| `POST` | `/api/claude/approve-tools` | `mn_claude_approve_tools` |
| `POST` | `/api/claude/send` **(streaming)** | `mn_claude_send` (Channel) |
| `GET` | `/api/claude/sessions[/:id]` | `mn_claude_{list,get}_session[s]` |

### Settings + config + events

| Method | Endpoint | Tauri command |
|---|---|---|
| `GET / PUT` | `/api/settings` | `mn_get_settings` / `mn_update_settings` |
| `GET` | `/api/config` | `mn_get_config` |
| `GET` | `/api/app-info` | `mn_get_app_info` |
| `WS` | `/api/ws` | Tauri event: `mn:server-event` |

## Design system

UI colors, typography, spacing, radii, and elevation all come from CSS custom properties in `apps/webui/tokens.css`. Tailwind's inline config in `index.html` binds those tokens into an `mn-*` utility namespace:

```html
<div class="bg-mn-elevated border border-mn-border rounded-mn-lg shadow-mn-sm text-mn-text">
  <h2 class="text-mn-lg font-semibold">Session</h2>
  <p class="text-mn-sm text-mn-text-dim">Every voice, on the record.</p>
</div>
```

Every token has a `prefers-color-scheme: dark` counterpart; no component hand-rolls its own dark style. The palette is neutral grays with a single accent (`#0a84ff`), plus `--mn-seal-red` (`#c62828`) for brand-emphasis elements that echo the 主 seal motif.

The scale follows macOS system defaults — 4 px spacing grid, 11/12/13/15/17/22/28 px type sizes, system font (SF Pro / Segoe UI Variable / system-ui) with no webfonts.

## Internationalization

`apps/webui/i18n.mjs` is a minimal bundler-free i18n layer. Strings live in `apps/webui/i18n/<locale>.json` as flat dot-notation keys:

```json
{
  "sidebar.new_session": "New session",
  "session.record_start": "Start recording",
  "common.save": "Save"
}
```

Components use the React hook:

```jsx
const { t, locale, setLocale } = useLocale();
return <button>{t('session.record_start')}</button>;
```

Locale resolution order on first load:

1. `localStorage.mn_locale` (explicit user pick)
2. Backend-detected system locale via `mn_get_app_info` / `/api/app-info`
3. `navigator.language`
4. Fallback `en`

Adding a new language is a single-file job: drop `apps/webui/i18n/<code>.json` in place with the same keys translated. **Simplified Chinese (`zh-CN`) is the obvious first add** given the app's Chinese name, 主簿.

## Development

```bash
# Run the daemon with debug logging
RUST_LOG=meeting_notes_daemon=debug cargo run -p meeting-notes-daemon -- serve --web-ui

# Run the Tauri desktop app with hot-reload
cd apps/desktop && cargo tauri dev

# Full workspace build (daemon + desktop + file-drop)
cargo build --workspace

# All tests
cargo test --workspace
```

The webui has no build step — it's plain ES modules loaded directly from disk. Tailwind runs at runtime via `/vendor/tailwind.js`, React and `marked` come from esm.sh via an `importmap`. Edit any `.mjs` file and just reload the window.

## Roadmap

- [x] Multi-track audio recording (mic + system)
- [x] Full meeting transcription
- [x] Speaker diarization
- [x] People management
- [x] Tags management
- [x] Chat with X (a particular meeting, person, tag)
- [x] Meeting summary and TODO extraction
- [x] Claude Code integration
- [x] Service layer refactor (single source of truth for both transports)
- [x] VoiceRecords macOS desktop app (Tauri 2)
- [x] Design system tokens (light + dark)
- [x] i18n scaffolding (English shipped; zh-CN hook ready)
- [ ] Final hand-drawn app icon (placeholder ships today)
- [ ] Simplified Chinese translation (`i18n/zh-CN.json`)
- [ ] Incremental component migration onto `mn-*` tokens
- [ ] Code-signing + notarization for macOS distribution
- [ ] Search
- [ ] Logseq plugin
- [ ] Obsidian plugin
- [ ] Windows support (WASAPI loopback — scaffolding already in place)
- [ ] Linux support (PipeWire — scaffolding already in place)
