"""Entry point for RunPod serverless: `python -m audio_extraction`."""

import logging
import platform
import subprocess
import sys

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

    # GPU + CUDA
    try:
        import torch
        logger.info("PyTorch: %s (CUDA: %s)", torch.__version__, torch.version.cuda)
        if torch.cuda.is_available():
            for i in range(torch.cuda.device_count()):
                props = torch.cuda.get_device_properties(i)
                logger.info("GPU %d: %s (%.1f GB)", i, props.name, props.total_mem / 1e9)
        else:
            logger.info("GPU: CUDA not available")
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
            "torch==", "torchaudio", "torchvision", "torchcodec",
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


log_system_info()

import runpod
from audio_extraction.handler import handler

runpod.serverless.start({"handler": handler})
