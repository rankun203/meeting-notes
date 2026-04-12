//! Transport-agnostic service layer.
//!
//! Every piece of business logic the REST API or Tauri command layer wants
//! to call lives here. Handlers in `server::routes` become thin wrappers
//! that map service results onto HTTP responses; future Tauri commands will
//! be equally thin wrappers that call the exact same functions.
//!
//! Services take `&AppState` and owned inputs, return `ServiceResult<T>`,
//! and never reference axum or tauri types directly.

pub mod chat;
pub mod claude;
pub mod config;
pub mod error;
pub mod files;
pub mod people;
pub mod serde_helpers;
pub mod sessions;
pub mod settings;
pub mod state;
pub mod summary;
pub mod tags;
pub mod transcripts;

pub use error::{ServiceError, ServiceResult};
pub use state::AppState;
