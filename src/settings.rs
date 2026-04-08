//! Application settings persisted to `{data-dir}/settings.json`.
//!
//! Created with defaults on first daemon startup if the file doesn't exist.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Shared settings handle used across handlers.
pub type SharedSettings = Arc<RwLock<AppSettings>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// RunPod serverless endpoint URL (e.g. "https://api.runpod.ai/v2/ENDPOINT_ID").
    #[serde(default)]
    pub audio_extraction_url: Option<String>,

    /// RunPod API key for the audio-extraction endpoint.
    #[serde(default)]
    pub audio_extraction_api_key: Option<String>,

    /// file-drop server URL for temporary audio file parking.
    #[serde(default = "default_file_drop_url")]
    pub file_drop_url: String,

    /// file-drop API key for uploads.
    #[serde(default = "default_file_drop_api_key")]
    pub file_drop_api_key: String,

    /// Whether to run speaker diarization (identify who spoke when).
    #[serde(default = "default_true")]
    pub diarize: bool,

    /// Whether to run speaker recognition against the People library after diarization.
    #[serde(default = "default_true")]
    pub people_recognition: bool,

    /// Cosine similarity threshold for auto-matching speakers to known people.
    #[serde(default = "default_threshold")]
    pub speaker_match_threshold: f64,

    /// Default summarization prompt prepended to ChatGPT transcript exports.
    #[serde(default)]
    pub summarization_prompt: Option<String>,

    /// LLM API host (OpenAI-compatible endpoint).
    #[serde(default = "default_llm_host")]
    pub llm_host: String,

    /// LLM model identifier (e.g. "anthropic/claude-sonnet-4").
    #[serde(default = "default_llm_model")]
    pub llm_model: String,

    /// Optional separate model for summarization. Falls back to `llm_model` if None.
    #[serde(default)]
    pub summarization_model: Option<String>,

    /// Automatically start transcription after recording stops.
    #[serde(default = "default_true")]
    pub auto_transcribe: bool,

    /// Automatically generate summary after transcription completes.
    #[serde(default)]
    pub auto_summarize: bool,

    /// Self-introduction injected into the chat system prompt (e.g. role, team, preferences).
    #[serde(default)]
    pub chat_self_intro: Option<String>,

    /// OpenRouter provider sort for the chat model ("price", "throughput", or "latency").
    #[serde(default)]
    pub openrouter_sort: Option<String>,

    /// OpenRouter provider sort for the summarization model.
    #[serde(default)]
    pub summarization_openrouter_sort: Option<String>,

    /// Chat backend: "openrouter" (default) or "claude_code".
    #[serde(default = "default_chat_backend")]
    pub chat_backend: String,

    /// Claude Code model (e.g. "sonnet", "opus", "haiku", or full name).
    #[serde(default)]
    pub claude_code_model: Option<String>,

    /// Path to the settings file (not serialized).
    #[serde(skip)]
    settings_path: PathBuf,
}

fn default_file_drop_url() -> String {
    "https://file-drop.dsync.net".to_string()
}

fn default_file_drop_api_key() -> String {
    "fd_XXabLowAonHJCvc8uj6ydMv3PsRuYUig8bfuYTcatR".to_string()
}

fn default_true() -> bool {
    true
}

fn default_threshold() -> f64 {
    0.75
}

pub fn default_summarization_prompt() -> String {
    String::new()
}

fn default_chat_backend() -> String {
    "openrouter".to_string()
}

fn default_llm_host() -> String {
    "https://openrouter.ai/api/v1".to_string()
}

fn default_llm_model() -> String {
    "anthropic/claude-sonnet-4".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            audio_extraction_url: None,
            audio_extraction_api_key: None,
            file_drop_url: default_file_drop_url(),
            file_drop_api_key: default_file_drop_api_key(),
            diarize: true,
            people_recognition: true,
            speaker_match_threshold: 0.75,
            summarization_prompt: None,
            llm_host: default_llm_host(),
            llm_model: default_llm_model(),
            summarization_model: None,
            auto_transcribe: true,
            auto_summarize: false,
            chat_self_intro: None,
            openrouter_sort: None,
            summarization_openrouter_sort: None,
            chat_backend: default_chat_backend(),
            claude_code_model: None,
            settings_path: PathBuf::new(),
        }
    }
}

impl AppSettings {
    /// Load settings from `{data_dir}/settings.json`, creating the file with
    /// defaults if it doesn't exist.
    pub fn load_or_create(data_dir: &Path) -> Self {
        let path = data_dir.join("settings.json");

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<AppSettings>(&json) {
                    Ok(mut settings) => {
                        settings.settings_path = path;
                        info!("Loaded settings from {}", settings.settings_path.display());
                        return settings;
                    }
                    Err(e) => {
                        warn!("Failed to parse settings.json: {}. Using defaults.", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to read settings.json: {}. Using defaults.", e);
                }
            }
        }

        let mut settings = AppSettings::default();
        settings.settings_path = path;

        if let Err(e) = settings.save() {
            warn!("Failed to write default settings.json: {}", e);
        } else {
            info!("Created default settings at {}", settings.settings_path.display());
        }

        settings
    }

    /// Save current settings to disk.
    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {e}"))?;
        std::fs::write(&self.settings_path, json)
            .map_err(|e| format!("Failed to write settings: {e}"))?;
        Ok(())
    }

    /// Merge a partial JSON update into current settings and save.
    pub fn merge_and_save(&mut self, update: &serde_json::Value) -> Result<(), String> {
        if let Some(v) = update.get("audio_extraction_url") {
            self.audio_extraction_url = v.as_str().map(|s| s.to_string());
        }
        if let Some(v) = update.get("audio_extraction_api_key") {
            self.audio_extraction_api_key = v.as_str().map(|s| s.to_string());
        }
        if let Some(v) = update.get("file_drop_url") {
            if let Some(s) = v.as_str() {
                self.file_drop_url = s.to_string();
            }
        }
        if let Some(v) = update.get("file_drop_api_key") {
            if let Some(s) = v.as_str() {
                self.file_drop_api_key = s.to_string();
            }
        }
        if let Some(v) = update.get("diarize") {
            if let Some(b) = v.as_bool() {
                self.diarize = b;
            }
        }
        if let Some(v) = update.get("people_recognition") {
            if let Some(b) = v.as_bool() {
                self.people_recognition = b;
            }
        }
        if let Some(v) = update.get("speaker_match_threshold") {
            if let Some(n) = v.as_f64() {
                self.speaker_match_threshold = n;
            }
        }
        if let Some(v) = update.get("summarization_prompt") {
            self.summarization_prompt = v.as_str().map(|s| s.to_string());
        }
        if let Some(v) = update.get("llm_host") {
            if let Some(s) = v.as_str() {
                self.llm_host = s.to_string();
            }
        }
        if let Some(v) = update.get("llm_model") {
            if let Some(s) = v.as_str() {
                self.llm_model = s.to_string();
            }
        }
        if let Some(v) = update.get("summarization_model") {
            self.summarization_model = v.as_str().map(|s| s.to_string());
        }
        if let Some(v) = update.get("auto_transcribe") {
            if let Some(b) = v.as_bool() {
                self.auto_transcribe = b;
            }
        }
        if let Some(v) = update.get("auto_summarize") {
            if let Some(b) = v.as_bool() {
                self.auto_summarize = b;
            }
        }
        if let Some(v) = update.get("chat_self_intro") {
            self.chat_self_intro = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
        if let Some(v) = update.get("openrouter_sort") {
            self.openrouter_sort = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
        if let Some(v) = update.get("summarization_openrouter_sort") {
            self.summarization_openrouter_sort = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
        if let Some(v) = update.get("chat_backend") {
            if let Some(s) = v.as_str() {
                self.chat_backend = s.to_string();
            }
        }
        if let Some(v) = update.get("claude_code_model") {
            self.claude_code_model = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
        self.save()
    }

    /// Return a masked copy for API responses (hides API keys).
    pub fn to_masked_json(&self) -> serde_json::Value {
        let mask = |s: &Option<String>| -> serde_json::Value {
            match s {
                Some(key) if key.len() > 4 => {
                    let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                    serde_json::Value::String(masked)
                }
                Some(_) => serde_json::Value::String("****".to_string()),
                None => serde_json::Value::Null,
            }
        };

        let mask_str = |s: &str| -> serde_json::Value {
            if s.is_empty() {
                serde_json::Value::String(String::new())
            } else if s.len() > 8 {
                serde_json::Value::String(format!("{}...{}", &s[..4], &s[s.len() - 4..]))
            } else {
                serde_json::Value::String("****".to_string())
            }
        };

        serde_json::json!({
            "audio_extraction_url": self.audio_extraction_url,
            "audio_extraction_api_key": mask(&self.audio_extraction_api_key),
            "file_drop_url": self.file_drop_url,
            "file_drop_api_key": mask_str(&self.file_drop_api_key),
            "diarize": self.diarize,
            "people_recognition": self.people_recognition,
            "speaker_match_threshold": self.speaker_match_threshold,
            "summarization_prompt": self.summarization_prompt,
            "llm_host": self.llm_host,
            "llm_model": self.llm_model,
            "summarization_model": self.summarization_model,
            "auto_transcribe": self.auto_transcribe,
            "auto_summarize": self.auto_summarize,
            "chat_self_intro": self.chat_self_intro,
            "openrouter_sort": self.openrouter_sort,
            "summarization_openrouter_sort": self.summarization_openrouter_sort,
            "chat_backend": self.chat_backend,
            "claude_code_model": self.claude_code_model,
        })
    }

    /// Check if audio extraction is configured (both URL and key present).
    pub fn is_extraction_configured(&self) -> bool {
        self.audio_extraction_url.is_some()
            && self.audio_extraction_api_key.is_some()
            && !self.file_drop_url.is_empty()
            && !self.file_drop_api_key.is_empty()
    }
}
