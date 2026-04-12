//! Settings service — read/update the persistent settings file, with the
//! LLM API key routed into the separate secrets file.

use serde_json::{Value, json};
use tracing::info;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[tracing::instrument(level = "info", skip_all)]
pub async fn get_settings(state: &AppState) -> ServiceResult<Value> {
    let settings = state.settings.read().await;
    let mut result = settings.to_masked_json();
    let secrets = state.llm_secrets.read().await;
    result.as_object_mut().unwrap().insert(
        "llm_api_key_set".to_string(),
        json!(secrets.has_api_key(&settings.llm_host)),
    );
    Ok(result)
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn update_settings(state: &AppState, body: Value) -> ServiceResult<Value> {
    let host_for_key = body
        .get("llm_host")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(v) = body.get("llm_api_key") {
        let key = v.as_str().map(|s| s.to_string());
        let host = match &host_for_key {
            Some(h) => h.clone(),
            None => state.settings.read().await.llm_host.clone(),
        };
        let mut secrets = state.llm_secrets.write().await;
        secrets
            .set_api_key(&host, key)
            .map_err(ServiceError::Internal)?;
        info!("LLM API key updated for host");
    }

    let mut settings = state.settings.write().await;
    settings.merge_and_save(&body).map_err(ServiceError::Internal)?;
    info!("Settings updated");

    let mut result = settings.to_masked_json();
    let secrets = state.llm_secrets.read().await;
    result.as_object_mut().unwrap().insert(
        "llm_api_key_set".to_string(),
        json!(secrets.has_api_key(&settings.llm_host)),
    );
    Ok(result)
}
