---
title: GPU worker dependency resolution
date: 2026-09-26
status: active
scope: worker-container-build
---

## Problem

The RunPod image build could not resolve the base image's `torch==2.8.0+cu128`
constraint through PyPI. Docker also warned about the `HF_TOKEN` build argument.

## Implemented solution

Set `--torch-backend=cu128` for both uv dependency compilation and installation in
`Dockerfile.runpod`. Preserve the base image's exact PyTorch constraints. Update
the manual build instructions to build and push GHCR tags in one command.
Initially replaced the token argument with a BuildKit secret, then restored
`ARG HF_TOKEN` at the user's request. Manual builds use `--build-arg HF_TOKEN`
with the token exported in the shell.

## Reasoning

CUDA-specific wheels require the matching PyTorch index. Selecting the backend
explicitly works without a GPU during the build and leaves CPU builds unchanged.
The restored build argument preserves the user's requested build interface.
References: [uv PyTorch integration](https://docs.astral.sh/uv/guides/integration/pytorch/)
and [Docker build secrets](https://docs.docker.com/build/building/secrets/).

## Technical debt

The requested `HF_TOKEN` build argument retains Docker's
`SecretsUsedInArgOrEnv` warning and may expose the token through build metadata.
It is retained for the requested build interface; a future migration to a
BuildKit secret would remove this exposure and warning.

The existing GPU build resolves dependencies at build time rather than from a
GPU-specific lockfile, so later builds may select different transitive versions.
Retained to keep this fix focused; follow up with a tested GPU dependency lock.
Existing optional model-cache steps tolerate download failures, so build success
does not prove all weights are cached. Follow up with explicit cache verification
and narrower failure handling before promising offline startup.

## Notes

Cross-platform uv resolution passed for Linux AMD64 / Python 3.11 with the three
exact constraints supplied in the build log. It selected WhisperX 3.8.6,
torch/torchaudio 2.8.0+cu128, and torchvision 0.23.0+cu128.
A full image build and GPU inference have not been run locally.
Docker's build check could not finish: local Docker Desktop reported no matching
platform for the RunPod base manifest despite the AMD64 target. Validate the full
build on the Linux build host; no warning-free Docker check is claimed.
