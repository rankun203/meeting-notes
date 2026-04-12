//! Tags service — tag CRUD + cascade logic + session-tag assignment.

use serde::Deserialize;
use serde_json::{Value, json};

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateTagInput {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateTagInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
    /// Tri-state: `None` = don't touch, `Some(None)` = clear, `Some(Some(..))` = set.
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SetSessionTagsInput {
    #[serde(default)]
    pub tags: Vec<String>,
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn list_tags(state: &AppState) -> ServiceResult<Value> {
    let tags = state.tags_manager.list_tags().await;
    let counts = state.session_manager.tag_session_counts().await;
    let list: Vec<Value> = tags
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "hidden": t.hidden,
                "notes": t.notes,
                "session_count": counts.get(&t.name).copied().unwrap_or(0),
            })
        })
        .collect();
    Ok(json!({ "tags": list }))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn create_tag(state: &AppState, input: CreateTagInput) -> ServiceResult<Value> {
    let tag = state
        .tags_manager
        .create_tag(&input.name)
        .await
        .map_err(ServiceError::BadRequest)?;
    Ok(json!(tag))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn update_tag(
    state: &AppState,
    name: &str,
    input: UpdateTagInput,
) -> ServiceResult<Value> {
    let (tag, old_name) = state
        .tags_manager
        .update_tag(name, input.name.as_deref(), input.hidden, input.notes)
        .await
        .map_err(ServiceError::BadRequest)?;
    if let Some(old) = old_name {
        state
            .session_manager
            .rename_tag_in_all_sessions(&old, &tag.name)
            .await;
    }
    Ok(json!(tag))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn delete_tag(state: &AppState, name: &str) -> ServiceResult<()> {
    state
        .tags_manager
        .delete_tag(name)
        .await
        .map_err(ServiceError::NotFound)?;
    state
        .session_manager
        .remove_tag_from_all_sessions(name)
        .await;
    Ok(())
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn get_tag_sessions(state: &AppState, name: &str) -> ServiceResult<Value> {
    if !state.tags_manager.tag_exists(name).await {
        return Err(ServiceError::NotFound("tag not found".into()));
    }
    let sessions = state.session_manager.sessions_for_tag(name).await;
    Ok(json!({ "sessions": sessions }))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn set_session_tags(
    state: &AppState,
    session_id: &str,
    input: SetSessionTagsInput,
) -> ServiceResult<Value> {
    for tag in &input.tags {
        if !state.tags_manager.tag_exists(tag).await {
            return Err(ServiceError::BadRequest(format!(
                "tag '{}' does not exist",
                tag
            )));
        }
    }

    let info = state
        .session_manager
        .update_session_tags(session_id, input.tags)
        .await
        .map_err(ServiceError::NotFound)?;
    state.refresh_recordings_index();
    Ok(serde_json::to_value(info).unwrap())
}
