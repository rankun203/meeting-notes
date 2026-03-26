"""Speaker embedding extraction from pyannote's diarization results.

Extracts per-speaker voice fingerprints (d-vectors) that the daemon can use
for cross-session speaker identification.
"""

import logging

import torch
import torchaudio
from pyannote.audio import Inference

logger = logging.getLogger(__name__)

# pyannote embedding model — produces 192-dimensional or 512-dimensional vectors
# depending on the model version
_embedding_model: Inference | None = None


def _get_embedding_model(device: str, hf_token: str | None) -> Inference:
    """Load the pyannote speaker embedding model (cached)."""
    global _embedding_model
    if _embedding_model is None:
        logger.info("Loading speaker embedding model")
        _embedding_model = Inference(
            "pyannote/embedding",
            window="whole",
            use_auth_token=hf_token,
            device=torch.device(device),
        )
    return _embedding_model


def extract_speaker_embeddings(
    audio_path: str,
    diarize_segments,
    device: str = "cuda",
    hf_token: str | None = None,
) -> dict[str, list[float]]:
    """Extract one embedding vector per speaker from diarization results.

    Args:
        audio_path: Path to the audio file.
        diarize_segments: pyannote diarization output (DataFrame with speaker, start, end).
        device: Torch device string.
        hf_token: HuggingFace token for model access.

    Returns:
        Dict mapping speaker ID (e.g., "SPEAKER_00") to embedding vector (list of floats).
    """
    embedding_model = _get_embedding_model(device, hf_token)

    # Load full audio
    waveform, sample_rate = torchaudio.load(audio_path)

    # Group segments by speaker
    speaker_segments: dict[str, list[tuple[float, float]]] = {}
    for _, row in diarize_segments.iterrows():
        speaker = row["speaker"]
        start = row["start"]
        end = row["end"]
        if speaker not in speaker_segments:
            speaker_segments[speaker] = []
        speaker_segments[speaker].append((start, end))

    # For each speaker, concatenate their audio segments and extract embedding
    embeddings: dict[str, list[float]] = {}
    for speaker, segments in speaker_segments.items():
        # Collect audio chunks for this speaker
        chunks = []
        for start, end in segments:
            start_sample = int(start * sample_rate)
            end_sample = int(end * sample_rate)
            end_sample = min(end_sample, waveform.shape[1])
            if start_sample < end_sample:
                chunks.append(waveform[:, start_sample:end_sample])

        if not chunks:
            continue

        # Concatenate all chunks for this speaker
        speaker_audio = torch.cat(chunks, dim=1)

        # Limit to ~60 seconds to avoid memory issues while still getting a good embedding
        max_samples = 60 * sample_rate
        if speaker_audio.shape[1] > max_samples:
            speaker_audio = speaker_audio[:, :max_samples]

        # Extract embedding using pyannote
        try:
            embedding = embedding_model(
                {"waveform": speaker_audio, "sample_rate": sample_rate}
            )
            embeddings[speaker] = embedding.flatten().tolist()
        except Exception:
            logger.exception("Failed to extract embedding for %s", speaker)

    return embeddings
