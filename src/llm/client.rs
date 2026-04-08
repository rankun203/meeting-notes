//! OpenAI-compatible chat completions client with streaming support.

use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tokio_stream::Stream;
use tracing::warn;

/// Client for OpenAI-compatible chat completion APIs (OpenRouter, etc.).
pub struct LlmClient {
    host: String,
    api_key: String,
    model: String,
    http: Client,
    /// OpenRouter provider sort preference ("price", "throughput", or "latency").
    provider_sort: Option<String>,
}

impl LlmClient {
    pub fn new(host: String, api_key: String, model: String) -> Self {
        Self {
            host,
            api_key,
            model,
            http: Client::new(),
            provider_sort: None,
        }
    }

    pub fn with_provider_sort(mut self, sort: Option<String>) -> Self {
        self.provider_sort = sort;
        self
    }

    /// Stream chat completion responses, yielding content delta strings.
    ///
    /// Sends a POST to `{host}/chat/completions` with `stream: true`,
    /// parses SSE `data:` lines, and yields `choices[0].delta.content` values.
    pub async fn stream_chat(
        &self,
        messages: Vec<Value>,
    ) -> Result<impl Stream<Item = Result<String, String>>, String> {
        let url = format!("{}/chat/completions", self.host.trim_end_matches('/'));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        if let Some(sort) = &self.provider_sort {
            body["provider"] = serde_json::json!({ "sort": sort });
        }

        let response = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to LLM API: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("LLM API returned {}: {}", status, body));
        }

        let byte_stream = response.bytes_stream();

        let stream = async_stream::stream! {
            let mut buffer = String::new();

            futures::pin_mut!(byte_stream);

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            if line == "data: [DONE]" {
                                return;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                match serde_json::from_str::<Value>(data) {
                                    Ok(json) => {
                                        let delta = json
                                            .get("choices")
                                            .and_then(|c| c.get(0))
                                            .and_then(|c| c.get("delta"));

                                        // Check for reasoning/thinking content (extended thinking models)
                                        if let Some(delta) = delta {
                                            let reasoning = delta.get("reasoning_content")
                                                .or_else(|| delta.get("reasoning"))
                                                .and_then(|r| r.as_str());
                                            if let Some(r) = reasoning {
                                                if !r.is_empty() {
                                                    yield Ok(format!("\x01{}", r)); // \x01 prefix = thinking
                                                }
                                            }

                                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                if !content.is_empty() {
                                                    yield Ok(content.to_string());
                                                }
                                            }
                                        }

                                        // Check for usage info (final chunk with stream_options)
                                        if let Some(usage) = json.get("usage") {
                                            if let Ok(usage_str) = serde_json::to_string(usage) {
                                                yield Ok(format!("\x02{}", usage_str)); // \x02 prefix = usage
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse SSE data: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(format!("Stream error: {e}"));
                        return;
                    }
                }
            }
        };

        Ok(stream)
    }

    /// Non-streaming chat completion. Returns the full response content.
    pub async fn complete(
        &self,
        messages: Vec<Value>,
    ) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.host.trim_end_matches('/'));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });
        if let Some(sort) = &self.provider_sort {
            body["provider"] = serde_json::json!({ "sort": sort });
        }

        let response = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to LLM API: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("LLM API returned {}: {}", status, body));
        }

        let json: Value = response.json().await
            .map_err(|e| format!("Failed to parse LLM response: {e}"))?;

        json.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "LLM response missing content".to_string())
    }

    /// Fetch available models from the API.
    pub async fn list_models(host: &str, api_key: &str) -> Result<Value, String> {
        let url = format!("{}/models", host.trim_end_matches('/'));
        let client = Client::new();

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| format!("Failed to fetch models: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Models API returned {}: {}", status, body));
        }

        response.json::<Value>().await
            .map_err(|e| format!("Failed to parse models response: {e}"))
    }
}
