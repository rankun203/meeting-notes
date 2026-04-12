//! Claude Code service — status, stop, approve-tools, list-sessions,
//! get-session, plus the streaming `send` pipeline that wraps the Claude
//! CLI subprocess output.
//!
//! The streaming function returns `impl Stream<Item = ClaudeStreamEvent>`
//! consumed by both `server::routes` (axum SSE) and the Tauri command
//! layer (via `tauri::ipc::Channel`).

use std::pin::Pin;

use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::chat::types::Mention;
use crate::llm::claude_code::{ClaudeCodeRunner, ClaudeEvent};

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

#[tracing::instrument(level = "info", skip_all)]
pub async fn status(state: &AppState) -> ServiceResult<Value> {
    let available = ClaudeCodeRunner::is_available().await;
    let running = state.claude_runner.is_running().await;
    Ok(json!({
        "available": available,
        "running": running,
    }))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn stop(state: &AppState) -> ServiceResult<Value> {
    let stopped = state.claude_runner.stop().await;
    Ok(json!({ "stopped": stopped }))
}

#[tracing::instrument(level = "info", skip_all)]
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

#[tracing::instrument(level = "info", skip_all)]
pub async fn list_sessions(state: &AppState) -> ServiceResult<Value> {
    let sessions = state.claude_runner.list_sessions();
    Ok(json!({ "sessions": sessions }))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn get_session(state: &AppState, id: &str) -> ServiceResult<Value> {
    let messages = state
        .claude_runner
        .load_session(id)
        .ok_or_else(|| ServiceError::NotFound("session not found".into()))?;
    Ok(json!({ "session_id": id, "messages": messages }))
}

// ---- Streaming send ----

/// Typed event emitted by the Claude streaming pipeline. Wraps every
/// `ClaudeEvent` variant plus an initial `Prompt` frame that carries the
/// fully-resolved prompt (with mentions inlined) so the webui can export
/// the exact prompt that was sent to Claude.
///
/// Tag values are kebab-friendly / snake_case, matching what the webui
/// already listens for on the SSE endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeStreamEvent {
    Prompt {
        full_prompt: String,
    },
    Init {
        session_id: String,
        model: String,
    },
    Delta {
        content: String,
    },
    ToolUse {
        tool: String,
        input_summary: String,
    },
    Done {
        session_id: String,
        cost_usd: f64,
        result: String,
    },
    Error {
        error: String,
    },
    PermissionRequest {
        tools: Vec<String>,
    },
}

impl From<ClaudeEvent> for ClaudeStreamEvent {
    fn from(ev: ClaudeEvent) -> Self {
        match ev {
            ClaudeEvent::Init { session_id, model } => Self::Init { session_id, model },
            ClaudeEvent::Delta { content } => Self::Delta { content },
            ClaudeEvent::ToolUse { tool, input_summary } => Self::ToolUse { tool, input_summary },
            ClaudeEvent::Done {
                session_id,
                cost_usd,
                result,
            } => Self::Done {
                session_id,
                cost_usd,
                result,
            },
            ClaudeEvent::Error { error } => Self::Error { error },
            ClaudeEvent::PermissionRequest { tools } => Self::PermissionRequest { tools },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SendInput {
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub mentions: Vec<Mention>,
}

pub type ClaudeStream = Pin<Box<dyn Stream<Item = ClaudeStreamEvent> + Send>>;

#[tracing::instrument(level = "info", skip_all)]
/// Resolve mentions, spawn a Claude Code CLI run, and return a stream that
/// first yields the full prompt (for export) and then every runner event.
pub async fn send_stream(state: &AppState, input: SendInput) -> ServiceResult<ClaudeStream> {
    if input.prompt.is_empty() {
        return Err(ServiceError::BadRequest("prompt is required".into()));
    }

    // Resolve @ mentions into a plain-text "Referenced:" block prepended to
    // the prompt so Claude has the inlined context.
    let mut lines = Vec::new();
    for m in &input.mentions {
        let mtype = m.kind.as_str();
        let id = m.id.as_str();
        let label = if m.label.is_empty() { id } else { m.label.as_str() };
        match mtype {
            "session" => {
                lines.push(format!("- Session \"{}\" (id: {})", label, id));
            }
            "person" => {
                let person = state.people_manager.get_person(id).await;
                let name = person.as_ref().map(|p| p.name.as_str()).unwrap_or(label);
                lines.push(format!("- Person \"{}\" (id: {})", name, id));
            }
            "tag" => {
                let tag = state.tags_manager.get_tag(label).await;
                let notes = tag.as_ref().and_then(|t| t.notes.as_deref()).unwrap_or("");
                if notes.is_empty() {
                    lines.push(format!("- Tag \"{}\"", label));
                } else {
                    lines.push(format!("- Tag \"{}\": {}", label, notes));
                }
            }
            _ => {}
        }
    }
    let mentions_context = if lines.is_empty() {
        None
    } else {
        Some(format!("Referenced:\n{}", lines.join("\n")))
    };

    let claude_model = state.settings.read().await.claude_code_model.clone();

    let full_prompt = match &mentions_context {
        Some(ctx) => format!("{}\n\n---\n{}", ctx, input.prompt),
        None => input.prompt.clone(),
    };

    let mut rx = state
        .claude_runner
        .run(
            &input.prompt,
            input.session_id.as_deref(),
            mentions_context.as_deref(),
            claude_model.as_deref(),
        )
        .await
        .map_err(ServiceError::Conflict)?;

    let stream = async_stream::stream! {
        yield ClaudeStreamEvent::Prompt { full_prompt };
        while let Some(event) = rx.recv().await {
            yield ClaudeStreamEvent::from(event);
        }
    };

    Ok(Box::pin(stream))
}
