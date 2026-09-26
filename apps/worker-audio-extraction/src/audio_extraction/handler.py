"""RunPod serverless handler for audio extraction.

Accepts multiple audio tracks, runs WhisperX transcription + alignment + diarization
on each, extracts speaker embeddings, and returns per-track results.
"""

from __future__ import annotations

import logging
import os
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

from audio_extraction.config import pipeline_options
from audio_extraction.capabilities import capabilities
from audio_extraction.transfer import download_audio, persist_output

logger = logging.getLogger(__name__)

# Chinese variant converters (lazy-initialized)
_zh_converters: dict[str, OpenCC] = {}

def _get_zh_converter(language: str) -> OpenCC | None:
    """Return an OpenCC converter for zh-cn / zh-tw, or None."""
    # tw2sp = Traditional (TW phrases) → Simplified, s2twp = Simplified → Traditional (TW phrases)
    from opencc import OpenCC

    config = {"zh-cn": "tw2sp", "zh-tw": "s2twp"}.get(language)
    if config is None:
        return None
    if language not in _zh_converters:
        _zh_converters[language] = OpenCC(config)
    return _zh_converters[language]


def _convert_zh_segments(segments: list[dict], converter: OpenCC) -> None:
    """Convert segment text in-place using an OpenCC converter."""
    for seg in segments:
        if "text" in seg:
            seg["text"] = converter.convert(seg["text"])
        for word in seg.get("words", []):
            if "word" in word:
                word["word"] = converter.convert(word["word"])

# Initialize pipeline once (models stay loaded across requests on the same worker)
_pipeline: TranscriptionPipeline | None = None


def get_pipeline() -> TranscriptionPipeline:
    global _pipeline
    if _pipeline is None:
        logger.info("Initializing pipeline (first request on this worker)")
        t0 = time.time()
        from audio_extraction.pipeline import TranscriptionPipeline
        _pipeline = TranscriptionPipeline(**pipeline_options())
        logger.info("Pipeline initialized in %.1fs", time.time() - t0)
    return _pipeline


def right_trim_silence(audio: np.ndarray, sr: int = 16000, threshold: float = 0.001, tail: float = 0.5) -> np.ndarray:
    """Remove trailing silence from audio array. Keep `tail` seconds after last non-silent sample."""
    import numpy as np

    indices = np.nonzero(np.abs(audio) > threshold)[0]
    if len(indices) == 0:
        return audio
    end = min(indices[-1] + int(tail * sr), len(audio))
    return audio[:end]


def _download_and_decode(track: dict, directory: str) -> tuple[str, str, str, any, float]:
    """Download and decode audio for a track (CPU-bound). Returns (track_name, source_type, audio_path, audio_array, duration)."""
    audio_url = track["audio_url"]
    track_name = track["track_name"]
    source_type = track["source_type"]

    path = audio_url.split("?")[0].split("#")[0]
    if "." not in path.rsplit("/", 1)[-1]:
        raise ValueError(f"Cannot determine file extension from URL: {audio_url}")
    suffix = "." + path.rsplit(".", 1)[-1]
    audio_path = download_audio(audio_url, suffix=suffix, directory=directory)

    # Pre-decode audio (CPU-bound ffmpeg work) so it's ready for GPU
    logger.info("Decoding %s", track_name)
    t0 = time.time()
    import whisperx

    audio = whisperx.load_audio(audio_path)
    raw_duration = len(audio) / 16000
    audio = right_trim_silence(audio)
    duration = len(audio) / 16000
    if duration < raw_duration:
        logger.info("Decoded %s: %.1fs audio (trimmed from %.1fs) in %.1fs", track_name, duration, raw_duration, time.time() - t0)
    else:
        logger.info("Decoded %s: %.1fs audio in %.1fs", track_name, duration, time.time() - t0)

    return track_name, source_type, audio_path, audio, duration


def handler(event: dict) -> dict:
    """RunPod serverless handler."""
    logger.info("Received extraction request")

    inp = event["input"]
    if inp.get("operation") == "capabilities":
        return capabilities()
    if inp.get("operation") not in (None, "transcribe"):
        raise ValueError("Unsupported worker operation")
    tracks = inp["tracks"]
    language = inp.get("language", "en")
    # WhisperX only understands base language codes (e.g. "zh"), not
    # regional variants like "zh-cn" / "zh-tw".  Strip the variant so
    # whisperx gets a code it recognises; the original language value is
    # preserved in the session metadata for downstream use (e.g. summaries).
    whisperx_language = None if language == "auto" else language.split("-")[0]
    diarize = inp.get("diarize", True)
    min_speakers = inp.get("min_speakers")
    max_speakers = inp.get("max_speakers")

    logger.info("Job started: %d tracks, language=%s (whisperx=%s), diarize=%s",
                len(tracks), language, whisperx_language, diarize)
    if diarize and not os.environ.get("HF_TOKEN"):
        logger.error("diarize=true but HF_TOKEN env var is not set! Diarization will be skipped.")
    job_t0 = time.time()

    pipeline = get_pipeline()
    results = {}
    track_timings: list[dict] = []

    with tempfile.TemporaryDirectory(prefix="audio-extraction-") as directory:
        # Download and decode all tracks in parallel (CPU-bound) while
        # overlapping with GPU processing of earlier tracks.
        with ThreadPoolExecutor(max_workers=min(4, max(1, len(tracks)))) as pool:
            futures = [pool.submit(_download_and_decode, t, directory) for t in tracks]

            for idx, future in enumerate(futures):
                track_name, source_type, audio_path, audio, duration = future.result()

                logger.info("Track %d/%d: \"%s\" (%s, %.1fs)",
                            idx + 1, len(tracks), track_name, source_type, duration)

                # Process on GPU (audio already decoded)
                prefix = "mic" if source_type == "mic" else "sys"
                track_t0 = time.time()
                result = pipeline.process_track(
                    audio_path=audio_path,
                    audio=audio,
                    language=whisperx_language,
                    diarize=diarize,
                    speaker_prefix=prefix,
                    min_speakers=min_speakers,
                    max_speakers=max_speakers,
                )
                track_elapsed = time.time() - track_t0

                # Convert Chinese characters to the requested variant
                zh_converter = _get_zh_converter(language)
                if zh_converter is not None:
                    _convert_zh_segments(result.get("segments", []), zh_converter)
                    logger.info("OpenCC %s conversion applied", language)

                segs = len(result.get("segments", []))
                embs = len(result.get("speaker_embeddings", {}))
                dur = result.get("duration_secs", 0)
                logger.info("Track \"%s\" done: %d segments, %d speakers, %.1fs audio in %.1fs (%.1fx realtime)",
                            track_name, segs, embs, dur, track_elapsed, dur / max(track_elapsed, 0.001))

                track_timings.append({
                    "track": track_name,
                    "duration": dur,
                    "process": track_elapsed,
                    **result.pop("step_timings", {}),
                })

                results[track_name] = {
                    "source_type": source_type,
                    **result,
                }
    total_elapsed = time.time() - job_t0

    # Single summary log with per-track step timings and total
    timing_lines = []
    for tt in track_timings:
        steps = " | ".join(
            f"{k}={tt[k]:.1f}s"
            for k in ["transcribe", "align", "diarize", "assign_speakers"]
            if k in tt
        )
        timing_lines.append(
            f'  "{tt["track"]}": {tt["duration"]:.1f}s audio, '
            f'process={tt["process"]:.1f}s ({steps})'
        )
    logger.info(
        "Job completed: %d tracks in %.1fs\n%s",
        len(results), total_elapsed, "\n".join(timing_lines),
    )

    response = {
        "tracks": results,
        "language": language,
        "model": pipeline.model_size,
    }

    if inp.get("result_sink"):
        persist_output(inp["result_sink"], response)

    return response
