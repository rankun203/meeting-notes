"""Entry point for RunPod serverless: `python -m audio_extraction`."""

import logging
import os
import platform
import subprocess
import sys

# --- Critical: set these BEFORE any other imports ---
# Prevent fork-related deadlocks in RunPod's serverless worker.
# RunPod forks a heartbeat process after our code runs; these env vars
# prevent deadlocks from tokenizers, OpenMP, and huggingface_hub.
os.environ["TOKENIZERS_PARALLELISM"] = "false"
os.environ["OMP_NUM_THREADS"] = "1"
os.environ["HF_HUB_DISABLE_PROGRESS_BARS"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"

logging.basicConfig(level=logging.INFO, format="%(filename)-20s:%(lineno)-4d %(asctime)s %(message)s")
logger = logging.getLogger(__name__)


def log_system_info():
    """Log full system information at startup."""
    logger.info("=== System Information ===")
    logger.info("Python: %s", sys.version)
    logger.info("Platform: %s", platform.platform())
    logger.info("CPU: %s", platform.processor() or "unknown")

    # RAM
    try:
        import psutil
        mem = psutil.virtual_memory()
        logger.info("RAM: %.1f GB total, %.1f GB available", mem.total / 1e9, mem.available / 1e9)
    except ImportError:
        try:
            with open("/proc/meminfo") as f:
                for line in f:
                    if line.startswith(("MemTotal", "MemAvailable")):
                        logger.info(line.strip())
        except FileNotFoundError:
            pass

    # GPU + CUDA — use nvidia-smi instead of torch.cuda to avoid
    # initializing the CUDA runtime context before RunPod forks.
    # CUDA is not fork-safe; initializing it here causes deadlocks.
    try:
        result = subprocess.run(
            ["nvidia-smi", "--query-gpu=name,memory.total,driver_version", "--format=csv,noheader"],
            capture_output=True, text=True, timeout=5,
        )
        if result.returncode == 0:
            for i, line in enumerate(result.stdout.strip().splitlines()):
                logger.info("GPU %d: %s", i, line.strip())
        else:
            logger.info("GPU: nvidia-smi failed")
    except FileNotFoundError:
        logger.info("GPU: nvidia-smi not found")

    # PyTorch version (import only, no CUDA init)
    try:
        import torch
        logger.info("PyTorch: %s (CUDA compiled: %s)", torch.__version__, torch.version.cuda)
    except ImportError:
        logger.info("PyTorch: not installed")

    # Key package versions
    logger.info("=== Package Versions ===")
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pip", "freeze"],
            capture_output=True, text=True, timeout=10,
        )
        key_packages = [
            "whisperx", "faster-whisper", "ctranslate2", "pyannote-audio",
            "torch==", "torchaudio", "torchvision",
            "transformers", "runpod", "numpy", "pandas",
        ]
        for line in sorted(result.stdout.splitlines()):
            if any(line.lower().startswith(pkg.lower()) for pkg in key_packages):
                logger.info("  %s", line)
    except Exception as e:
        logger.info("Failed to list packages: %s", e)

    # FFmpeg
    try:
        result = subprocess.run(["ffmpeg", "-version"], capture_output=True, text=True, timeout=5)
        first_line = result.stdout.splitlines()[0] if result.stdout else "unknown"
        logger.info("FFmpeg: %s", first_line)
    except FileNotFoundError:
        logger.info("FFmpeg: not installed")

    logger.info("=== End System Information ===")


try:
    log_system_info()
except Exception:
    logger.exception("Failed to log system info")

import runpod
from audio_extraction.handler import handler

runpod.serverless.start({"handler": handler})
