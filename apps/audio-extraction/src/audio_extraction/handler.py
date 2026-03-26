"""RunPod serverless handler for audio extraction.

Accepts multiple audio tracks, runs WhisperX transcription + alignment + diarization
on each, extracts speaker embeddings, and returns per-track results.
"""

import os
import tempfile
import requests

from audio_extraction.pipeline import TranscriptionPipeline

# Initialize pipeline once (models stay loaded across requests on the same worker)
_pipeline: TranscriptionPipeline | None = None


def get_pipeline() -> TranscriptionPipeline:
    global _pipeline
    if _pipeline is None:
        _pipeline = TranscriptionPipeline(
            model_size=os.environ.get("WHISPER_MODEL_SIZE", "large-v2"),
            device=os.environ.get("WHISPER_DEVICE", "cuda"),
            compute_type=os.environ.get("WHISPER_COMPUTE_TYPE", "float16"),
            batch_size=int(os.environ.get("WHISPER_BATCH_SIZE", "16")),
            hf_token=os.environ.get("HF_TOKEN"),
        )
    return _pipeline


def download_audio(url: str, suffix: str = ".audio") -> str:
    """Download audio from URL to a temporary file. Returns the file path."""
    resp = requests.get(url, timeout=300, stream=True)
    resp.raise_for_status()
    tmp = tempfile.NamedTemporaryFile(delete=False, suffix=suffix)
    for chunk in resp.iter_content(chunk_size=8192):
        tmp.write(chunk)
    tmp.close()
    return tmp.name


def handler(event: dict) -> dict:
    """RunPod serverless handler.

    Input (event["input"]):
      - tracks: list of {audio_url, track_name, source_type, channels}
      - language: str ("en", "zh", etc.)
      - model_size: str (optional override, default from env)
      - diarize: bool (default true)
      - min_speakers: int | None
      - max_speakers: int | None

    Output:
      - tracks: {track_name: TrackResult}
      - language: str
      - model: str
    """
    inp = event["input"]
    tracks = inp["tracks"]
    language = inp.get("language", "en")
    diarize = inp.get("diarize", True)
    min_speakers = inp.get("min_speakers")
    max_speakers = inp.get("max_speakers")

    pipeline = get_pipeline()
    results = {}
    downloaded_files = []

    try:
        for track in tracks:
            audio_url = track["audio_url"]
            track_name = track["track_name"]
            source_type = track["source_type"]

            # Determine file suffix from URL (strip query params first)
            path = audio_url.split("?")[0].split("#")[0]
            suffix = "." + path.rsplit(".", 1)[-1] if "." in path else ".audio"
            audio_path = download_audio(audio_url, suffix=suffix)
            downloaded_files.append(audio_path)

            # Determine speaker ID prefix from source type
            prefix = "mic" if source_type == "mic" else "sys"

            result = pipeline.process_track(
                audio_path=audio_path,
                language=language,
                diarize=diarize,
                speaker_prefix=prefix,
                min_speakers=min_speakers,
                max_speakers=max_speakers,
            )

            results[track_name] = {
                "source_type": source_type,
                **result,
            }
    finally:
        # Clean up downloaded files
        for path in downloaded_files:
            try:
                os.unlink(path)
            except OSError:
                pass

    return {
        "tracks": results,
        "language": language,
        "model": pipeline.model_size,
    }


