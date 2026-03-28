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
| `SEGMENTATION_BATCH_SIZE` | `32` | Batch size for pyannote speaker segmentation (see [tuning](#batch-size-tuning)) |
| `EMBEDDING_BATCH_SIZE` | `4` | Batch size for pyannote speaker embedding extraction (see [tuning](#batch-size-tuning)) |

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

The container downloads audio from URLs. To test with local files, run a file server. With `--network host` the container shares the host network, so no tunnel is needed.

```bash
# Serve the directory containing session audio files
cd /path/to/recordings   # e.g. target/sources/
python3 -m http.server 8199 &
```

### 3. Run the container as an API server

The `--rp_serve_api` flag starts a local HTTP server instead of the default one-shot `test_input.json` mode.

```bash
docker run --rm --gpus all \
  --network host \
  -e HF_TOKEN="$HF_TOKEN" \
  meeting-notes-extraction \
  python -m audio_extraction --rp_serve_api --rp_api_host 0.0.0.0
```

The server listens on port 8000 and exposes the same endpoints as RunPod's API.

### 4. Send a request

```bash
curl -X POST http://localhost:8000/runsync \
  -H "Content-Type: application/json" \
  -d '{
    "input": {
      "tracks": [
        {
          "audio_url": "http://localhost:8199/SESSION_ID/system_microphone.opus",
          "track_name": "system_microphone",
          "source_type": "mic",
          "channels": 1
        },
        {
          "audio_url": "http://localhost:8199/SESSION_ID/system_audio.opus",
          "track_name": "system_audio",
          "source_type": "system_mix",
          "channels": 2
        }
      ],
      "language": "en",
      "diarize": true
    }
  }'
```

Available endpoints:

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/runsync` | Synchronous — blocks until processing completes, returns result |
| POST | `/run` | Async — returns a job ID immediately |
| POST | `/status/{job_id}` | Poll job status (use with `/run`) |

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

### Batch size tuning

Benchmarked on L40S (46 GB GDDR6, 864 GB/s), 51 min Chinese audio, 1 mic track. Each parameter swept independently; others held at defaults (whisper=16, seg=32, emb=32).

#### WHISPER_BATCH_SIZE (transcription)

Controls how many VAD segments are batched through the Whisper model. Affects transcription only; alignment, segmentation, and embeddings are unchanged.

| Batch | Transcribe | Align | Seg | Emb | Total | VRAM |
|------:|-----------:|------:|----:|----:|------:|-----:|
| 1 | 41.6s | 21.3s | 1.5s | 26.2s | 91.1s | 8.8 GiB |
| 4 | 24.3s | 22.3s | 1.5s | 26.6s | 75.2s | 8.8 GiB |
| 8 | 19.2s | 21.5s | 1.5s | 26.6s | 69.3s | 8.8 GiB |
| **16** | **17.3s** | 21.6s | 1.7s | 27.1s | **68.2s** | **8.8 GiB** |
| 24 | 16.4s | 22.2s | 1.5s | 26.7s | 67.3s | 8.8 GiB |
| 32 | 16.0s | 23.9s | 1.5s | 26.8s | 68.7s | 8.8 GiB |
| 48 | 15.7s | 22.0s | 1.5s | 26.6s | 66.3s | 8.8 GiB |
| 64 | 14.9s | 21.5s | 1.5s | 26.6s | 65.0s | 8.8 GiB |

Big gains 1→16 (41.6→17.3s). Diminishing returns after 16; VRAM flat at ~8.8 GiB regardless.

#### SEGMENTATION_BATCH_SIZE (diarization — speaker activity detection)

Controls batching of the sliding-window segmentation model. Large impact on segmentation time; indirectly affects embedding time because the segmentation step feeds into embeddings.

| Batch | Transcribe | Align | Seg | Emb | Total | VRAM |
|------:|-----------:|------:|----:|----:|------:|-----:|
| 1 | 17.3s | 22.2s | 30.1s | 55.5s | 125.6s | 8.7 GiB |
| 4 | 17.2s | 21.7s | 8.0s | 33.3s | 80.7s | 8.7 GiB |
| 8 | 17.2s | 21.2s | 4.2s | 29.4s | 72.5s | 8.7 GiB |
| 16 | 17.2s | 21.5s | 2.4s | 27.6s | 69.2s | 9.0 GiB |
| 24 | 17.3s | 22.0s | 1.8s | 27.1s | 68.7s | 9.0 GiB |
| **32** | 17.3s | 21.7s | **1.5s** | 26.8s | **67.8s** | **8.8 GiB** |
| 48 | 17.3s | 21.7s | 1.3s | 26.7s | 67.5s | 9.4 GiB |
| 64 | 17.3s | 21.5s | 1.2s | 26.3s | 66.8s | 8.8 GiB |

Massive 1→16 (30.1→2.4s). Plateaus at 32; VRAM stays ~8.8-9.4 GiB.

#### EMBEDDING_BATCH_SIZE (diarization — speaker voice fingerprinting)

Controls batching of speaker embedding extraction (WeSpeaker model). This is the most VRAM-sensitive parameter and has a surprising non-monotonic speed curve.

| Batch | Transcribe | Align | Seg | Emb | Total | VRAM |
|------:|-----------:|------:|----:|----:|------:|-----:|
| 1 | 17.3s | 21.7s | 1.5s | 43.5s | 84.5s | 7.5 GiB |
| **4** | 17.1s | 21.5s | 1.5s | **21.3s** | **61.9s** | **7.5 GiB** |
| 8 | 17.2s | 21.5s | 1.5s | 22.0s | 62.7s | 7.5 GiB |
| 16 | 17.2s | 21.1s | 1.5s | 24.8s | 65.1s | 7.5 GiB |
| 24 | 17.2s | 22.4s | 1.5s | 25.9s | 67.5s | 8.5 GiB |
| 32 | 17.2s | 21.7s | 1.5s | 26.6s | 67.5s | 8.8 GiB |
| 48 | 17.2s | 22.5s | 1.5s | 27.1s | 68.8s | 9.9 GiB |
| 64 | 17.2s | 21.7s | 1.5s | 28.4s | 69.3s | 10.7 GiB |

Fastest at batch=4 (21.3s), then **gets slower** as batch increases — GPU↔CPU transfer overhead grows faster than compute gains (see [pyannote-audio#1566](https://github.com/pyannote/pyannote-audio/issues/1566)). VRAM climbs significantly: 7.5 GiB at 4 → 10.7 GiB at 64.

#### Recommended settings by GPU VRAM

Each processing step loads different models and consumes VRAM independently. The table below gives optimum batch sizes that maximize speed without exceeding the VRAM budget. GPUs with faster HBM memory bandwidth (A100, H100) may sustain higher embedding batch sizes before hitting the transfer bottleneck. GPUs with more CUDA cores benefit from higher whisper/segmentation batch sizes.

| VRAM | Whisper | Seg | Emb | Est. total | Notes |
|-----:|--------:|----:|----:|-----------:|-------|
| 8 GB | 8 | 16 | 4 | ~67s | Consumer GPUs (RTX 3070/4060). Tight — lower whisper batch if OOM. |
| 10 GB | 16 | 32 | 4 | ~62s | RTX 3080/4070. Sweet spot for cost/performance. |
| 12 GB | 16 | 32 | 4 | ~62s | RTX 4070 Ti. Same settings, extra headroom. |
| 16 GB | 16 | 32 | 4 | ~62s | RTX 4080/5080, A4000. No speed gain from more VRAM. |
| 24 GB | 16 | 32 | 4 | ~62s | RTX 4090/5090, A5000. Headroom for longer audio. |
| 48 GB | 16 | 32 | 4 | ~62s | A40, L40S. Excess VRAM unused. |
| 80 GB | 16 | 32 | 4 | ~55s\* | A100 (2 TB/s HBM2e), H100 (3.4 TB/s HBM3). \*Higher memory bandwidth may allow emb=8-16 without slowdown — benchmark on target hardware. |

\*HBM GPUs (A100/H100) have 2-4x the memory bandwidth of GDDR6 GPUs. The embedding batch bottleneck is GPU↔CPU transfer, so HBM may shift the optimal embedding batch size higher. The whisper and segmentation steps are compute-bound and scale with CUDA core count / tensor core throughput instead.

### Troubleshooting

- **"Pyannote model pre-cache skipped"** at build time: `HF_TOKEN` wasn't passed as a build arg. Models download at runtime instead (~14s on first request).
- **"Access denied to pyannote/..."**: Accept the model licenses on HuggingFace (see [gated models](#huggingface-gated-models) above).
- **OOM / CUDA out of memory**: Lower `WHISPER_BATCH_SIZE` (e.g. `-e WHISPER_BATCH_SIZE=8`).
- **"test_input.json not found, exiting"**: You're running without `--rp_serve_api`. Either add that flag for API server mode, or mount a test input file with `-v /path/to/test_input.json:/app/test_input.json:ro`.

## Local development (without Docker)

This is a [uv](https://docs.astral.sh/uv/)-managed project. See `pyproject.toml` for dependencies and project configuration.

```bash
uv sync
uv run python -m audio_extraction
```
