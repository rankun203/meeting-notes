//! Filesystem-backed transcripts with small, revision-checked speaker projections.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::storage::{self, Revision};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct FilesDb {
    recordings_dir: PathBuf,
    // No transcript text, word arrays or voice vectors are retained here.
    speakers: Arc<Mutex<HashMap<String, (Revision, SpeakerIndex)>>>,
}

#[derive(Clone, Default, Deserialize)]
struct SpeakerIndex {
    #[serde(default)]
    speaker_embeddings: HashMap<String, Speaker>,
}

#[derive(Clone, Deserialize)]
struct Speaker {
    person_id: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
}

impl FilesDb {
    pub fn new(recordings_dir: PathBuf) -> Self {
        Self {
            recordings_dir,
            speakers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn read_transcript(&self, session_id: &str) -> Result<Value, String> {
        let path = self.recordings_dir.join(session_id).join("transcript.json");
        storage::blocking(move || storage::read_json(&path)).await
    }

    pub async fn get_transcript(&self, session_id: &str) -> Option<Value> {
        self.read_transcript(session_id).await.ok()
    }

    pub async fn has_transcript(&self, session_id: &str) -> bool {
        self.recordings_dir
            .join(session_id)
            .join("transcript.json")
            .is_file()
    }

    async fn speaker_index(&self, session_id: &str) -> Result<SpeakerIndex, String> {
        let path = self.recordings_dir.join(session_id).join("transcript.json");
        // Serialize projection builds so concurrent person queries don't scan the
        // same library repeatedly. Parsing runs on a bounded blocking worker.
        let mut indexes = self.speakers.lock().await;
        let Some(before) = storage::revision(&path) else {
            indexes.remove(session_id);
            return Ok(SpeakerIndex::default());
        };
        if let Some((rev, index)) = indexes.get(session_id) {
            if *rev == before {
                return Ok(index.clone());
            }
        }
        let read_path = path.clone();
        let index: SpeakerIndex = storage::blocking(move || storage::read_json(&read_path)).await?;
        if storage::revision(&path).as_ref() != Some(&before) {
            indexes.remove(session_id);
            return Err("transcript changed while indexing; retry the request".into());
        }
        indexes.insert(session_id.to_string(), (before, index.clone()));
        Ok(index)
    }

    pub async fn unconfirmed_speakers(&self, session_id: &str) -> u32 {
        self.speaker_index(session_id)
            .await
            .map(|index| {
                index
                    .speaker_embeddings
                    .values()
                    .filter(|s| s.person_id.is_none())
                    .count() as u32
            })
            .unwrap_or(0)
    }

    pub async fn matched_speakers(
        &self,
        session_id: &str,
        person_id: &str,
    ) -> Result<Vec<Value>, String> {
        Ok(self.speaker_index(session_id).await?.speaker_embeddings.into_iter()
            .filter(|(_, s)| s.person_id.as_deref() == Some(person_id))
            .map(|(speaker, s)| json!({"speaker": speaker, "confidence": s.confidence.unwrap_or(0.0)}))
            .collect())
    }

    pub async fn person_session_ids(&self, person_id: &str) -> Result<Vec<String>, String> {
        let root = self.recordings_dir.clone();
        let dirs = storage::blocking(move || storage::session_dirs(&root)).await;
        let mut ids = Vec::new();
        let mut present = std::collections::HashSet::new();
        for dir in dirs {
            let Some(id) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            present.insert(id.to_string());
            if self
                .speaker_index(id)
                .await?
                .speaker_embeddings
                .values()
                .any(|s| s.person_id.as_deref() == Some(person_id))
            {
                ids.push(id.to_string());
            }
        }
        self.speakers
            .lock()
            .await
            .retain(|id, _| present.contains(id));
        ids.sort();
        Ok(ids)
    }

    pub async fn put_transcript(&self, session_id: &str, data: Value) -> Result<(), String> {
        let dir = self.recordings_dir.join(session_id);
        storage::blocking(move || {
            let path = dir.join("transcript.json");
            let _lock = storage::write_lock(&path);
            storage::write_json(&path, &data)?;
            crate::markdown::write_transcript_md(&dir, &data);
            Ok::<_, String>(())
        })
        .await?;
        self.remove_transcript(session_id).await;
        Ok(())
    }

    pub async fn update_transcript(
        &self,
        session_id: &str,
        base: Value,
        data: Value,
    ) -> Result<(), String> {
        let dir = self.recordings_dir.join(session_id);
        storage::blocking(move || {
            let merged = storage::update_json(&dir.join("transcript.json"), &base, &data)?;
            crate::markdown::write_transcript_md(&dir, &merged);
            Ok::<_, String>(())
        })
        .await?;
        self.remove_transcript(session_id).await;
        Ok(())
    }

    pub async fn remove_transcript(&self, session_id: &str) {
        self.speakers.lock().await.remove(session_id);
    }

    pub fn recordings_dir(&self) -> &Path {
        &self.recordings_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lazy_index_tracks_external_edits_additions_deletions_and_corruption() {
        let root = std::env::temp_dir().join(format!("mn-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("one")).unwrap();
        let path = root.join("one/transcript.json");
        storage::write_json(&path, &json!({"segments":[{"text":"old"}], "speaker_embeddings":{"a":{"person_id":"p1", "embedding":[1,2,3]}}})).unwrap();
        let db = FilesDb::new(root.clone());
        assert!(db.speakers.lock().await.is_empty());
        assert_eq!(db.person_session_ids("p1").await.unwrap(), vec!["one"]);
        storage::write_json(&path, &json!({"segments":[{"text":"new"}], "speaker_embeddings":{"a":{"person_id":"p2"}, "b":{}}})).unwrap();
        assert_eq!(
            db.get_transcript("one").await.unwrap()["segments"][0]["text"],
            "new"
        );
        assert!(db.person_session_ids("p1").await.unwrap().is_empty());
        assert_eq!(db.unconfirmed_speakers("one").await, 1);
        std::fs::create_dir(root.join("two")).unwrap();
        storage::write_json(
            &root.join("two/transcript.json"),
            &json!({"speaker_embeddings":{"c":{"person_id":"p2"}}}),
        )
        .unwrap();
        assert_eq!(
            db.person_session_ids("p2").await.unwrap(),
            vec!["one", "two"]
        );
        std::fs::write(&path, "{partial").unwrap();
        assert!(db.person_session_ids("p2").await.is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(db.get_transcript("one").await.is_none());
        assert_eq!(db.person_session_ids("p2").await.unwrap(), vec!["two"]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
