---
date: 2026-09-23
title: Local CPU and GPU audio worker
status: complete
---

## Problem

The audio worker only launched through RunPod and unconditionally required CUDA.
A client/server/worker monorepo needs an independently deployable local worker,
including CPU hosts, without losing reliable downloads or durable callbacks.

## Implemented solution

Renamed `apps/audio-extraction` to `apps/worker-audio-extraction`. Added authenticated
RunPod-shaped HTTP submission/status endpoints with a SQLite job queue, serialized
inference, idempotency keys, restart recovery, bounded pending count, configurable
result retention, redacted errors, and exclusive process ownership of the queue.
Bearer authorization remains machine-to-machine, independent of user OAuth.

Added device-aware defaults (CPU small/int8/batch1; GPU preserves large-v2/float16),
a CPU Dockerfile, and conditional CUDA startup checks. Both images run as uid 10001
and use /cache for Hugging Face and torch models, allowing CPU/GPU volume reuse. Existing alignment and
diarization already propagate the selected device. Auto language now passes `None`
to WhisperX rather than unsupported `auto`. RunPod remains the default transport;
the CPU image defaults to HTTP. Download/callback retry behavior remains intact.
The RunPod Docker dependency layer now resolves dependencies before source is copied
and no longer masks package installation failure through a broad shell fallback.

## Reasoning

Reuse the existing RunPod message shape so the server can choose local or cloud
execution without changing pipeline input/output. Persist acceptance before returning
202; restart may repeat interrupted inference but idempotent callbacks prevent duplicate
outputs. Keep internal Docker URLs supplied by the trusted server, with worker ports
unpublished by Compose. No request URL rewriting or alternate user credentials.

## Technical debt

The local runner serializes inference in one process and stores results in SQLite.
Accepted to keep model memory bounded and enable a single-host deployment. Horizontal
scaling requires distinct worker volumes or a future shared lease-based queue. Completed
and failed jobs expire lazily on new submission after seven days; SQLite keeps allocated
pages for reuse. Operators must monitor disk use; add scheduled compaction/byte quotas
if sustained retention requires them. First inference needs model downloads unless caches
are prepopulated. GPU and gated diarization verification require their respective hardware
and authorized Hugging Face access.

## Notes

Lightweight suite: 13 passed, one opt-in real CPU test skipped. Tests use actual HTTP
and SQLite to exercise authorization, async status, idempotency conflict, queue limits,
restarts, failure redaction, plus existing truncated-download and callback retries.
Docker daemon is unavailable on this host, so container builds are not yet verified.
Real CPU smoke passed on native macOS ARM with WhisperX 3.8.6 / PyTorch 2.8.0,
Whisper tiny/int8/batch1, and the public Hugging Face `Narsil/asr_dummy/1.flac` fixture:
nonempty transcript and aligned word timestamps verified (44.9 seconds including the
English alignment model download; repeated with warm caches in 6.6 seconds). The first attempted macOS `say` fixture was empty;
replaced it with public test speech, never user recordings. Native TorchCodec warned
about this host's FFmpeg library ABI, but the pipeline's external FFmpeg decoding and
waveform input completed successfully. No gated diarization or GPU inference was run.

Reproduction in the configured model environment:

```sh
WORKER_CPU_SMOKE_AUDIO=/path/to/short-english-speech.flac OMP_NUM_THREADS=4 \
  python -m unittest discover -s tests -p test_cpu_smoke.py -v
```

Upstream ONNX runtime created a telemetry session marker during the first native import;
removed that generated file and disabled ONNX, pyannote, and Hugging Face telemetry
before model imports. Synthetic caches and model environments live only in `/private/tmp`.

Review follow-up: separated local extraction from result delivery. The HTTP runner removes
`result_sink` before invoking the unchanged RunPod handler, commits successful output to
SQLite, then attempts bounded callback delivery. Exhausted callbacks keep the completed
output recoverable by server polling across restarts. No callback capability is exposed in
status responses. RunPod still fails on callback exhaustion, preserving its durability
contract. Regression test exhausts all four callback attempts, reopens the job database,
retrieves the original transcript through authenticated HTTP status, and verifies no
repeated inference or duplicate submission. Thirteen lightweight tests pass (one optional
CPU smoke skipped); prior real CPU inference checks remain applicable, as inference code
was unchanged.
