//! Claude Code service — non-streaming endpoints (status, stop, approve-tools,
//! list-sessions, get-session).
//!
//! The streaming `claude_send` handler (SSE with init/delta/tool-use/done
//! events) is not yet extracted here — it stays in `server::routes` until
//! the typed event-stream abstraction is introduced. See `chat.rs` for the
//! same note.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::llm::claude_code::ClaudeCodeRunner;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ApproveToolsInput {
    #[serde(default)]
    pub tools: Vec<String>,
    /// `"once" | "session" | "permanent"`. Falls back to `permanent` for the
    /// legacy `permanent: true` shape used by older clients.
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub permanent: Option<bool>,
}

pub async fn status(state: &AppState) -> ServiceResult<Value> {
    let available = ClaudeCodeRunner::is_available().await;
    let running = state.claude_runner.is_running().await;
    Ok(json!({
        "available": available,
        "running": running,
    }))
}

pub async fn stop(state: &AppState) -> ServiceResult<Value> {
    let stopped = state.claude_runner.stop().await;
    Ok(json!({ "stopped": stopped }))
}

pub async fn approve_tools(state: &AppState, input: ApproveToolsInput) -> ServiceResult<Value> {
    if input.tools.is_empty() {
        return Err(ServiceError::BadRequest("no tools specified".into()));
    }

    let scope = input
        .scope
        .as_deref()
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if input.permanent.unwrap_or(false) {
                "permanent".to_string()
            } else {
                "once".to_string()
            }
        });

    match scope.as_str() {
        "permanent" => {
            state
                .claude_runner
                .approve_tools_permanent(&input.tools)
                .map_err(ServiceError::Internal)?;
            state.claude_runner.approve_tools_session(&input.tools).await;
        }
        "session" => {
            state.claude_runner.approve_tools_session(&input.tools).await;
        }
        _ => {
            // "once": add to session list so the immediate retry works,
            // removed automatically after the next run completes.
            state.claude_runner.approve_tools_once(&input.tools).await;
        }
    }

    Ok(json!({ "approved": input.tools, "scope": scope }))
}

pub async fn list_sessions(state: &AppState) -> ServiceResult<Value> {
    let sessions = state.claude_runner.list_sessions();
    Ok(json!({ "sessions": sessions }))
}

pub async fn get_session(state: &AppState, id: &str) -> ServiceResult<Value> {
    let messages = state
        .claude_runner
        .load_session(id)
        .ok_or_else(|| ServiceError::NotFound("session not found".into()))?;
    Ok(json!({ "session_id": id, "messages": messages }))
}
