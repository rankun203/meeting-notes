# Deploy the client, server and worker

The native client always runs on the host OS. The server and worker are separate processes and can run on the same machine or on different machines. This guide uses source builds so it includes the new monorepo/local-worker code; previously published GdayMeetings images do not include these changes.

## All-local CPU deployment

Requirements: Docker with Compose, enough RAM/disk for the selected models, and the Rust/macOS requirements in the client README. The worker downloads model weights on first use. Diarization requires an HF_TOKEN and accepted access terms for its gated pyannote models; see the worker README.

From the repository root:

```sh
cp .env.example .env
# Edit .env: PAYLOAD_SECRET and LOCAL_WORKER_API_TOKEN must be set.
# Generate each new secret independently:
openssl rand -hex 32
openssl rand -hex 32

docker compose up --build -d
```

For an existing GdayMeetings installation, retain its PAYLOAD_SECRET and reuse its named volume. The default server volume is gday-meetings-data, matching the previous docker run command. Stop the previous server container before using the same port/volume from Compose. Do not generate a replacement PAYLOAD_SECRET for an existing database: it is part of its authentication/capability identity.

Open http://localhost:3033/admin and create the first administrator if this is a fresh installation. Start the host client separately:

```sh
bash scripts/run-macos.sh
```

Open http://127.0.0.1:33487. Under Settings → Services enter http://localhost:3033 and select Login to Meeting Notes Server. Existing local meetings can be copied using the migration controls; this is an explicit operation and leaves local originals intact.

Useful commands:

```sh
docker compose logs -f server worker-audio-extraction
docker compose down
```

Ordinary `down` retains the named data/model volumes. The default worker is CPU, small model, int8, batch size 1. Override WHISPER_MODEL_SIZE, WHISPER_BATCH_SIZE and OMP_NUM_THREADS in .env as needed. The server keeps SQLite in /app/data and audio under /app/data/audio. Worker execution state is in /data; model cache is under /cache. Worker port 8000 stays inside the Compose network.

SERVER_URL is for the browser/client. SERVER_INTERNAL_URL=http://server:3000 and LOCAL_WORKER_URL=http://worker-audio-extraction:8000 are Docker service addresses used between containers. Changing SERVER_PORT also requires updating SERVER_URL to the same public port. The worker secret is for machine requests only; clients and MCP always use user OAuth.

## Local NVIDIA GPU worker

On a Linux host with a supported NVIDIA GPU and NVIDIA Container Toolkit:

```sh
docker compose -f compose.yaml -f compose.gpu.yaml up --build -d
```

The GPU override builds Dockerfile.runpod but starts its local HTTP mode, sets CUDA/float16 and requests GPU access. It defaults to large-v2 with batch size 16. Explicit model/batch variables in .env override those defaults. Docker Desktop on macOS should use the CPU configuration; this CUDA image does not accelerate on Apple GPUs.

The native macOS client can use a server/worker on another host. Give the server a reachable HTTPS origin and configure that origin in the client. HTTP exceptions in the native OAuth client are limited to loopback addresses.

## Cloud server with RunPod worker

Build/deploy apps/server with its own Dockerfile or Compose configuration. Persist /app/data for SQLite and managed audio, set a stable PAYLOAD_SECRET, and set SERVER_URL to the public HTTPS origin. Configure:

```dotenv
TRANSCRIPTION_PROVIDER=runpod
RUNPOD_ENDPOINT_URL=https://api.runpod.ai/v2/YOUR_ENDPOINT_ID
RUNPOD_API_KEY=YOUR_RUNPOD_KEY
```

Deploy apps/worker-audio-extraction using Dockerfile.runpod and the same directory as its Docker build context. RunPod mode remains the default for that image. Add HF_TOKEN at runtime. The worker must reach the public server's signed audio URLs and result callback. A server behind localhost cannot be reached by a cloud worker; provide an explicit reachable HTTPS deployment/tunnel if combining a local server with cloud execution.

## Server with a remote standalone worker

Run the worker in HTTP mode and configure the server:

```dotenv
TRANSCRIPTION_PROVIDER=local
LOCAL_WORKER_URL=https://worker.example.com
LOCAL_WORKER_API_TOKEN=YOUR_MACHINE_SECRET
```

Set WORKER_API_TOKEN to the same value on the worker. Here `local` selects the self-hosted HTTP transport, even if the worker is on a remote machine. Omit SERVER_INTERNAL_URL when the worker can use the public SERVER_URL; set it only when both download and callback endpoints are reachable at an explicitly configured private server origin. Keep user OAuth credentials out of worker configuration.

Drain outstanding jobs before changing providers or worker endpoints. Persisted jobs currently retain provider job IDs but not a per-job provider configuration snapshot; this limitation is tracked in the server worklog.

## PostgreSQL and host-native services

The server supports PostgreSQL through its component configuration:

```sh
cd apps/server
cp .env.example .env
# Configure PAYLOAD_SECRET, SERVER_URL, POSTGRES_PASSWORD and worker variables.
docker compose -f compose.yaml -f compose.postgres.yaml up --build -d
```

The root local Compose recipe deliberately defaults to SQLite. A PostgreSQL database does not replace the managed audio volume; retain both. See apps/server/README.md for pnpm installation and running the server directly on a host. See apps/worker-audio-extraction/README.md for the native Python worker command. Neither server nor worker is required to be containerized.

## Releases

The imported server was based on standalone GdayMeetings 0.3.7. Future independent server releases are triggered by server-vX.Y.Z tags matching apps/server/package.json. The repository-root workflow builds AMD64 and ARM64 using apps/server as the context and publishes ghcr.io/rankun203/meeting-notes-server; it verifies dashboard/file handling before updating latest. Worker images are built from their own context, independently of the server. The Rust client remains a native binary/app bundle and has no container deployment.
