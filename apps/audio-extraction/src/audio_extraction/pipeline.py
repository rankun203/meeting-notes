"""WhisperX transcription + alignment + diarization pipeline."""

import logging
import os
import subprocess
import sys
import time

import numpy as np
import pandas as pd
import requests as req
import torch
import whisperx
from pyannote.audio import Pipeline as PyannotePipeline
from whisperx.diarize import DiarizationPipeline

logger = logging.getLogger(__name__)


class TranscriptionPipeline:
    """Manages WhisperX models and runs the full extraction pipeline."""

    def __init__(
        self,
        model_size: str = "large-v2",
        device: str = "cuda",
        compute_type: str = "float16",
        batch_size: int = 16,
        hf_token: str | None = None,
    ):
        self.model_size = model_size
        self.device = device
        self.compute_type = compute_type
        self.batch_size = batch_size
        self.hf_token = hf_token

        logger.info(
            "Loading WhisperX model: %s on %s (%s)", model_size, device, compute_type
        )
        self.model = whisperx.load_model(
            model_size, device, compute_type=compute_type
        )

        self._align_models: dict[str, tuple] = {}
        self._diarize_model = None

    def _get_diarize_model(self) -> DiarizationPipeline | None:
        """Load diarization model on first use. Requires HF_TOKEN."""
        if self._diarize_model is None and self.hf_token:
            logger.info("Loading diarization pipeline (downloads pyannote sub-models on first run)")
            logger.info("Required gated model licenses:")
            logger.info("  - https://huggingface.co/pyannote/speaker-diarization-community-1")
            logger.info("  - https://huggingface.co/pyannote/segmentation-3.0")

            # Pre-check: verify HF_TOKEN can access gated models before attempting
            # the full pipeline load (which hangs silently on 403).
            self._verify_hf_access()


            # Disable HF progress bars — can cause issues in non-interactive envs.
            os.environ["HF_HUB_DISABLE_PROGRESS_BARS"] = "1"
            os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
            os.environ["HF_HUB_DISABLE_IMPLICIT_TOKEN"] = "1"

            model_name = "pyannote/speaker-diarization-community-1"

            # Pipeline.from_pretrained() hangs in RunPod's serverless worker
            # (works fine from SSH). Workaround: run the download in a subprocess,
            # then load from local cache.
            logger.info("Step 1/3: Pre-downloading models via subprocess...")
            t0 = time.time()
            download_script = f"""
import os
os.environ["HF_HUB_DISABLE_PROGRESS_BARS"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
from pyannote.audio import Pipeline
pipeline = Pipeline.from_pretrained('{model_name}', token='{self.hf_token}')
print('DOWNLOAD_OK')
"""
            result = subprocess.run(
                [sys.executable, "-c", download_script],
                capture_output=True, text=True, timeout=180,
            )
            if "DOWNLOAD_OK" not in result.stdout:
                logger.error("Subprocess stdout: %s", result.stdout[-500:] if result.stdout else "(empty)")
                logger.error("Subprocess stderr: %s", result.stderr[-500:] if result.stderr else "(empty)")
                raise RuntimeError(f"Failed to download diarization models in subprocess")
            logger.info("Step 1/3 done in %.1fs: models cached to disk", time.time() - t0)

            # Now load from cache in this process.
            # Force offline mode to prevent any network calls that might hang.
            logger.info("Step 2/3: Loading pipeline from cache (offline)...")
            t0 = time.time()
            os.environ["HF_HUB_OFFLINE"] = "1"
            try:
                pipeline = PyannotePipeline.from_pretrained(
                    model_name, token=self.hf_token
                )
            finally:
                os.environ.pop("HF_HUB_OFFLINE", None)
            logger.info("Step 2/3 done in %.1fs", time.time() - t0)

            logger.info("Step 3/3: Moving to device=%s...", self.device)
            t0 = time.time()
            pipeline = pipeline.to(torch.device(self.device))
            logger.info("Step 3/3 done in %.1fs", time.time() - t0)

            # Construct DiarizationPipeline wrapper without re-downloading
            self._diarize_model = object.__new__(DiarizationPipeline)
            self._diarize_model.model = pipeline
            logger.info("Diarization pipeline loaded successfully")
        return self._diarize_model

    def _verify_hf_access(self):
        """Pre-check that HF_TOKEN can access required gated models."""
        gated_models = [
            "pyannote/speaker-diarization-community-1",
            "pyannote/segmentation-3.0",
        ]
        for model in gated_models:
            url = f"https://huggingface.co/api/models/{model}"
            logger.info("Checking access: %s", model)
            try:
                resp = req.get(url, headers={"Authorization": f"Bearer {self.hf_token}"}, timeout=10)
                if resp.status_code == 401:
                    raise RuntimeError(
                        f"HF_TOKEN is invalid or expired. Check your token at https://huggingface.co/settings/tokens"
                    )
                elif resp.status_code == 403:
                    raise RuntimeError(
                        f"Access denied to {model}. Accept the license at https://huggingface.co/{model}"
                    )
                elif resp.status_code != 200:
                    logger.warning("Unexpected status %d for %s, proceeding anyway", resp.status_code, model)
                else:
                    logger.info("Access OK: %s", model)
            except req.ConnectionError as e:
                raise RuntimeError(f"Cannot reach HuggingFace API: {e}")
            except req.Timeout:
                raise RuntimeError(f"HuggingFace API timeout checking {model}")

    def _get_align_model(self, language: str):
        """Load and cache alignment model per language."""
        if language not in self._align_models:
            logger.info("Loading alignment model for language: %s", language)
            model_a, metadata = whisperx.load_align_model(
                language_code=language, device=self.device
            )
            self._align_models[language] = (model_a, metadata)
        return self._align_models[language]

    def process_track(
        self,
        audio_path: str,
        language: str = "en",
        diarize: bool = True,
        speaker_prefix: str = "mic",
        min_speakers: int | None = None,
        max_speakers: int | None = None,
    ) -> dict:
        """Process a single audio track through the full pipeline.

        Returns dict with: segments, speaker_embeddings, duration_secs
        """
        # Load audio
        audio = whisperx.load_audio(audio_path)
        duration_secs = len(audio) / 16000  # whisperx loads at 16kHz

        # Step 1: Transcribe
        logger.info("Transcribing %s (%.1fs)", audio_path, duration_secs)
        result = self.model.transcribe(audio, batch_size=self.batch_size, language=language)

        # Step 2: Align (word-level timestamps)
        logger.info("Aligning transcript")
        model_a, metadata = self._get_align_model(result["language"])
        result = whisperx.align(
            result["segments"],
            model_a,
            metadata,
            audio,
            self.device,
            return_char_alignments=False,
        )

        # Step 3: Diarize (speaker labels + embeddings)
        speaker_embeddings = {}
        diarize_model = self._get_diarize_model() if diarize else None
        if diarize_model is not None:
            logger.info("Diarizing")
            diarize_kwargs = {}
            if min_speakers is not None:
                diarize_kwargs["min_speakers"] = min_speakers
            if max_speakers is not None:
                diarize_kwargs["max_speakers"] = max_speakers

            # Call pyannote pipeline directly (not whisperx wrapper) because:
            # 1. We use object.__new__ to bypass whisperx's constructor
            # 2. whisperx's __call__ expects str/ndarray, not dict
            # 3. We need to pass waveform dict to avoid torchcodec dependency
            waveform_tensor = torch.from_numpy(audio).unsqueeze(0)  # (1, samples)
            audio_input = {"waveform": waveform_tensor, "sample_rate": 16000}

            # Call the underlying pyannote pipeline directly
            diarize_output = diarize_model.model(
                audio_input, return_embeddings=True, **diarize_kwargs
            )

            # pyannote returns (Annotation, embeddings) or just Annotation
            if isinstance(diarize_output, tuple):
                annotation, raw_embeddings = diarize_output
                if raw_embeddings:
                    speaker_embeddings = {
                        k: v if isinstance(v, list) else v.tolist()
                        for k, v in raw_embeddings.items()
                    }
            else:
                annotation = diarize_output

            # Convert pyannote Annotation to DataFrame for whisperx
            diarize_segments = pd.DataFrame(
                [
                    {"start": turn.start, "end": turn.end, "speaker": speaker}
                    for turn, _, speaker in annotation.itertracks(yield_label=True)
                ]
            )

            result = whisperx.assign_word_speakers(
                diarize_segments, result, fill_nearest=True
            )

        # Prefix speaker IDs to avoid cross-track collisions
        segments = _prefix_speakers(result["segments"], speaker_prefix)
        speaker_embeddings = {
            f"{speaker_prefix}_{k}": v for k, v in speaker_embeddings.items()
        }

        return {
            "duration_secs": round(duration_secs, 2),
            "segments": segments,
            "speaker_embeddings": speaker_embeddings,
        }


def _prefix_speakers(segments: list[dict], prefix: str) -> list[dict]:
    """Add track prefix to speaker IDs in segments."""
    for seg in segments:
        if "speaker" in seg and seg["speaker"]:
            seg["speaker"] = f"{prefix}_{seg['speaker']}"
        if "words" in seg:
            for word in seg["words"]:
                if "speaker" in word and word["speaker"]:
                    word["speaker"] = f"{prefix}_{word['speaker']}"
    return segments
