//! HTTP client for the audio-extraction RunPod serverless endpoint.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Input track descriptor sent to audio-extraction.
#[derive(Debug, Serialize)]
pub struct TrackInput {
    pub audio_url: String,
    pub track_name: String,
    pub source_type: String,
    pub channels: u16,
}

/// Request body for the RunPod serverless /run endpoint.
#[derive(Debug, Serialize)]
struct RunPodRunRequest {
    input: ExtractionInput,
}

#[derive(Debug, Serialize)]
struct ExtractionInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    result_sink: Option<ResultSink>,
    tracks: Vec<TrackInput>,
    language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_size: Option<String>,
    diarize: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_speakers: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_speakers: Option<u32>,
}

/// Task-scoped durable output callback capability. Never log its token.
#[derive(Clone, Serialize, Deserialize)]
pub struct ResultSink {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for ResultSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResultSink { <redacted> }")
    }
}

/// RunPod /run response.
#[derive(Debug, Deserialize)]
struct RunPodRunResponse {
    id: String,
    status: String,
}

/// RunPod /status/{id} response.
#[derive(Debug, Deserialize)]
struct RunPodStatusResponse {
    #[allow(dead_code)]
    id: String,
    status: String,
    #[serde(default)]
    output: Option<ExtractionOutput>,
    #[serde(default)]
    error: Option<String>,
}

/// Output from the audio-extraction service.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractionOutput {
    pub tracks: HashMap<String, TrackResult>,
    pub language: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackResult {
    pub source_type: String,
    pub duration_secs: f64,
    pub segments: Vec<TranscriptSegment>,
    pub speaker_embeddings: HashMap<String, Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub words: Vec<WordSegment>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WordSegment {
    pub word: String,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub score: Option<f64>,
}

/// Retrieval failures do not prove that the worker failed or that durable output is absent.
#[derive(Debug)]
pub struct PollError {
    pub message: String,
    pub terminal: bool,
}

pub struct ExtractionClient {
    endpoint_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl ExtractionClient {
    pub fn new(endpoint_url: String, api_key: String) -> Self {
        Self {
            endpoint_url: endpoint_url.trim_end_matches('/').to_string(),
            api_key,
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .timeout(std::time::Duration::from_secs(60))
                .build().expect("valid HTTP client configuration"),
        }
    }

    /// Submit a job to the RunPod serverless endpoint. Returns the job ID.
    pub async fn submit_job(
        &self,
        tracks: Vec<TrackInput>,
        language: &str,
        diarize: bool,
        min_speakers: Option<u32>,
        max_speakers: Option<u32>,
    ) -> Result<String, String> {
        self.submit_job_with_result_sink(tracks, language, diarize, min_speakers, max_speakers, None).await
    }

    pub async fn submit_job_with_result_sink(
        &self,
        tracks: Vec<TrackInput>,
        language: &str,
        diarize: bool,
        min_speakers: Option<u32>,
        max_speakers: Option<u32>,
        result_sink: Option<ResultSink>,
    ) -> Result<String, String> {
        let body = RunPodRunRequest {
            input: ExtractionInput {
                result_sink,
                tracks,
                language: language.to_string(),
                model_size: None,
                diarize,
                min_speakers,
                max_speakers,
            },
        };

        let resp = self
            .client
            .post(format!("{}/run", self.endpoint_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("failed to submit extraction job: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("RunPod /run failed ({}): {}", status, text));
        }

        let run_resp: RunPodRunResponse = resp
            .json()
            .await
            .map_err(|e| format!("failed to parse RunPod response: {e}"))?;

        info!("Submitted extraction job: {} (status: {})", run_resp.id, run_resp.status);
        Ok(run_resp.id)
    }

    /// Poll a job's status. Returns `None` if still in progress, `Some(output)` if complete.
    pub async fn poll_status(
        &self,
        job_id: &str,
    ) -> Result<Option<ExtractionOutput>, String> {
        self.poll_status_detailed(job_id).await.map_err(|error| error.message)
    }

    /// Distinguish an explicit terminal job failure from unavailable/expired responses.
    pub async fn poll_status_detailed(
        &self,
        job_id: &str,
    ) -> Result<Option<ExtractionOutput>, PollError> {
        let status_resp = self.fetch_status(job_id).await.map_err(|message| PollError {
            message,
            terminal: false,
        })?;

        match status_resp.status.as_str() {
            "COMPLETED" => status_resp.output.map(Some).ok_or_else(|| PollError {
                message: "job completed but no output".to_string(),
                terminal: false,
            }),
            "FAILED" => Err(PollError {
                message: format!("extraction job failed: {}", status_resp.error.unwrap_or_else(|| "unknown error".to_string())),
                terminal: true,
            }),
            "CANCELLED" => Err(PollError {
                message: "extraction job was cancelled".to_string(),
                terminal: true,
            }),
            "TIMED_OUT" => Err(PollError {
                message: "extraction job timed out on RunPod".to_string(),
                terminal: true,
            }),
            "IN_QUEUE" | "IN_PROGRESS" => {
                info!("Job {} status: {}", job_id, status_resp.status);
                Ok(None)
            }
            other => Err(PollError {
                message: format!("unexpected extraction status: {other}"),
                terminal: false,
            }),
        }
    }

    async fn fetch_status(&self, job_id: &str) -> Result<RunPodStatusResponse, String> {
        for attempt in 0..4 {
            let result = async {
                self.client.get(format!("{}/status/{}", self.endpoint_url, job_id))
                    .bearer_auth(&self.api_key).send().await?
                    .error_for_status()?.json::<RunPodStatusResponse>().await
            }.await;
            match result {
                Ok(status) => return Ok(status),
                Err(error) => {
                    let retryable = error.is_timeout() || error.is_connect() || error.is_body()
                        || error.is_decode() || error.status().is_some_and(|status|
                            status.is_server_error() || status.as_u16() == 429 || status.as_u16() == 408);
                    if !retryable || attempt == 3 {
                        return Err(format!("failed to poll job {job_id}: {error}"));
                    }
                    warn!("Transient status request failure for job {}; retrying", job_id);
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                }
            }
        }
        unreachable!()
    }

    /// Submit a job and poll until completion. Returns the extraction output.
    pub async fn run_and_wait(
        &self,
        tracks: Vec<TrackInput>,
        language: &str,
        diarize: bool,
        min_speakers: Option<u32>,
        max_speakers: Option<u32>,
    ) -> Result<ExtractionOutput, String> {
        let job_id = self
            .submit_job(tracks, language, diarize, min_speakers, max_speakers)
            .await?;

        // Poll with exponential backoff: 1s, 2s, 4s, ... capped at 15s
        let mut delay = std::time::Duration::from_secs(1);
        let max_delay = std::time::Duration::from_secs(15);
        let timeout = std::time::Duration::from_secs(600); // 10 min max
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err("extraction job timed out after 10 minutes".to_string());
            }

            tokio::time::sleep(delay).await;

            match self.poll_status(&job_id).await? {
                Some(output) => return Ok(output),
                None => {
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    #[tokio::test]
    async fn transient_poll_failure_retries_without_resubmitting() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let app = Router::new().route("/status/job", get(move || {
            let calls = calls.clone();
            async move {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({})))
                } else {
                    (axum::http::StatusCode::OK, Json(serde_json::json!({"id":"job", "status":"IN_PROGRESS"})))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = ExtractionClient::new(format!("http://{address}"), "test".into());
        assert!(client.poll_status("job").await.unwrap().is_none());
        assert_eq!(observed.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[tokio::test]
    async fn timed_out_is_terminal() {
        let app = Router::new().route("/status/job", get(|| async {
            Json(serde_json::json!({"id":"job", "status":"TIMED_OUT"}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = ExtractionClient::new(format!("http://{address}"), "test".into());
        assert!(client.poll_status("job").await.unwrap_err().contains("timed out"));
        server.abort();
    }

    #[tokio::test]
    async fn detailed_errors_only_mark_explicit_worker_failures_terminal() {
        let app = Router::new().route("/status/{status}", get(|axum::extract::Path(status): axum::extract::Path<String>| async move {
            if status == "EXPIRED" {
                (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({})))
            } else {
                (axum::http::StatusCode::OK, Json(serde_json::json!({"id":"job", "status":status})))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = ExtractionClient::new(format!("http://{address}"), "test".into());
        for (status, terminal) in [
            ("FAILED", true), ("CANCELLED", true), ("TIMED_OUT", true),
            ("COMPLETED", false), ("UNKNOWN", false), ("EXPIRED", false),
        ] {
            let error = client.poll_status_detailed(status).await.unwrap_err();
            assert_eq!(error.terminal, terminal, "classification for {status}");
            assert!(!error.message.is_empty());
        }
        server.abort();
    }

}
