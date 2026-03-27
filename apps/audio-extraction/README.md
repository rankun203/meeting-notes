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

## Local testing with Docker + GPU

Test the full pipeline locally before deploying to RunPod. Requires Docker with NVIDIA GPU support (`nvidia-container-toolkit`).

### 1. Build the image

```bash
cd apps/audio-extraction

# Build with all models pre-cached (~5 GB download on first build)
# Pass HF_TOKEN to cache pyannote gated models at build time
docker build -f Dockerfile.runpod \
  --build-arg HF_TOKEN="$HF_TOKEN" \
  -t meeting-notes-extraction .
```

### 2. Serve test audio files

The container downloads audio from URLs. To test with local files, run a file server and expose it with a tunnel so the container can reach it.

```bash
# Serve a directory containing audio files (e.g. .opus, .wav, .mp3)
cd /path/to/recordings
python3 -m http.server 8199 &

# Expose via cloudflared (or ngrok, etc.) so Docker can reach it
cloudflared tunnel --url http://localhost:8199
# Note the https://*.trycloudflare.com URL
```

### 3. Create test_input.json

```json
{
  "input": {
    "tracks": [
      {
        "audio_url": "https://YOUR-TUNNEL-URL/session_id/system_microphone.opus",
        "track_name": "system_microphone",
        "source_type": "mic",
        "channels": 1
      },
      {
        "audio_url": "https://YOUR-TUNNEL-URL/session_id/system_audio.opus",
        "track_name": "system_audio",
        "source_type": "system_mix",
        "channels": 2
      }
    ],
    "language": "en",
    "diarize": true
  }
}
```

### 4. Run the container

```bash
docker run --rm --gpus all \
  -e HF_TOKEN="$HF_TOKEN" \
  -v /path/to/test_input.json:/app/test_input.json:ro \
  meeting-notes-extraction
```

RunPod's worker detects `test_input.json`, processes it as a local job, prints the full result to stdout, and exits. No network API needed.

### What to look for

**Startup** should be fast (~3s) with no model downloads:
```
Pipeline initialized in 2.7s
Loading from cache (offline)...
Loaded from cache in 0.7s
```

**Per-track output** shows segments, speakers, and realtime factor:
```
Track "system_microphone" done: 72 segments, 2 speakers, 4021.6s audio in 49.3s (81.6x realtime)
```

**Diarization progress** is logged at each step:
```
Diarizing 3097.1s of audio...
  diarize/segmentation: 3089/3089 (100%)
  diarize/segmentation done (1.5s elapsed)
  diarize/embeddings: 58/290 (20%)
  ...
  diarize/embeddings: 290/290 (100%)
  diarize/embeddings done (26.2s elapsed)
Diarization done in 27.5s (2 speakers)
```

**No warnings** should appear in the output (onnxruntime GPU discovery warnings in Docker are expected and harmless).

### Performance reference (NVIDIA L40S, 46 GB)

| Test | Audio | Tracks | Time | Realtime factor |
|------|-------|--------|------|-----------------|
| English, 16.6 hr | 67 min mic + 15.5 hr system | 2 | 722s | 77-96x |
| Chinese, 1.7 hr | 51 min mic + 51 min system | 2 | 140s | 46-49x |

Peak VRAM is ~5.5 GiB regardless of audio length. Any GPU with 8+ GB works.

### GPU comparison

| | A40 | RTX 4090 | L40S | RTX 5090 |
|---|---|---|---|---|
| **Architecture** | Ampere (sm_80) | Ada (sm_89) | Ada (sm_89) | Blackwell (sm_100) |
| **VRAM** | 48 GB GDDR6 | 24 GB GDDR6X | 48 GB GDDR6 | 32 GB GDDR7 |
| **Memory BW** | 696 GB/s | 1,008 GB/s | 864 GB/s | ~1,792 GB/s |
| **FP16 Tensor** | 150 TFLOPS | 330 TFLOPS | 366 TFLOPS | ~420 TFLOPS |
| **Est. speed** | 1x | ~1.3x | ~1.2x | ~2x |

This workload is memory-bandwidth bound (inference, not training), so the 4090 outperforms the L40S despite lower FP16 TFLOPS. The 48 GB on A40/L40S is unused headroom — 24 GB is more than enough.

### Troubleshooting

- **"Pyannote model pre-cache skipped"** at build time: `HF_TOKEN` wasn't passed as a build arg. Models download at runtime instead (~14s on first request).
- **"Access denied to pyannote/..."**: Accept the model licenses on HuggingFace (see [gated models](#huggingface-gated-models) above).
- **OOM / CUDA out of memory**: Lower `WHISPER_BATCH_SIZE` (e.g. `-e WHISPER_BATCH_SIZE=8`).
- **"test_input.json not found, exiting"**: Mount the file with `-v /path/to/test_input.json:/app/test_input.json:ro`.

## Local development (without Docker)

```bash
uv sync
uv run python -m audio_extraction
```
