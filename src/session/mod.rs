pub mod config;
pub mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{RwLock, broadcast};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{info, warn};

use self::config::SessionConfig;
use self::session::{
    AutoStopSettings, Notice, NoticeLevel, Session, SessionInfo, SessionMetadata, SessionState,
};
use crate::audio::mic::MicSource;
use crate::audio::recorder::{LostSource, Recorder};
use crate::audio::source::{AudioSource, SourceDescriptor, SourceType};
use crate::audio::writer::{AudioActivitySnapshot, AudioFormat};
use crate::audio::system_audio::SystemAudioSource;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum ServerEvent {
    FilesChanged { sessions: Vec<String>, people: bool, tags: bool, conversations: bool },
    SessionCreated(SessionInfo),
    SessionUpdated(SessionInfo),
    SessionDeleted { id: String },
    FileSizes {
        id: String,
        file_sizes: HashMap<String, u64>,
        /// Seconds remaining before auto-stop triggers (None = not counting down).
        #[serde(skip_serializing_if = "Option::is_none")]
        auto_stop_remaining_secs: Option<u64>,
    },
    SessionNotice {
        id: String,
        notice: Notice,
    },
    /// Full replacement of a session's live notices list.
    SessionNotices {
        id: String,
        notices: Vec<Notice>,
    },
    TranscriptionProgress {
        id: String,
        status: String,
    },
    TranscriptionCompleted {
        id: String,
        unconfirmed_speakers: u32,
    },
    TranscriptionFailed {
        id: String,
        error: String,
    },
    SummaryProgress {
        id: String,
        status: String,
        started_at: DateTime<Utc>,
    },
    SummaryDelta {
        id: String,
        delta: String,
    },
    SummaryThinking {
        id: String,
        delta: String,
    },
    SummaryCompleted {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todos: Option<Value>,
    },
    SummaryFailed {
        id: String,
        error: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum AutoStopTrigger {
    SystemAudioSilence { seconds: u64 },
    ScreenLock,
    SystemSleep,
}

impl AutoStopTrigger {
    fn enabled(self, settings: AutoStopSettings) -> bool {
        match self {
            Self::SystemAudioSilence { seconds } => {
                settings.system_audio_silence_secs == Some(seconds)
            }
            Self::ScreenLock => settings.screen_lock,
            Self::SystemSleep => settings.system_sleep,
        }
    }

    fn notice(self) -> Notice {
        let message = match self {
            Self::SystemAudioSilence { seconds } => format!(
                "Recording auto-stopped: system audio was silent for {} second{}",
                seconds,
                if seconds == 1 { "" } else { "s" },
            ),
            Self::ScreenLock => "Recording auto-stopped: the screen was locked".to_string(),
            Self::SystemSleep => "Recording auto-stopped: the system was going to sleep".to_string(),
        };
        Notice {
            key: None,
            level: NoticeLevel::Info,
            message,
            platform: match self {
                Self::SystemAudioSilence { .. } => None,
                _ => Some(std::env::consts::OS.to_string()),
            },
            details: None,
            created_at: Utc::now(),
        }
    }
}

struct RuntimeTransition(Arc<std::sync::atomic::AtomicBool>);
impl Drop for RuntimeTransition {
    fn drop(&mut self) { self.0.store(false, std::sync::atomic::Ordering::SeqCst); }
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    output_dir: PathBuf,
    event_tx: broadcast::Sender<ServerEvent>,
    disk_revisions: Arc<tokio::sync::Mutex<HashMap<String, (Option<crate::storage::Revision>, Vec<String>)>>>,
}

impl SessionManager {
    pub fn new(output_dir: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            output_dir,
            event_tx,
            disk_revisions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_tx.subscribe()
    }

    pub(crate) fn emit(&self, event: ServerEvent) {
        let _ = self.event_tx.send(event);
    }

    fn write_metadata(session: &Session) -> Result<(), String> {
        std::fs::create_dir_all(&session.config.output_dir)
            .map_err(|e| format!("failed to create session dir: {}", e))?;
        let meta = session.to_metadata();
        let path = session.config.output_dir.join("metadata.json");
        let mut baseline = session.persisted_metadata.lock().unwrap();
        let mut updated = serde_json::to_value(&meta).map_err(|e| e.to_string())?;
        let _file_lock = crate::storage::write_lock(&path);
        let revision = crate::storage::revision(&path);
        let mut latest: Value = if revision.is_some() { crate::storage::read_json(&path)? }
            else if baseline.is_null() { Value::Null }
            else { return Err("metadata deleted externally; reload and retry".into()); };
        // Normalize known legacy/default fields for comparison, while retaining
        // unknown fields in the actual source document.
        let mut known_latest = if latest.is_null() { Value::Null } else {
            serde_json::to_value(serde_json::from_value::<SessionMetadata>(latest.clone())
                .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
        };
        if !session.notes_loaded {
            if let Some(value) = updated.as_object_mut() { value.remove("notes"); }
            if let Some(value) = known_latest.as_object_mut() { value.remove("notes"); }
        }
        let merged = crate::storage::merge_document(&baseline, &updated, &known_latest)?;
        if let (Some(raw), Some(known)) = (latest.as_object_mut(), merged.as_object()) {
            for key in known_latest.as_object().into_iter().flat_map(|v| v.keys()) {
                if !known.contains_key(key) { raw.remove(key); }
            }
            raw.extend(known.clone());
        } else { latest = merged.clone(); }
        if crate::storage::revision(&path) != revision {
            return Err("metadata changed during update; reload and retry".into());
        }
        crate::storage::write_json(&path, &latest)?;
        *baseline = merged;
        crate::markdown::write_metadata_md(&session.config.output_dir, &latest);

        Ok(())
    }

    /// Spawn a background task that broadcasts file sizes for recording sessions,
    /// detects audio issues (e.g., mic permission denied), and auto-reconnects
    /// sources that lost their device (e.g., Teams joining a call).
    pub fn start_file_size_ticker(&self) {
        let manager = self.clone();
        let sessions = self.sessions.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            /// Max reconnect attempts per (session, source) before giving up.
            const MAX_RECONNECT_ATTEMPTS: u32 = 3;
            // Keyed by (session_id, source_label) so each source has its own retry budget.
            let mut reconnect_attempts: HashMap<(String, String), u32> = HashMap::new();
            loop {
                interval.tick().await;
                // NOTE: never gate this loop on event_tx.receiver_count().
                // Device-loss recovery and auto-stop must run even with no
                // web page connected — otherwise a mic lost while the browser
                // is closed silently records nothing forever. Broadcasting to
                // zero receivers is harmless (send just returns Err).

                // Phase 1: broadcast file sizes, detect silent sources, find device-lost sessions.
                let mut device_lost_sessions: Vec<String> = Vec::new();
                let mut auto_stop_sessions: Vec<(String, u64)> = Vec::new();
                {
                    let mut sessions = sessions.write().await;
                    for session in sessions.values_mut() {
                        if session.state != SessionState::Recording {
                            continue;
                        }

                        let info = session.info();

                        // Auto-stop: detect silence via RMS at the raw PCM level.
                        let mut auto_stop_remaining_secs: Option<u64> = None;
                        if let Some(silence_threshold_secs) = session
                            .auto_stop
                            .system_audio_silence_secs
                            .filter(|seconds| *seconds > 0)
                        {
                            if let Some(ref recorder) = session.recorder {
                                if let Some(last_active_ms) = recorder.system_audio_last_active_ms() {
                                    if last_active_ms > 0 {
                                        let now_ms = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as u64;
                                        let silent_secs = now_ms.saturating_sub(last_active_ms) / 1000;
                                        if silent_secs >= silence_threshold_secs {
                                            auto_stop_sessions.push((session.id.clone(), silence_threshold_secs));
                                        } else {
                                            let countdown_start = silence_threshold_secs.saturating_sub(50);
                                            if silent_secs >= countdown_start {
                                                if silent_secs < countdown_start.saturating_add(2) {
                                                    info!("Session {}: system audio silent for {}s, auto-stop countdown started", session.id, silent_secs);
                                                }
                                                auto_stop_remaining_secs = Some(silence_threshold_secs - silent_secs);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let _ = event_tx.send(ServerEvent::FileSizes {
                            id: session.id.clone(),
                            file_sizes: info.file_sizes.clone(),
                            auto_stop_remaining_secs,
                        });

                        // Update live notices (silent mic, no system audio, etc).
                        // Allow devices and the permission prompt time to start.
                        // Use PCM activity, not buffered/encoded file sizes.
                        if let Some(started) = session.started_at {
                            let elapsed = Utc::now() - started;
                            if elapsed.num_seconds() >= 10 {
                                let activity = session.recorder.as_ref()
                                    .map(Recorder::source_activity).unwrap_or_default();
                                update_source_notices(session, &activity, Utc::now().timestamp_millis() as u64, &event_tx);
                            }
                        }

                        // Check for device-lost sources that need reconnection.
                        // Per-source budget: a source is retried only if it still
                        // has attempts left.
                        if let Some(ref recorder) = session.recorder {
                            if recorder.has_device_lost_sources() {
                                device_lost_sessions.push(session.id.clone());
                            }
                        }
                    }
                } // write lock released

                // Phase 2: attempt reconnection outside the lock on a blocking
                // thread, one source at a time. If Core Audio deadlocks inside
                // `source.start()`, only the lone source is leaked into the
                // orphan thread — the recorder keeps owning every other source
                // and its writers, so we can still call `stop()` cleanly.
                /// Hard timeout for a single source restart attempt.
                const RESTART_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
                // Sessions whose every source slot became empty after this
                // pass — caller auto-stops them so writers finalize and the
                // ticker stops processing them.
                let mut auto_stop_after_loss: Vec<String> = Vec::new();

                for session_id in device_lost_sessions {
                    let event_tx_ref = event_tx.clone();
                    let sid = session_id.clone();

                    // Take ONLY the lost sources out, filtered by retry budget.
                    // The recorder (and every healthy source/writer) stays in
                    // the session.
                    let lost_to_restart: Vec<LostSource> = {
                        let mut sessions_guard = sessions.write().await;
                        let session = match sessions_guard.get_mut(&sid) {
                            Some(s) => s,
                            None => continue,
                        };
                        let recorder = match session.recorder.as_mut() {
                            Some(r) => r,
                            None => continue,
                        };
                        recorder
                            .take_lost_sources()
                            .into_iter()
                            .filter(|ls| {
                                let attempts = reconnect_attempts
                                    .get(&(sid.clone(), ls.label.clone()))
                                    .copied()
                                    .unwrap_or(0);
                                if attempts < MAX_RECONNECT_ATTEMPTS {
                                    true
                                } else {
                                    // Over-budget: don't restart, but also don't
                                    // leak the slot — drop the source so the
                                    // recorder reflects reality.
                                    false
                                }
                            })
                            .collect()
                    };

                    for ls in lost_to_restart {
                        let label = ls.label.clone();
                        let join = tokio::task::spawn_blocking(move || ls.restart());
                        let timed = tokio::time::timeout(RESTART_TIMEOUT, join).await;

                        // Outcomes:
                        //   Ok(Ok(Ok((label, source))))      -> success, put back
                        //   Ok(Ok(Err((label, source, e))))  -> restart failed, source returned for retry
                        //   Ok(Err(join_err))                -> panicked, source dropped during unwind
                        //   Err(_)                           -> hung past timeout, source stuck in orphan thread
                        enum Outcome {
                            Ok(Box<dyn AudioSource>),
                            Err(Box<dyn AudioSource>, crate::audio::source::AudioError),
                            Panicked,
                            TimedOut,
                        }
                        let outcome = match timed {
                            Ok(Ok(Ok((_label, source)))) => Outcome::Ok(source),
                            Ok(Ok(Err((_label, source, e)))) => Outcome::Err(source, e),
                            Ok(Err(join_err)) => {
                                warn!("Mic reconnect thread panicked for session {} source {}: {}", sid, label, join_err);
                                Outcome::Panicked
                            }
                            Err(_) => {
                                warn!(
                                    "Mic reconnect timed out after {:?} for session {} source {} — source is unrecoverable",
                                    RESTART_TIMEOUT, sid, label
                                );
                                Outcome::TimedOut
                            }
                        };

                        let mut sessions_guard = sessions.write().await;
                        let session = match sessions_guard.get_mut(&sid) {
                            Some(s) => s,
                            None => continue,
                        };
                        let recorder = match session.recorder.as_mut() {
                            Some(r) => r,
                            None => continue,
                        };

                        match outcome {
                            Outcome::Ok(source) => {
                                recorder.put_back_source(&label, source);
                                reconnect_attempts.remove(&(sid.clone(), label.clone()));
                                let notice = Notice {
                                    key: None,
                                    level: NoticeLevel::Info,
                                    message: format!("Microphone \"{}\" reconnected after audio device change", label),
                                    platform: Some(std::env::consts::OS.to_string()),
                                    details: None,
                                    created_at: Utc::now(),
                                };
                                // Successful reconnects are transient status
                                // messages. Send them to connected clients but
                                // do not retain them in the session, otherwise
                                // every later SessionUpdated event resurrects
                                // an already-dismissed banner.
                                let _ = event_tx_ref.send(ServerEvent::SessionNotice {
                                    id: sid.clone(),
                                    notice,
                                });
                                info!("Reconnected source for session {}: {}", sid, label);
                            }
                            Outcome::Err(source, e) => {
                                // Restart failed but the source came back.
                                // Under budget: put it back (still flagged
                                // device-lost) so the next tick retries.
                                // At the limit: drop it — its Drop impl stops
                                // any live engine — and surface a notice.
                                let key = (sid.clone(), label.clone());
                                let attempts = reconnect_attempts.entry(key).or_insert(0);
                                *attempts += 1;
                                warn!("Mic reconnect failed for session {} source {} (attempt {}): {}", sid, label, attempts, e);
                                if *attempts < MAX_RECONNECT_ATTEMPTS {
                                    recorder.put_back_source(&label, source);
                                } else {
                                    recorder.clear_source(&label);
                                    let notice = max_attempts_notice();
                                    session.notices.push(notice.clone());
                                    let _ = event_tx_ref.send(ServerEvent::SessionNotice {
                                        id: sid.clone(),
                                        notice,
                                    });
                                }
                            }
                            Outcome::Panicked | Outcome::TimedOut => {
                                // Source is gone — either dropped during unwind
                                // or leaked into a hung Core Audio thread. Burn
                                // the retry budget so we don't try again, and
                                // surface a per-source unrecoverable notice.
                                reconnect_attempts.insert((sid.clone(), label.clone()), MAX_RECONNECT_ATTEMPTS);
                                recorder.clear_source(&label);
                                let notice = source_unrecoverable_notice(&label);
                                session.notices.push(notice.clone());
                                let _ = event_tx_ref.send(ServerEvent::SessionNotice {
                                    id: sid.clone(),
                                    notice,
                                });
                            }
                        }

                        // If every source slot is empty, the recorder can't
                        // produce any more audio — auto-stop the session so the
                        // surviving writers finalize and the ticker stops
                        // re-processing it.
                        if recorder.has_no_live_sources() {
                            auto_stop_after_loss.push(sid.clone());
                        }
                    }
                }

                // Auto-stop sessions whose every source was lost. Done outside
                // the per-source loop so stop_recording can take its own lock.
                for sid in auto_stop_after_loss {
                    info!("Auto-stopping session {} — all sources lost", sid);
                    if let Err(e) = manager.stop_recording(&sid).await {
                        warn!("Failed to auto-stop session {} after source loss: {}", sid, e);
                    }
                }

                // Phase 3: auto-stop sessions where system audio went silent.
                for (session_id, seconds) in auto_stop_sessions {
                    info!("Auto-stopping session {} (system audio silent for {}s)", session_id, seconds);
                    if let Err(e) = manager
                        .auto_stop_recording(
                            &session_id,
                            AutoStopTrigger::SystemAudioSilence { seconds },
                        )
                        .await
                    {
                        warn!("Failed to auto-stop session {}: {}", session_id, e);
                    }
                }
            }
        });
    }

    /// Roll back an unsuccessful user edit without disturbing live operations.
    fn persist_user_edit(session: &mut Session) -> Result<(), String> {
        if let Err(error) = Self::write_metadata(session) {
            let baseline = session.persisted_metadata.lock().unwrap().clone();
            if let Ok(meta) = serde_json::from_value::<SessionMetadata>(baseline) {
                session.name = meta.name;
                session.notes = meta.notes;
                session.tags = meta.tags;
                session.auto_stop = meta.auto_stop;
                session.config.language = meta.language;
                session.updated_at = meta.updated_at;
            }
            return Err(error);
        }
        Ok(())
    }

    /// Reconcile small metadata documents and directory listings. Content files
    /// are never parsed here. Live recorder/job state survives reconciliation.
    pub async fn load_from_disk(&self) {
        self.reconcile().await;
    }

    pub async fn reconcile(&self) {
        let mut revisions = self.disk_revisions.lock().await;
        let root = self.output_dir.clone();
        let old = revisions.clone();
        let scanned = crate::storage::blocking(move || {
            let mut entries = Vec::new();
            for dir in crate::storage::session_dirs(&root) {
                let Some(id) = dir.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue; };
                let path = dir.join("metadata.json");
                let rev = crate::storage::revision(&path);
                if rev.is_none() { continue; }
                let mut files: Vec<String> = std::fs::read_dir(&dir).into_iter().flatten().flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| !n.starts_with('.')).collect();
                files.sort();
                let stamp = (rev, files.clone());
                let meta = if old.get(&id) != Some(&stamp) {
                    match crate::storage::read_json::<SessionMetadata>(&path) {
                        Ok(meta) if meta.session_id == id => Some(meta),
                        Ok(_) => { warn!("Session ID does not match directory {}", id); continue; }
                        Err(e) => { warn!("{}", e); entries.push((id, None, None)); continue; }
                    }
                } else { None };
                entries.push((id, Some(stamp), meta));
            }
            entries
        }).await;
        let mut sessions = self.sessions.write().await;
        let present: std::collections::HashSet<_> = scanned.iter().map(|(id, _, _)| id.clone()).collect();
        sessions.retain(|id, s| present.contains(id) || !revisions.contains_key(id) || s.state == SessionState::Recording || s.recorder.is_some() || s.transitioning.load(std::sync::atomic::Ordering::SeqCst));
        revisions.retain(|id, _| present.contains(id));
        for (id, stamp, meta) in scanned {
            let Some(stamp) = stamp else { continue; };
            if let Some(meta) = meta {
                if crate::storage::revision(&self.output_dir.join(&id).join("metadata.json")) != stamp.0 { continue; }
                if let Some(active) = sessions.get(&id) {
                    if active.state == SessionState::Recording || active.recorder.is_some() || active.transitioning.load(std::sync::atomic::Ordering::SeqCst) {
                        // Revisit on the next reconciliation after recording stops.
                        continue;
                    }
                }
                let mut session = Session::from_metadata(&meta, &self.output_dir, stamp.1.clone());
                if let Some(previous) = sessions.remove(&id) {
                    session.notices = previous.notices;
                    session.dismissed_notice_keys = previous.dismissed_notice_keys;
                    session.processing_state = previous.processing_state;
                    session.summary_started_at = previous.summary_started_at;
                    session.config.summarization_instruction = previous.config.summarization_instruction;
                    session.config.sources = previous.config.sources;
                }
                sessions.insert(id.clone(), session);
            }
            revisions.insert(id, stamp);
        }
    }

    pub fn output_dir(&self) -> &PathBuf {
        &self.output_dir
    }

    /// Returns the folder path for a given session: recordings/{session_id}/
    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.output_dir.join(session_id)
    }

    pub async fn create_session(&self, mut config: SessionConfig) -> SessionInfo {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let id = format_base36(nanos);
        let session_dir = self.session_dir(&id);
        config.output_dir = session_dir;
        let mut session = Session::new(id.clone(), config);
        if let Err(e) = Self::write_metadata(&session) {
            warn!("Failed to write metadata on create: {}", e);
        }
        session.release_notes();
        let info = session.info();
        self.sessions.write().await.insert(id, session);
        self.emit(ServerEvent::SessionCreated(info.clone()));
        info
    }

    pub async fn get_session(&self, id: &str) -> Option<SessionInfo> {
        self.reconcile().await;
        self.get_session_cached(id).await
    }

    /// For batch operations that already reconciled once at their boundary.
    pub(crate) async fn get_session_cached(&self, id: &str) -> Option<SessionInfo> {
        self.sessions.read().await.get(id).map(|s| s.info())
    }

    /// Finalize a freshly created session after an uploaded media file has
    /// been converted to the app's target Opus format.
    pub async fn complete_media_import(
        &self,
        id: &str,
        original_filename: &str,
        opus_filename: String,
    ) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        if session.state != SessionState::Created {
            return Err("files can only be uploaded to a newly created session".to_string());
        }

        session.config.format = AudioFormat::Opus;
        session.config.raw_sample_rate = 48_000;
        session.config.sources = Some(Vec::new());
        session.state = SessionState::Stopped;
        session.started_at = Some(Utc::now());
        session.files = vec![opus_filename.clone(), "metadata.json".to_string()];
        session.source_meta = vec![session::SourceMetadata {
            filename: opus_filename,
            source_type: SourceType::App,
            source_label: original_filename.to_string(),
            channels: 1,
            raw_sample_rate: 48_000,
        }];

        if session.name.is_none() {
            let stem = std::path::Path::new(original_filename)
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Uploaded recording");
            session.name = Some(stem.to_string());
        }

        session.touch();
        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata after media import: {}", e);
        }
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn dismiss_notice(
        &self,
        id: &str,
        created_at: DateTime<Utc>,
    ) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        let original_len = session.notices.len();
        for notice in session.notices.iter().filter(|notice| notice.created_at == created_at) {
            if let Some(key) = &notice.key {
                session.dismissed_notice_keys.insert(key.clone());
            }
        }
        session.notices.retain(|notice| notice.created_at != created_at);
        if session.notices.len() == original_len {
            return Err("notice not found".to_string());
        }
        let info = session.info();
        self.emit(ServerEvent::SessionNotices {
            id: id.to_string(),
            notices: session.notices.clone(),
        });
        Ok(info)
    }

    /// IDs of all sessions currently in `Recording` state. Used for graceful
    /// shutdown so writers can finalize before the process exits.
    pub async fn recording_session_ids(&self) -> Vec<String> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| s.state == SessionState::Recording)
            .map(|s| s.id.clone())
            .collect()
    }

    /// Stop every recording session, finalizing audio writers. Called on
    /// SIGINT/SIGTERM so opus files get their trailing pages written.
    pub async fn shutdown(&self) {
        let ids = self.recording_session_ids().await;
        if ids.is_empty() {
            return;
        }
        info!("Shutdown: stopping {} active recording session(s)", ids.len());
        for id in ids {
            match self.stop_recording(&id).await {
                Ok(_) => info!("Shutdown: stopped session {}", id),
                Err(e) => warn!("Shutdown: failed to stop session {}: {}", id, e),
            }
        }
    }

    /// Stop every active recording that opted into this system event. Stops
    /// run concurrently so all writers can finish within macOS's pre-sleep
    /// acknowledgement window.
    pub async fn auto_stop_recordings(&self, trigger: AutoStopTrigger) -> usize {
        let ids: Vec<String> = self.sessions
            .read()
            .await
            .values()
            .filter(|session| {
                session.state == SessionState::Recording
                    && trigger.enabled(session.auto_stop)
            })
            .map(|session| session.id.clone())
            .collect();

        let results = futures::future::join_all(ids.into_iter().map(|id| {
            let manager = self.clone();
            async move { manager.auto_stop_recording(&id, trigger).await }
        }))
        .await;
        results.into_iter().filter(Result::is_ok).count()
    }

    async fn auto_stop_recording(
        &self,
        id: &str,
        trigger: AutoStopTrigger,
    ) -> Result<(), String> {
        let still_enabled = self.sessions
            .read()
            .await
            .get(id)
            .map(|session| {
                session.state == SessionState::Recording
                    && trigger.enabled(session.auto_stop)
            })
            .unwrap_or(false);
        if !still_enabled {
            return Err("auto-stop trigger is no longer enabled".to_string());
        }
        self.stop_recording(id).await?;

        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.notices.push(trigger.notice());
            self.emit(ServerEvent::SessionUpdated(session.info()));
        }
        Ok(())
    }

    pub async fn list_sessions(
        &self,
        limit: usize,
        offset: usize,
        hidden_tags: &std::collections::HashSet<String>,
    ) -> (Vec<SessionInfo>, usize) {
        self.reconcile().await;
        let sessions = self.sessions.read().await;
        let mut selected: Vec<&Session> = sessions.values()
            .filter(|s| hidden_tags.is_empty() || s.tags.is_empty()
                || !s.tags.iter().all(|t| hidden_tags.contains(t)))
            .collect();
        selected.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
        let total = selected.len();
        let page = selected.into_iter().skip(offset).take(limit).map(Session::info).collect();
        (page, total)
    }

    pub async fn rename_session(&self, id: &str, name: String) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.name = if name.trim().is_empty() { None } else { Some(name.trim().to_string()) };
        session.touch();
        Self::persist_user_edit(session)?;
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn update_auto_stop(
        &self,
        id: &str,
        system_audio_silence_secs: Option<Option<u64>>,
        screen_lock: Option<bool>,
        system_sleep: Option<bool>,
    ) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        if matches!(system_audio_silence_secs, Some(Some(0))) {
            return Err("system_audio_silence_secs must be at least 1".to_string());
        }
        if let Some(seconds) = system_audio_silence_secs {
            session.auto_stop.system_audio_silence_secs = seconds;
        }
        if let Some(enabled) = screen_lock {
            session.auto_stop.screen_lock = enabled;
        }
        if let Some(enabled) = system_sleep {
            session.auto_stop.system_sleep = enabled;
        }
        info!("Auto-stop settings updated for session {}: {:?}", id, session.auto_stop);
        session.touch();
        Self::persist_user_edit(session)?;
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn update_session_language(&self, id: &str, language: String) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.config.language = language;
        session.touch();
        Self::persist_user_edit(session)?;
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), String> {
        // Extract recorder under lock, then stop outside lock
        let mut recorder_to_stop = None;
        let session_dir;
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.remove(id) {
                session_dir = session.config.output_dir.clone();
                if session.state == SessionState::Recording {
                    recorder_to_stop = session.recorder.take();
                }
            } else {
                return Err("session not found".to_string());
            }
        }
        if let Some(mut recorder) = recorder_to_stop {
            // Blocking Core Audio calls — keep them off the async runtime and
            // don't let a wedged stop block the delete response.
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(20),
                tokio::task::spawn_blocking(move || {
                    let _ = recorder.stop();
                }),
            )
            .await;
        }
        // Delete the session directory and all its files from disk
        if session_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&session_dir) {
                warn!("Failed to delete session directory {}: {}", session_dir.display(), e);
            }
        }
        self.emit(ServerEvent::SessionDeleted { id: id.to_string() });
        Ok(())
    }

    pub async fn start_recording(&self, id: &str) -> Result<Vec<String>, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;

        if session.state == SessionState::Recording {
            return Err("session is already recording".to_string());
        }

        if session.transitioning.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err("recording operation already in progress".into());
        }
        let _transition = RuntimeTransition(session.transitioning.clone());

        // Resolve source IDs to (descriptor, AudioSource) pairs
        let source_ids = session
            .config
            .sources
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(default_source_ids);

        let mut sources: Vec<(SourceDescriptor, Box<dyn AudioSource>)> = Vec::new();

        for source_id in &source_ids {
            match resolve_source(source_id, session.config.raw_sample_rate) {
                Ok(pair) => {
                    info!("Source '{}' created for session {}", source_id, session.id);
                    sources.push(pair);
                }
                Err(e) => {
                    warn!("Skipping source '{}': {}", source_id, e);
                }
            }
        }

        if sources.is_empty() {
            return Err("no audio sources could be initialized".to_string());
        }

        let mut recorder = Recorder::new(
            session.id.clone(),
            session.config.output_dir.clone(),
            session.config.raw_sample_rate,
            session.config.format,
            session.config.mp3,
            session.config.opus,
            sources,
        );

        // Drop the write lock before the blocking start — source.start() calls
        // Core Audio APIs that can hang when devices are being reconfigured.
        let session_id = session.id.clone();
        drop(sessions);

        // Run on a blocking thread so Core Audio calls don't stall the async runtime.
        // Timeout after 15s — Core Audio can hang when the audio device graph is
        // being reconfigured (USB-to-HDMI adapter, virtual audio devices, etc.).
        let (mut recorder, files) = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            tokio::task::spawn_blocking(move || {
                recorder.start().map(|files| (recorder, files))
            }),
        )
        .await
        .map_err(|_| "recording start timed out after 15s — audio system may be busy (try restarting the app)".to_string())?
        .map_err(|e| format!("recorder thread panicked: {e}"))?
        .map_err(|e| e.to_string())?;

        // Re-acquire lock to update session state
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(&session_id).ok_or("session not found")?;

        // Guard against concurrent start (session may have been modified while unlocked)
        if session.state == SessionState::Recording {
            // Another start_recording won the race — stop what we just started
            // (off the runtime; blocking Core Audio calls)
            tokio::task::spawn_blocking(move || {
                let _ = recorder.stop();
            });
            return Err("session is already recording".to_string());
        }

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .collect();

        session.recorder = Some(recorder);
        session.state = SessionState::Recording;
        session.started_at = Some(Utc::now());
        session.files = file_names.clone();
        session.capture_source_meta();
        session.touch();

        if let Err(e) = Self::write_metadata(session) {
            warn!("Failed to write metadata on start: {}", e);
        }
        self.emit(ServerEvent::SessionUpdated(session.info()));

        Ok(file_names)
    }

    pub async fn stop_recording(&self, id: &str) -> Result<Vec<String>, String> {
        // Phase 1: Extract recorder under write lock, mark stopped.
        // If the recorder is missing (e.g. lost during a failed Core Audio
        // recovery), force-stop: mark Stopped anyway so the user is unstuck.
        // Whatever audio writers are still alive will be cleaned up on
        // process exit.
        let (recorder, _transition) = {
            let mut sessions = self.sessions.write().await;
            let session = sessions.get_mut(id).ok_or("session not found")?;

            if session.state != SessionState::Recording {
                return Err("session is not recording".to_string());
            }

            if session.transitioning.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return Err("recording operation already in progress".into());
            }
            let transition = RuntimeTransition(session.transitioning.clone());

            let recorder = session.recorder.take();

            session.state = SessionState::Stopped;
            session.touch();

            // Emit early update so UI sees "stopped" state immediately
            self.emit(ServerEvent::SessionUpdated(session.info()));

            (recorder, transition)
        };
        // Write lock released here

        // Phase 2: Stop recorder without holding any lock, on a blocking
        // thread with a hard timeout. Core Audio calls (and writer joins)
        // can wedge; the session must still end up cleanly Stopped with
        // metadata written, no matter what — otherwise the HTTP request
        // hangs forever and blocks graceful shutdown with it.
        // Force-stop path: no recorder means we just finalize the session
        // metadata with whatever files exist on disk.
        const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
        let mut force_stopped = false;
        let mut stop_error: Option<String> = None;
        if let Some(mut rec) = recorder {
            let timed = tokio::time::timeout(
                STOP_TIMEOUT,
                tokio::task::spawn_blocking(move || rec.stop()),
            )
            .await;
            match timed {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(e))) => {
                    warn!("Session {} stopped with errors: {}", id, e);
                    stop_error = Some(e.to_string());
                }
                Ok(Err(join_err)) => {
                    warn!("Recorder stop thread panicked for session {}: {}", id, join_err);
                    force_stopped = true;
                }
                Err(_) => {
                    warn!(
                        "Recorder stop timed out after {:?} for session {} — abandoning recorder thread",
                        STOP_TIMEOUT, id
                    );
                    force_stopped = true;
                }
            }
        } else {
            warn!("Force-stopping session {} with no active recorder", id);
            force_stopped = true;
        }

        // Phase 3: Re-acquire lock, update files, write metadata
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            if !session.files.contains(&"metadata.json".to_string()) {
                session.files.push("metadata.json".to_string());
            }
            session.touch();
            if force_stopped {
                let notice = Notice {
                    key: None,
                    level: NoticeLevel::Warning,
                    message: "Recording was force-stopped — files may be missing trailing audio".to_string(),
                    platform: Some(std::env::consts::OS.to_string()),
                    details: Some(
                        "The audio recorder was lost during a Core Audio device change. \
                        The session was marked stopped without finalizing the encoders, so \
                        the final ~1s of audio and the file's trailing metadata may be missing. \
                        Most decoders can still play the partial files."
                            .to_string(),
                    ),
                    created_at: Utc::now(),
                };
                session.notices.push(notice.clone());
                self.emit(ServerEvent::SessionNotice {
                    id: id.to_string(),
                    notice,
                });
            }
            if let Some(err) = stop_error {
                let notice = Notice {
                    key: None,
                    level: NoticeLevel::Warning,
                    message: "Recording stopped, but with errors — audio files may be incomplete".to_string(),
                    platform: Some(std::env::consts::OS.to_string()),
                    details: Some(format!(
                        "One or more sources or encoders failed while stopping: {}. \
                        The recorded files were finalized as far as possible and are \
                        usually still playable.",
                        err
                    )),
                    created_at: Utc::now(),
                };
                session.notices.push(notice.clone());
                self.emit(ServerEvent::SessionNotice {
                    id: id.to_string(),
                    notice,
                });
            }
            if let Err(e) = Self::write_metadata(session) {
                warn!("Failed to write metadata on stop: {}", e);
            }
            let info = session.info();
            self.emit(ServerEvent::SessionUpdated(info));
            Ok(session.files.clone())
        } else {
            Ok(vec![])
        }
    }

    pub async fn get_files(&self, id: &str) -> Result<Vec<String>, String> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or("session not found")?;
        Ok(session.files.clone())
    }

    /// Re-scan the session directory and update the files list.
    pub async fn refresh_files(&self, id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            let dir = self.output_dir.join(id);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                session.files = entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|name| !name.starts_with('.'))
                    .collect();
                self.emit(ServerEvent::SessionUpdated(session.info()));
            }
        }
    }

    /// Set the processing state for a session.
    pub async fn set_processing_state(&self, id: &str, state: Option<String>) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.processing_state = state;
            session.touch();
            self.emit(ServerEvent::SessionUpdated(session.info()));
        }
    }

    pub fn emit_transcription_progress(&self, id: &str, status: &str) {
        self.emit(ServerEvent::TranscriptionProgress {
            id: id.to_string(),
            status: status.to_string(),
        });
    }

    pub fn emit_transcription_completed(&self, id: &str, unconfirmed_speakers: u32) {
        self.emit(ServerEvent::TranscriptionCompleted {
            id: id.to_string(),
            unconfirmed_speakers,
        });
    }

    pub fn emit_transcription_failed(&self, id: &str, error: &str) {
        self.emit(ServerEvent::TranscriptionFailed {
            id: id.to_string(),
            error: error.to_string(),
        });
    }

    pub fn emit_summary_delta(&self, id: &str, delta: &str) {
        self.emit(ServerEvent::SummaryDelta {
            id: id.to_string(),
            delta: delta.to_string(),
        });
    }

    pub fn emit_summary_thinking(&self, id: &str, delta: &str) {
        self.emit(ServerEvent::SummaryThinking {
            id: id.to_string(),
            delta: delta.to_string(),
        });
    }

    pub async fn emit_summary_progress(&self, id: &str, status: &str) {
        let started_at;
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(id) {
                if session.summary_started_at.is_none() {
                    session.summary_started_at = Some(Utc::now());
                }
                started_at = session.summary_started_at.unwrap();
            } else {
                started_at = Utc::now();
            }
        }
        self.emit(ServerEvent::SummaryProgress {
            id: id.to_string(),
            status: status.to_string(),
            started_at,
        });
    }

    pub async fn emit_summary_completed(&self, id: &str) {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(id) {
                session.summary_started_at = None;
            }
        }
        let dir = self.session_dir(id);
        // Read saved summary and todos so frontend has them immediately
        let summary = std::fs::read_to_string(dir.join("summary.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let todos = std::fs::read_to_string(dir.join("todos.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        self.emit(ServerEvent::SummaryCompleted {
            id: id.to_string(),
            summary,
            todos,
        });
    }

    pub async fn emit_summary_failed(&self, id: &str, error: &str) {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(id) {
                session.summary_started_at = None;
            }
        }
        self.emit(ServerEvent::SummaryFailed {
            id: id.to_string(),
            error: error.to_string(),
        });
    }

    /// Set the audio extraction job info and persist to metadata.json.
    pub async fn set_audio_extraction(&self, id: &str, job: Option<session::AudioExtractionJob>) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.audio_extraction = job;
            session.touch();
            if let Err(e) = Self::write_metadata(session) {
                tracing::warn!("Failed to write metadata for {}: {}", id, e);
            }
        }
    }

    /// Get all sessions with in-progress extraction jobs (for resume on startup).
    pub async fn get_pending_extractions(&self) -> Vec<(String, session::AudioExtractionJob)> {
        let sessions = self.sessions.read().await;
        sessions.values()
            .filter_map(|s| {
                s.audio_extraction.as_ref()
                    .filter(|j| j.status == "in_progress")
                    .map(|j| (s.id.clone(), j.clone()))
            })
            .collect()
    }

    /// Set the tags for a session.
    pub async fn update_session_tags(&self, id: &str, tags: Vec<String>) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        session.tags = tags;
        session.touch();
        Self::persist_user_edit(session)?;
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    pub async fn update_session_notes(&self, id: &str, notes: Option<String>) -> Result<SessionInfo, String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id).ok_or("session not found")?;
        let current = session.current_notes();
        session.persisted_metadata.lock().unwrap()["notes"] = serde_json::json!(current);
        session.notes_loaded = true;
        session.notes = notes;
        session.touch();
        let result = Self::persist_user_edit(session);
        session.release_notes();
        result?;
        let info = session.info();
        self.emit(ServerEvent::SessionUpdated(info.clone()));
        Ok(info)
    }

    /// Rename a tag across all sessions.
    pub async fn rename_tag_in_all_sessions(&self, old_name: &str, new_name: &str) {
        let mut sessions = self.sessions.write().await;
        for session in sessions.values_mut() {
            if let Some(pos) = session.tags.iter().position(|t| t == old_name) {
                session.tags[pos] = new_name.to_string();
                session.touch();
                if let Err(e) = Self::write_metadata(session) {
                    warn!("Failed to write metadata after tag rename for {}: {}", session.id, e);
                }
                self.emit(ServerEvent::SessionUpdated(session.info()));
            }
        }
    }

    /// Remove a tag from all sessions (cascade on tag deletion).
    pub async fn remove_tag_from_all_sessions(&self, tag_name: &str) {
        let mut sessions = self.sessions.write().await;
        for session in sessions.values_mut() {
            if session.tags.contains(&tag_name.to_string()) {
                session.tags.retain(|t| t != tag_name);
                session.touch();
                if let Err(e) = Self::write_metadata(session) {
                    warn!("Failed to write metadata after tag removal for {}: {}", session.id, e);
                }
                self.emit(ServerEvent::SessionUpdated(session.info()));
            }
        }
    }

    /// Count sessions per tag.
    pub async fn tag_session_counts(&self) -> HashMap<String, usize> {
        let sessions = self.sessions.read().await;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for session in sessions.values() {
            for tag in &session.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Get sessions that have a given tag.
    pub async fn sessions_for_tag(&self, tag_name: &str) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos: Vec<SessionInfo> = sessions.values()
            .filter(|s| s.tags.contains(&tag_name.to_string()))
            .map(|s| s.info())
            .collect();
        infos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        infos
    }

    /// Get the session directory path, session language, and source metadata.
    pub async fn get_session_extraction_info(
        &self,
        id: &str,
    ) -> Result<(std::path::PathBuf, String, Vec<session::SourceMetadata>), String> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or("session not found")?;
        if session.state == SessionState::Recording {
            return Err("cannot transcribe while recording".to_string());
        }
        if session.files.iter().all(|f| f == "metadata.json") {
            return Err("no audio files to transcribe".to_string());
        }
        Ok((
            session.config.output_dir.clone(),
            session.config.language.clone(),
            session.source_meta.clone(),
        ))
    }

    /// Export all sessions as entries for index generation.
    pub async fn session_entries(&self) -> Vec<crate::markdown::SessionEntry> {
        let sessions = self.sessions.read().await;
        sessions.values().map(|s| {
            let duration_secs = Session::compute_duration(
                &s.config.output_dir, &s.files, s.config.mp3.bitrate_kbps,
            );
            crate::markdown::SessionEntry {
                id: s.id.clone(),
                name: s.name.clone(),
                language: s.config.language.clone(),
                tags: s.tags.clone(),
                created_at: s.created_at,
                duration_secs,
                state: format!("{:?}", s.state).to_lowercase(),
            }
        }).collect()
    }
}

/// Default source IDs when none specified: mic + system_mix.
fn default_source_ids() -> Vec<String> {
    vec!["mic".to_string(), "system_mix".to_string()]
}

/// Resolve a source ID string to a (descriptor, AudioSource) pair.
fn resolve_source(
    source_id: &str,
    sample_rate: u32,
) -> Result<(SourceDescriptor, Box<dyn AudioSource>), String> {
    if source_id == "system_mix" {
        let source = SystemAudioSource::new(sample_rate).map_err(|e| e.to_string())?;
        let desc = SourceDescriptor {
            id: "system_mix".to_string(),
            source_type: SourceType::SystemMix,
            label: "System Audio".to_string(),
            device_name: None,
        };
        Ok((desc, Box::new(source)))
    } else if source_id == "mic" || source_id.starts_with("mic:") {
        // All mic source IDs resolve to the same AVAudioEngine-based source
        // that uses the system default input device.
        let source = MicSource::new(sample_rate);
        let desc = SourceDescriptor {
            id: "mic".to_string(),
            source_type: SourceType::Mic,
            label: "System Microphone".to_string(),
            device_name: None,
        };
        Ok((desc, Box::new(source)))
    } else {
        Err(format!("unknown source: {}", source_id))
    }
}

fn format_base36(mut n: u64) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::with_capacity(12);
    while n > 0 {
        buf.push(CHARS[(n % 36) as usize]);
        n /= 36;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

/// Recompute live notices for recording sources based on current file sizes.
/// Notices with a `key` are auto-managed: added when a condition is detected,
/// removed when it resolves. Emits SessionNotices when the set changes.
fn source_unrecoverable_notice(label: &str) -> Notice {
    Notice {
        key: None,
        level: NoticeLevel::Error,
        message: format!("Source \"{}\" is unrecoverable and was dropped", label),
        platform: Some(std::env::consts::OS.to_string()),
        details: Some(
            "Core Audio hung or panicked during a device-change recovery for this source. \
            Other sources keep recording; the session will auto-stop only if every source \
            is lost. Start a new recording to capture this source again."
                .to_string(),
        ),
        created_at: Utc::now(),
    }
}

fn max_attempts_notice() -> Notice {
    Notice {
        key: None,
        level: NoticeLevel::Error,
        message: "Microphone lost — could not reconnect".to_string(),
        platform: Some(std::env::consts::OS.to_string()),
        details: Some(
            "A video conferencing app (e.g. Teams) may have disrupted Core Audio. \
            Stop and restart recording. If that fails, restart the audio system: \
            sudo launchctl kickstart -kp system/com.apple.audio.coreaudiod"
                .to_string(),
        ),
        created_at: Utc::now(),
    }
}

fn update_source_notices(
    session: &mut Session,
    activity: &HashMap<String, AudioActivitySnapshot>,
    now_ms: u64,
    event_tx: &broadcast::Sender<ServerEvent>,
) {
    let platform = std::env::consts::OS;
    let mut expected_keys: HashMap<String, Notice> = HashMap::new();

    for meta in &session.source_meta {
        let activity = activity.get(&meta.filename).copied().unwrap_or_default();
        let no_data = activity.last_received_ms == 0
            || now_ms.saturating_sub(activity.last_received_ms) >= 10_000;
        let key = format!("silent:{}", meta.filename);

        match meta.source_type {
            SourceType::Mic => {
                if no_data {
                    let (message, details) = if platform == "macos" {
                        (
                            format!("\"{}\" is not receiving audio", meta.source_label),
                            Some(
                                "macOS may have denied microphone access. \
                                Check System Settings > Privacy & Security > Microphone \
                                and allow Meeting Notes (or your terminal app when running the CLI)."
                                    .to_string(),
                            ),
                        )
                    } else {
                        (
                            format!("\"{}\" is not receiving audio", meta.source_label),
                            Some("Check that your microphone is connected and permissions are granted.".to_string()),
                        )
                    };
                    expected_keys.insert(key, Notice {
                        key: Some(format!("silent:{}", meta.filename)),
                        level: NoticeLevel::Warning,
                        message,
                        platform: Some(platform.to_string()),
                        details,
                        created_at: Utc::now(),
                    });
                }
            }
            SourceType::SystemMix => {
                // Initial silence is suspicious after the startup grace period.
                // Once sound has arrived, tolerate normal pauses up to 30 seconds.
                let silent = activity.last_active_ms == 0
                    || now_ms.saturating_sub(activity.last_active_ms) >= 30_000;
                if no_data || silent {
                    let message = if no_data {
                        "System audio is not receiving data"
                    } else {
                        "System audio is receiving only silence"
                    };
                    let details = if platform == "macos" {
                        "If sound is playing, check System Settings > Privacy & Security > \
                        Screen & System Audio Recording and allow Meeting Notes (or your terminal app \
                        when running the CLI), then restart recording. If no permission dialog appears, \
                        launch Meeting Notes with bash scripts/run-macos.sh as described in the README."
                    } else {
                        "If sound is playing, check the system audio source, output device, and recording permissions."
                    };
                    expected_keys.insert(key, Notice {
                        key: Some(format!("silent:{}", meta.filename)),
                        level: NoticeLevel::Warning,
                        message: message.to_string(),
                        platform: Some(platform.to_string()),
                        details: Some(details.to_string()),
                        created_at: Utc::now(),
                    });
                }
            }
            _ => {}
        }
    }

    // A dismissed live warning stays hidden while the same problem remains,
    // but becomes eligible to appear again after the condition resolves once.
    session
        .dismissed_notice_keys
        .retain(|key| expected_keys.contains_key(key));
    expected_keys.retain(|key, _| !session.dismissed_notice_keys.contains(key));

    // Compute what changed: compare current keyed notices with expected
    let current: HashMap<_, _> = session.notices.iter()
        .filter_map(|n| n.key.as_ref().map(|key| (key.clone(), n)))
        .collect();
    let unchanged = current.len() == expected_keys.len() && expected_keys.iter().all(|(key, notice)| {
        current.get(key).is_some_and(|old| old.message == notice.message && old.details == notice.details)
    });
    if unchanged {
        return; // No change
    }

    // Log transitions once, including when no browser is connected. Preserve
    // timestamps of unchanged notices so dismissal by created_at still works.
    for (key, notice) in &mut expected_keys {
        if let Some(old) = current.get(key).filter(|old| old.message == notice.message && old.details == notice.details) {
            notice.created_at = old.created_at;
        } else {
            warn!(session_id = %session.id, "{}: {}", notice.message, notice.details.as_deref().unwrap_or_default());
        }
    }
    for key in current.keys().filter(|key| !expected_keys.contains_key(*key)) {
        info!(session_id = %session.id, source = %key, "Audio capture warning cleared");
    }

    // Remove stale keyed notices, keep non-keyed (manual) notices
    session.notices.retain(|n| n.key.is_none());
    // Add current keyed notices
    for (_, notice) in expected_keys {
        session.notices.push(notice);
    }

    // Emit full notices list
    let _ = event_tx.send(ServerEvent::SessionNotices {
        id: session.id.clone(),
        notices: session.notices.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dismissed_live_notice_stays_hidden_until_condition_resolves() {
        let dir = std::env::temp_dir().join(format!(
            "meeting-notes-notice-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        ));
        let manager = SessionManager::new(dir.clone());
        let info = manager.create_session(SessionConfig::default()).await;
        let created_at = Utc::now();
        {
            let mut sessions = manager.sessions.write().await;
            let session = sessions.get_mut(&info.id).unwrap();
            session.source_meta.push(session::SourceMetadata {
                filename: "mic.opus".to_string(),
                source_type: SourceType::Mic,
                source_label: "Test Mic".to_string(),
                channels: 1,
                raw_sample_rate: 48_000,
            });
            session.notices.push(Notice {
                key: Some("silent:mic.opus".to_string()),
                level: NoticeLevel::Warning,
                message: "not receiving audio".to_string(),
                platform: None,
                details: None,
                created_at,
            });
        }

        let mut events = manager.subscribe();
        manager.dismiss_notice(&info.id, created_at).await.unwrap();
        match events.recv().await.unwrap() {
            ServerEvent::SessionNotices { notices, .. } => assert!(notices.is_empty()),
            _ => panic!("expected a full notices replacement"),
        }

        let mut sessions = manager.sessions.write().await;
        let session = sessions.get_mut(&info.id).unwrap();
        let now_ms = Utc::now().timestamp_millis() as u64;
        update_source_notices(session, &HashMap::new(), now_ms, &manager.event_tx);
        assert!(session.notices.is_empty(), "dismissed condition reappeared");

        let mut healthy_activity = HashMap::new();
        healthy_activity.insert("mic.opus".to_string(), AudioActivitySnapshot {
            last_received_ms: now_ms,
            last_active_ms: now_ms,
        });
        update_source_notices(session, &healthy_activity, now_ms, &manager.event_tx);
        assert!(session.dismissed_notice_keys.is_empty());

        update_source_notices(session, &HashMap::new(), now_ms, &manager.event_tx);
        assert_eq!(session.notices.len(), 1, "a new occurrence should be shown");
        drop(sessions);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn system_audio_notices_follow_samples_even_when_silence_file_grows() {
        let dir = std::env::temp_dir().join(format!("mn-silence-test-{}", uuid::Uuid::new_v4()));
        let manager = SessionManager::new(dir.clone());
        let info = manager.create_session(SessionConfig::default()).await;
        let mut events = manager.subscribe();
        let mut sessions = manager.sessions.write().await;
        let session = sessions.get_mut(&info.id).unwrap();
        session.source_meta.push(session::SourceMetadata {
            filename: "system_audio.opus".to_string(),
            source_type: SourceType::SystemMix,
            source_label: "System Audio".to_string(),
            channels: 2,
            raw_sample_rate: 48_000,
        });
        // Reproduces the observed incident: a nonempty file of encoded silence.
        std::fs::write(session.config.output_dir.join("system_audio.opus"), vec![0u8; 40_000]).unwrap();
        let now = 100_000;
        let silent = HashMap::from([("system_audio.opus".to_string(), AudioActivitySnapshot {
            last_received_ms: now,
            last_active_ms: 0,
        })]);
        update_source_notices(session, &silent, now, &manager.event_tx);
        assert_eq!(session.notices.len(), 1);
        assert_eq!(session.notices[0].message, "System audio is receiving only silence");
        assert!(matches!(events.try_recv(), Ok(ServerEvent::SessionNotices { .. })));
        let created_at = session.notices[0].created_at;
        update_source_notices(session, &silent, now + 2_000, &manager.event_tx);
        assert!(events.try_recv().is_err(), "unchanged warning must not repeat");
        assert_eq!(session.notices[0].created_at, created_at);

        // A stalled callback changes the message even though its key is the same.
        update_source_notices(session, &silent, now + 10_000, &manager.event_tx);
        assert_eq!(session.notices[0].message, "System audio is not receiving data");
        assert!(matches!(events.try_recv(), Ok(ServerEvent::SessionNotices { .. })));

        // Audible PCM is healthy even while the output file is still buffered.
        std::fs::write(session.config.output_dir.join("system_audio.opus"), []).unwrap();
        let healthy = HashMap::from([("system_audio.opus".to_string(), AudioActivitySnapshot {
            last_received_ms: now + 12_000,
            last_active_ms: now + 12_000,
        })]);
        update_source_notices(session, &healthy, now + 12_000, &manager.event_tx);
        assert!(session.notices.is_empty());
        let pause = HashMap::from([("system_audio.opus".to_string(), AudioActivitySnapshot {
            last_received_ms: now + 41_999,
            last_active_ms: now + 12_000,
        })]);
        update_source_notices(session, &pause, now + 41_999, &manager.event_tx);
        assert!(session.notices.is_empty(), "short pauses are normal");
        update_source_notices(session, &pause, now + 42_000, &manager.event_tx);
        assert_eq!(session.notices[0].message, "System audio is receiving only silence");
        drop(sessions);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn completed_media_import_becomes_a_stopped_opus_session() {
        let dir = std::env::temp_dir().join(format!(
            "meeting-notes-import-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        ));
        let manager = SessionManager::new(dir.clone());
        let created = manager.create_session(SessionConfig::default()).await;
        std::fs::write(
            manager.session_dir(&created.id).join("meeting.opus"),
            b"test",
        )
        .unwrap();

        let imported = manager
            .complete_media_import(
                &created.id,
                "Quarterly Meeting.mp4",
                "meeting.opus".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(imported.state, SessionState::Stopped);
        assert_eq!(imported.name.as_deref(), Some("Quarterly Meeting"));
        assert_eq!(imported.format, Some(AudioFormat::Opus));
        assert_eq!(imported.raw_sample_rate, Some(48_000));
        assert_eq!(imported.files, vec!["meeting.opus", "metadata.json"]);
        assert_eq!(imported.source_meta.len(), 1);
        assert_eq!(imported.source_meta[0].source_type, SourceType::App);
        assert_eq!(imported.source_meta[0].channels, 1);

        let metadata =
            std::fs::read_to_string(manager.session_dir(&created.id).join("metadata.json"))
                .unwrap();
        assert!(metadata.contains("\"format\": \"opus\""));

        let _ = std::fs::remove_dir_all(dir);
    }
    #[tokio::test]
    async fn reconciliation_preserves_live_recorder_and_stop_merges_external_metadata() {
        use crate::audio::source::{AudioChunk, AudioError};
        struct SyntheticAudio;
        impl AudioSource for SyntheticAudio {
            fn start(&mut self, tx: crossbeam_channel::Sender<AudioChunk>) -> Result<(), AudioError> {
                tx.send(AudioChunk { samples: vec![0.25; 4800], channels: 1, sample_rate: 48000, timestamp_us: 0 }).unwrap();
                Ok(())
            }
            fn stop(&mut self) -> Result<(), AudioError> { Ok(()) }
            fn name(&self) -> &str { "Synthetic" }
        }
        for format in [AudioFormat::Wav, AudioFormat::Mp3, AudioFormat::Opus] {
            let root = std::env::temp_dir().join(format!("mn-lifecycle-{}", uuid::Uuid::new_v4()));
            let manager = SessionManager::new(root.clone());
            let created = manager.create_session(SessionConfig { format, ..Default::default() }).await;
            let dir = manager.session_dir(&created.id);
            let mut recorder = Recorder::new(created.id.clone(), dir.clone(), 48000, format,
                Default::default(), Default::default(), vec![(SourceDescriptor {
                    id: "synthetic".into(), source_type: SourceType::Mic, label: "Synthetic".into(), device_name: None,
                }, Box::new(SyntheticAudio))]);
            recorder.start().unwrap();
            {
                let mut sessions = manager.sessions.write().await;
                let session = sessions.get_mut(&created.id).unwrap();
                session.recorder = Some(recorder);
                session.state = SessionState::Recording;
                session.files = vec![format!("synthetic.{}", format.extension()), "metadata.json".into()];
                session.capture_source_meta();
                SessionManager::write_metadata(session).unwrap();
            }
            let path = dir.join("metadata.json");
            let mut external: Value = crate::storage::read_json(&path).unwrap();
            external["notes"] = serde_json::json!("External notes during recording");
            external["extension"] = serde_json::json!({"keep":true});
            crate::storage::write_json(&path, &external).unwrap();
            manager.reconcile().await;
            assert_eq!(manager.get_session(&created.id).await.unwrap().state, SessionState::Recording);
            assert!(manager.sessions.read().await[&created.id].recorder.is_some());
            manager.stop_recording(&created.id).await.unwrap();
            let filename = format!("synthetic.{}", format.extension());
            assert!(std::fs::metadata(dir.join(&filename)).unwrap().len() > 0);
            if format == AudioFormat::Wav {
                let wav = hound::WavReader::open(dir.join(&filename)).unwrap();
                assert_eq!(wav.duration(), 4800);
            }
            assert!(Session::compute_duration(&dir, &[filename], 128).unwrap() > 0.0);
            let saved: Value = crate::storage::read_json(&path).unwrap();
            assert_eq!(saved["state"], "stopped");
            assert_eq!(saved["notes"], "External notes during recording");
            assert_eq!(saved["extension"], serde_json::json!({"keep":true}));
            manager.shutdown().await;
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn notes_are_not_retained_and_selected_sources_survive_reconciliation() {
        let root = std::env::temp_dir().join(format!("mn-catalog-{}", uuid::Uuid::new_v4()));
        let manager = SessionManager::new(root.clone());
        let created = manager.create_session(SessionConfig { sources: Some(vec!["test-source".into()]), ..Default::default() }).await;
        let path = manager.session_dir(&created.id).join("metadata.json");
        let mut metadata: Value = crate::storage::read_json(&path).unwrap();
        let notes = "large note ".repeat(10000);
        metadata["notes"] = serde_json::json!(notes);
        crate::storage::write_json(&path, &metadata).unwrap();
        manager.reconcile().await;
        {
            let sessions = manager.sessions.read().await;
            let session = &sessions[&created.id];
            assert!(session.notes.is_none());
            assert!(!session.persisted_metadata.lock().unwrap().as_object().unwrap().contains_key("notes"));
            assert_eq!(session.config.sources.as_ref().unwrap(), &["test-source"]);
        }
        assert_eq!(manager.get_session(&created.id).await.unwrap().notes.as_deref(), Some(notes.as_str()));
        manager.rename_session(&created.id, "Renamed".into()).await.unwrap();
        assert_eq!(crate::storage::read_json::<Value>(&path).unwrap()["notes"], notes);
        manager.update_session_notes(&created.id, Some("Updated".into())).await.unwrap();
        assert!(manager.sessions.read().await[&created.id].notes.is_none());
        assert_eq!(manager.get_session(&created.id).await.unwrap().notes.as_deref(), Some("Updated"));
        std::fs::remove_dir_all(root).unwrap();
    }

}
