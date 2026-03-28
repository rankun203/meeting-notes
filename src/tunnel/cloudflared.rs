//! Cloudflared binary discovery and download.

use std::path::{Path, PathBuf};

use tracing::info;

/// Find cloudflared in system PATH, or download to `tools_dir`.
pub async fn ensure_binary(tools_dir: &Path) -> Result<PathBuf, String> {
    // Check system PATH first (e.g., installed via brew)
    if let Ok(output) = std::process::Command::new("which")
        .arg("cloudflared")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                info!("Using system cloudflared: {}", path);
                return Ok(PathBuf::from(path));
            }
        }
    }

    let binary_name = if cfg!(target_os = "windows") {
        "cloudflared.exe"
    } else {
        "cloudflared"
    };
    let binary_path = tools_dir.join(binary_name);

    if binary_path.exists() {
        return Ok(binary_path);
    }

    let url = download_url()
        .ok_or_else(|| "unsupported platform for cloudflared".to_string())?;

    info!("Downloading cloudflared from {}", url);
    std::fs::create_dir_all(tools_dir)
        .map_err(|e| format!("failed to create tools dir: {e}"))?;

    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("failed to download cloudflared: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("cloudflared download failed: HTTP {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("failed to read cloudflared bytes: {e}"))?;

    std::fs::write(&binary_path, &bytes)
        .map_err(|e| format!("failed to write cloudflared binary: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to chmod cloudflared: {e}"))?;
    }

    info!("cloudflared downloaded to {}", binary_path.display());
    Ok(binary_path)
}

fn download_url() -> Option<&'static str> {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        Some("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-arm64.tgz")
    } else if cfg!(target_os = "macos") {
        Some("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-amd64.tgz")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Some("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-arm64")
    } else if cfg!(target_os = "linux") {
        Some("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64")
    } else if cfg!(target_os = "windows") {
        Some("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe")
    } else {
        None
    }
}
