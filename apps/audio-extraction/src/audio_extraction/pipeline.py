"""WhisperX transcription + alignment + diarization pipeline."""

import logging
import whisperx

from audio_extraction.embeddings import extract_speaker_embeddings

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

        self.diarize_model = None
        if hf_token:
            logger.info("Loading diarization pipeline")
            self.diarize_model = whisperx.DiarizationPipeline(
                use_auth_token=hf_token, device=device
            )

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

        # Step 3: Diarize (speaker labels)
        speaker_embeddings = {}
        if diarize and self.diarize_model is not None:
            logger.info("Diarizing")
            diarize_kwargs = {}
            if min_speakers is not None:
                diarize_kwargs["min_speakers"] = min_speakers
            if max_speakers is not None:
                diarize_kwargs["max_speakers"] = max_speakers

            diarize_segments = self.diarize_model(audio_path, **diarize_kwargs)
            result = whisperx.assign_word_speakers(diarize_segments, result)

            # Step 4: Extract speaker embeddings
            logger.info("Extracting speaker embeddings")
            speaker_embeddings = extract_speaker_embeddings(
                audio_path=audio_path,
                diarize_segments=diarize_segments,
                device=self.device,
                hf_token=self.hf_token,
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
