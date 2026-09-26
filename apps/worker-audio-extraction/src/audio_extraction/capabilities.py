"""Audio-free metadata derived from the installed recognition and alignment stack."""

from audio_extraction.config import pipeline_options


def transcription_languages():
    # Import metadata only: do not instantiate recognition/alignment models or
    # download their weights. Missing dependencies must fail discovery honestly.
    from whisperx.alignment import DEFAULT_ALIGN_MODELS_HF, DEFAULT_ALIGN_MODELS_TORCH
    from whisperx.utils import LANGUAGES

    codes = set(LANGUAGES)
    codes &= set(DEFAULT_ALIGN_MODELS_HF) | set(DEFAULT_ALIGN_MODELS_TORCH)
    if pipeline_options()["model_size"].endswith(".en"):
        codes &= {"en"}
    languages = [{"code": code, "name": LANGUAGES[code].title()} for code in sorted(codes)]
    if "zh" in codes:
        languages += [
            {"code": "zh-cn", "name": "Chinese (Simplified)"},
            {"code": "zh-tw", "name": "Chinese (Traditional)"},
        ]
    if not languages:
        raise RuntimeError("No supported transcription languages are available")
    return sorted(languages, key=lambda item: item["name"])


def capabilities():
    return {"protocolVersion": 1, "transcription": {"languages": transcription_languages()}}
