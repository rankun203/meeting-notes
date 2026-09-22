//! GdayMeetings task API, with explicit compatibility for legacy file-drop.
use crate::session::session::PlatformTask;
use crate::understanding::{ExtractionOutput, ResultSink, TrackInput};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub struct PlatformClient {
    base_url: String,
    key: String,
    http: reqwest::Client,
}

impl PlatformClient {
    pub fn new(base_url: &str, key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn available(&self) -> Result<bool, String> {
        let response = self.http.get(format!("{}/api/platform/capabilities", self.base_url))
            .bearer_auth(&self.key).timeout(Duration::from_secs(30)).send().await
            .map_err(|e| format!("GdayMeetings capability check failed: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND { return Ok(false); }
        let value: Value = response.error_for_status()
            .map_err(|e| format!("GdayMeetings capability check failed: {e}"))?
            .json().await.map_err(|e| format!("Invalid platform capabilities: {e}"))?;
        if value["durableTasks"] == true { Ok(true) }
        else { Err("Platform does not advertise durableTasks".into()) }
    }

    pub async fn create_task(&self, session_id: &str, title: &str, tracks: &[TrackInput])
        -> Result<(PlatformTask, ResultSink), String>
    {
        #[derive(Deserialize)]
        struct CreatedTask {
            id: String,
            #[serde(rename = "resultSink")]
            result_sink: ResultSink,
        }
        let inputs: Vec<Value> = tracks.iter().map(|t| json!({
            "url": t.audio_url, "trackName": t.track_name,
            "sourceType": t.source_type, "channels": t.channels,
        })).collect();
        let created: CreatedTask = self.http.post(format!("{}/api/platform/tasks", self.base_url))
            .bearer_auth(&self.key).timeout(Duration::from_secs(30))
            .json(&json!({"externalId": session_id, "title": title, "inputs": inputs}))
            .send().await.map_err(|e| format!("Failed to create durable task: {e}"))?
            .error_for_status().map_err(|e| format!("Failed to create durable task: {e}"))?
            .json().await.map_err(|e| format!("Invalid durable task response: {e}"))?;
        Ok((PlatformTask { base_url: self.base_url.clone(), task_id: created.id }, created.result_sink))
    }

    pub async fn output(&self, task_id: &str) -> Result<Option<ExtractionOutput>, String> {
        let value: Value = self.http.get(format!("{}/api/platform/tasks/{}", self.base_url, task_id))
            .bearer_auth(&self.key).timeout(Duration::from_secs(30))
            .send().await.map_err(|e| format!("Failed to retrieve durable task: {e}"))?
            .error_for_status().map_err(|e| format!("Failed to retrieve durable task: {e}"))?
            .json().await.map_err(|e| format!("Invalid durable task response: {e}"))?;
        extract_output(&value)
    }

    pub async fn update_task(&self, task_id: &str, update: Value) -> Result<(), String> {
        self.http.patch(format!("{}/api/platform/tasks/{}", self.base_url, task_id))
            .bearer_auth(&self.key).timeout(Duration::from_secs(30)).json(&update)
            .send().await.map_err(|e| format!("Failed to update durable task: {e}"))?
            .error_for_status().map_err(|e| format!("Failed to update durable task: {e}"))?;
        Ok(())
    }
}

fn extract_output(task: &Value) -> Result<Option<ExtractionOutput>, String> {
    let outputs = task["outputs"].as_array().ok_or("Durable task missing outputs array")?;
    match outputs.iter().rev().find(|o| o["type"] == "TRANSCRIPT_OUTPUT") {
        Some(output) => serde_json::from_value(output["body"].clone()).map(Some)
            .map_err(|e| format!("Invalid durable transcript output: {e}")),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::{get, post}, Router, Json, http::{HeaderMap, StatusCode}};

    async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        (url, handle)
    }

    #[tokio::test]
    async fn only_404_enables_legacy_fallback() {
        let (url, server) = serve(Router::new()).await;
        assert!(!PlatformClient::new(&url, "test").available().await.unwrap());
        server.abort();
        let (url, server) = serve(Router::new().route("/api/platform/capabilities", get(|| async {
            StatusCode::UNAUTHORIZED
        }))).await;
        assert!(PlatformClient::new(&url, "test").available().await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn task_contract_and_durable_recovery_without_runpod() {
        let router = Router::new()
            .route("/api/platform/capabilities", get(|| async { Json(json!({"durableTasks":true})) }))
            .route("/api/platform/tasks", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(headers["authorization"], "Bearer service-key");
                assert_eq!(body["externalId"], "meeting-one");
                assert_eq!(body["inputs"][0]["trackName"], "mic");
                assert_eq!(body["inputs"][0]["channels"], 1);
                Json(json!({"id":"task-one","resultSink":{"url":"https://example.test/result","token":"task-key"}}))
            }))
            .route("/api/platform/tasks/task-one", get(|| async { Json(json!({"outputs":[{
                "type":"TRANSCRIPT_OUTPUT","body":{"tracks":{},"language":"en","model":"recovered"}
            }]})) }));
        let (url, server) = serve(router).await;
        let client = PlatformClient::new(&url, "service-key");
        assert!(client.available().await.unwrap());
        let (task, sink) = client.create_task("meeting-one", "Meeting", &[TrackInput {
            audio_url: "https://example.test/audio".into(), track_name: "mic".into(),
            source_type: "mic".into(), channels: 1,
        }]).await.unwrap();
        assert_eq!(sink.token, "task-key");
        // Only the task location survives a restart, not its worker credential.
        let saved = serde_json::to_string(&task).unwrap();
        assert!(!saved.contains("task-key"));
        let restored: PlatformTask = serde_json::from_str(&saved).unwrap();
        assert_eq!(client.output(&restored.task_id).await.unwrap().unwrap().model, "recovered");
        server.abort();
    }

    #[test]
    fn reads_transcript_among_other_output_types() {
        let task = json!({"outputs":[{"type":"SUMMARY_OUTPUT","body":"summary"},
            {"type":"TRANSCRIPT_OUTPUT","body":{"tracks":{},"language":"en","model":"test"}}]});
        assert_eq!(extract_output(&task).unwrap().unwrap().language, "en");
        assert!(extract_output(&json!({"outputs":[]})).unwrap().is_none());
        assert!(extract_output(&json!({"outputs":[{"type":"TRANSCRIPT_OUTPUT","body":{}}]})).is_err());
        assert!(extract_output(&json!({})).is_err());
    }
}
