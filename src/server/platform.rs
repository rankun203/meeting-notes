//! OAuth-authenticated GdayMeetings transcription tasks.
use crate::session::session::PlatformTask;
use crate::understanding::{ExtractionOutput, TrackInput};
use serde_json::{json, Value};
use std::time::Duration;

pub enum TaskResult {
    Pending,
    Complete(ExtractionOutput),
    Failed,
}

pub struct PlatformClient {
    base_url: String,
    auth: std::sync::Arc<super::gday_auth::GdayAuth>,
    http: reqwest::Client,
}

impl PlatformClient {
    pub fn for_user(base_url: &str, auth: std::sync::Arc<super::gday_auth::GdayAuth>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            auth,
            http: reqwest::Client::new(),
        }
    }

    async fn token(&self) -> Result<String, String> {
        self.auth.access_token(&self.base_url).await
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
        let body = json!({"externalId":session_id,"title":title,"inputs":inputs,
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

    pub async fn ensure_transcription_available(&self) -> Result<(), String> {
        let response = self
            .http
            .get(format!("{}/api/platform/capabilities", self.base_url))
            .bearer_auth(self.token().await?)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("GdayMeetings capability check failed: {e}"))?;
        let value: Value = response
            .error_for_status()
            .map_err(|e| format!("GdayMeetings capability check failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Invalid platform capabilities: {e}"))?;
        if value["transcription"] != true {
            return Err("Gday transcription is not configured; ask its administrator to configure the worker".into());
        }
        if value["durableTasks"] == true {
            Ok(())
        } else {
            Err("Platform does not advertise durableTasks".into())
        }
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
    #[tokio::test]
    async fn platform_requires_user_login_before_any_request() {
        let directory = std::env::temp_dir().join(format!("gday-no-user-{}", uuid::Uuid::new_v4()));
        let auth = super::super::gday_auth::GdayAuth::load(&directory);
        let platform = PlatformClient::for_user("http://127.0.0.1:1", auth);
        assert!(platform
            .ensure_transcription_available()
            .await
            .unwrap_err()
            .contains("Sign in"));
        assert!(
            matches!(platform.task_result("old-task").await, Err(error) if error.contains("Sign in"))
        );
    }

    #[test]
    fn old_task_metadata_does_not_select_an_authentication_mode() {
        let task: PlatformTask = serde_json::from_value(json!({
            "base_url":"https://gday.example", "task_id":"previous-task", "user_auth":false
        }))
        .unwrap();
        assert_eq!(task.task_id, "previous-task");
        assert!(serde_json::to_value(task)
            .unwrap()
            .get("user_auth")
            .is_none());
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
