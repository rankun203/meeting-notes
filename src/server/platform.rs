//! GdayMeetings task API, with explicit compatibility for legacy file-drop.
use crate::session::session::PlatformTask;
use crate::understanding::{ExtractionOutput, ResultSink, TrackInput};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub enum TaskResult {
    Pending,
    Complete(ExtractionOutput),
    Failed,
}

pub struct PlatformClient {
    base_url: String,
    key: String,
    auth: Option<std::sync::Arc<super::gday_auth::GdayAuth>>,
    http: reqwest::Client,
}

impl PlatformClient {
    pub fn new(base_url: &str, key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
            auth: None,
            http: reqwest::Client::new(),
        }
    }

    pub fn for_user(base_url: &str, auth: std::sync::Arc<super::gday_auth::GdayAuth>) -> Self {
        let mut client = Self::new(base_url, "");
        client.auth = Some(auth);
        client
    }

    async fn token(&self) -> Result<String, String> {
        match &self.auth {
            Some(auth) => auth.access_token(&self.base_url).await,
            None => Ok(self.key.clone()),
        }
    }

    pub async fn upload(&self, filename: &str, bytes: Vec<u8>) -> Result<String, String> {
        let value: Value = self
            .http
            .post(format!("{}/upload", self.base_url))
            .query(&[("filename", filename)])
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(300))
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("Gday audio upload failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Gday audio upload failed: {e}"))?
            .json()
            .await
            .map_err(|_| "Invalid Gday upload response")?;
        let url = value["url"]
            .as_str()
            .ok_or("Gday upload response has no URL")?;
        let base = reqwest::Url::parse(&format!("{}/", self.base_url))
            .map_err(|_| "Invalid platform URL")?;
        let url = base.join(url).map_err(|_| "Invalid uploaded audio URL")?;
        if url.origin() != base.origin() {
            return Err("Audio URL belongs to another server".into());
        }
        Ok(url.into())
    }

    pub async fn execute(
        &self,
        session_id: &str,
        title: &str,
        tracks: &[TrackInput],
        language: &str,
        diarize: bool,
        idempotency_key: &str,
    ) -> Result<PlatformTask, String> {
        let inputs: Vec<Value> = tracks
            .iter()
            .map(|track| {
                json!({"url":track.audio_url,
            "trackName":track.track_name,"sourceType":track.source_type,"channels":track.channels})
            })
            .collect();
        let body = json!({"externalId":session_id,"title":title,"inputs":inputs,"execute":true,
            "idempotencyKey":idempotency_key,"executionOptions":{"language":language,"diarize":diarize}});
        // Repeating this POST is safe only because the platform persists the idempotency key.
        for attempt in 0..4 {
            let response = self
                .http
                .post(format!("{}/api/platform/tasks", self.base_url))
                .bearer_auth(self.token().await?)
                .timeout(Duration::from_secs(60))
                .json(&body)
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    let value: Value = response.json().await.map_err(|_| {
                        "Invalid task response; retry transcription to recover the same task"
                    })?;
                    let id = value["id"].as_str().ok_or("Task response has no ID")?;
                    return Ok(PlatformTask {
                        base_url: self.base_url.clone(),
                        task_id: id.into(),
                        user_auth: true,
                        submission: None,
                    });
                }
                Ok(response)
                    if !response.status().is_server_error()
                        && response.status().as_u16() != 429 =>
                {
                    return Err(format!(
                        "Gday rejected transcription task ({})",
                        response.status()
                    ));
                }
                _ if attempt == 3 => {
                    return Err(
                        "Gday task submission unavailable; retry transcription to recover the same task".into()
                    );
                }
                _ => tokio::time::sleep(Duration::from_secs(1 << attempt)).await,
            }
        }
        unreachable!()
    }

    pub async fn available(&self) -> Result<bool, String> {
        let response = self
            .http
            .get(format!("{}/api/platform/capabilities", self.base_url))
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("GdayMeetings capability check failed: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let value: Value = response
            .error_for_status()
            .map_err(|e| format!("GdayMeetings capability check failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Invalid platform capabilities: {e}"))?;
        if self.auth.is_some() && value["transcription"] != true {
            return Err("Gday transcription is not configured; ask its administrator to configure the worker".into());
        }
        if value["durableTasks"] == true {
            Ok(true)
        } else {
            Err("Platform does not advertise durableTasks".into())
        }
    }

    pub async fn create_task(
        &self,
        session_id: &str,
        title: &str,
        tracks: &[TrackInput],
    ) -> Result<(PlatformTask, ResultSink), String> {
        #[derive(Deserialize)]
        struct CreatedTask {
            id: String,
            #[serde(rename = "resultSink")]
            result_sink: ResultSink,
        }
        let inputs: Vec<Value> = tracks
            .iter()
            .map(|t| {
                json!({
                    "url": t.audio_url, "trackName": t.track_name,
                    "sourceType": t.source_type, "channels": t.channels,
                })
            })
            .collect();
        let created: CreatedTask = self
            .http
            .post(format!("{}/api/platform/tasks", self.base_url))
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(30))
            .json(&json!({"externalId": session_id, "title": title, "inputs": inputs}))
            .send()
            .await
            .map_err(|e| format!("Failed to create durable task: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Failed to create durable task: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Invalid durable task response: {e}"))?;
        Ok((
            PlatformTask {
                base_url: self.base_url.clone(),
                task_id: created.id,
                user_auth: false,
                submission: None,
            },
            created.result_sink,
        ))
    }

    pub async fn task_result(&self, task_id: &str) -> Result<TaskResult, String> {
        let value: Value = self
            .http
            .get(format!("{}/api/platform/tasks/{}", self.base_url, task_id))
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|_| "Gday task lookup unavailable; it can be resumed later")?
            .error_for_status()
            .map_err(|_| "Gday task lookup failed; check your sign-in")?
            .json()
            .await
            .map_err(|_| "Invalid Gday task response")?;
        if let Some(output) = extract_output(&value)? {
            return Ok(TaskResult::Complete(output));
        }
        if matches!(
            value["status"].as_str(),
            Some("FAILED" | "CANCELLED" | "TIMED_OUT")
        ) {
            return Ok(TaskResult::Failed);
        }
        Ok(TaskResult::Pending)
    }

    pub async fn output(&self, task_id: &str) -> Result<Option<ExtractionOutput>, String> {
        let value: Value = self
            .http
            .get(format!("{}/api/platform/tasks/{}", self.base_url, task_id))
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Failed to retrieve durable task: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Failed to retrieve durable task: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Invalid durable task response: {e}"))?;
        let output = extract_output(&value)?;
        if output.is_none()
            && matches!(
                value["status"].as_str(),
                Some("FAILED" | "CANCELLED" | "TIMED_OUT")
            )
        {
            return Err(
                "Gday transcription task failed; retry transcription from the meeting".into(),
            );
        }
        Ok(output)
    }

    pub async fn update_task(&self, task_id: &str, update: Value) -> Result<(), String> {
        self.http
            .patch(format!("{}/api/platform/tasks/{}", self.base_url, task_id))
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(30))
            .json(&update)
            .send()
            .await
            .map_err(|e| format!("Failed to update durable task: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Failed to update durable task: {e}"))?;
        Ok(())
    }
}

fn extract_output(task: &Value) -> Result<Option<ExtractionOutput>, String> {
    let outputs = task["outputs"]
        .as_array()
        .ok_or("Durable task missing outputs array")?;
    match outputs
        .iter()
        .rev()
        .find(|o| o["type"] == "TRANSCRIPT_OUTPUT")
    {
        Some(output) => serde_json::from_value(output["body"].clone())
            .map(Some)
            .map_err(|e| format!("Invalid durable transcript output: {e}")),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderMap, StatusCode},
        routing::{get, post},
        Json, Router,
    };

    async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (url, handle)
    }

    #[tokio::test]
    async fn only_404_enables_legacy_fallback() {
        let (url, server) = serve(Router::new()).await;
        assert!(!PlatformClient::new(&url, "test").available().await.unwrap());
        server.abort();
        let (url, server) = serve(Router::new().route(
            "/api/platform/capabilities",
            get(|| async { StatusCode::UNAUTHORIZED }),
        ))
        .await;
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
        let (task, sink) = client
            .create_task(
                "meeting-one",
                "Meeting",
                &[TrackInput {
                    audio_url: "https://example.test/audio".into(),
                    track_name: "mic".into(),
                    source_type: "mic".into(),
                    channels: 1,
                }],
            )
            .await
            .unwrap();
        assert_eq!(sink.token, "task-key");
        // Only the task location survives a restart, not its worker credential.
        let saved = serde_json::to_string(&task).unwrap();
        assert!(!saved.contains("task-key"));
        let restored: PlatformTask = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            client
                .output(&restored.task_id)
                .await
                .unwrap()
                .unwrap()
                .model,
            "recovered"
        );
        server.abort();
    }

    #[test]
    fn reads_transcript_among_other_output_types() {
        let task = json!({"outputs":[{"type":"SUMMARY_OUTPUT","body":"summary"},
            {"type":"TRANSCRIPT_OUTPUT","body":{"tracks":{},"language":"en","model":"test"}}]});
        assert_eq!(extract_output(&task).unwrap().unwrap().language, "en");
        assert!(extract_output(&json!({"outputs":[]})).unwrap().is_none());
        assert!(
            extract_output(&json!({"outputs":[{"type":"TRANSCRIPT_OUTPUT","body":{}}]})).is_err()
        );
        assert!(extract_output(&json!({})).is_err());
    }
}
