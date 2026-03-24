use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::audio::writer::{AudioFormat, Mp3Config};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub language: String,

    #[serde(default)]
    pub summarization_instruction: Option<String>,

    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    #[serde(default)]
    pub format: AudioFormat,

    #[serde(default)]
    pub mp3: Mp3Config,

    #[serde(default)]
    pub mic_device: Option<String>,

    #[serde(skip_deserializing)]
    pub output_dir: PathBuf,
}

fn default_sample_rate() -> u32 {
    48000
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            summarization_instruction: None,
            sample_rate: default_sample_rate(),
            format: AudioFormat::default(),
            mp3: Mp3Config::default(),
            mic_device: None,
            output_dir: PathBuf::from("./recordings"),
        }
    }
}
