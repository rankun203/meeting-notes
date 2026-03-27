"""RunPod serverless handler for audio extraction.

Accepts multiple audio tracks, runs WhisperX transcription + alignment + diarization
on each, extracts speaker embeddings, and returns per-track results.
"""

import logging
import os
import tempfile
import time

import requests

from audio_extraction.pipeline import TranscriptionPipeline

logger = logging.getLogger(__name__)

# Initialize pipeline once (models stay loaded across requests on the same worker)
_pipeline: TranscriptionPipeline | None = None


def get_pipeline() -> TranscriptionPipeline:
    global _pipeline
    if _pipeline is None:
        logger.info("Initializing pipeline (first request on this worker)")
        t0 = time.time()
        _pipeline = TranscriptionPipeline(
            model_size=os.environ.get("WHISPER_MODEL_SIZE", "large-v2"),
            device=os.environ.get("WHISPER_DEVICE", "cuda"),
            compute_type=os.environ.get("WHISPER_COMPUTE_TYPE", "float16"),
            batch_size=int(os.environ.get("WHISPER_BATCH_SIZE", "16")),
            hf_token=os.environ.get("HF_TOKEN"),
        )
        logger.info("Pipeline initialized in %.1fs", time.time() - t0)
    return _pipeline


def download_audio(url: str, suffix: str = ".audio") -> str:
    """Download audio from URL to a temporary file. Returns the file path."""
    logger.info("Downloading %s", url)
    t0 = time.time()
    resp = requests.get(url, timeout=300, stream=True)
    resp.raise_for_status()
    tmp = tempfile.NamedTemporaryFile(delete=False, suffix=suffix)
    size = 0
    for chunk in resp.iter_content(chunk_size=8192):
        tmp.write(chunk)
        size += len(chunk)
    tmp.close()
    elapsed = time.time() - t0
    logger.info("Downloaded %.1f MB in %.1fs (%.1f MB/s) -> %s",
                size / 1e6, elapsed, size / 1e6 / max(elapsed, 0.001), tmp.name)
    return tmp.name


def handler(event: dict) -> dict:
    """RunPod serverless handler."""
    inp = event["input"]
    tracks = inp["tracks"]
    language = inp.get("language", "en")
    diarize = inp.get("diarize", True)
    min_speakers = inp.get("min_speakers")
    max_speakers = inp.get("max_speakers")

    logger.info("Job started: %d tracks, language=%s, diarize=%s", len(tracks), language, diarize)
    job_t0 = time.time()

    pipeline = get_pipeline()
    results = {}
    downloaded_files = []

    try:
        for idx, track in enumerate(tracks):
            audio_url = track["audio_url"]
            track_name = track["track_name"]
            source_type = track["source_type"]

            logger.info("Track %d/%d: \"%s\" (%s)", idx + 1, len(tracks), track_name, source_type)

            # Download
            path = audio_url.split("?")[0].split("#")[0]
            suffix = "." + path.rsplit(".", 1)[-1] if "." in path else ".audio"
            audio_path = download_audio(audio_url, suffix=suffix)
            downloaded_files.append(audio_path)

            # Process
            prefix = "mic" if source_type == "mic" else "sys"
            track_t0 = time.time()
            result = pipeline.process_track(
                audio_path=audio_path,
                language=language,
                diarize=diarize,
                speaker_prefix=prefix,
                min_speakers=min_speakers,
                max_speakers=max_speakers,
            )
            track_elapsed = time.time() - track_t0

            segs = len(result.get("segments", []))
            embs = len(result.get("speaker_embeddings", {}))
            dur = result.get("duration_secs", 0)
            logger.info("Track \"%s\" done: %d segments, %d speakers, %.1fs audio in %.1fs (%.1fx realtime)",
                        track_name, segs, embs, dur, track_elapsed, dur / max(track_elapsed, 0.001))

            results[track_name] = {
                "source_type": source_type,
                **result,
            }
    finally:
        for path in downloaded_files:
            try:
                os.unlink(path)
            except OSError:
                pass

    total_elapsed = time.time() - job_t0
    logger.info("Job completed: %d tracks in %.1fs", len(results), total_elapsed)

    return {
        "tracks": results,
        "language": language,
        "model": pipeline.model_size,
    }
