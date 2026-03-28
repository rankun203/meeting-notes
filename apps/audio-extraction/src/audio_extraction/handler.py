"""RunPod serverless handler for audio extraction.

Accepts multiple audio tracks, runs WhisperX transcription + alignment + diarization
on each, extracts speaker embeddings, and returns per-track results.
"""

import json
import logging
import os
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

import requests
import whisperx

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


def _download_and_decode(track: dict) -> tuple[str, str, str, any, float]:
    """Download and decode audio for a track (CPU-bound). Returns (track_name, source_type, audio_path, audio_array, duration)."""
    audio_url = track["audio_url"]
    track_name = track["track_name"]
    source_type = track["source_type"]

    path = audio_url.split("?")[0].split("#")[0]
    suffix = "." + path.rsplit(".", 1)[-1] if "." in path else ".audio"
    audio_path = download_audio(audio_url, suffix=suffix)

    # Pre-decode audio (CPU-bound ffmpeg work) so it's ready for GPU
    logger.info("Decoding %s", track_name)
    t0 = time.time()
    audio = whisperx.load_audio(audio_path)
    duration = len(audio) / 16000
    logger.info("Decoded %s: %.1fs audio in %.1fs", track_name, duration, time.time() - t0)

    return track_name, source_type, audio_path, audio, duration


def _truncate_for_log(obj, max_str_len=128, max_list_len=5):
    """Recursively truncate long strings and lists for logging."""
    if isinstance(obj, str):
        return obj[:max_str_len] + "..." if len(obj) > max_str_len else obj
    elif isinstance(obj, dict):
        return {k: _truncate_for_log(v, max_str_len, max_list_len) for k, v in obj.items()}
    elif isinstance(obj, (list, tuple)):
        truncated = [_truncate_for_log(v, max_str_len, max_list_len) for v in obj[:max_list_len]]
        if len(obj) > max_list_len:
            truncated.append(f"... +{len(obj) - max_list_len} more")
        return truncated
    return obj


def handler(event: dict) -> dict:
    """RunPod serverless handler."""
    logger.info("Request body:\n%s", json.dumps(_truncate_for_log(event), indent=2, ensure_ascii=False))

    inp = event["input"]
    tracks = inp["tracks"]
    language = inp.get("language", "en")
    diarize = inp.get("diarize", True)
    min_speakers = inp.get("min_speakers")
    max_speakers = inp.get("max_speakers")

    logger.info("Job started: %d tracks, language=%s, diarize=%s", len(tracks), language, diarize)
    if diarize and not os.environ.get("HF_TOKEN"):
        logger.error("diarize=true but HF_TOKEN env var is not set! Diarization will be skipped.")
    job_t0 = time.time()

    pipeline = get_pipeline()
    results = {}
    downloaded_files = []

    try:
        # Download and decode all tracks in parallel (CPU-bound) while
        # overlapping with GPU processing of earlier tracks.
        with ThreadPoolExecutor(max_workers=len(tracks)) as pool:
            futures = [pool.submit(_download_and_decode, t) for t in tracks]

            for idx, future in enumerate(futures):
                track_name, source_type, audio_path, audio, duration = future.result()
                downloaded_files.append(audio_path)

                logger.info("Track %d/%d: \"%s\" (%s, %.1fs)",
                            idx + 1, len(tracks), track_name, source_type, duration)

                # Process on GPU (audio already decoded)
                prefix = "mic" if source_type == "mic" else "sys"
                track_t0 = time.time()
                result = pipeline.process_track(
                    audio_path=audio_path,
                    audio=audio,
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
            except OSError as e:
                logger.warning("Failed to clean up temp file %s: %s", path, e)

    total_elapsed = time.time() - job_t0
    logger.info("Job completed: %d tracks in %.1fs", len(results), total_elapsed)

    response = {
        "tracks": results,
        "language": language,
        "model": pipeline.model_size,
    }

    logger.info("Response body:\n%s", json.dumps(_truncate_for_log(response), indent=2, ensure_ascii=False))

    return response
