//! Claude Code CLI integration.
//!
//! Spawns `claude -p --output-format stream-json --verbose` as a subprocess,
//! parses the JSONL output, and yields structured events for SSE streaming.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::info;

/// Events emitted by the Claude Code runner, mapped to SSE events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ClaudeEvent {
    #[serde(rename = "init")]
    Init {
        session_id: String,
        model: String,
    },
    #[serde(rename = "delta")]
    Delta {
        content: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        tool: String,
        input_summary: String,
    },
    #[serde(rename = "done")]
    Done {
        session_id: String,
        cost_usd: f64,
        result: String,
    },
    #[serde(rename = "error")]
    Error {
        error: String,
    },
    #[serde(rename = "permission_request")]
    PermissionRequest {
        tools: Vec<String>,
    },
}

/// Manages spawning and tracking of Claude Code CLI processes.
#[derive(Clone)]
pub struct ClaudeCodeRunner {
    data_dir: PathBuf,
    active: Arc<Mutex<Option<Child>>>,
    /// Tools temporarily approved for this server session (not persisted).
    session_approved: Arc<Mutex<Vec<String>>>,
    /// Tools approved for a single run only — removed after the next run completes.
    once_approved: Arc<Mutex<Vec<String>>>,
}

impl ClaudeCodeRunner {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            active: Arc::new(Mutex::new(None)),
            session_approved: Arc::new(Mutex::new(Vec::new())),
            once_approved: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if the `claude` CLI is available on PATH.
    pub async fn is_available() -> bool {
        tokio::process::Command::new("which")
            .arg("claude")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Run Claude Code with the given prompt, returning a stream of events.
    ///
    /// If `session_id` is provided, resumes that conversation.
    /// `mentions_context` is prepended to the prompt with file path references.
    pub async fn run(
        &self,
        prompt: &str,
        session_id: Option<&str>,
        mentions_context: Option<&str>,
        model: Option<&str>,
    ) -> Result<tokio::sync::mpsc::Receiver<ClaudeEvent>, String> {
        // Only one concurrent process
        {
            let guard = self.active.lock().await;
            if guard.is_some() {
                return Err("A Claude Code process is already running".to_string());
            }
        }

        let full_prompt = match mentions_context {
            Some(ctx) if !ctx.is_empty() => format!("{}\n\n---\n{}", ctx, prompt),
            _ => prompt.to_string(),
        };

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(&full_prompt)
            .arg("--output-format").arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&self.data_dir);

        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }

        // Add session-approved tools
        {
            let approved = self.session_approved.lock().await;
            if !approved.is_empty() {
                cmd.arg("--allowedTools").arg(approved.join(","));
                info!("Session-approved tools: {}", approved.join(", "));
            }
        }

        if let Some(sid) = session_id {
            // Validate session ID format (UUID) to prevent argument injection
            if sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                cmd.arg("--resume").arg(sid);
                info!("Spawning claude CLI (resuming {}) in {}", sid, self.data_dir.display());
            } else {
                info!("Spawning claude CLI (new session) in {}", self.data_dir.display());
            }
        } else {
            info!("Spawning claude CLI (new session) in {}", self.data_dir.display());
        }

        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to spawn claude: {}", e))?;

        let stdout = child.stdout.take()
            .ok_or("Failed to capture claude stdout")?;

        // Store the child handle for cancellation
        {
            let mut guard = self.active.lock().await;
            *guard = Some(child);
        }

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let active = self.active.clone();
        let session_approved = self.session_approved.clone();
        let once_approved = self.once_approved.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            // Track cumulative text length to emit only new deltas from partial messages
            let mut emitted_text_len: usize = 0;

            while let Ok(Some(line)) = lines.next_line().await {
                if line.is_empty() {
                    continue;
                }

                let json: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let events = parse_claude_json(&json, &mut emitted_text_len);
                let mut should_stop = false;
                for event in events {
                    let is_terminal = matches!(event, ClaudeEvent::Done { .. } | ClaudeEvent::Error { .. } | ClaudeEvent::PermissionRequest { .. });
                    if tx.send(event).await.is_err() {
                        should_stop = true;
                        break;
                    }
                    if is_terminal {
                        should_stop = true;
                        break;
                    }
                }
                if should_stop {
                    break;
                }
            }

            // Clean up: kill and remove child from active
            let mut guard = active.lock().await;
            if let Some(mut child) = guard.take() {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            drop(guard);

            // Remove once-approved tools from session list
            let mut once = once_approved.lock().await;
            if !once.is_empty() {
                let mut approved = session_approved.lock().await;
                approved.retain(|t| !once.contains(t));
                info!("Cleared once-approved tools: {}", once.join(", "));
                once.clear();
            }
        });

        Ok(rx)
    }

    /// Kill the active Claude process if one is running.
    pub async fn stop(&self) -> bool {
        let mut guard = self.active.lock().await;
        if let Some(ref mut child) = *guard {
            let _ = child.kill().await;
            let _ = child.wait().await;
            *guard = None;
            info!("Killed active Claude Code process");
            true
        } else {
            false
        }
    }

    /// Check if a process is currently running.
    pub async fn is_running(&self) -> bool {
        self.active.lock().await.is_some()
    }

    /// Return the Claude Code project directory for this data_dir.
    fn claude_project_dir(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        // Claude Code hashes the path by replacing / and . with -
        let path_str = self.data_dir.to_string_lossy();
        let hashed = path_str.replace('/', "-").replace('.', "-");
        let dir = home.join(".claude").join("projects").join(&hashed);
        if dir.is_dir() { Some(dir) } else { None }
    }

    /// List Claude Code sessions for this project.
    pub fn list_sessions(&self) -> Vec<ClaudeSession> {
        let dir = match self.claude_project_dir() {
            Some(d) => d,
            None => return Vec::new(),
        };

        let mut sessions = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let session_id = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if session_id.is_empty() { continue; }

            let meta = entry.metadata().ok();
            let mtime = meta.as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

            // Read last-prompt, title, and count messages
            let mut title = String::new();
            let mut last_prompt = String::new();
            let mut message_count: u32 = 0;
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines().rev() {
                    if let Ok(obj) = serde_json::from_str::<Value>(line) {
                        if obj.get("type").and_then(|t| t.as_str()) == Some("last-prompt") {
                            last_prompt = obj.get("lastPrompt")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            break;
                        }
                    }
                }
                for line in content.lines() {
                    if let Ok(obj) = serde_json::from_str::<Value>(line) {
                        match obj.get("type").and_then(|t| t.as_str()) {
                            // Use first queue-operation content as title
                            Some("queue-operation") if title.is_empty() => {
                                if let Some(c) = obj.get("content").and_then(|v| v.as_str()) {
                                    if !c.is_empty() {
                                        title = c.chars().take(60).collect();
                                    }
                                }
                            }
                            Some("user") | Some("assistant") => {
                                message_count += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }

            if title.is_empty() {
                title = last_prompt.chars().take(60).collect();
            }

            sessions.push(ClaudeSession {
                id: session_id,
                title,
                last_prompt,
                mtime,
                size,
                message_count,
            });
        }

        sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
        sessions
    }

    /// Load messages from a Claude Code session.
    pub fn load_session(&self, session_id: &str) -> Option<Vec<ClaudeMessage>> {
        // Validate session_id format
        if !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        let dir = self.claude_project_dir()?;
        let path = dir.join(format!("{}.jsonl", session_id));
        let content = std::fs::read_to_string(&path).ok()?;

        let mut messages = Vec::new();
        for line in content.lines() {
            let obj: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match obj.get("type").and_then(|t| t.as_str()) {
                Some("user") => {
                    // Extract text from message.content
                    let text = extract_text_content(&obj, "message");
                    if !text.is_empty() {
                        messages.push(ClaudeMessage { role: "user".into(), content: text });
                    }
                }
                Some("assistant") => {
                    let text = extract_text_content(&obj, "message");
                    if !text.is_empty() {
                        messages.push(ClaudeMessage { role: "assistant".into(), content: text });
                    }
                }
                _ => {}
            }
        }
        Some(messages)
    }

    /// Temporarily approve tools for this server session only (in-memory).
    pub async fn approve_tools_session(&self, tools: &[String]) {
        let mut approved = self.session_approved.lock().await;
        for tool in tools {
            if !approved.contains(tool) {
                approved.push(tool.clone());
                info!("Session-approved tool: {}", tool);
            }
        }
    }

    pub async fn approve_tools_once(&self, tools: &[String]) {
        let mut approved = self.session_approved.lock().await;
        let mut once = self.once_approved.lock().await;
        for tool in tools {
            if !approved.contains(tool) {
                approved.push(tool.clone());
            }
            if !once.contains(tool) {
                once.push(tool.clone());
                info!("Once-approved tool: {}", tool);
            }
        }
    }


    /// Permanently approve tools by adding them to the project's .claude/settings.json.
    pub fn approve_tools_permanent(&self, tools: &[String]) -> Result<(), String> {
        let claude_dir = self.data_dir.join(".claude");
        std::fs::create_dir_all(&claude_dir)
            .map_err(|e| format!("Failed to create .claude dir: {}", e))?;

        let settings_path = claude_dir.join("settings.json");
        let mut settings: Value = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)
                .map_err(|e| format!("Failed to read settings: {}", e))?;
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };

        // Ensure permissions.allow array exists
        if settings.get("permissions").is_none() {
            settings["permissions"] = json!({ "allow": [], "deny": [] });
        }
        if settings["permissions"].get("allow").is_none() {
            settings["permissions"]["allow"] = json!([]);
        }

        let allow = settings["permissions"]["allow"].as_array_mut()
            .ok_or("permissions.allow is not an array")?;

        for tool in tools {
            let tool_val = Value::String(tool.clone());
            if !allow.contains(&tool_val) {
                allow.push(tool_val);
                info!("Approved tool: {}", tool);
            }
        }

        let json_str = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        std::fs::write(&settings_path, json_str)
            .map_err(|e| format!("Failed to write settings: {}", e))?;

        Ok(())
    }
}

/// Summary info for a Claude Code session.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeSession {
    pub id: String,
    pub title: String,
    pub last_prompt: String,
    pub mtime: u64,
    pub size: u64,
    pub message_count: u32,
}

/// A message from a Claude Code session.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeMessage {
    pub role: String,
    pub content: String,
}

/// Parse a single JSONL line from Claude CLI into events.
/// `emitted_text_len` tracks how much text has been sent as deltas so far,
/// so partial messages only emit the new portion.
fn parse_claude_json(json: &Value, emitted_text_len: &mut usize) -> Vec<ClaudeEvent> {
    let mut events = Vec::new();

    match json.get("type").and_then(|t| t.as_str()) {
        Some("system") => {
            if json.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                let session_id = json.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let model = json.get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                events.push(ClaudeEvent::Init { session_id, model });
            }
        }

        Some("stream_event") => {
            // Real-time streaming deltas — faster than waiting for full assistant messages
            if let Some(event) = json.get("event") {
                match event.get("type").and_then(|t| t.as_str()) {
                    Some("content_block_delta") => {
                        if let Some(delta) = event.get("delta") {
                            if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                                if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                    if !text.is_empty() {
                                        *emitted_text_len += text.len();
                                        events.push(ClaudeEvent::Delta { content: text.to_string() });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Some("assistant") => {
            // Extract text content and tool use from message.content array
            if let Some(content) = json.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                // Collect the full text from all text blocks in this message
                let mut full_text = String::new();
                for block in content {
                    match block.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                full_text.push_str(text);
                            }
                        }
                        Some("tool_use") => {
                            let tool = block.get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let input_summary = summarize_tool_input(
                                &tool,
                                block.get("input").unwrap_or(&Value::Null),
                            );
                            events.push(ClaudeEvent::ToolUse { tool, input_summary });
                        }
                        _ => {}
                    }
                }

                // Emit only the new text since last emission.
                // If full_text is shorter than emitted, a new turn started — reset
                // and add a separator so turns don't run together.
                if full_text.len() < *emitted_text_len {
                    *emitted_text_len = 0;
                    events.push(ClaudeEvent::Delta { content: "\n\n".to_string() });
                }
                if full_text.len() > *emitted_text_len && full_text.is_char_boundary(*emitted_text_len) {
                    let delta = &full_text[*emitted_text_len..];
                    if !delta.is_empty() {
                        events.push(ClaudeEvent::Delta {
                            content: delta.to_string(),
                        });
                    }
                    *emitted_text_len = full_text.len();
                }
            }
        }

        Some("user") => {
            if let Some(result) = json.get("tool_use_result").and_then(|v| v.as_str()) {
                if result.contains("requested permissions to use") {
                    if let Some(start) = result.find("to use ") {
                        let rest = &result[start + 7..];
                        if let Some(end) = rest.find(',') {
                            let tool_name = rest[..end].trim().to_string();
                            events.push(ClaudeEvent::PermissionRequest { tools: vec![tool_name] });
                        }
                    }
                }
            }
        }

        Some("result") => {
            // Reset text tracking for next turn
            *emitted_text_len = 0;

            // Check for permission denials in result
            if let Some(denials) = json.get("permission_denials").and_then(|v| v.as_array()) {
                let tools: Vec<String> = denials.iter()
                    .filter_map(|d| d.get("tool_name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
                if !tools.is_empty() {
                    events.push(ClaudeEvent::PermissionRequest { tools });
                }
            }
            let session_id = json.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cost_usd = json.get("total_cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let result = json.get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if json.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                events.push(ClaudeEvent::Error { error: result });
            } else {
                events.push(ClaudeEvent::Done { session_id, cost_usd, result });
            }
        }

        _ => {}
    }

    events
}

/// Extract text content from a message object.
/// Handles both string content (`"content": "text"`) and array content
/// (`"content": [{"type": "text", "text": "..."}]`).
fn extract_text_content(obj: &Value, msg_key: &str) -> String {
    let content = match obj.get(msg_key).and_then(|m| m.get("content")) {
        Some(c) => c,
        None => return String::new(),
    };

    // Simple string content (user messages from CLI)
    if let Some(s) = content.as_str() {
        return s.to_string();
    }

    // Array of content blocks (assistant messages)
    let mut text = String::new();
    if let Some(blocks) = content.as_array() {
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(s) = block.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() { text.push('\n'); }
                    text.push_str(s);
                }
            }
        }
    }
    text
}

/// Create a short summary of tool input for UI display.
fn summarize_tool_input(tool: &str, input: &Value) -> String {
    match tool {
        "Read" => {
            let path = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("{}", path)
        }
        "Grep" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                format!("\"{}\"", pattern)
            } else {
                format!("\"{}\" in {}", pattern, path)
            }
        }
        "Glob" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            format!("{}", pattern)
        }
        "Bash" => {
            input.get("command").and_then(|v| v.as_str()).unwrap_or("?").to_string()
        }
        "Edit" => {
            let path = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("{}", path)
        }
        "Write" => {
            let path = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("{}", path)
        }
        _ => {
            // For MCP tools and others, show a compact JSON summary of input
            if let Some(obj) = input.as_object() {
                obj.iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| {
                        let val = match v {
                            Value::String(s) if s.len() > 80 => {
                                let truncated: String = s.chars().take(77).collect();
                                format!("\"{}...\"", truncated)
                            }
                            Value::String(s) => format!("\"{}\"", s),
                            other => other.to_string(),
                        };
                        format!("{}={}", k, val)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                String::new()
            }
        }
    }
}
