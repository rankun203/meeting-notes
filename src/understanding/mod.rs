//! HTTP client for the audio-extraction RunPod serverless endpoint.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::info;

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
            client: reqwest::Client::new(),
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
        let body = RunPodRunRequest {
            input: ExtractionInput {
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
        let resp = self
            .client
            .get(format!("{}/status/{}", self.endpoint_url, job_id))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| format!("failed to poll job {}: {e}", job_id))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("RunPod /status failed ({}): {}", status, text));
        }

        let status_resp: RunPodStatusResponse = resp
            .json()
            .await
            .map_err(|e| format!("failed to parse status response: {e}"))?;

        match status_resp.status.as_str() {
            "COMPLETED" => {
                status_resp
                    .output
                    .ok_or_else(|| "job completed but no output".to_string())
                    .map(Some)
            }
            "FAILED" => {
                let err = status_resp.error.unwrap_or_else(|| "unknown error".to_string());
                Err(format!("extraction job failed: {}", err))
            }
            "CANCELLED" => Err("extraction job was cancelled".to_string()),
            // IN_QUEUE, IN_PROGRESS, etc.
            other => {
                info!("Job {} status: {}", job_id, other);
                Ok(None)
            }
        }
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
