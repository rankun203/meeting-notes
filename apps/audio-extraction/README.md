# Audio Extraction

Stateless audio transcription + speaker diarization service, deployed as a RunPod serverless endpoint.

Takes audio files in, returns transcripts with word-level timestamps, speaker labels, and speaker voice embeddings out. No storage, no people management, no LLM — just WhisperX.

## What it does

- **Transcription** — speech-to-text via WhisperX (faster-whisper backend)
- **Forced alignment** — word-level timestamps via wav2vec2
- **Speaker diarization** — speaker labels via pyannote.audio
- **Speaker embeddings** — per-speaker voice fingerprints for cross-session identification

## Input

```json
{
  "input": {
    "tracks": [
      {
        "audio_url": "https://example.com/session/mic.opus",
        "track_name": "system_microphone",
        "source_type": "mic",
        "channels": 1
      },
      {
        "audio_url": "https://example.com/session/system.opus",
        "track_name": "system_mix",
        "source_type": "system_mix",
        "channels": 2
      }
    ],
    "language": "en",
    "diarize": true,
    "min_speakers": null,
    "max_speakers": null
  }
}
```

## Output

```json
{
  "tracks": {
    "system_microphone": {
      "source_type": "mic",
      "duration_secs": 1832.5,
      "segments": [
        {
          "start": 0.0,
          "end": 3.5,
          "text": "Let's start with the status update.",
          "speaker": "mic_SPEAKER_00",
          "words": [
            { "word": "Let's", "start": 0.0, "end": 0.3, "score": 0.99 }
          ]
        }
      ],
      "speaker_embeddings": {
        "mic_SPEAKER_00": [0.12, -0.34, 0.56]
      }
    }
  },
  "language": "en",
  "model": "large-v2"
}
```

## Deploy to RunPod

```bash
# Build the image
docker build -f Dockerfile.runpod -t audio-extraction .

# Tag and push to your registry
docker tag audio-extraction your-registry/audio-extraction:latest
docker push your-registry/audio-extraction:latest
```

Then create a RunPod serverless endpoint using this image.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `WHISPER_MODEL_SIZE` | `large-v2` | Whisper model size |
| `WHISPER_BATCH_SIZE` | `16` | Batch size for transcription |
| `WHISPER_COMPUTE_TYPE` | `float16` | Compute type (`float16`, `int8`) |
| `HF_TOKEN` | — | HuggingFace token (required for diarization) |

## Local development

```bash
uv sync
uv run python -m audio_extraction
```
