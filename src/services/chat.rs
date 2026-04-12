//! Chat service — conversation CRUD, message management, and LLM model listing.
//!
//! Note: `send_message` (the streaming chat endpoint) is not yet extracted
//! here — it still lives in `server::routes` because its SSE response
//! construction is tightly coupled to axum. A future pass will introduce a
//! typed `ChatEvent` stream in this module that both the axum SSE handler
//! and a Tauri event-emitter can consume.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::chat::types::Message;
use crate::llm::client::LlmClient;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct CreateConversationInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub chat_backend: Option<String>,
}

pub async fn list_conversations(state: &AppState) -> ServiceResult<Value> {
    let summaries = state.conversation_manager.list(10);
    Ok(json!({ "conversations": summaries }))
}

pub async fn create_conversation(
    state: &AppState,
    input: CreateConversationInput,
) -> ServiceResult<Value> {
    let mut conv = state
        .conversation_manager
        .create(input.title)
        .map_err(ServiceError::Internal)?;
    if input.chat_backend.is_some() {
        conv.chat_backend = input.chat_backend;
        state
            .conversation_manager
            .save(&conv)
            .map_err(ServiceError::Internal)?;
    }
    Ok(serde_json::to_value(&conv).unwrap_or_default())
}

pub async fn get_conversation(state: &AppState, id: &str) -> ServiceResult<Value> {
    state
        .conversation_manager
        .get_transformed(id)
        .ok_or_else(|| ServiceError::NotFound("conversation not found".into()))
}

pub async fn delete_conversation(state: &AppState, id: &str) -> ServiceResult<()> {
    state
        .conversation_manager
        .delete(id)
        .map_err(ServiceError::Internal)
}

pub async fn delete_message(
    state: &AppState,
    conv_id: &str,
    msg_id: &str,
) -> ServiceResult<()> {
    state
        .conversation_manager
        .delete_message(conv_id, msg_id)
        .map_err(|e| {
            if e.contains("not found") {
                ServiceError::NotFound(e)
            } else {
                ServiceError::Internal(e)
            }
        })
}

/// Sync messages from a Claude Code session into an app conversation.
pub async fn sync_claude_messages(
    state: &AppState,
    conv_id: &str,
    body: Value,
) -> ServiceResult<Value> {
    let mut conv = state
        .conversation_manager
        .get(conv_id)
        .ok_or_else(|| ServiceError::NotFound("conversation not found".into()))?;

    if let Some(sid) = body.get("claude_session_id").and_then(|v| v.as_str()) {
        conv.claude_session_id = Some(sid.to_string());
    }

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        let now = chrono::Utc::now();
        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let msg_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("msg_{}", now.timestamp_nanos_opt().unwrap_or(0)))
                .to_string();

            if content.is_empty() {
                continue;
            }

            match role {
                "user" => {
                    let mentions: Vec<crate::chat::types::Mention> = msg
                        .get("mentions")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    conv.messages.push(Message::User {
                        id: msg_id,
                        content,
                        mentions,
                        timestamp: now,
                    });
                }
                "assistant" => {
                    conv.messages.push(Message::Assistant {
                        id: msg_id,
                        content,
                        timestamp: now,
                        usage: None,
                    });
                }
                _ => {}
            }
        }
    }

    conv.updated_at = chrono::Utc::now();
    if conv.title.is_empty() {
        if let Some(first_user) = conv
            .messages
            .iter()
            .find(|m| matches!(m, Message::User { .. }))
        {
            if let Message::User { content, .. } = first_user {
                conv.title = content.chars().take(60).collect();
            }
        }
    }

    state
        .conversation_manager
        .save(&conv)
        .map_err(ServiceError::Internal)?;

    Ok(json!({ "ok": true }))
}

/// Export a conversation as plain text (system prompt + context + messages).
pub async fn export_prompt(state: &AppState, id: &str) -> ServiceResult<String> {
    let conv = state
        .conversation_manager
        .get(id)
        .ok_or_else(|| ServiceError::NotFound("conversation not found".into()))?;

    let context_chunks: Vec<crate::chat::types::ContextChunk> = conv
        .messages
        .iter()
        .rev()
        .find_map(|m| {
            if let Message::ContextResult { chunks, .. } = m {
                Some(chunks.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let context_str = crate::llm::prompt::format_context(&context_chunks);
    let self_intro = state.settings.read().await.chat_self_intro.clone();
    Ok(crate::llm::prompt::format_as_text(
        &conv,
        &context_str,
        self_intro.as_deref(),
    ))
}

pub async fn list_models(state: &AppState) -> ServiceResult<Value> {
    let settings = state.settings.read().await;
    let host = settings.llm_host.clone();
    drop(settings);

    let secrets = state.llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    if api_key.is_empty() {
        return Err(ServiceError::BadRequest(
            "LLM API key not configured".into(),
        ));
    }

    LlmClient::list_models(&host, &api_key)
        .await
        .map_err(ServiceError::BadGateway)
}
