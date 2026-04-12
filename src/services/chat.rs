//! Chat service — conversation CRUD, message management, LLM model
//! listing, and the streaming `send_message` pipeline.
//!
//! The streaming endpoint returns an `impl Stream<Item = ChatEvent>`
//! that both `server::routes` (axum SSE) and the Tauri command layer
//! (via `tauri::ipc::Channel`) consume. Event payloads are plain data
//! that map cleanly onto both transports.

use std::collections::HashSet;
use std::pin::Pin;

use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::chat::types::{ContextCriteria, Message, Mention};
use crate::llm::client::LlmClient;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

/// Typed event emitted by the chat streaming pipeline.
///
/// `tag = "type"` plus `snake_case` means the JSON wire format is exactly
/// what the webui already expects from the SSE endpoint (`"type":"delta"`,
/// `"type":"done"`, etc.) — with a `content` or inlined payload alongside.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    ContextLoaded {
        chunk_count: usize,
        session_count: usize,
    },
    Thinking {
        content: String,
    },
    Delta {
        content: String,
    },
    Usage(Value),
    Done {
        message_id: String,
    },
    Error {
        error: String,
    },
}

/// Input for `POST /conversations/{id}/messages` and `mn_send_message`.
#[derive(Debug, Deserialize)]
pub struct SendMessageInput {
    pub content: String,
    #[serde(default)]
    pub mentions: Vec<Mention>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateConversationInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub chat_backend: Option<String>,
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn list_conversations(state: &AppState) -> ServiceResult<Value> {
    let summaries = state.conversation_manager.list(10);
    Ok(json!({ "conversations": summaries }))
}

#[tracing::instrument(level = "info", skip_all)]
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

#[tracing::instrument(level = "info", skip_all)]
pub async fn get_conversation(state: &AppState, id: &str) -> ServiceResult<Value> {
    state
        .conversation_manager
        .get_transformed(id)
        .ok_or_else(|| ServiceError::NotFound("conversation not found".into()))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn delete_conversation(state: &AppState, id: &str) -> ServiceResult<()> {
    state
        .conversation_manager
        .delete(id)
        .map_err(ServiceError::Internal)
}

#[tracing::instrument(level = "info", skip_all)]
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

#[tracing::instrument(level = "info", skip_all)]
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

#[tracing::instrument(level = "info", skip_all)]
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

#[tracing::instrument(level = "info", skip_all)]
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

/// Pinned owned stream alias — convenient for callers storing the stream
/// on a struct field without tangling lifetimes.
pub type ChatEventStream = Pin<Box<dyn Stream<Item = ChatEvent> + Send>>;

#[tracing::instrument(level = "info", skip_all)]
/// Append a user message to a conversation, retrieve LLM context, call the
/// configured LLM with streaming, persist the final assistant message, and
/// return a `Stream<Item = ChatEvent>` that emits every incremental update
/// the frontend needs to render the chat.
///
/// The input validation (conversation lookup, message append, context
/// merge, user-message save) happens synchronously *before* the stream is
/// yielded — so a 404 or failed save produces a plain `ServiceError`
/// instead of a stream that immediately errors. The returned stream only
/// contains LLM-driven events (context_loaded, thinking, delta, usage,
/// done, error).
pub async fn send_message_stream(
    state: &AppState,
    conv_id: &str,
    input: SendMessageInput,
) -> ServiceResult<ChatEventStream> {
    let mut conv = state
        .conversation_manager
        .get(conv_id)
        .ok_or_else(|| ServiceError::NotFound("conversation not found".into()))?;

    let now = chrono::Utc::now();

    // Append user message.
    let user_msg_id = format!("msg_{}", now.timestamp_nanos_opt().unwrap_or(0));
    conv.messages.push(Message::User {
        id: user_msg_id,
        content: input.content.clone(),
        mentions: input.mentions.clone(),
        timestamp: now,
    });

    if conv.title.is_empty() {
        conv.title = input.content.chars().take(60).collect();
    }

    // Build context: fetch only new chunks, dedupe against the existing
    // context_result, preserve the rest.
    let new_criteria = ContextCriteria::from_mentions(&input.mentions);
    let existing_context: Vec<crate::chat::types::ContextChunk> = conv
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

    let context_chunks: Vec<crate::chat::types::ContextChunk> = if !new_criteria.is_empty() {
        let new_chunks = crate::llm::context::retrieve_context(
            &new_criteria,
            &state.files_db,
            &state.session_manager,
            &state.tags_manager,
            &state.people_manager,
        )
        .await;

        let chunk_key = |c: &crate::chat::types::ContextChunk| -> (String, String, u64) {
            if c.kind == "note" {
                let hash = c.note.as_deref().unwrap_or("").len() as u64;
                (c.kind.clone(), c.source_id.clone(), hash)
            } else {
                let start_bits = c
                    .segment
                    .as_ref()
                    .and_then(|s| s.get("start"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .to_bits();
                (c.kind.clone(), c.source_id.clone(), start_bits)
            }
        };

        let existing_keys: HashSet<_> = existing_context.iter().map(chunk_key).collect();
        let delta: Vec<_> = new_chunks
            .into_iter()
            .filter(|c| !existing_keys.contains(&chunk_key(c)))
            .collect();

        let mut combined = existing_context;
        combined.extend(delta.clone());

        combined.sort_by(|a, b| {
            let a_note = a.kind == "note";
            let b_note = b.kind == "note";
            if a_note != b_note {
                return if a_note {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            a.created_at.cmp(&b.created_at).then_with(|| {
                let as_ = a
                    .segment
                    .as_ref()
                    .and_then(|s| s.get("start"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let bs = b
                    .segment
                    .as_ref()
                    .and_then(|s| s.get("start"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                as_.partial_cmp(&bs).unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        let mut merged_criteria = ContextCriteria::default();
        for msg in &conv.messages {
            if let Message::ContextResult { criteria: prev, .. } = msg {
                merged_criteria.merge(prev);
            }
        }
        merged_criteria.merge(&new_criteria);

        if !combined.is_empty() {
            let ctx_msg_id = format!(
                "ctx_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            );
            conv.messages.push(Message::ContextResult {
                id: ctx_msg_id,
                criteria: merged_criteria,
                chunks: combined.clone(),
                timestamp: chrono::Utc::now(),
            });
        }

        combined
    } else if !existing_context.is_empty() {
        existing_context
    } else {
        Vec::new()
    };

    conv.updated_at = chrono::Utc::now();
    state
        .conversation_manager
        .save(&conv)
        .map_err(ServiceError::Internal)?;

    // Snapshot everything the async stream needs so no references into
    // `state` survive past this function's return.
    let context_str = crate::llm::prompt::format_context(&context_chunks);

    let settings = state.settings.read().await;
    let host = settings.llm_host.clone();
    let model = settings.llm_model.clone();
    let openrouter_sort = settings.openrouter_sort.clone();
    let self_intro = settings.chat_self_intro.clone();
    drop(settings);

    let llm_messages = crate::llm::prompt::build_messages(&conv, &context_str, self_intro.as_deref());

    let secrets = state.llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    let chunk_count = context_chunks.len();
    let session_count = {
        let mut sids: HashSet<&String> = HashSet::new();
        for c in &context_chunks {
            sids.insert(&c.source_id);
        }
        sids.len()
    };

    let conv_manager = state.conversation_manager.clone();
    let conv_id_owned = conv_id.to_string();

    let stream = async_stream::stream! {
        if chunk_count > 0 {
            yield ChatEvent::ContextLoaded { chunk_count, session_count };
        }

        if api_key.is_empty() {
            yield ChatEvent::Error {
                error: "LLM API key not configured. Set it in Settings > Services.".into(),
            };
            return;
        }

        let model_name = model.clone();
        let client = LlmClient::new(host, api_key, model).with_provider_sort(openrouter_sort);
        match client.stream_chat(llm_messages).await {
            Ok(llm_stream) => {
                let mut full_content = String::new();
                let mut usage_value: Option<Value> = None;
                let assistant_msg_id = format!(
                    "msg_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                );

                futures::pin_mut!(llm_stream);

                while let Some(result) = llm_stream.next().await {
                    match result {
                        Ok(content) => {
                            if let Some(thinking) = content.strip_prefix('\x01') {
                                yield ChatEvent::Thinking { content: thinking.to_string() };
                            } else if let Some(usage_str) = content.strip_prefix('\x02') {
                                if let Ok(u) = serde_json::from_str::<Value>(usage_str) {
                                    let prompt = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let completion = u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let total = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(prompt + completion);
                                    let cost = u.get("cost").and_then(|v| v.as_f64());
                                    let cost_str = cost.map(|c| format!(", cost ${:.4}", c)).unwrap_or_default();
                                    info!(
                                        "[{}] Chat {} — {} prompt + {} completion = {} tokens{}",
                                        conv_id_owned, model_name, prompt, completion, total, cost_str
                                    );
                                }
                                usage_value = serde_json::from_str(usage_str).ok();
                                let usage_val: Value = serde_json::from_str(usage_str)
                                    .unwrap_or_else(|_| json!({}));
                                yield ChatEvent::Usage(usage_val);
                            } else {
                                full_content.push_str(&content);
                                yield ChatEvent::Delta { content };
                            }
                        }
                        Err(e) => {
                            yield ChatEvent::Error { error: e };
                            return;
                        }
                    }
                }

                if !full_content.is_empty() {
                    if let Some(mut conv) = conv_manager.get(&conv_id_owned) {
                        conv.messages.push(Message::Assistant {
                            id: assistant_msg_id.clone(),
                            content: full_content,
                            timestamp: chrono::Utc::now(),
                            usage: usage_value,
                        });
                        conv.updated_at = chrono::Utc::now();
                        if let Err(e) = conv_manager.save(&conv) {
                            warn!("Failed to save assistant message: {}", e);
                        }
                    }
                }

                yield ChatEvent::Done { message_id: assistant_msg_id };
            }
            Err(e) => {
                yield ChatEvent::Error { error: e };
            }
        }
    };

    Ok(Box::pin(stream))
}
