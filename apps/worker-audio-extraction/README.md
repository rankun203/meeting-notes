---
title: Audio extraction worker
date: 2026-09-26
status: active
scope: worker-guide
---

# worker-audio-extraction

Audio transcription + speaker diarization worker. Runs locally on CPU or NVIDIA GPU, or on RunPod serverless.

Takes audio files in, returns transcripts with word-level timestamps, speaker labels, and speaker voice embeddings out. The local transport retains its job queue/results in SQLite; the server owns meetings and durable outputs. No people management or LLM runs here.

## Local CPU deployment

This directory owns the worker's Compose files and environment template. It can run
on a different host from the server. To start it from this directory:

```sh
cp .env.example .env
# Set WORKER_API_TOKEN; optionally set HF_TOKEN for diarization.
docker compose up --build -d
```

The root `make worker-start` command delegates here. Configure the server's
LOCAL_WORKER_URL to reach this host and LOCAL_WORKER_API_TOKEN to match WORKER_API_TOKEN.
No shared Docker network is created. To build the CPU image directly instead:

```sh
docker build -f Dockerfile.cpu -t gday-meetings-worker-audio-extraction:cpu-local .
docker run --rm --env-file worker.env \
  -p 127.0.0.1:8000:8000 -v meeting-notes-worker-data:/data -v meeting-notes-worker-cache:/cache \
  gday-meetings-worker-audio-extraction:cpu-local
```

Set a strong `WORKER_API_TOKEN` in `worker.env` (do not commit it). Local mode accepts
only authenticated server-to-worker requests; this token is not a user login credential.
Compose binds port 8000 to host loopback by default. Set WORKER_BIND_ADDRESS and
WORKER_PORT for a trusted private binding, or put a TLS reverse proxy in front for
remote access. Use TLS if crossing an untrusted network.

The CPU image installs CPU PyTorch wheels and uses `WHISPER_DEVICE=cpu`,
`WHISPER_MODEL_SIZE=small`, `WHISPER_COMPUTE_TYPE=int8`, `WHISPER_BATCH_SIZE=1`, and
segmentation/embedding batch sizes of 1. `OMP_NUM_THREADS` defaults to 4 on CPU.
Override these for memory/accuracy/throughput needs. WhisperX documents CPU int8
execution in its [upstream guide](https://github.com/m-bain/whisperX).
Alignment and diarization also use the selected device. `language=auto` requests
language detection. CPU inference is slower than GPU; a smaller model trades accuracy
for lower resource use. Diarization still requires `HF_TOKEN` and accepted pyannote model
licenses; without it the existing pipeline skips diarization. Transcription works without
that token. First inference downloads model assets; persist `/cache` (`HF_HOME`) to reuse
them. Fully local execution does not mean offline first startup: prepopulate model caches
before disconnecting the host from the network.

For local NVIDIA execution use `Dockerfile.runpod` with `WORKER_MODE=http`,
`WHISPER_DEVICE=cuda`, NVIDIA Container Toolkit and GPU allocation. Its default mode
remains RunPod. The CPU image defaults to HTTP. Both use `python -m audio_extraction`.

## Local HTTP contract

All endpoints require `Authorization: Bearer <WORKER_API_TOKEN>`:

- `POST /run` with the Input object below returns HTTP 202 `{id,status:"IN_QUEUE"}`.
  Optional `Idempotency-Key` returns the original job for the same JSON input; changed
  input with that key is rejected with HTTP 400. The server uses its task UUID.
- `GET /status/<id>` returns `IN_QUEUE`, `IN_PROGRESS`, `COMPLETED` with `output`, or
  `FAILED` with a redacted `error`. Unknown jobs return 404.
- `GET /capabilities` returns version 1 transcription language metadata without loading models or processing audio; unavailable metadata returns 503.
- `GET /health` returns HTTP 200 when the transport is running. It does not preload
  or certify model availability.

`WORKER_HOST` defaults to `0.0.0.0`, `WORKER_PORT` to `8000`, and `WORKER_DATA_DIR`
to `/data`. Accepted jobs are committed to SQLite before acknowledgement. A single
inference thread avoids competing model instances. Restart requeues interrupted work;
completed results remain available. Keep the data volume; one worker process holds an
exclusive lock per directory. `WORKER_MAX_PENDING` (default 100) bounds pending jobs;
a full queue returns 429. Requests are limited to 1 MiB and 32 tracks.
`WORKER_RESULT_RETENTION_SECONDS` defaults to 604800 (seven days); expired completed
and failed jobs are pruned on subsequent submission. Idempotency expires with the job.
Monitor disk space; SQLite can retain allocated pages for reuse after pruning.

Inputs may carry `result_sink:{url,token}`. The local HTTP worker commits successful
extraction output to SQLite before attempting `TRANSCRIPT_OUTPUT` callback delivery.
Callback failures receive bounded retries. Even after retry exhaustion or a restart,
`GET /status/<id>` retains `COMPLETED` and the computed output for the server to recover.
A callback failure is logged without exposing its capability; it does not erase a
successful transcript. A restart after the output commit relies on server polling rather
than repeating inference or delivery. Interrupted extraction can rerun, so callbacks must
remain idempotent. RunPod mode retains its separate callback-before-success behavior. Pending inputs contain signed
capabilities in the private job database; completed/failed inputs are cleared. Treat
worker volumes as private data, alongside the server database and audio volume.

The server validates ownership of audio and callback locations. It can supply a reachable
private server origin via `SERVER_INTERNAL_URL`, while public OAuth continues to use
`SERVER_URL`. No cross-project service DNS is assumed. `localhost` inside a worker
container means the worker, not the server. The worker accepts HTTP on trusted private networks and never
rewrites URLs. Only the trusted server should possess the worker machine token.

## Lightweight checks

```sh
PYTHONPATH=src uv run --only-group transfer-test python -m unittest discover -s tests -v
```

These exercise real local HTTP, SQLite recovery, authentication, idempotency and transfer
retries without GPU/model downloads. They do not certify transcription accuracy or GPU
compatibility. An opt-in real CPU transcription/alignment check is available in an
environment with the full inference dependencies installed:

```sh
WORKER_CPU_SMOKE_AUDIO=/path/to/short-english-speech.flac OMP_NUM_THREADS=4 \
  python -m unittest discover -s tests -p test_cpu_smoke.py -v
```

It uses tiny/int8 and disables diarization. Alternatively, submit a short speech recording with `diarize:false`
and an explicit language after starting the CPU image; inspect completed word timestamps.

## What it does

- **Transcription** — speech-to-text via WhisperX (faster-whisper backend)
- **Forced alignment** — word-level timestamps via wav2vec2
- **Speaker diarization** — speaker labels via pyannote.audio
- **Speaker embeddings** — per-speaker voice fingerprints for cross-session identification

## Capability contracts

The app-facing [transcription](../../docs/protocols/transcription.md) and [diarization](../../docs/protocols/diarization.md) protocols describe provider behavior. This worker implements their audio-processing results through the transports documented here. Its input uses downloadable audio URLs; a direct desktop client needs a configured transfer destination. A connection check does not submit audio or load models.

## Language metadata

RunPod accepts `{"input":{"operation":"capabilities"}}` through `/runsync`. The handler returns `{"protocolVersion":1,"transcription":{"languages":[{"code":"en","name":"English"}]}}`, with the actual list derived from the installed WhisperX recognition and alignment metadata. Local HTTP exposes the same object through authenticated `GET /capabilities`. English-only `.en` models restrict the list to English; supported Chinese adds simplified and traditional script variants. `auto` is not advertised.

Discovery runs before track validation and pipeline initialization. It downloads no weights and processes no recording. Metadata imports still require the installed runtime libraries, and RunPod may charge for worker startup/execution. Older deployed workers need an update; clients must show unavailable metadata rather than assume language support. See the [language discovery contract](../../docs/protocols/transcription.md#discover-supported-languages).

## Input

```json
{
  "input": {
    "tracks": [
      {
        "audio_url": "https://gdaymeetings.com/files/mic.opus?token=SIGNED_AUDIO_CAPABILITY",
        "track_name": "system_microphone",
        "source_type": "mic",
        "channels": 1
      },
      {
        "audio_url": "https://gdaymeetings.com/files/system.opus?token=SIGNED_AUDIO_CAPABILITY",
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
   - Dockerfile path: `apps/worker-audio-extraction/Dockerfile.runpod`
   - Docker context: `apps/worker-audio-extraction`
   - **Hugging Face access token**: paste your HF token (RunPod does NOT pass this at build time for GitHub builds — it's only used for RunPod's own model registry)
   - **Environment variables**: add `HF_TOKEN=hf_...` (required at runtime — pyannote gated models download on first diarization request)
4. **Select GPU**: A40 (48GB, best value) or A100 (80GB, fastest)
5. **Deploy** — builds trigger on GitHub releases

### Option B: Manual Docker build

```bash
cd apps/worker-audio-extraction

# Build with all models pre-cached (pass HF_TOKEN to cache pyannote models)
docker build --platform linux/amd64 \
  --build-arg HF_TOKEN=hf_... \
  -f Dockerfile.runpod -t YOUR_DOCKERHUB/worker-audio-extraction:v0.1.0 .

# Push to Docker Hub
docker push YOUR_DOCKERHUB/worker-audio-extraction:v0.1.0

# Then create a RunPod serverless endpoint using this image
```

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `WHISPER_MODEL_SIZE` | CPU `small`; GPU `large-v2` | Whisper model size (`tiny`, `base`, `small`, `medium`, `large-v2`, `large-v3`) |
| `WHISPER_BATCH_SIZE` | CPU `1`; GPU `16` | Batch size for transcription (lower = less VRAM) |
| `WHISPER_COMPUTE_TYPE` | CPU `int8`; GPU `float16` | Compute type (`float16` for GPU, `int8` for CPU) |
| `WHISPER_DEVICE` | CPU image `cpu`; otherwise `cuda` | Device (`cuda` or `cpu`) |
| `HF_TOKEN` | — | HuggingFace token (required for pyannote diarization) |
| `SEGMENTATION_BATCH_SIZE` | CPU `1`; GPU `32` | Batch size for pyannote speaker segmentation (see [tuning](#batch-size-tuning)) |
| `EMBEDDING_BATCH_SIZE` | CPU `1`; GPU `4` | Batch size for pyannote speaker embedding extraction (see [tuning](#batch-size-tuning)) |

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

## Local GPU deployment

Build `Dockerfile.runpod`, then select the authenticated local transport explicitly:

```sh
docker build -f Dockerfile.runpod -t gday-meetings-worker-audio-extraction:gpu-local .
docker run --rm --gpus all --env-file worker.env -e WORKER_MODE=http \
  -p 127.0.0.1:8000:8000 -v meeting-notes-worker-data:/data -v meeting-notes-worker-cache:/cache \
  -e HF_HOME=/cache/huggingface gday-meetings-worker-audio-extraction:gpu-local
```

Use the same authenticated `/run` and `GET /status/<id>` endpoints as CPU mode.
RunPod credentials and its SDK debug server are not used. Alternatively run
`docker compose -f compose.yaml -f compose.gpu.yaml up --build -d` from this directory,
or `make worker-start-gpu` at the repository root. The server remains independently deployed.

## Historical GPU tuning measurements

The following measurements predate the CPU/local HTTP mode and are not new validation
of this release. Rebenchmark your chosen model and host before estimating capacity.

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
- **"test_input.json not found, exiting"**: You started the RunPod transport outside RunPod. Set `WORKER_MODE=http` and `WORKER_API_TOKEN` for the local server.

## Local development (without Docker)

This is a [uv](https://docs.astral.sh/uv/)-managed project. Install FFmpeg first.
The CPU Dockerfile is the supported Linux CPU dependency recipe; plain `uv sync` on
Linux may download CUDA-enabled PyTorch wheels even though inference selects CPU.
On macOS, use the native PyTorch wheels with the CPU device. No RunPod account is needed.

```bash
uv sync
# Set WORKER_API_TOKEN securely in your shell or environment file first.
WORKER_MODE=http WHISPER_DEVICE=cpu WORKER_DATA_DIR=./worker-data \
  uv run python -m audio_extraction
```

## Transfer reliability and durable results

Downloads retry transient connection/read failures (including truncated streamed bodies),
HTTP 408/429, and selected 5xx responses up to four attempts with 1/2/4-second backoff.
Each attempt starts from byte zero; incomplete files are removed. A per-job temporary
directory also cleans up decoded tracks if another track or GPU processing fails.
The source URL must allow repeated downloads and remain valid throughout queueing.

An optional `input.result_sink` object accepts `url` and `token`. The worker POSTs `{"type":"TRANSCRIPT_OUTPUT","body":<output>}` with
`Authorization: Bearer <token>`. The sink must persist the output before returning a
2xx response and upsert by task/output type, because retries can deliver duplicates.
Use a task-scoped callback capability; request bodies and capabilities are not logged.
In RunPod mode a failed callback fails the job instead of claiming an unpersisted result
is durable. In local HTTP mode output is already durable in the worker SQLite database;
callback failure leaves that output recoverable through polling until its retention expires.

GPU-free transfer regressions use the project test dependency group:

```bash
uv run --only-group transfer-test env PYTHONPATH=src python -m unittest discover -s tests -v
```
