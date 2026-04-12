//! Summary service — summary CRUD, todos, and the summarization LLM kickoff.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{error, info, warn};

use crate::filesdb::FilesDb;
use crate::llm::secrets::SharedSecrets;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::settings::SharedSettings;
use crate::tags::TagsManager;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateSummaryInput {
    pub content: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct SummarizeInput {
    #[serde(default)]
    pub additional_instructions: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SummarizeAccepted {
    pub status: &'static str,
}

pub async fn get_summary(state: &AppState, id: &str) -> ServiceResult<Value> {
    let dir = state.session_manager.session_dir(id);
    let path = dir.join("summary.json");
    if !path.exists() {
        return Err(ServiceError::NotFound("summary not found".into()));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| ServiceError::Internal(format!("Failed to read summary: {e}")))?;
    serde_json::from_str(&content)
        .map_err(|e| ServiceError::Internal(format!("Failed to parse summary: {e}")))
}

pub async fn update_summary(
    state: &AppState,
    id: &str,
    input: UpdateSummaryInput,
) -> ServiceResult<Value> {
    let dir = state.session_manager.session_dir(id);
    let json_path = dir.join("summary.json");
    if !json_path.exists() {
        return Err(ServiceError::NotFound("summary not found".into()));
    }

    let existing = std::fs::read_to_string(&json_path)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;
    let mut summary: Value = serde_json::from_str(&existing)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;
    summary["content"] = json!(input.content);

    let json_str = serde_json::to_string_pretty(&summary)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;
    std::fs::write(&json_path, json_str)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;

    let md_path = dir.join("summary.md");
    let _ = std::fs::write(&md_path, &input.content);

    Ok(summary)
}

pub async fn delete_summary(state: &AppState, id: &str) -> ServiceResult<()> {
    let dir = state.session_manager.session_dir(id);
    let path = dir.join("summary.json");
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| ServiceError::Internal(format!("Failed to delete summary: {e}")))?;
    }
    Ok(())
}

pub async fn get_session_todos(state: &AppState, id: &str) -> ServiceResult<Value> {
    let path = state
        .session_manager
        .session_dir(id)
        .join("todos.json");
    if !path.exists() {
        return Ok(json!({"items": []}));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;
    serde_json::from_str(&content).map_err(|e| ServiceError::Internal(format!("{e}")))
}

pub async fn toggle_todo(state: &AppState, id: &str, idx: usize) -> ServiceResult<Value> {
    let dir = state.session_manager.session_dir(id);
    let todos_path = dir.join("todos.json");
    if !todos_path.exists() {
        return Err(ServiceError::NotFound("no todos".into()));
    }
    let content = std::fs::read_to_string(&todos_path)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;
    let mut todos: Value = serde_json::from_str(&content)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;

    let items = todos
        .get_mut("items")
        .and_then(|i| i.as_array_mut())
        .ok_or_else(|| ServiceError::BadRequest("invalid todos format".into()))?;

    if idx >= items.len() {
        return Err(ServiceError::NotFound("todo index out of range".into()));
    }

    let completed = items[idx]
        .get("completed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    items[idx]["completed"] = json!(!completed);

    let json_str = serde_json::to_string_pretty(&todos)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;
    std::fs::write(&todos_path, json_str)
        .map_err(|e| ServiceError::Internal(format!("{e}")))?;

    // Also update the summary.md and summary.json checkbox states
    let summary_json_path = dir.join("summary.json");
    if summary_json_path.exists() {
        if let Ok(s) = std::fs::read_to_string(&summary_json_path) {
            if let Ok(mut sj) = serde_json::from_str::<Value>(&s) {
                if let Some(md) = sj
                    .get("content")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
                {
                    let mut n = 0usize;
                    let new_md = regex::Regex::new(r"- \[([ xX])\]")
                        .unwrap()
                        .replace_all(&md, |_caps: &regex::Captures| {
                            let result = if n == idx {
                                if !completed {
                                    "- [x]"
                                } else {
                                    "- [ ]"
                                }
                            } else {
                                _caps.get(0).unwrap().as_str()
                            };
                            n += 1;
                            result.to_string()
                        })
                        .to_string();
                    sj["content"] = json!(new_md);
                    let _ = std::fs::write(
                        &summary_json_path,
                        serde_json::to_string_pretty(&sj).unwrap_or_default(),
                    );
                    let _ = std::fs::write(dir.join("summary.md"), &new_md);
                }
            }
        }
    }

    Ok(todos)
}

pub async fn get_person_todos(state: &AppState, person_id: &str) -> ServiceResult<Value> {
    let session_ids = state.files_db.get_person_session_ids(person_id).await;

    let mut result: Vec<Value> = Vec::new();
    for sid in &session_ids {
        let todos_path = state
            .session_manager
            .session_dir(sid)
            .join("todos.json");
        if !todos_path.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&todos_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let todos: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let items = match todos.get("items").and_then(|i| i.as_array()) {
            Some(items) => items,
            None => continue,
        };

        let session_info = state.session_manager.get_session(sid).await;
        let session_name = session_info
            .as_ref()
            .and_then(|s| s.name.clone())
            .unwrap_or_else(|| sid.clone());
        let session_created = session_info.as_ref().map(|s| s.created_at.to_rfc3339());

        for (idx, item) in items.iter().enumerate() {
            if item.get("person_id").and_then(|v| v.as_str()) == Some(person_id) {
                let mut todo = item.clone();
                todo["session_id"] = json!(sid);
                todo["session_name"] = json!(session_name);
                todo["session_created_at"] = json!(session_created);
                todo["todo_index"] = json!(idx);
                result.push(todo);
            }
        }
    }

    Ok(json!({"todos": result}))
}

/// Kick off LLM summarization for a session. Returns 202-semantics; the
/// actual LLM call runs in `spawn_task` on the tokio runtime.
pub async fn summarize_session(
    state: &AppState,
    id: &str,
    input: SummarizeInput,
    spawn_task: impl FnOnce(SummarizeArgs) + Send + 'static,
) -> ServiceResult<SummarizeAccepted> {
    let settings = state.settings.read().await;
    let host = settings.llm_host.clone();
    let model = settings
        .summarization_model
        .clone()
        .unwrap_or_else(|| settings.llm_model.clone());
    let mut prompt = settings.summarization_prompt.clone().unwrap_or_default();
    let sum_sort = settings.summarization_openrouter_sort.clone();
    drop(settings);

    if let Some(extra) = input.additional_instructions {
        if !extra.trim().is_empty() {
            prompt.push_str(&format!("\n\nAdditional instructions: {}", extra.trim()));
        }
    }

    let secrets = state.llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    if api_key.is_empty() {
        return Err(ServiceError::BadRequest(
            "LLM API key not configured".into(),
        ));
    }

    let dir = state.session_manager.session_dir(id);
    let transcript_path = dir.join("transcript.json");
    if !transcript_path.exists() {
        return Err(ServiceError::BadRequest(
            "No transcript available to summarize".into(),
        ));
    }

    spawn_task(SummarizeArgs {
        session_id: id.to_string(),
        session_dir: dir,
        host,
        api_key,
        model,
        prompt,
        sum_sort,
    });

    Ok(SummarizeAccepted {
        status: "processing",
    })
}

/// Owned snapshot the summarize background task needs.
pub struct SummarizeArgs {
    pub session_id: String,
    pub session_dir: std::path::PathBuf,
    pub host: String,
    pub api_key: String,
    pub model: String,
    pub prompt: String,
    pub sum_sort: Option<String>,
}

/// Check settings and run auto-summarization if enabled.
/// Called from the end of the transcription pipeline.
pub async fn maybe_auto_summarize(
    session_id: &str,
    session_manager: &SessionManager,
    settings: &SharedSettings,
    llm_secrets: &SharedSecrets,
    tags_manager: &TagsManager,
    people_manager: &PeopleManager,
    files_db: &FilesDb,
) {
    let s = settings.read().await;
    if !s.auto_summarize {
        return;
    }
    let host = s.llm_host.clone();
    let model = s
        .summarization_model
        .clone()
        .unwrap_or_else(|| s.llm_model.clone());
    let prompt = s.summarization_prompt.clone().unwrap_or_default();
    let sum_sort = s.summarization_openrouter_sort.clone();
    drop(s);

    let secrets = llm_secrets.read().await;
    let api_key = secrets.get_api_key(&host).cloned().unwrap_or_default();
    drop(secrets);

    if api_key.is_empty() {
        warn!("[{}] Auto-summarize skipped: no LLM API key configured", session_id);
        return;
    }

    let dir = session_manager.session_dir(session_id);
    let session_info = session_manager.get_session(session_id).await;

    session_manager.emit_summary_progress(session_id, "summarizing").await;

    match crate::chat::summarize::run_summarization(
        session_id,
        &dir,
        &host,
        &api_key,
        &model,
        &prompt,
        session_info.as_ref(),
        tags_manager,
        people_manager,
        session_manager,
        sum_sort.as_deref(),
    )
    .await
    {
        Ok(_) => {
            session_manager.refresh_files(session_id).await;
            session_manager.emit_summary_completed(session_id).await;
            let recordings_dir = files_db.recordings_dir().to_path_buf();
            let mut sessions = session_manager.session_entries().await;
            crate::markdown::write_recordings_index(&recordings_dir, &mut sessions);
            info!("[{}] Auto-summary generated", session_id);
        }
        Err(e) => {
            error!("[{}] Auto-summary failed: {}", session_id, e);
            session_manager.emit_summary_failed(session_id, &e).await;
        }
    }
}
