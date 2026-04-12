//! Transcript service — transcript CRUD, speaker attribution, and the
//! full transcription pipeline kickoff (upload → extract → diarize → merge).
//!
//! The long pipeline helpers (`run_transcription_pipeline`,
//! `process_extraction_output`, `poll_extraction_job`,
//! `resume_pending_extractions`) previously lived in `server::routes`;
//! they have been moved here verbatim so the Tauri command layer can call
//! them without going through HTTP.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{error, info, warn};

use crate::filesdb::FilesDb;
use crate::llm::secrets::SharedSecrets;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::settings::SharedSettings;
use crate::tags::TagsManager;
use crate::understanding::{ExtractionClient, ExtractionOutput, TrackInput};

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;
use super::summary;

#[derive(Debug, Deserialize)]
pub struct AttributionAction {
    pub speaker: String,
    #[serde(default)]
    pub person_id: Option<String>,
    pub action: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AttributionRequest {
    pub attributions: Vec<AttributionAction>,
}

#[derive(Debug, Serialize)]
pub struct TranscribeAccepted {
    pub status: &'static str,
}

pub async fn get_transcript(state: &AppState, id: &str) -> ServiceResult<Value> {
    state
        .files_db
        .get_transcript(id)
        .await
        .ok_or_else(|| ServiceError::NotFound("transcript not found".into()))
}

pub async fn delete_transcript(state: &AppState, id: &str) -> ServiceResult<()> {
    state.files_db.remove_transcript(id).await;

    let session_dir = state.session_manager.session_dir(id);
    for filename in &["transcript.json", "extraction_raw.json"] {
        let path = session_dir.join(filename);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    state
        .session_manager
        .set_processing_state(id, None)
        .await;
    Ok(())
}

pub async fn get_attribution(state: &AppState, id: &str) -> ServiceResult<Value> {
    match state.files_db.get_transcript(id).await {
        Some(data) => Ok(data.get("speaker_embeddings").cloned().unwrap_or(json!({}))),
        None => Err(ServiceError::NotFound("transcript not found".into())),
    }
}

pub async fn update_attribution(
    state: &AppState,
    id: &str,
    body: AttributionRequest,
) -> ServiceResult<()> {
    let mut transcript = state
        .files_db
        .get_transcript(id)
        .await
        .ok_or_else(|| ServiceError::NotFound("transcript not found".into()))?;

    for action in &body.attributions {
        let embedding: Vec<f64> = transcript
            .get("speaker_embeddings")
            .and_then(|embs| embs.get(&action.speaker))
            .and_then(|e| e.get("embedding"))
            .and_then(|e| serde_json::from_value(e.clone()).ok())
            .unwrap_or_default();

        match action.action.as_str() {
            "confirm" => {
                if let Some(pid) = &action.person_id {
                    let _ = state
                        .people_manager
                        .add_embedding(pid, embedding, id, None)
                        .await;
                }
            }
            "correct" => {
                if let Some(pid) = &action.person_id {
                    let _ = state
                        .people_manager
                        .add_embedding(pid, embedding, id, None)
                        .await;
                    let person = state.people_manager.get_person(pid).await;
                    if let Some(embs) = transcript.get_mut("speaker_embeddings") {
                        if let Some(entry) = embs.get_mut(&action.speaker) {
                            entry["person_id"] = json!(pid);
                            entry["person_name"] = json!(person.as_ref().map(|p| &p.name));
                            entry["confidence"] = json!(1.0);
                        }
                    }
                    update_segment_speakers(
                        &mut transcript,
                        &action.speaker,
                        pid,
                        person.as_ref().map(|p| p.name.as_str()),
                    );
                }
            }
            "create" => {
                if let Some(name) = &action.name {
                    match state
                        .people_manager
                        .create_person_from_speaker(name.clone(), embedding, id)
                        .await
                    {
                        Ok(person) => {
                            if let Some(embs) = transcript.get_mut("speaker_embeddings") {
                                if let Some(entry) = embs.get_mut(&action.speaker) {
                                    entry["person_id"] = json!(&person.id);
                                    entry["person_name"] = json!(&person.name);
                                    entry["confidence"] = json!(1.0);
                                }
                            }
                            update_segment_speakers(
                                &mut transcript,
                                &action.speaker,
                                &person.id,
                                Some(&person.name),
                            );
                        }
                        Err(e) => return Err(ServiceError::Internal(e)),
                    }
                }
            }
            "reject" => {
                if let Some(embs) = transcript.get_mut("speaker_embeddings") {
                    if let Some(entry) = embs.get_mut(&action.speaker) {
                        entry["person_id"] = json!(null);
                        entry["person_name"] = json!(null);
                        entry["confidence"] = json!(0.0);
                    }
                }
                update_segment_speakers(&mut transcript, &action.speaker, "", None);
            }
            _ => {}
        }
    }

    state
        .files_db
        .put_transcript(id, transcript)
        .await
        .map_err(ServiceError::Internal)?;

    Ok(())
}

/// Update person_id and person_name in all transcript segments matching the speaker.
fn update_segment_speakers(
    transcript: &mut Value,
    speaker: &str,
    person_id: &str,
    person_name: Option<&str>,
) {
    if let Some(segments) = transcript.get_mut("segments").and_then(|s| s.as_array_mut()) {
        for seg in segments {
            if seg.get("speaker").and_then(|s| s.as_str()) == Some(speaker) {
                if person_id.is_empty() {
                    seg["person_id"] = json!(null);
                    seg["person_name"] = json!(null);
                } else {
                    seg["person_id"] = json!(person_id);
                    seg["person_name"] = json!(person_name);
                }
            }
        }
    }
}

/// Kick off a transcription job. Returns 202 semantics: the pipeline runs
/// in the background via `auto_transcribe`, which the transport layer
/// spawns onto the tokio runtime.
///
/// The closure receives an owned `TranscribePipelineArgs` snapshot so the
/// spawned task has no lifetime ties to the calling handler.
pub async fn transcribe_session(
    state: &AppState,
    id: &str,
    spawn_pipeline: impl FnOnce(TranscribePipelineArgs) + Send + 'static,
) -> ServiceResult<TranscribeAccepted> {
    let settings = state.settings.read().await;
    if !settings.is_extraction_configured() {
        return Err(ServiceError::BadRequest(
            "Audio extraction not configured. Set audio_extraction_url and audio_extraction_api_key in settings.".into(),
        ));
    }
    let extraction_url = settings.audio_extraction_url.clone().unwrap();
    let extraction_key = settings.audio_extraction_api_key.clone().unwrap();
    let file_drop_url = settings.file_drop_url.clone();
    let file_drop_api_key = settings.file_drop_api_key.clone();
    let diarize = settings.diarize;
    let people_recognition = settings.people_recognition;
    let match_threshold = settings.speaker_match_threshold;
    drop(settings);

    let (session_dir, language, source_meta) = state
        .session_manager
        .get_session_extraction_info(id)
        .await
        .map_err(ServiceError::BadRequest)?;

    if let Some(info) = state.session_manager.get_session(id).await {
        if info.processing_state.is_some() {
            return Err(ServiceError::Conflict(
                "Transcription already in progress".into(),
            ));
        }
    }

    state
        .session_manager
        .set_processing_state(id, Some("starting".to_string()))
        .await;

    spawn_pipeline(TranscribePipelineArgs {
        session_id: id.to_string(),
        session_dir,
        language,
        source_meta,
        extraction_url,
        extraction_key,
        file_drop_url,
        file_drop_api_key,
        diarize,
        people_recognition,
        match_threshold,
    });

    Ok(TranscribeAccepted {
        status: "processing",
    })
}

/// Owned snapshot of every input the transcription pipeline needs.
pub struct TranscribePipelineArgs {
    pub session_id: String,
    pub session_dir: std::path::PathBuf,
    pub language: String,
    pub source_meta: Vec<crate::session::session::SourceMetadata>,
    pub extraction_url: String,
    pub extraction_key: String,
    pub file_drop_url: String,
    pub file_drop_api_key: String,
    pub diarize: bool,
    pub people_recognition: bool,
    pub match_threshold: f64,
}

/// Run the full transcription pipeline in a background task.
pub async fn run_transcription_pipeline(
    session_id: &str,
    session_dir: &std::path::Path,
    language: &str,
    source_meta: &[crate::session::session::SourceMetadata],
    extraction_url: &str,
    extraction_key: &str,
    file_drop_url: &str,
    file_drop_api_key: &str,
    diarize: bool,
    people_recognition: bool,
    match_threshold: f64,
    session_manager: &SessionManager,
    people_manager: &PeopleManager,
    files_db: &FilesDb,
) -> Result<u32, String> {
    // Step 1: Upload audio files to file-drop
    session_manager
        .set_processing_state(session_id, Some("uploading".to_string()))
        .await;

    let http = reqwest::Client::new();
    let mut tracks: Vec<TrackInput> = Vec::new();
    let mut drop_urls: Vec<String> = Vec::new();

    for meta in source_meta.iter().filter(|m| !m.filename.is_empty()) {
        let file_path = session_dir.join(&meta.filename);
        if !file_path.exists() {
            warn!("[{}] Audio file not found: {}", session_id, file_path.display());
            continue;
        }

        info!("[{}] Uploading {} to file-drop...", session_id, meta.filename);
        let bytes = std::fs::read(&file_path)
            .map_err(|e| format!("Failed to read {}: {e}", meta.filename))?;
        let file_size = bytes.len();

        let upload_url = format!("{}/upload?filename={}", file_drop_url, meta.filename);
        let resp = http
            .post(&upload_url)
            .header("Authorization", format!("Bearer {}", file_drop_api_key))
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("Failed to upload {} to file-drop: {e}", meta.filename))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "file-drop upload failed for {} ({}): {}",
                meta.filename, status, body
            ));
        }

        let upload_result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse file-drop response: {e}"))?;

        let download_path = upload_result["url"]
            .as_str()
            .ok_or_else(|| "file-drop response missing 'url' field".to_string())?;

        let download_url = format!("{}{}", file_drop_url, download_path);
        drop_urls.push(download_url.clone());

        let source_type = match meta.source_type {
            crate::audio::source::SourceType::Mic => "mic",
            crate::audio::source::SourceType::SystemMix => "system_mix",
            _ => "unknown",
        };

        info!(
            "[{}] Uploaded {} ({} bytes) -> {}",
            session_id, meta.filename, file_size, download_url
        );

        tracks.push(TrackInput {
            audio_url: download_url,
            track_name: meta
                .filename
                .split('.')
                .next()
                .unwrap_or(&meta.filename)
                .to_string(),
            source_type: source_type.to_string(),
            channels: meta.channels,
        });
    }

    if tracks.is_empty() {
        return Err("No audio tracks to transcribe".to_string());
    }

    info!("[{}] Submitting {} tracks to RunPod", session_id, tracks.len());

    session_manager
        .set_processing_state(session_id, Some("extracting".to_string()))
        .await;

    let client = ExtractionClient::new(extraction_url.to_string(), extraction_key.to_string());

    let job_id = client.submit_job(tracks, language, diarize, None, None).await?;

    info!("[{}] RunPod job submitted: {}", session_id, job_id);

    session_manager
        .set_audio_extraction(
            session_id,
            Some(crate::session::session::AudioExtractionJob {
                job_id: job_id.clone(),
                status: "in_progress".to_string(),
                submitted_at: Some(chrono::Utc::now()),
                extraction_url: Some(extraction_url.to_string()),
            }),
        )
        .await;

    let output = poll_extraction_job(&client, &job_id, session_id, session_manager).await?;

    info!(
        "[{}] Extraction complete, {} tracks returned",
        session_id,
        output.tracks.len()
    );

    process_extraction_output(
        session_id,
        session_dir,
        source_meta,
        output,
        people_recognition,
        match_threshold,
        session_manager,
        people_manager,
        files_db,
    )
    .await
}

/// Process extraction output: save raw, merge segments, match speakers, write transcript.
/// Used by both the initial pipeline and the resume-on-restart path.
pub async fn process_extraction_output(
    session_id: &str,
    session_dir: &std::path::Path,
    _source_meta: &[crate::session::session::SourceMetadata],
    output: ExtractionOutput,
    people_recognition: bool,
    match_threshold: f64,
    session_manager: &SessionManager,
    people_manager: &PeopleManager,
    files_db: &FilesDb,
) -> Result<u32, String> {
    let raw_path = session_dir.join("extraction_raw.json");
    let raw_json = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("Failed to serialize raw output: {e}"))?;
    std::fs::write(&raw_path, raw_json)
        .map_err(|e| format!("Failed to write extraction_raw.json: {e}"))?;

    let mut all_segments: Vec<Value> = Vec::new();
    let mut all_embeddings: HashMap<String, Vec<f64>> = HashMap::new();

    for (track_name, track_result) in &output.tracks {
        for seg in &track_result.segments {
            let mut seg_json = serde_json::to_value(seg)
                .map_err(|e| format!("Failed to serialize segment: {e}"))?;
            seg_json["track"] = json!(track_name);
            seg_json["source_type"] = json!(&track_result.source_type);
            all_segments.push(seg_json);
        }
        for (speaker, emb) in &track_result.speaker_embeddings {
            all_embeddings.insert(speaker.clone(), emb.clone());
        }
    }

    all_segments.sort_by(|a, b| {
        let a_start = a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b_start = b.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
        a_start.partial_cmp(&b_start).unwrap_or(std::cmp::Ordering::Equal)
    });

    session_manager
        .set_processing_state(session_id, Some("matching".to_string()))
        .await;

    let mut speaker_info: HashMap<String, Value> = HashMap::new();
    let mut unconfirmed: u32 = 0;

    if people_recognition && !all_embeddings.is_empty() {
        info!(
            "[{}] Matching {} speakers against People library",
            session_id,
            all_embeddings.len()
        );

        let embeddings_f64: HashMap<String, Vec<f64>> = all_embeddings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let attributions = people_manager
            .match_speakers(&embeddings_f64, match_threshold)
            .await;

        for attr in &attributions {
            if attr.person_id.is_none() {
                unconfirmed += 1;
            }
            speaker_info.insert(
                attr.speaker.clone(),
                json!({
                    "embedding": attr.embedding,
                    "person_id": attr.person_id,
                    "person_name": attr.person_name,
                    "confidence": attr.confidence,
                }),
            );

            for seg in &mut all_segments {
                if seg.get("speaker").and_then(|s| s.as_str()) == Some(&attr.speaker) {
                    seg["person_id"] = json!(attr.person_id);
                    seg["person_name"] = json!(attr.person_name);
                    seg["attribution_confidence"] = json!(attr.confidence);
                }
            }
        }

        info!(
            "[{}] Matched speakers: {} confirmed, {} unconfirmed",
            session_id,
            attributions.len() - unconfirmed as usize,
            unconfirmed
        );
    } else {
        for (speaker, emb) in &all_embeddings {
            unconfirmed += 1;
            speaker_info.insert(
                speaker.clone(),
                json!({
                    "embedding": emb,
                    "person_id": null,
                    "person_name": null,
                    "confidence": 0.0,
                }),
            );
        }
    }

    let transcript = json!({
        "language": output.language,
        "model": output.model,
        "segments": all_segments,
        "speaker_embeddings": speaker_info,
    });

    files_db.put_transcript(session_id, transcript).await?;
    session_manager.set_audio_extraction(session_id, None).await;

    info!(
        "[{}] Transcript saved: {} segments, {} speakers",
        session_id,
        all_segments.len(),
        speaker_info.len()
    );

    Ok(unconfirmed)
}

/// Poll an extraction job until completion. No timeout — keeps polling forever.
pub async fn poll_extraction_job(
    client: &ExtractionClient,
    job_id: &str,
    session_id: &str,
    session_manager: &SessionManager,
) -> Result<ExtractionOutput, String> {
    let mut delay = std::time::Duration::from_secs(2);
    let max_delay = std::time::Duration::from_secs(15);

    loop {
        tokio::time::sleep(delay).await;

        match client.poll_status(job_id).await? {
            Some(output) => return Ok(output),
            None => {
                session_manager.emit_transcription_progress(session_id, "extracting");
                delay = (delay * 2).min(max_delay);
            }
        }
    }
}

/// Resume polling for any sessions with in-progress extraction jobs.
/// Called once on daemon startup (and Tauri app startup).
pub async fn resume_pending_extractions(
    session_manager: SessionManager,
    people_manager: PeopleManager,
    files_db: FilesDb,
    settings: SharedSettings,
    llm_secrets: SharedSecrets,
    tags_manager: TagsManager,
) {
    let pending = session_manager.get_pending_extractions().await;
    if pending.is_empty() {
        return;
    }

    info!("Resuming {} pending extraction job(s)...", pending.len());

    for (session_id, job) in pending {
        let extraction_url = match &job.extraction_url {
            Some(url) => url.clone(),
            None => {
                let s = settings.read().await;
                match &s.audio_extraction_url {
                    Some(url) => url.clone(),
                    None => {
                        warn!(
                            "No extraction URL for pending job {} (session {})",
                            job.job_id, session_id
                        );
                        continue;
                    }
                }
            }
        };

        let extraction_key = {
            let s = settings.read().await;
            match &s.audio_extraction_api_key {
                Some(key) => key.clone(),
                None => {
                    warn!(
                        "No extraction API key for pending job {} (session {})",
                        job.job_id, session_id
                    );
                    continue;
                }
            }
        };

        let sm = session_manager.clone();
        let pm = people_manager.clone();
        let fdb = files_db.clone();
        let stg = settings.clone();
        let secrets = llm_secrets.clone();
        let tm = tags_manager.clone();

        info!("Resuming extraction job {} for session {}", job.job_id, session_id);

        tokio::spawn(async move {
            let client = ExtractionClient::new(extraction_url, extraction_key);

            let result = poll_extraction_job(&client, &job.job_id, &session_id, &sm).await;

            match result {
                Ok(output) => {
                    info!(
                        "[{}] Resumed extraction completed, processing results...",
                        session_id
                    );

                    let (session_dir, _language, source_meta) = match sm
                        .get_session_extraction_info(&session_id)
                        .await
                    {
                        Ok(info) => info,
                        Err(e) => {
                            error!(
                                "[{}] Failed to get session info for resumed job: {}",
                                session_id, e
                            );
                            sm.set_audio_extraction(&session_id, None).await;
                            return;
                        }
                    };

                    let stg_r = stg.read().await;
                    let people_recognition = stg_r.people_recognition;
                    let match_threshold = stg_r.speaker_match_threshold;
                    drop(stg_r);

                    let result = process_extraction_output(
                        &session_id,
                        &session_dir,
                        &source_meta,
                        output,
                        people_recognition,
                        match_threshold,
                        &sm,
                        &pm,
                        &fdb,
                    )
                    .await;

                    match result {
                        Ok(unconfirmed) => {
                            sm.set_processing_state(&session_id, None).await;
                            sm.emit_transcription_completed(&session_id, unconfirmed);
                            info!("[{}] Resumed transcription completed", session_id);

                            summary::maybe_auto_summarize(
                                &session_id, &sm, &stg, &secrets, &tm, &pm, &fdb,
                            )
                            .await;
                        }
                        Err(e) => {
                            error!(
                                "[{}] Resumed transcription post-processing failed: {}",
                                session_id, e
                            );
                            sm.set_processing_state(&session_id, None).await;
                            sm.emit_transcription_failed(&session_id, &e);
                            sm.set_audio_extraction(&session_id, None).await;
                        }
                    }
                }
                Err(e) => {
                    error!("[{}] Resumed extraction job failed: {}", session_id, e);
                    sm.set_processing_state(&session_id, None).await;
                    sm.emit_transcription_failed(&session_id, &e);
                    sm.set_audio_extraction(&session_id, None).await;
                }
            }
        });
    }
}
