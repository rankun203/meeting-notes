use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// In-memory data layer for session files (transcripts, etc.).
/// Provides indexed reads and write-through caching to disk.
#[derive(Clone)]
pub struct FilesDb {
    recordings_dir: PathBuf,
    transcripts: Arc<RwLock<HashMap<String, CachedTranscript>>>,
    /// Index: person_id → set of session_ids where that person appears.
    person_sessions: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

/// In-memory cache of a session's transcript.json.
#[derive(Clone)]
struct CachedTranscript {
    /// Full parsed JSON (served directly via get_transcript).
    data: Value,
    /// Count of speakers without a person_id assigned.
    unconfirmed_speakers: u32,
    /// Person IDs found in speaker_embeddings (for index maintenance).
    person_ids: Vec<String>,
}

impl FilesDb {
    pub fn new(recordings_dir: PathBuf) -> Self {
        Self {
            recordings_dir,
            transcripts: Arc::new(RwLock::new(HashMap::new())),
            person_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Scan all session directories and load existing transcripts into cache.
    pub async fn load_from_disk(recordings_dir: &Path) -> Self {
        let db = Self::new(recordings_dir.to_path_buf());

        let entries: Vec<_> = std::fs::read_dir(recordings_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .collect();

        let mut loaded = 0u32;
        for entry in &entries {
            let session_id = entry.file_name().to_string_lossy().to_string();
            let transcript_path = entry.path().join("transcript.json");
            if transcript_path.exists() {
                match std::fs::read_to_string(&transcript_path) {
                    Ok(json_str) => {
                        match serde_json::from_str::<Value>(&json_str) {
                            Ok(data) => {
                                let cached = Self::build_cache(&session_id, data);
                                // Update person index
                                let mut idx = db.person_sessions.write().await;
                                for pid in &cached.person_ids {
                                    idx.entry(pid.clone()).or_default().insert(session_id.clone());
                                }
                                drop(idx);
                                db.transcripts.write().await.insert(session_id, cached);
                                loaded += 1;
                            }
                            Err(e) => warn!("Failed to parse transcript for {}: {}", session_id, e),
                        }
                    }
                    Err(e) => warn!("Failed to read transcript for {}: {}", session_id, e),
                }
            }
        }

        info!("FilesDb loaded {} transcripts from {} sessions", loaded, entries.len());
        db
    }

    /// Build a CachedTranscript from parsed JSON.
    fn build_cache(_session_id: &str, data: Value) -> CachedTranscript {
        let mut unconfirmed: u32 = 0;
        let mut person_ids = Vec::new();

        if let Some(embs) = data.get("speaker_embeddings").and_then(|v| v.as_object()) {
            for info in embs.values() {
                match info.get("person_id").and_then(|p| p.as_str()) {
                    Some(pid) => person_ids.push(pid.to_string()),
                    None => unconfirmed += 1,
                }
            }
        }

        CachedTranscript { data, unconfirmed_speakers: unconfirmed, person_ids }
    }

    // ── Read methods ──

    /// Get the full transcript JSON for a session.
    pub async fn get_transcript(&self, session_id: &str) -> Option<Value> {
        self.transcripts.read().await.get(session_id).map(|c| c.data.clone())
    }

    /// Check if a transcript exists for a session.
    pub async fn has_transcript(&self, session_id: &str) -> bool {
        self.transcripts.read().await.contains_key(session_id)
    }

    /// Get the number of unconfirmed speakers for a session.
    pub async fn unconfirmed_speakers(&self, session_id: &str) -> u32 {
        self.transcripts.read().await
            .get(session_id)
            .map(|c| c.unconfirmed_speakers)
            .unwrap_or(0)
    }

    /// Get session IDs where a person appears (instant index lookup).
    pub async fn get_person_session_ids(&self, person_id: &str) -> Vec<String> {
        self.person_sessions.read().await
            .get(person_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    // ── Write methods (write-through: update memory + disk) ──

    /// Store a transcript (writes to disk, updates cache and indexes).
    pub async fn put_transcript(&self, session_id: &str, data: Value) -> Result<(), String> {
        // Write to disk
        let session_dir = self.recordings_dir.join(session_id);
        let path = session_dir.join("transcript.json");
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("serialize transcript: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("write transcript: {}", e))?;

        // Write transcript.md
        crate::markdown::write_transcript_md(&session_dir, &data);

        // Update cache
        let cached = Self::build_cache(session_id, data);
        self.update_index(session_id, &cached).await;
        self.transcripts.write().await.insert(session_id.to_string(), cached);

        Ok(())
    }

    /// Remove a transcript (removes from cache, disk deletion handled by caller).
    pub async fn remove_transcript(&self, session_id: &str) {
        let old = self.transcripts.write().await.remove(session_id);

        // Remove from person index
        if let Some(cached) = old {
            let mut idx = self.person_sessions.write().await;
            for pid in &cached.person_ids {
                if let Some(set) = idx.get_mut(pid) {
                    set.remove(session_id);
                    if set.is_empty() { idx.remove(pid); }
                }
            }
        }
    }

    /// Update the person_sessions index for a session.
    async fn update_index(&self, session_id: &str, cached: &CachedTranscript) {
        let mut idx = self.person_sessions.write().await;

        // Remove old entries for this session
        let old_pids: Vec<String> = idx.iter()
            .filter(|(_, sessions)| sessions.contains(session_id))
            .map(|(pid, _)| pid.clone())
            .collect();
        for pid in &old_pids {
            if let Some(set) = idx.get_mut(pid) {
                set.remove(session_id);
                if set.is_empty() { idx.remove(pid); }
            }
        }

        // Add new entries
        for pid in &cached.person_ids {
            idx.entry(pid.clone()).or_default().insert(session_id.to_string());
        }
    }

    /// Returns the recordings directory path.
    pub fn recordings_dir(&self) -> &Path {
        &self.recordings_dir
    }
}
