# Deploy the client, server and worker

Each component is independently installed and deployed. The client runs on the host OS; the server and worker can each run natively or in containers, on the same host or different hosts. Dockerfiles, Compose files and environment templates live in their owning component directories. The root Makefile only delegates commands and has no shared environment file or Docker network.

## Native macOS client

From the repository root:

```sh
make start                              # Build and launch; Ctrl+C stops it
make start CLIENT_ARGS="--port 8080"    # Optional client CLI arguments
make build                              # Build the .app without launching
make install                            # Build and open the Finder installer folder
make stop                               # Stop the development app gracefully
```

The app is built under `apps/client-macos-rust/target/macos/`. `make start` opens its browser UI without copying it into Applications or starting any containers. It uses the same macOS permission identity and local data directory as before.

`make install` creates a separate app copy under the client's `target/installer/`, beside an Applications shortcut. Drag the app onto Applications in Finder, then open it from Applications. Finder launches default to starting the local web UI and opening the browser. This is a local ad-hoc signed build; signing and notarization for public binary distribution are separate work. Quit a running installed copy before replacing it. See the [client README](../apps/client-macos-rust/README.md) for recording permissions and CLI usage.

## Server host

Prepare the environment in `apps/server/`:

```sh
cd apps/server
cp .env.example .env
# Set a stable PAYLOAD_SECRET and the public SERVER_URL.
# Generate a new secret for a fresh database: openssl rand -hex 32
docker compose up --build -d
```

From the repository root, `make server-start`, `make server-stop` and `make server-logs` delegate to this directory. They operate only on the server's Compose project. The default port binding is `127.0.0.1:3000`; set SERVER_PORT and SERVER_BIND_ADDRESS to configure the deployment, and make SERVER_URL match the browser-facing origin. Use HTTPS for a non-loopback client connection.

SQLite is the default. The named volume `gday-meetings-data` retains the database and managed audio under `/app/data`. Set SERVER_DATA_VOLUME if your existing installation uses another volume. Preserve its PAYLOAD_SECRET and stop the old server before reusing its volume. If upgrading from the previous component Compose default, point SERVER_DATA_VOLUME at its existing project-prefixed `gday-data` volume instead of creating an empty library. No data is copied automatically.

Open the server's `/admin` page for first-admin setup. In the client, use **Settings → Services → Login to Gday Meetings Server** with that public origin. Existing meetings can be copied through the client's migration controls after signing in.

For PostgreSQL, set POSTGRES_PASSWORD and run from the server directory:

```sh
docker compose -f compose.yaml -f compose.postgres.yaml up --build -d
```

For an existing database server, configure DATABASE_ADAPTER and DATABASE_URI. PostgreSQL does not replace managed file storage; retain the audio volume. Native server deployment uses `pnpm install --frozen-lockfile`, then `pnpm build && pnpm start`, with the same server environment.

## Worker host

Prepare a separate environment in `apps/worker-audio-extraction/`:

```sh
cd apps/worker-audio-extraction
cp .env.example .env
# Set WORKER_API_TOKEN to a strong secret: openssl rand -hex 32
docker compose up --build -d                            # CPU
# Or, on Linux with an NVIDIA GPU and Container Toolkit:
docker compose -f compose.yaml -f compose.gpu.yaml up --build -d
```

Root shortcuts are `make worker-start`, `make worker-start-gpu`, `make worker-stop`, `make worker-stop-gpu` and `make worker-logs`. These never start or configure the CMS. The default worker port binds to `127.0.0.1:8000`. Put a TLS reverse proxy in front for remote access, or configure WORKER_BIND_ADDRESS for an explicitly trusted private network. All worker endpoints require its machine token.

CPU mode defaults to the small model, int8 and batch size 1. GPU mode defaults to large-v2, CUDA/float16 and batch size 16. Docker Desktop on macOS uses CPU mode. Jobs/results persist in the worker's `/data` volume and model weights in `/cache`; the worker never mounts the CMS data volume. Model downloads require initial network access. Gated diarization models additionally need HF_TOKEN and accepted access terms; pre-cache models for offline use.

## Connect independently deployed services

On the **server**, select the self-hosted HTTP worker transport:

```dotenv
TRANSCRIPTION_PROVIDER=local
LOCAL_WORKER_URL=https://worker.example.com
LOCAL_WORKER_API_TOKEN=the-workers-WORKER_API_TOKEN
```

Here `local` names the standalone HTTP transport, even when the worker is on another host. The server must reach LOCAL_WORKER_URL. The worker must reach the server's signed audio URLs and result callback at SERVER_URL. If that public origin is inaccessible to the worker, set SERVER_INTERNAL_URL to an explicit server origin that it can reach. There are no implicit `server` or `worker-audio-extraction` DNS names across the two Compose projects. A container's localhost refers to itself.

The machine token never substitutes for user OAuth. The server gives the worker only task-scoped input and callback capabilities. Drain outstanding jobs before changing providers or endpoints: current tasks store provider job IDs without a per-job provider configuration snapshot.

## Entirely local, without Docker

Both services can run directly on the same machine, avoiding container routing. Install the prerequisites in their READMEs, including FFmpeg for the worker. Set these values in `apps/server/.env`:

```dotenv
SERVER_URL=http://localhost:3000
TRANSCRIPTION_PROVIDER=local
LOCAL_WORKER_URL=http://127.0.0.1:8000
LOCAL_WORKER_API_TOKEN=the-workers-WORKER_API_TOKEN
```

Leave SERVER_INTERNAL_URL unset. In separate terminals, run:

```sh
# Terminal 1, in apps/server:
pnpm dev

# Terminal 2, in apps/worker-audio-extraction, after configuring its .env:
WORKER_MODE=http WHISPER_DEVICE=cpu WORKER_HOST=127.0.0.1 WORKER_DATA_DIR=./worker-data \
  uv run --env-file .env python -m audio_extraction

# Terminal 3, at the repository root:
make start
```

The CPU Dockerfile provides the Linux CPU-wheel dependency recipe; plain native `uv sync` on Linux may download CUDA-enabled PyTorch wheels. Native macOS runs CPU inference with its platform wheels.

## RunPod worker

Configure the **server** with TRANSCRIPTION_PROVIDER=runpod, RUNPOD_ENDPOINT_URL and RUNPOD_API_KEY. Deploy `apps/worker-audio-extraction/Dockerfile.runpod` using that component directory as its build context. The image defaults to RunPod mode; configure HF_TOKEN there. The worker must reach the server's public HTTPS origin. See the [worker README](../apps/worker-audio-extraction/README.md) for details.

## Releases

Server releases use `server-vX.Y.Z` tags matching `apps/server/package.json`. The repository-root GitHub workflow builds AMD64 and ARM64 from that component and publishes `ghcr.io/rankun203/gday-meetings-server`, verifying dashboard/file handling before updating latest. Earlier Gday Meetings images predate the independent local-worker source changes. Worker images have their own build contexts. The Rust client is a native app/binary and has no container deployment.
