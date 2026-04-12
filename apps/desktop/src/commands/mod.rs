//! Tauri command wrappers — one file per service module.
//!
//! Every command is a 3–6 line `#[tauri::command]` shim that calls the
//! matching function in `meeting_notes_daemon::services::*`. The services
//! layer is the single source of truth for business logic; these wrappers
//! only translate between Tauri's `State<AppState>` / owned-input calling
//! convention and the service functions' `&AppState` signatures.
//!
//! Missing on purpose (for now):
//!   - Streaming endpoints (chat `send_message`, claude `send`). Those
//!     need a typed event-stream abstraction in the services layer so
//!     they can feed both axum SSE and `app.emit()` from the same source.
//!     Tracked in the commit message.

pub mod chat;
pub mod claude;
pub mod config;
pub mod diagnostics;
pub mod files;
pub mod people;
pub mod sessions;
pub mod settings;
pub mod summary;
pub mod tags;
pub mod transcripts;
