//! LLM API key storage in a separate secrets file with restrictive permissions.
//!
//! The secrets file (`{data_dir}/secrets.json`) is created with 0600 permissions
//! so only the current user can read it. API keys are never returned via any API.
//!
//! Keys are stored per host provider: the host URL is encoded to a safe key
//! (e.g. `"https://openrouter.ai/api/v1"` → `"openrouter.ai"`), so switching
//! between providers preserves each provider's key.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Shared secrets handle used across handlers.
pub type SharedSecrets = Arc<RwLock<LlmSecrets>>;

/// Encode a host URL to a storage key.
/// Extracts the hostname (e.g. "openrouter.ai" from "https://openrouter.ai/api/v1").
fn host_key(host: &str) -> String {
    // Strip scheme
    let without_scheme = host
        .strip_prefix("https://")
        .or_else(|| host.strip_prefix("http://"))
        .unwrap_or(host);
    // Take hostname (up to first `/` or `:`)
    let hostname = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .split(':')
        .next()
        .unwrap_or(without_scheme);
    hostname.to_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmSecrets {
    /// API keys indexed by encoded host provider key.
    #[serde(default)]
    pub api_keys: HashMap<String, String>,

    // Legacy field — migrated to api_keys on load.
    #[serde(default, skip_serializing)]
    llm_api_key: Option<String>,

    /// Path to the secrets file (not serialized).
    #[serde(skip)]
    secrets_path: PathBuf,
}

impl LlmSecrets {
    /// Load secrets from `{data_dir}/secrets.json`, creating with 0600 perms if missing.
    pub fn load_or_create(data_dir: &Path) -> Self {
        let path = data_dir.join("secrets.json");

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<LlmSecrets>(&json) {
                    Ok(mut secrets) => {
                        secrets.secrets_path = path;
                        // Migrate legacy single key to per-host map
                        if let Some(key) = secrets.llm_api_key.take() {
                            if !key.is_empty() && secrets.api_keys.is_empty() {
                                // Default to openrouter.ai since that was the original default
                                secrets.api_keys.insert("openrouter.ai".to_string(), key);
                                if let Err(e) = secrets.save() {
                                    warn!("Failed to save migrated secrets: {}", e);
                                } else {
                                    info!("Migrated legacy llm_api_key to per-host storage");
                                }
                            }
                        }
                        info!("Loaded secrets from {} ({} provider keys)", secrets.secrets_path.display(), secrets.api_keys.len());
                        return secrets;
                    }
                    Err(e) => {
                        warn!("Failed to parse secrets.json: {}. Using defaults.", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to read secrets.json: {}. Using defaults.", e);
                }
            }
        }

        let mut secrets = LlmSecrets::default();
        secrets.secrets_path = path;

        if let Err(e) = secrets.save() {
            warn!("Failed to write default secrets.json: {}", e);
        } else {
            info!("Created default secrets at {}", secrets.secrets_path.display());
        }

        secrets
    }

    /// Save secrets to disk with restrictive permissions.
    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize secrets: {e}"))?;
        std::fs::write(&self.secrets_path, &json)
            .map_err(|e| format!("Failed to write secrets: {e}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.secrets_path, perms)
                .map_err(|e| format!("Failed to set secrets file permissions: {e}"))?;
        }

        Ok(())
    }

    /// Set the API key for a given host and save.
    pub fn set_api_key(&mut self, host: &str, key: Option<String>) -> Result<(), String> {
        let k = host_key(host);
        match key {
            Some(v) if !v.is_empty() => { self.api_keys.insert(k, v); }
            _ => { self.api_keys.remove(&k); }
        }
        self.save()
    }

    /// Get the API key for a given host.
    pub fn get_api_key(&self, host: &str) -> Option<&String> {
        self.api_keys.get(&host_key(host))
    }

    /// Check if an API key is configured for a given host.
    pub fn has_api_key(&self, host: &str) -> bool {
        self.get_api_key(host).map_or(false, |k| !k.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::host_key;

    #[test]
    fn test_host_key() {
        assert_eq!(host_key("https://openrouter.ai/api/v1"), "openrouter.ai");
        assert_eq!(host_key("https://api.openai.com/v1"), "api.openai.com");
        assert_eq!(host_key("http://localhost:8080/v1"), "localhost");
        assert_eq!(host_key("https://my-host.example.com"), "my-host.example.com");
    }
}
