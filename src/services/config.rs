//! Config service — static app configuration (audio sources + form schema).
//!
//! This is the hard-coded schema the frontend uses to render the Settings
//! form. It doesn't read state but keeping it in the services layer means
//! the Tauri commands can expose it with zero duplication.

use serde_json::{Value, json};

pub fn get_config() -> Value {
    let sources = crate::audio::discover_sources();
    json!({
        "sources": sources,
        "fields": {
            "language": {
                "type": "select",
                "default": "en",
                "label": "Language",
                "description": "Language for transcription",
                "options": [
                    { "value": "en", "label": "English" },
                    { "value": "zh-cn", "label": "Chinese (Simplified)" },
                    { "value": "zh-tw", "label": "Chinese (Traditional)" },
                    { "value": "ja", "label": "Japanese" },
                    { "value": "ko", "label": "Korean" },
                    { "value": "es", "label": "Spanish" },
                    { "value": "fr", "label": "French" },
                    { "value": "de", "label": "German" },
                    { "value": "pt", "label": "Portuguese" },
                    { "value": "ru", "label": "Russian" },
                    { "value": "ar", "label": "Arabic" },
                ],
            },
            "format": {
                "type": "select",
                "default": "opus",
                "label": "Format",
                "description": "Audio file format",
                "options": [
                    { "value": "wav", "label": "WAV", "title": "Lossless, lowest CPU (~2%), but large files" },
                    { "value": "mp3", "label": "MP3", "title": "Lossy, widely compatible, ~6% CPU" },
                    { "value": "opus", "label": "Opus", "title": "Designed for speech, smallest files, ~4% CPU" },
                ],
            },
            "raw_sample_rate": {
                "type": "select",
                "default": 48000,
                "label": "Raw Sample Rate",
                "description": "Recording sample rate — higher means better quality but larger files",
                "advanced": true,
                "options": [
                    { "value": 16000, "label": "16000 Hz" },
                    { "value": 22050, "label": "22050 Hz" },
                    { "value": 44100, "label": "44100 Hz" },
                    { "value": 48000, "label": "48000 Hz" },
                ],
            },
            "mp3_bitrate": {
                "type": "select",
                "default": 64,
                "label": "MP3 Bitrate",
                "description": "MP3 encoder bitrate — higher means better quality and larger files",
                "advanced": true,
                "show_when": { "field": "format", "value": "mp3" },
                "config_path": "mp3.bitrate_kbps",
                "options": [
                    { "value": 32, "label": "32 kbps" },
                    { "value": 48, "label": "48 kbps" },
                    { "value": 64, "label": "64 kbps" },
                    { "value": 96, "label": "96 kbps" },
                    { "value": 128, "label": "128 kbps" },
                    { "value": 192, "label": "192 kbps" },
                    { "value": 256, "label": "256 kbps" },
                    { "value": 320, "label": "320 kbps" },
                ],
            },
            "mp3_sample_rate": {
                "type": "select",
                "default": 16000,
                "label": "MP3 Sample Rate",
                "description": "MP3 encoder output sample rate — can differ from recording rate; the encoder will resample",
                "advanced": true,
                "show_when": { "field": "format", "value": "mp3" },
                "config_path": "mp3.sample_rate",
                "options": [
                    { "value": 8000, "label": "8000 Hz" },
                    { "value": 16000, "label": "16000 Hz" },
                    { "value": 22050, "label": "22050 Hz" },
                    { "value": 44100, "label": "44100 Hz" },
                    { "value": 48000, "label": "48000 Hz" },
                ],
            },
            "opus_bitrate": {
                "type": "select",
                "default": 32,
                "label": "Opus Bitrate",
                "description": "Opus encoder target bitrate — 24-32 kbps is transparent for speech",
                "advanced": true,
                "show_when": { "field": "format", "value": "opus" },
                "config_path": "opus.bitrate_kbps",
                "options": [
                    { "value": 16, "label": "16 kbps" },
                    { "value": 24, "label": "24 kbps" },
                    { "value": 32, "label": "32 kbps" },
                    { "value": 48, "label": "48 kbps" },
                    { "value": 64, "label": "64 kbps" },
                    { "value": 96, "label": "96 kbps" },
                    { "value": 128, "label": "128 kbps" },
                ],
            },
            "opus_complexity": {
                "type": "select",
                "default": 5,
                "label": "Opus Complexity",
                "description": "Encoder complexity (0-10) — higher is better quality but more CPU",
                "advanced": true,
                "show_when": { "field": "format", "value": "opus" },
                "config_path": "opus.complexity",
                "options": [
                    { "value": 0, "label": "0 (fastest)" },
                    { "value": 3, "label": "3" },
                    { "value": 5, "label": "5 (default)" },
                    { "value": 7, "label": "7" },
                    { "value": 10, "label": "10 (best)" },
                ],
            },
        },
    })
}
