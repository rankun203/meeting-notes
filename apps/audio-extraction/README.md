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

## Deploy to RunPod Serverless

### Option A: GitHub integration (recommended)

RunPod can build and deploy directly from your GitHub repo — no local Docker builds needed.

1. **Connect GitHub**: RunPod console → Settings → GitHub → Authorize
2. **Create endpoint**: Serverless → New Endpoint → Import Git Repository
3. **Configure**:
   - Repository: select this repo
   - Branch: `master`
   - Dockerfile path: `apps/audio-extraction/Dockerfile.runpod`
   - Docker context: `apps/audio-extraction`
   - **Hugging Face access token**: paste your HF token (RunPod does NOT pass this at build time for GitHub builds — it's only used for RunPod's own model registry)
   - **Environment variables**: add `HF_TOKEN=hf_...` (required at runtime — pyannote gated models download on first diarization request)
4. **Select GPU**: A40 (48GB, best value) or A100 (80GB, fastest)
5. **Deploy** — builds trigger on GitHub releases

### Option B: Manual Docker build

```bash
cd apps/audio-extraction

# Build with all models pre-cached (pass HF_TOKEN to cache pyannote models)
docker build --platform linux/amd64 \
  --build-arg HF_TOKEN=hf_... \
  -f Dockerfile.runpod -t YOUR_DOCKERHUB/audio-extraction:v0.1.0 .

# Push to Docker Hub
docker push YOUR_DOCKERHUB/audio-extraction:v0.1.0

# Then create a RunPod serverless endpoint using this image
```

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `WHISPER_MODEL_SIZE` | `large-v2` | Whisper model size (`tiny`, `base`, `small`, `medium`, `large-v2`, `large-v3`) |
| `WHISPER_BATCH_SIZE` | `16` | Batch size for transcription (lower = less VRAM) |
| `WHISPER_COMPUTE_TYPE` | `float16` | Compute type (`float16` for GPU, `int8` for CPU) |
| `WHISPER_DEVICE` | `cuda` | Device (`cuda` or `cpu`) |
| `HF_TOKEN` | — | HuggingFace token (required for pyannote diarization) |

### HuggingFace gated models

Speaker diarization uses pyannote models that require accepting licenses on HuggingFace.
Visit each link below, log in, and click "Agree and access repository":

1. https://huggingface.co/pyannote/speaker-diarization-community-1
2. https://huggingface.co/pyannote/segmentation-3.0
3. https://huggingface.co/pyannote/embedding

All three must be accepted for `diarize: true` to work. Without them, diarization will timeout with a clear error message.

### Calling the endpoint

```bash
# Submit async job
curl -X POST https://api.runpod.ai/v2/YOUR_ENDPOINT_ID/run \
  -H "authorization: Bearer YOUR_RUNPOD_API_KEY" \
  -H "content-type: application/json" \
  -d '{"input": {"tracks": [{"audio_url": "https://...", "track_name": "mic", "source_type": "mic", "channels": 1}], "language": "en", "diarize": true}}'

# Poll for result
curl https://api.runpod.ai/v2/YOUR_ENDPOINT_ID/status/JOB_ID \
  -H "authorization: Bearer YOUR_RUNPOD_API_KEY"
```

## Local development

```bash
uv sync
uv run python -m audio_extraction
```
