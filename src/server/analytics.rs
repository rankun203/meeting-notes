//! Explicit, content-free usage events. Best effort delivery never blocks app actions.
use super::routes::AppState;
use crate::llm::secrets::SharedSecrets;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

#[derive(Clone)]
struct AnalyticsState {
    secrets: SharedSecrets,
    sender: mpsc::Sender<Value>,
}

pub fn routes(secrets: SharedSecrets) -> Router<AppState> {
    let (sender, mut receiver) = mpsc::channel::<Value>(128);
    let worker_secrets = secrets.clone();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        while let Some(first) = receiver.recv().await {
            let mut batch = vec![first];
            while batch.len() < 32 {
                match receiver.try_recv() {
                    Ok(v) => batch.push(v),
                    Err(_) => break,
                }
            }
            let config = worker_secrets.read().await.clone();
            if !config.analytics_enabled() || !valid_host(&config.posthog_host) {
                continue;
            }
            let result = client
                .post(format!("{}/batch/", config.posthog_host))
                .json(&json!({"api_key": config.posthog_project_token, "batch": batch}))
                .send()
                .await;
            if !matches!(result, Ok(ref response) if response.status().is_success()) {
                // Never log request bodies, tokens, URLs, or content.
                tracing::debug!("Usage analytics delivery failed; dropping batch");
            }
        }
    });
    Router::new()
        .route("/analytics/config", get(config).put(update_config))
        .route("/analytics/events", post(capture))
        .with_state(AnalyticsState { secrets, sender })
}

fn valid_host(host: &str) -> bool {
    matches!(
        host,
        "https://us.i.posthog.com" | "https://eu.i.posthog.com"
    )
}

fn masked_config(secrets: &crate::llm::secrets::LlmSecrets) -> Value {
    json!({"enabled": secrets.analytics_enabled(), "posthog_enabled": secrets.posthog_enabled,
        "posthog_host": secrets.posthog_host,
        "posthog_project_token_set": secrets.posthog_project_token.as_ref().is_some_and(|v| !v.trim().is_empty())})
}

async fn config(State(state): State<AnalyticsState>) -> Json<Value> {
    Json(masked_config(&*state.secrets.read().await))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigUpdate {
    posthog_enabled: bool,
    posthog_host: String,
    // Omitted retains the existing token. Empty string clears it.
    posthog_project_token: Option<String>,
}

async fn update_config(
    State(state): State<AnalyticsState>,
    Json(body): Json<ConfigUpdate>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !valid_host(&body.posthog_host)
        || body
            .posthog_project_token
            .as_ref()
            .is_some_and(|v| v.len() > 512)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Choose a PostHog Cloud region and a valid project token"})),
        ));
    }
    let mut secrets = state.secrets.write().await;
    let mut updated = secrets.clone();
    updated.posthog_enabled = body.posthog_enabled;
    updated.posthog_host = body.posthog_host;
    if let Some(token) = body.posthog_project_token {
        updated.posthog_project_token = if token.trim().is_empty() {
            None
        } else {
            Some(token.trim().into())
        };
    }
    updated.save().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Could not save analytics settings"})),
        )
    })?;
    *secrets = updated;
    Ok(Json(masked_config(&secrets)))
}

#[derive(Deserialize)]
struct UsageEvent {
    event: String,
    distinct_id: String,
    session_id: String,
    #[serde(default)]
    properties: serde_json::Map<String, Value>,
}

// Only enumerated labels and bounded numbers can leave the application.
fn sanitized(event: UsageEvent) -> Option<Value> {
    const EVENTS: &[&str] = &[
        "app_opened",
        "view_opened",
        "meeting_opened",
        "feature_used",
        "playback_started",
        "playback_paused",
        "playback_completed",
        "playback_seeked",
        "playback_speed_changed",
        "playback_track_toggled",
        "content_tab_opened",
        "transcript_exported",
        "summary_exported",
        "chat_message_sent",
        "recording_form_opened",
        "library_filtered",
    ];
    if !EVENTS.contains(&event.event.as_str()) {
        return None;
    }
    let distinct = uuid::Uuid::parse_str(&event.distinct_id).ok()?;
    let session = uuid::Uuid::parse_str(&event.session_id).ok()?;
    let mut props = serde_json::Map::new();
    for (key, value) in event.properties {
        let allowed = match key.as_str() {
            "view" => value
                .as_str()
                .is_some_and(|s| ["sessions", "people", "settings"].contains(&s)),
            "tab" => value
                .as_str()
                .is_some_and(|s| ["transcript", "summary", "notes", "files"].contains(&s)),
            "feature" => value.as_str().is_some_and(|s| {
                [
                    "meeting_create",
                    "meeting_edit",
                    "meeting_delete",
                    "recording_start",
                    "recording_stop",
                    "recording_upload",
                    "transcription_request",
                    "transcript_delete",
                    "summary_request",
                    "summary_edit",
                    "summary_delete",
                    "todo_toggle",
                    "speaker_attribution",
                    "people_manage",
                    "tags_manage",
                    "settings_save",
                    "conversation_manage",
                ]
                .contains(&s)
            }),
            "outcome" => value
                .as_str()
                .is_some_and(|s| ["success", "error"].contains(&s)),
            "source" => value.as_str().is_some_and(|s| {
                ["player", "transcript", "summary", "link", "waveform"].contains(&s)
            }),
            "format" => value
                .as_str()
                .is_some_and(|s| ["lrc", "chatgpt", "markdown", "html", "pdf"].contains(&s)),
            "backend" => value
                .as_str()
                .is_some_and(|s| ["openrouter", "claude_code"].contains(&s)),
            "position_seconds" | "duration_seconds" | "speed" | "count" => value
                .as_f64()
                .is_some_and(|n| n.is_finite() && (0.0..=604800.0).contains(&n)),
            _ => false,
        };
        if allowed {
            props.insert(key, value);
        }
    }
    props.insert("$session_id".into(), json!(session.to_string()));
    props.insert("$process_person_profile".into(), json!(false));
    props.insert("$geoip_disable".into(), json!(true));
    props.insert("app_version".into(), json!(env!("CARGO_PKG_VERSION")));
    props.insert("event_schema_version".into(), json!(1));
    Some(
        json!({"event": event.event, "distinct_id": distinct.to_string(), "properties": props,
        "timestamp": chrono::Utc::now().to_rfc3339()}),
    )
}

async fn capture(State(state): State<AnalyticsState>, Json(event): Json<UsageEvent>) -> StatusCode {
    if !state.secrets.read().await.analytics_enabled() {
        return StatusCode::NO_CONTENT;
    }
    match sanitized(event) {
        Some(event) => {
            let _ = state.sender.try_send(event);
            StatusCode::NO_CONTENT
        }
        None => StatusCode::BAD_REQUEST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_safe_metadata_leaves_app() {
        let event = UsageEvent { event: "feature_used".into(), distinct_id: uuid::Uuid::new_v4().to_string(), session_id: uuid::Uuid::new_v4().to_string(), properties: serde_json::from_value(json!({"feature":"recording_start", "outcome":"success", "filename":"private.mp3", "view":"private meeting", "position_seconds": -1, "$ip":"1.2.3.4"})).unwrap() };
        let value = sanitized(event).unwrap();
        assert_eq!(value["properties"]["feature"], "recording_start");
        for key in ["filename", "view", "position_seconds", "$ip"] {
            assert!(value["properties"].get(key).is_none());
        }
        assert_eq!(value["properties"]["$process_person_profile"], false);
    }
    #[test]
    fn rejects_arbitrary_event_names_and_identity() {
        for (name, id) in [
            ("private title", uuid::Uuid::new_v4().to_string()),
            ("app_opened", "person@example.com".into()),
        ] {
            assert!(sanitized(UsageEvent {
                event: name.into(),
                distinct_id: id,
                session_id: uuid::Uuid::new_v4().to_string(),
                properties: Default::default()
            })
            .is_none());
        }
    }
    #[tokio::test]
    async fn admin_config_retains_clears_and_masks_token() {
        let dir = std::env::temp_dir().join(format!("analytics-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut secrets = crate::llm::secrets::LlmSecrets::load_or_create(&dir);
        secrets.posthog_project_token = Some("phc_private_test".into());
        secrets.save().unwrap();
        let secrets = std::sync::Arc::new(tokio::sync::RwLock::new(secrets));
        let (sender, mut receiver) = mpsc::channel(2);
        let state = AnalyticsState {
            secrets: secrets.clone(),
            sender,
        };
        let response = update_config(
            State(state.clone()),
            Json(ConfigUpdate {
                posthog_enabled: false,
                posthog_host: "https://eu.i.posthog.com".into(),
                posthog_project_token: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.0["enabled"], false);
        assert!(!response.0.to_string().contains("phc_private_test"));
        assert_eq!(
            secrets.read().await.posthog_project_token.as_deref(),
            Some("phc_private_test")
        );
        let ignored = UsageEvent {
            event: "app_opened".into(),
            distinct_id: uuid::Uuid::new_v4().to_string(),
            session_id: uuid::Uuid::new_v4().to_string(),
            properties: Default::default(),
        };
        assert_eq!(
            capture(State(state.clone()), Json(ignored)).await,
            StatusCode::NO_CONTENT
        );
        assert!(receiver.try_recv().is_err());
        assert!(update_config(
            State(state.clone()),
            Json(ConfigUpdate {
                posthog_enabled: true,
                posthog_host: "http://localhost".into(),
                posthog_project_token: None
            })
        )
        .await
        .is_err());
        let _ = update_config(
            State(state),
            Json(ConfigUpdate {
                posthog_enabled: true,
                posthog_host: "https://us.i.posthog.com".into(),
                posthog_project_token: Some("".into()),
            }),
        )
        .await
        .unwrap();
        assert!(!crate::llm::secrets::LlmSecrets::load_or_create(&dir).analytics_enabled());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
