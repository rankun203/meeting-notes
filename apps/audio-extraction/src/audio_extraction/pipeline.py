"""WhisperX transcription + alignment + diarization pipeline."""

import logging
import numpy as np
import torch
import whisperx
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
            logger.info("Loading diarization pipeline")
            self._diarize_model = DiarizationPipeline(
                token=self.hf_token, device=self.device
            )
        return self._diarize_model

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

            # Pass waveform tensor instead of file path to avoid torchcodec
            # dependency (pyannote uses torchcodec for file I/O which needs
            # libnvrtc.so.13, not available in all CUDA base images).
            # whisperx.load_audio returns float32 mono at 16kHz; pyannote
            # expects a (channel, time) tensor at any sample rate.
            waveform_tensor = torch.from_numpy(audio).unsqueeze(0)  # (1, samples)
            audio_input = {"waveform": waveform_tensor, "sample_rate": 16000}

            # return_embeddings=True gives us speaker voice fingerprints
            # directly from pyannote — no need for a separate embedding model
            diarize_result = diarize_model(
                audio_input, return_embeddings=True, **diarize_kwargs
            )

            if isinstance(diarize_result, tuple):
                diarize_segments, raw_embeddings = diarize_result
                # raw_embeddings: dict[str, list[float]]
                if raw_embeddings:
                    speaker_embeddings = {
                        k: v if isinstance(v, list) else v.tolist()
                        for k, v in raw_embeddings.items()
                    }
            else:
                diarize_segments = diarize_result

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
