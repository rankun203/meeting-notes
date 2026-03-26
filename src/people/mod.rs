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
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub notes: Option<String>,
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
    embeddings: Arc<RwLock<HashMap<String, EmbeddingStore>>>,
}

impl PeopleManager {
    pub fn new(data_dir: &Path) -> Self {
        let people_dir = data_dir.join("people");
        Self {
            people_dir,
            people: Arc::new(RwLock::new(HashMap::new())),
            embeddings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all people and their embeddings from disk.
    pub async fn load_from_disk(&self) {
        if !self.people_dir.exists() {
            return;
        }

        let entries = match std::fs::read_dir(&self.people_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read people dir: {}", e);
                return;
            }
        };

        let mut people = self.people.write().await;
        let mut embeddings = self.embeddings.write().await;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let profile_path = path.join("profile.json");
            if !profile_path.exists() {
                continue;
            }

            let person: Person = match read_json(&profile_path) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Failed to read {}: {}", profile_path.display(), e);
                    continue;
                }
            };

            let emb_path = path.join("embeddings.json");
            if emb_path.exists() {
                match read_json::<EmbeddingStore>(&emb_path) {
                    Ok(store) => {
                        embeddings.insert(person.id.clone(), store);
                    }
                    Err(e) => {
                        warn!("Failed to read {}: {}", emb_path.display(), e);
                    }
                }
            }

            people.insert(person.id.clone(), person);
        }

        info!("Loaded {} people from disk", people.len());
    }

    pub async fn list_people(&self) -> Vec<PersonIndexEntry> {
        let people = self.people.read().await;
        let embeddings = self.embeddings.read().await;
        let mut entries: Vec<PersonIndexEntry> = people
            .values()
            .map(|p| {
                let store = embeddings.get(&p.id);
                PersonIndexEntry {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    embedding_count: store.map_or(0, |s| s.samples.len()),
                    last_seen: store.and_then(|s| s.samples.last().map(|e| e.confirmed_at)),
                }
            })
            .collect();
        entries.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        entries
    }

    pub async fn get_person(&self, id: &str) -> Option<Person> {
        self.people.read().await.get(id).cloned()
    }

    pub async fn create_person(&self, name: String, notes: Option<String>) -> Result<Person, String> {
        let id = generate_person_id();
        let now = Utc::now();
        let person = Person {
            id: id.clone(),
            name,
            notes,
            created_at: now,
            updated_at: now,
        };

        self.write_person(&person)?;
        self.people.write().await.insert(id, person.clone());
        Ok(person)
    }

    pub async fn update_person(
        &self,
        id: &str,
        name: Option<String>,
        notes: Option<Option<String>>,
    ) -> Result<Person, String> {
        let mut people = self.people.write().await;
        let person = people.get_mut(id).ok_or("person not found")?;

        if let Some(name) = name {
            person.name = name;
        }
        if let Some(notes) = notes {
            person.notes = notes;
        }
        person.updated_at = Utc::now();

        self.write_person(person)?;
        Ok(person.clone())
    }

    pub async fn delete_person(&self, id: &str) -> Result<(), String> {
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
    pub async fn add_embedding(
        &self,
        person_id: &str,
        embedding: Vec<f64>,
        session_id: &str,
        duration_secs: Option<f64>,
    ) -> Result<(), String> {
        {
            let people = self.people.read().await;
            if !people.contains_key(person_id) {
                return Err("person not found".to_string());
            }
        }

        let mut stores = self.embeddings.write().await;
        let store = stores.entry(person_id.to_string()).or_insert_with(|| EmbeddingStore {
            centroid: vec![],
            samples: vec![],
        });

        store.samples.push(EmbeddingSample {
            embedding,
            session_id: session_id.to_string(),
            duration_secs,
            confirmed_at: Utc::now(),
        });

        // Recompute centroid as mean of all sample embeddings
        recompute_centroid(store);

        self.write_embeddings(person_id, store)?;
        Ok(())
    }

    /// Match speaker embeddings against all known people.
    /// Returns attributions sorted by speaker name.
    pub async fn match_speakers(
        &self,
        speaker_embeddings: &HashMap<String, Vec<f64>>,
        threshold: f64,
    ) -> Vec<Attribution> {
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
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&json)
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(path, json)
        .map_err(|e| format!("write {}: {e}", path.display()))
}
