---
title: Reduce GPU worker image size
date: 2026-09-26
status: implemented
scope: worker-container-build
---

## Problem

The user reported a 32.4 GB v0.1.15 image. Its Docker history shows a 6.02 GB
layer from changing ownership of the model cache after downloading the weights.
uv also retained its package cache in image layers.

## Implemented solution

In `apps/worker-audio-extraction/Dockerfile.runpod`, assign `/data` and `/cache`
to `worker` when creating them and download models as that user. Remove the final
recursive ownership change. Run the existing optional Lightning checkpoint
migration as root before downloading models, since it writes into site-packages.
Return to root for the editable package install, then use `worker` at runtime.
Set `UV_NO_CACHE=1` for dependency resolution and both uv installs.
Correct the model-size comments using the supplied image history.

## Reasoning

Creating model files with their final owner preserves separate download layers
without copying weights into a later ownership layer. Disabling uv caching
avoids retaining downloaded packages and works without additional build mounts.
Docker can still reuse unchanged dependency-install layers. A rebuild should
remove the measured 6.02 GB ownership layer; additional cache savings are unmeasured.

## Technical debt

The CUDA development base remains large. Retained to avoid changing GPU library
compatibility in this fix; a future runtime-base migration needs GPU inference
validation. Disabling uv caching cannot remove cache files inherited from the base.

Existing build-time dependency resolution and optional model-download failure
handling remain. Builds can select different transitive versions or omit models;
follow up with a GPU lockfile and explicit cache verification.

The existing `HF_TOKEN` build argument retains Docker's
`SecretsUsedInArgOrEnv` warning risk and possible exposure through build metadata.
It remains for the previously requested build interface; migrate to a BuildKit
secret when that interface can change.

## Validation

Reviewed the complete task diff, including user transitions and cache ownership.
`git diff --check` passed. Docker's AMD64 build check stopped while resolving the
unchanged RunPod base with `no match for platform in manifest`; it did not finish
the checks. No full image build or GPU inference ran locally. Rebuild on the Linux
host and inspect image history to verify the resulting size and removed layer.
