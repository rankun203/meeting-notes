//! Ephemeral Cloudflare Tunnel for exposing session audio files to RunPod.
//!
//! On daemon startup, downloads the `cloudflared` binary in the background.
//! When transcription is triggered, spins up a temporary file server + tunnel,
//! waits for RunPod to download files, then kills both immediately.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command};
use tracing::{info, warn, error};

/// Manages cloudflared binary download and ephemeral tunnel lifecycle.
#[derive(Clone)]
pub struct TunnelManager {
    tools_dir: PathBuf,
}

impl TunnelManager {
    pub fn new(data_dir: &Path) -> Self {
        let tools_dir = data_dir.join("tools");
        Self { tools_dir }
    }

    /// Find cloudflared binary: check system PATH first, then downloaded copy.
    pub async fn ensure_cloudflared(&self) -> Result<PathBuf, String> {
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
        let binary_path = self.tools_dir.join(binary_name);

        if binary_path.exists() {
            return Ok(binary_path);
        }

        let url = cloudflared_download_url()
            .ok_or_else(|| "unsupported platform for cloudflared".to_string())?;

        info!("Downloading cloudflared from {}", url);
        std::fs::create_dir_all(&self.tools_dir)
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

        // Make executable on unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("failed to chmod cloudflared: {e}"))?;
        }

        info!("cloudflared downloaded to {}", binary_path.display());
        Ok(binary_path)
    }

    /// Start a temporary file server + cloudflare tunnel to expose session files.
    /// Returns an `EphemeralTunnel` that must be shut down after files are downloaded.
    pub async fn serve_and_tunnel(
        &self,
        session_dir: &Path,
        daemon_port: u16,
    ) -> Result<EphemeralTunnel, String> {
        let cloudflared = self.ensure_cloudflared().await?;

        // Start a minimal file server on an available port
        let session_dir = session_dir.to_path_buf();
        let (server_handle, temp_port) = start_file_server(session_dir.clone(), daemon_port + 1).await?;

        // Start cloudflared tunnel pointing to the file server
        let (tunnel_process, tunnel_url) =
            start_tunnel(&cloudflared, temp_port).await?;

        info!(
            "Tunnel active: {} -> 127.0.0.1:{} (serving {})",
            tunnel_url,
            temp_port,
            session_dir.display()
        );

        Ok(EphemeralTunnel {
            tunnel_url,
            tunnel_process,
            server_handle,
        })
    }
}

/// An active ephemeral tunnel + file server. Call `shutdown()` when done.
pub struct EphemeralTunnel {
    pub tunnel_url: String,
    tunnel_process: Child,
    server_handle: tokio::task::JoinHandle<()>,
}

impl EphemeralTunnel {
    /// Build a public URL for a file in the session directory.
    pub fn file_url(&self, filename: &str) -> String {
        format!("{}/{}", self.tunnel_url, filename)
    }

    /// Kill the tunnel process and stop the file server.
    pub async fn shutdown(mut self) {
        if let Err(e) = self.tunnel_process.kill().await {
            warn!("Failed to kill cloudflared: {}", e);
        }
        self.server_handle.abort();
        info!("Ephemeral tunnel shut down");
    }
}

/// Start a minimal file server, bind to an available port starting from `start_port`.
/// Returns the handle and the actual port bound.
async fn start_file_server(
    dir: PathBuf,
    start_port: u16,
) -> Result<(tokio::task::JoinHandle<()>, u16), String> {
    use axum::Router;
    use tower_http::services::ServeDir;

    let app = Router::new().fallback_service(ServeDir::new(&dir));

    // Try ports sequentially using async bind (no race condition)
    let mut port = start_port;
    let listener = loop {
        match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await {
            Ok(l) => break l,
            Err(_) if port < start_port.saturating_add(100) => {
                port += 1;
            }
            Err(e) => {
                return Err(format!("failed to bind file server (tried ports {}-{}): {e}", start_port, port));
            }
        }
    };

    let bound_port = listener.local_addr()
        .map_err(|e| format!("failed to get bound address: {e}"))?
        .port();

    info!("File server bound to 127.0.0.1:{} (serving {})", bound_port, dir.display());

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("File server error: {}", e);
        }
    });

    // Verify the server is actually responding with a known file
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let check = reqwest::get(format!("http://127.0.0.1:{}/metadata.json", bound_port)).await;
    match check {
        Ok(resp) if resp.status().is_success() => info!("File server verified: HTTP {}", resp.status()),
        Ok(resp) => warn!("File server check returned HTTP {} — expected 200", resp.status()),
        Err(e) => return Err(format!("File server failed to respond on port {}: {}", bound_port, e)),
    }

    Ok((handle, bound_port))
}

/// Start cloudflared quick tunnel and parse the public URL from its output.
async fn start_tunnel(
    cloudflared_path: &Path,
    local_port: u16,
) -> Result<(Child, String), String> {
    let mut child = Command::new(cloudflared_path)
        .args(["tunnel", "--url", &format!("http://127.0.0.1:{}", local_port)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to start cloudflared: {e}"))?;

    // cloudflared prints the tunnel URL to stderr
    let stderr = child.stderr.take()
        .ok_or_else(|| "could not capture cloudflared stderr".to_string())?;

    let mut reader = tokio::io::BufReader::new(stderr).lines();
    let tunnel_url = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while let Ok(Some(line)) = reader.next_line().await {
            // cloudflared prints a line like:
            // "... https://xxx-yyy-zzz.trycloudflare.com ..."
            if let Some(url) = extract_tunnel_url(&line) {
                return Ok(url);
            }
        }
        Err("cloudflared exited without printing tunnel URL".to_string())
    })
    .await
    .map_err(|_| "timed out waiting for cloudflared tunnel URL (30s)".to_string())??;

    Ok((child, tunnel_url))
}

/// Extract a trycloudflare.com URL from a cloudflared log line.
fn extract_tunnel_url(line: &str) -> Option<String> {
    // Look for https://*.trycloudflare.com
    for word in line.split_whitespace() {
        if word.contains(".trycloudflare.com") {
            let url = word.trim_matches(|c: char| !c.is_alphanumeric() && c != ':' && c != '/' && c != '.' && c != '-');
            if url.starts_with("https://") {
                return Some(url.to_string());
            }
        }
    }
    None
}

fn cloudflared_download_url() -> Option<&'static str> {
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
