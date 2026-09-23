SHELL := /bin/bash
.DEFAULT_GOAL := help
HOST_OS := $(shell uname -s)
CLIENT_DIR := apps/client-macos-rust

.PHONY: help start build install stop server-start server-stop server-logs worker-start worker-start-gpu worker-stop worker-stop-gpu worker-logs test-client test-server test-worker

help:
	@printf '%s\n' \
	  'make start               Build and run the native client (macOS)' \
	  'make build               Build the macOS .app without launching it' \
	  'make install             Build and open a drag-to-Applications installer' \
	  'make stop                Gracefully stop the development client' \
	  'make server-start        Start the CMS using its own Compose configuration' \
	  'make server-stop         Stop the CMS containers, retaining their data' \
	  'make server-logs         Follow CMS logs' \
	  'make worker-start        Start the standalone CPU worker' \
	  'make worker-start-gpu    Start the standalone NVIDIA GPU worker' \
	  'make worker-stop         Stop the CPU worker, retaining its data' \
	  'make worker-stop-gpu     Stop the GPU worker, retaining its data' \
	  'make worker-logs         Follow worker logs' \
	  'make test-client         Run Rust client tests' \
	  'make test-server         Run server tests (pnpm dependencies required)' \
	  'make test-worker         Run lightweight worker tests' \
	  '' \
	  'Example: make start CLIENT_ARGS="--port 8080"'

ifeq ($(HOST_OS),Darwin)
start:
	bash "$(CLIENT_DIR)/scripts/run-macos.sh" $(CLIENT_ARGS)

build:
	bash "$(CLIENT_DIR)/scripts/build-macos.sh"

install:
	bash "$(CLIENT_DIR)/scripts/install-macos.sh"

stop:
	bash "$(CLIENT_DIR)/scripts/run-macos.sh" --stop
else
start build install stop:
	@printf 'The native client currently supports macOS only (host: %s). Server and worker targets can run independently.\n' "$(HOST_OS)" >&2
	@exit 1
endif

server-start:
	cd apps/server && docker compose up --build -d

server-stop:
	cd apps/server && docker compose stop

server-logs:
	cd apps/server && docker compose logs -f

worker-start:
	cd apps/worker-audio-extraction && docker compose up --build -d

worker-start-gpu:
	cd apps/worker-audio-extraction && docker compose -f compose.yaml -f compose.gpu.yaml up --build -d

worker-stop:
	cd apps/worker-audio-extraction && docker compose stop

worker-stop-gpu:
	cd apps/worker-audio-extraction && docker compose -f compose.yaml -f compose.gpu.yaml stop

worker-logs:
	cd apps/worker-audio-extraction && docker compose logs -f

test-client:
	cargo test --manifest-path "$(CLIENT_DIR)/Cargo.toml" --locked

test-server:
	cd apps/server && pnpm test

test-worker:
	cd apps/worker-audio-extraction && uv run --only-group transfer-test env PYTHONPATH=src python -m unittest discover -s tests -v
