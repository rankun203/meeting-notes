"""Audio inference worker; disable upstream usage telemetry before model imports."""
import os

os.environ["ORT_DISABLE_TELEMETRY"] = "1"
os.environ["PYANNOTE_METRICS_ENABLED"] = "0"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
