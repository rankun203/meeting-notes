//! People service — CRUD + session membership lookups.

use serde::Deserialize;
use serde_json::{Value, json};

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreatePersonInput {
    pub name: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdatePersonInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default)]
    pub starred: Option<bool>,
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn list_people(state: &AppState) -> ServiceResult<Value> {
    let people = state.people_manager.list_people().await;
    Ok(json!({ "people": people }))
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn create_person(state: &AppState, input: CreatePersonInput) -> ServiceResult<Value> {
    let person = state
        .people_manager
        .create_person(input.name, input.notes)
        .await
        .map_err(ServiceError::Internal)?;
    state.refresh_people_index();
    Ok(serde_json::to_value(person).unwrap())
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn get_person(state: &AppState, id: &str) -> ServiceResult<Value> {
    let person = state
        .people_manager
        .get_person(id)
        .await
        .ok_or_else(|| ServiceError::NotFound("person not found".into()))?;
    Ok(serde_json::to_value(person).unwrap())
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn update_person(
    state: &AppState,
    id: &str,
    input: UpdatePersonInput,
) -> ServiceResult<Value> {
    let person = state
        .people_manager
        .update_person(id, input.name, input.notes, input.starred)
        .await
        .map_err(ServiceError::NotFound)?;
    state.refresh_people_index();
    Ok(serde_json::to_value(person).unwrap())
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn delete_person(state: &AppState, id: &str) -> ServiceResult<()> {
    state
        .people_manager
        .delete_person(id)
        .await
        .map_err(ServiceError::NotFound)?;
    state.refresh_people_index();
    Ok(())
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn get_person_sessions(state: &AppState, person_id: &str) -> ServiceResult<Value> {
    let session_ids = state.files_db.get_person_session_ids(person_id).await;

    let mut result: Vec<Value> = Vec::new();
    for sid in &session_ids {
        if let Some(info) = state.session_manager.get_session(sid).await {
            let mut matched_speakers: Vec<Value> = Vec::new();
            if let Some(transcript) = state.files_db.get_transcript(sid).await {
                if let Some(embs) = transcript.get("speaker_embeddings").and_then(|v| v.as_object()) {
                    for (speaker_key, entry) in embs {
                        if entry.get("person_id").and_then(|v| v.as_str()) == Some(person_id) {
                            matched_speakers.push(json!({
                                "speaker": speaker_key,
                                "confidence": entry.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            }));
                        }
                    }
                }
            }

            result.push(json!({
                "id": info.id,
                "name": info.name,
                "state": info.state,
                "created_at": info.created_at,
                "updated_at": info.updated_at,
                "duration_secs": info.duration_secs,
                "matched_speakers": matched_speakers,
            }));
        }
    }

    result.sort_by(|a, b| {
        let a_t = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        let b_t = b.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        b_t.cmp(a_t)
    });

    Ok(json!({ "sessions": result }))
}
