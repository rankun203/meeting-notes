"""Entry point for RunPod serverless: `python -m audio_extraction`."""

import runpod

from audio_extraction.handler import handler

runpod.serverless.start({"handler": handler})
