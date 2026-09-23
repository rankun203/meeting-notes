"""Device-aware inference defaults shared by both worker transports."""
import os


def pipeline_options():
    device = os.environ.get("WHISPER_DEVICE", "cuda")
    if device not in {"cpu", "cuda"}:
        raise ValueError("WHISPER_DEVICE must be cpu or cuda")
    cpu = device == "cpu"
    return dict(
        device=device,
        model_size=os.environ.get("WHISPER_MODEL_SIZE", "small" if cpu else "large-v2"),
        compute_type=os.environ.get("WHISPER_COMPUTE_TYPE", "int8" if cpu else "float16"),
        batch_size=int(os.environ.get("WHISPER_BATCH_SIZE", "1" if cpu else "16")),
        segmentation_batch_size=int(os.environ.get("SEGMENTATION_BATCH_SIZE", "1" if cpu else "32")),
        embedding_batch_size=int(os.environ.get("EMBEDDING_BATCH_SIZE", "1" if cpu else "4")),
        hf_token=os.environ.get("HF_TOKEN"),
    )
