//! People library — manages known speakers and their voice embeddings.
//!
//! Storage layout:
//!   {data-dir}/people/people.json         — index of all people
//!   {data-dir}/people/{id}/profile.json   — name, notes, timestamps
//!   {data-dir}/people/{id}/embeddings.json — centroid + sample embeddings

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub starred: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonIndex {
    pub people: Vec<PersonIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonIndexEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub starred: bool,
    pub embedding_count: usize,
    #[serde(default)]
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStore {
    /// Average of all sample embeddings — used for fast matching.
    pub centroid: Vec<f64>,
    pub samples: Vec<EmbeddingSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSample {
    pub embedding: Vec<f64>,
    pub session_id: String,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    pub confirmed_at: DateTime<Utc>,
}

// Retain only recognition centroids and list statistics, never sample vectors.
#[derive(Debug, Clone, Default, Deserialize)]
struct EmbeddingSummary {
    #[serde(default)]
    centroid: Vec<f64>,
    #[serde(default, deserialize_with = "sample_stats")]
    samples: SampleStats,
}
#[derive(Debug, Clone, Default)]
struct SampleStats { count: usize, last_seen: Option<DateTime<Utc>> }
fn sample_stats<'de, D: serde::Deserializer<'de>>(d: D) -> Result<SampleStats, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = SampleStats;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("embedding samples") }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            #[derive(Deserialize)] struct Sample { confirmed_at: DateTime<Utc> }
            let mut stats = SampleStats::default();
            while let Some(sample) = seq.next_element::<Sample>()? {
                stats.count += 1;
                stats.last_seen = Some(sample.confirmed_at);
            }
            Ok(stats)
        }
    }
    d.deserialize_seq(Visitor)
}

/// Result of matching a speaker embedding against the people library.
#[derive(Debug, Clone, Serialize)]
pub struct Attribution {
    pub speaker: String,
    pub person_id: Option<String>,
    pub person_name: Option<String>,
    pub confidence: f64,
    pub embedding: Vec<f64>,
}

#[derive(Clone)]
pub struct PeopleManager {
    people_dir: PathBuf,
    people: Arc<RwLock<HashMap<String, Person>>>,
    embeddings: Arc<RwLock<HashMap<String, EmbeddingSummary>>>,
    // Serializes reconciliation against mutations so an old scan cannot replace
    // a just-written projection.
    revisions: Arc<tokio::sync::Mutex<HashMap<PathBuf, crate::storage::Revision>>>,
    #[cfg(test)]
    content_reads: Arc<std::sync::atomic::AtomicUsize>,
}

impl PeopleManager {
    pub fn new(data_dir: &Path) -> Self {
        let people_dir = data_dir.join("people");
        Self {
            people_dir,
            people: Arc::new(RwLock::new(HashMap::new())),
            embeddings: Arc::new(RwLock::new(HashMap::new())),
            revisions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            #[cfg(test)]
            content_reads: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Reconcile compact projections; unchanged files require only metadata checks.
    pub async fn load_from_disk(&self) {
        let mut revisions = self.revisions.lock().await;
        let root = self.people_dir.clone();
        let mut next_revisions = revisions.clone();
        let mut people = self.people.read().await.clone();
        let mut embeddings = self.embeddings.read().await.clone();
        let scanned = crate::storage::blocking(move || {
            let entries = match std::fs::read_dir(&root) {
                Ok(entries) => entries.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(e) => return Err(e.to_string()),
            };
            let mut present = std::collections::HashSet::new();
            let mut reads = 0;
            for entry in entries {
                if !entry.file_type().map_err(|e| e.to_string())?.is_dir() { continue; }
                let id = entry.file_name().to_string_lossy().to_string();
                let profile = entry.path().join("profile.json");
                if !profile.exists() { continue; }
                present.insert(id.clone());
                refresh_projection(&profile, &id, &mut people, &mut next_revisions, &mut reads, || {
                    let mut person: Person = read_json(&profile)?;
                    if person.id != id { return Err("person ID does not match directory".into()); }
                    person.notes = None;
                    Ok(person)
                });
                let path = entry.path().join("embeddings.json");
                refresh_projection(&path, &id, &mut embeddings, &mut next_revisions, &mut reads,
                    || read_json::<EmbeddingSummary>(&path));
            }
            people.retain(|id, _| present.contains(id));
            embeddings.retain(|id, _| present.contains(id));
            next_revisions.retain(|path, _| path.parent().and_then(|p| p.file_name())
                .is_some_and(|id| present.contains(id.to_string_lossy().as_ref())));
            Ok((people, embeddings, next_revisions, reads))
        }).await;
        match scanned {
            Ok((people, embeddings, next_revisions, reads)) => {
                if reads > 0 { debug!("Refreshed people projections from {} changed files", reads); }
                #[cfg(test)]
                self.content_reads.fetch_add(reads, std::sync::atomic::Ordering::Relaxed);
                *self.people.write().await = people;
                *self.embeddings.write().await = embeddings;
                *revisions = next_revisions;
            }
            Err(error) => warn!("Failed to reconcile people directory: {}", error),
        }
    }

    pub async fn list_people(&self) -> Vec<PersonIndexEntry> {
        self.load_from_disk().await;
        let people = self.people.read().await;
        let embeddings = self.embeddings.read().await;
        let mut entries: Vec<PersonIndexEntry> = people
            .values()
            .map(|p| {
                let store = embeddings.get(&p.id);
                PersonIndexEntry {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    starred: p.starred,
                    embedding_count: store.map_or(0, |s| s.samples.count),
                    last_seen: store.and_then(|s| s.samples.last_seen),
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            b.starred.cmp(&a.starred)
                .then_with(|| if a.starred { a.name.to_lowercase().cmp(&b.name.to_lowercase()) } else { b.last_seen.cmp(&a.last_seen) })
        });
        entries
    }

    pub async fn get_person(&self, id: &str) -> Option<Person> {
        read_json(&self.people_dir.join(id).join("profile.json")).ok()
    }

    pub async fn create_person(&self, name: String, notes: Option<String>) -> Result<Person, String> {
        let _reconcile = self.revisions.lock().await;
        let id = generate_person_id();
        let now = Utc::now();
        let person = Person {
            id: id.clone(),
            name,
            notes,
            starred: false,
            created_at: now,
            updated_at: now,
        };

        self.write_person(&person)?;
        let mut catalog = person.clone();
        catalog.notes = None;
        self.people.write().await.insert(id, catalog);
        Ok(person)
    }

    pub async fn update_person(
        &self,
        id: &str,
        name: Option<String>,
        notes: Option<Option<String>>,
        starred: Option<bool>,
    ) -> Result<Person, String> {
        let _reconcile = self.revisions.lock().await;
        let path = self.people_dir.join(id).join("profile.json");
        let original: serde_json::Value = crate::storage::read_json(&path)?;
        let mut updated = original.clone();
        if let Some(name) = name { updated["name"] = name.into(); }
        if let Some(notes) = notes { updated["notes"] = serde_json::json!(notes); }
        if let Some(starred) = starred { updated["starred"] = starred.into(); }
        updated["updated_at"] = serde_json::json!(Utc::now());
        let merged = crate::storage::update_json(&path, &original, &updated)?;
        let person: Person = serde_json::from_value(merged).map_err(|e| e.to_string())?;
        let mut catalog = person.clone();
        catalog.notes = None;
        self.people.write().await.insert(id.to_string(), catalog);
        Ok(person)
    }

    pub async fn delete_person(&self, id: &str) -> Result<(), String> {
        let _reconcile = self.revisions.lock().await;
        self.people.write().await.remove(id)
            .ok_or_else(|| "person not found".to_string())?;
        self.embeddings.write().await.remove(id);

        let dir = self.people_dir.join(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("failed to delete person dir: {e}"))?;
        }
        Ok(())
    }

    /// Add a confirmed embedding to a person's store and recompute centroid.
    ///
    /// An empty embedding is accepted and ignored. Sessions imported from an
    /// external transcript (e.g. Teams) carry speaker names but no voice
    /// samples, so confirming one of their speakers has nothing to contribute.
    /// Storing the empty sample anyway would pull the centroid toward zero —
    /// it adds nothing to the sum but still counts in the divisor — and would
    /// leave a brand-new person with a zero-dimension centroid that can never
    /// match again.
    pub async fn add_embedding(
        &self,
        person_id: &str,
        embedding: Vec<f64>,
        session_id: &str,
        duration_secs: Option<f64>,
    ) -> Result<(), String> {
        let _reconcile = self.revisions.lock().await;
        {
            let people = self.people.read().await;
            if !people.contains_key(person_id) {
                return Err("person not found".to_string());
            }
        }

        if embedding.is_empty() {
            return Ok(());
        }

        let path = self.people_dir.join(person_id).join("embeddings.json");
        let _lock = crate::storage::write_lock(&path);
        let mut store = if path.exists() { read_json::<EmbeddingStore>(&path)? }
            else { EmbeddingStore { centroid: vec![], samples: vec![] } };
        store.samples.push(EmbeddingSample {
            embedding, session_id: session_id.to_string(), duration_secs, confirmed_at: Utc::now(),
        });
        recompute_centroid(&mut store);
        self.write_embeddings(person_id, &store)?;
        drop(_lock);
        self.embeddings.write().await.insert(person_id.to_string(), EmbeddingSummary {
            centroid: store.centroid,
            samples: SampleStats { count: store.samples.len(), last_seen: store.samples.last().map(|s| s.confirmed_at) },
        });

        Ok(())
    }

    /// Match speaker embeddings against all known people.
    /// Returns attributions sorted by speaker name.
    pub async fn match_speakers(
        &self,
        speaker_embeddings: &HashMap<String, Vec<f64>>,
        threshold: f64,
    ) -> Vec<Attribution> {
        self.load_from_disk().await;
        let people = self.people.read().await;
        let stores = self.embeddings.read().await;

        // Build centroid list
        let known: Vec<(&str, &str, &[f64])> = people
            .values()
            .filter_map(|p| {
                stores.get(&p.id).and_then(|s| {
                    if s.centroid.is_empty() {
                        None
                    } else {
                        Some((p.id.as_str(), p.name.as_str(), s.centroid.as_slice()))
                    }
                })
            })
            .collect();

        let mut attributions: Vec<Attribution> = Vec::new();
        let mut claimed_people: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Compute all (speaker, person) similarities
        let mut scores: Vec<(String, String, String, f64, Vec<f64>)> = Vec::new();
        for (speaker, emb) in speaker_embeddings {
            for &(pid, pname, centroid) in &known {
                let sim = cosine_similarity(emb, centroid);
                scores.push((
                    speaker.clone(),
                    pid.to_string(),
                    pname.to_string(),
                    sim,
                    emb.clone(),
                ));
            }
        }

        // Greedy matching: highest similarity first
        scores.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

        let mut matched_speakers: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (speaker, pid, pname, sim, emb) in &scores {
            if matched_speakers.contains(speaker) || claimed_people.contains(pid) {
                continue;
            }
            if *sim >= threshold {
                attributions.push(Attribution {
                    speaker: speaker.clone(),
                    person_id: Some(pid.clone()),
                    person_name: Some(pname.clone()),
                    confidence: *sim,
                    embedding: emb.clone(),
                });
                matched_speakers.insert(speaker.clone());
                claimed_people.insert(pid.clone());
            }
        }

        // Add unmatched speakers
        for (speaker, emb) in speaker_embeddings {
            if !matched_speakers.contains(speaker) {
                attributions.push(Attribution {
                    speaker: speaker.clone(),
                    person_id: None,
                    person_name: None,
                    confidence: 0.0,
                    embedding: emb.clone(),
                });
            }
        }

        attributions.sort_by(|a, b| a.speaker.cmp(&b.speaker));
        attributions
    }

    /// Create a new person from an unknown speaker's embedding.
    pub async fn create_person_from_speaker(
        &self,
        name: String,
        embedding: Vec<f64>,
        session_id: &str,
    ) -> Result<Person, String> {
        let person = self.create_person(name, None).await?;
        self.add_embedding(&person.id, embedding, session_id, None).await?;
        Ok(person)
    }

    fn write_person(&self, person: &Person) -> Result<(), String> {
        let dir = self.people_dir.join(&person.id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create person dir: {e}"))?;
        write_json(&dir.join("profile.json"), person)
    }

    fn write_embeddings(&self, person_id: &str, store: &EmbeddingStore) -> Result<(), String> {
        let dir = self.people_dir.join(person_id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create person dir: {e}"))?;
        write_json(&dir.join("embeddings.json"), store)
    }

    /// Export all people as entries for index generation.
    pub async fn person_entries(&self) -> Vec<crate::markdown::PersonEntry> {
        let people = self.people.read().await;
        people.values().map(|p| crate::markdown::PersonEntry {
            id: p.id.clone(),
            name: p.name.clone(),
            starred: p.starred,
            created_at: p.created_at,
        }).collect()
    }

    /// Returns the people directory path.
    pub fn people_dir(&self) -> &Path {
        &self.people_dir
    }
}

// Parse only a changed file. Never stamp a result from a concurrent editor save
// as current. Failed parses are retried on the next reconciliation.
fn refresh_projection<T>(
    path: &Path, id: &str, values: &mut HashMap<String, T>,
    revisions: &mut HashMap<PathBuf, crate::storage::Revision>, reads: &mut usize,
    read: impl FnOnce() -> Result<T, String>,
) {
    let Some(before) = crate::storage::revision(path) else {
        values.remove(id);
        revisions.remove(path);
        return;
    };
    if revisions.get(path) == Some(&before) { return; }
    *reads += 1;
    let result = read();
    if crate::storage::revision(path).as_ref() != Some(&before) { return; }
    match result {
        Ok(value) => {
            values.insert(id.to_owned(), value);
            revisions.insert(path.to_path_buf(), before);
        }
        Err(error) => {
            values.remove(id);
            revisions.remove(path);
            warn!("Failed to read {}: {}", path.display(), error);
        }
    }
}

fn recompute_centroid(store: &mut EmbeddingStore) {
    if store.samples.is_empty() {
        store.centroid = vec![];
        return;
    }
    let dim = store.samples[0].embedding.len();
    let n = store.samples.len() as f64;
    let mut centroid = vec![0.0f64; dim];
    for sample in &store.samples {
        for (i, &v) in sample.embedding.iter().enumerate() {
            if i < dim {
                centroid[i] += v;
            }
        }
    }
    for v in &mut centroid {
        *v /= n;
    }
    store.centroid = centroid;
}

pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

fn generate_person_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    format!("p_{}", format_base36(nanos))
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

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    crate::storage::read_json(path)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    crate::storage::write_json(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn repeated_queries_read_only_changed_people_files() {
        let root = std::env::temp_dir().join(format!("mn-people-{}", uuid::Uuid::new_v4()));
        let manager = PeopleManager::new(&root);
        let first = manager.create_person("First".into(), Some("private notes".into())).await.unwrap();
        let second = manager.create_person("Second".into(), None).await.unwrap();
        manager.add_embedding(&first.id, vec![1.0, 0.0], "session", None).await.unwrap();
        manager.add_embedding(&second.id, vec![0.0, 1.0], "session", None).await.unwrap();
        assert_eq!(manager.list_people().await.len(), 2);
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 4);
        let speakers = HashMap::from([("speaker".into(), vec![1.0, 0.0])]);
        for _ in 0..20 {
            assert_eq!(manager.list_people().await.len(), 2);
            assert_eq!(manager.match_speakers(&speakers, 0.9).await[0].person_id.as_deref(), Some(first.id.as_str()));
        }
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 4, "warm list/matching must not reread content");
        assert!(manager.people.read().await.values().all(|person| person.notes.is_none()));

        // An editor's atomic replacement must be read exactly once, even when
        // several API requests and the background reconciler arrive together.
        let profile_path = manager.people_dir.join(&first.id).join("profile.json");
        let mut external = first.clone();
        external.name = "Other".into(); // same length as First
        write_json(&profile_path, &external).unwrap();
        let mut requests = Vec::new();
        for _ in 0..12 {
            let manager = manager.clone();
            requests.push(tokio::spawn(async move { manager.list_people().await }));
        }
        for request in requests {
            assert!(request.await.unwrap().iter().any(|p| p.name == "Other"));
        }
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 5);

        let embedding_path = manager.people_dir.join(&first.id).join("embeddings.json");
        let mut store: EmbeddingStore = read_json(&embedding_path).unwrap();
        store.centroid = vec![-1.0, 0.0];
        store.samples.push(store.samples[0].clone());
        write_json(&embedding_path, &store).unwrap();
        assert_eq!(manager.list_people().await.iter().find(|p| p.id == first.id).unwrap().embedding_count, 2);
        assert!(manager.match_speakers(&speakers, 0.9).await[0].person_id.is_none());
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 6);

        // Imports and deletes update the catalog without rereading other people.
        let imported_dir = manager.people_dir.join("imported");
        std::fs::create_dir_all(&imported_dir).unwrap();
        external.id = "imported".into();
        write_json(&imported_dir.join("profile.json"), &external).unwrap();
        assert_eq!(manager.list_people().await.len(), 3);
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 7);
        std::fs::remove_dir_all(imported_dir).unwrap();
        std::fs::remove_file(embedding_path).unwrap();
        assert_eq!(manager.list_people().await.len(), 2);
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 7);
        assert_eq!(manager.list_people().await.iter().find(|p| p.id == first.id).unwrap().embedding_count, 0);

        std::fs::write(&profile_path, "{partial").unwrap();
        assert_eq!(manager.list_people().await.len(), 1);
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 8);
        write_json(&profile_path, &first).unwrap();
        assert_eq!(manager.list_people().await.len(), 2);
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 9);
        manager.update_person(&first.id, Some("Daemon edit".into()), None, None).await.unwrap();
        assert!(manager.list_people().await.iter().any(|p| p.name == "Daemon edit"));
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 10);
        manager.delete_person(&first.id).await.unwrap();
        assert_eq!(manager.list_people().await.len(), 1);
        assert_eq!(manager.content_reads.load(Ordering::Relaxed), 10);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(manager.list_people().await.is_empty());
    }
}
