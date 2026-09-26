"""Metadata must not initialize inference or need audio input."""
import sys
import tempfile
import threading
import types
import unittest
from unittest.mock import patch

import requests
from audio_extraction.capabilities import capabilities
from audio_extraction.handler import handler
from audio_extraction.http_worker import Jobs, create_server


def metadata_modules():
    alignment = types.ModuleType("whisperx.alignment")
    alignment.DEFAULT_ALIGN_MODELS_TORCH = {"en": "test"}
    alignment.DEFAULT_ALIGN_MODELS_HF = {"zh": "test", "fr": "test", "unknown": "test"}
    utils = types.ModuleType("whisperx.utils")
    utils.LANGUAGES = {"en": "english", "zh": "chinese", "fr": "french", "no-alignment": "excluded"}
    return {"whisperx": types.ModuleType("whisperx"), "whisperx.alignment": alignment, "whisperx.utils": utils}


class CapabilityTests(unittest.TestCase):
    def test_handler_metadata_requires_no_tracks_or_pipeline(self):
        with patch.dict(sys.modules, metadata_modules()), patch.dict("os.environ", {"WHISPER_MODEL_SIZE": "small"}), patch("audio_extraction.handler.get_pipeline", side_effect=AssertionError("inference initialized")):
            result = handler({"input": {"operation": "capabilities"}})
        self.assertEqual(result["protocolVersion"], 1)
        languages = result["transcription"]["languages"]
        self.assertEqual({item["code"] for item in languages}, {"en", "fr", "zh", "zh-cn", "zh-tw"})
        self.assertTrue(all(item["name"] for item in languages))

    def test_english_only_model_filters_catalog(self):
        with patch.dict(sys.modules, metadata_modules()), patch.dict("os.environ", {"WHISPER_MODEL_SIZE": "small.en"}):
            self.assertEqual(capabilities()["transcription"]["languages"], [{"code": "en", "name": "English"}])

    def test_missing_dependencies_do_not_invent_catalog(self):
        with patch.dict(sys.modules, {"whisperx.alignment": None}):
            with self.assertRaises(ImportError):
                capabilities()

    def test_http_metadata_is_authenticated_and_does_not_queue_work(self):
        with tempfile.TemporaryDirectory() as directory:
            jobs = Jobs(directory, lambda _: self.fail("inference called"))
            server = create_server("127.0.0.1", 0, "test", jobs)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                url = f"http://127.0.0.1:{server.server_port}/capabilities"
                self.assertEqual(requests.get(url).status_code, 401)
                with patch.dict(sys.modules, metadata_modules()):
                    response = requests.get(url, headers={"Authorization": "Bearer test"})
                    self.assertEqual(response.status_code, 200)
                    self.assertEqual(response.json()["protocolVersion"], 1)
                with patch("audio_extraction.capabilities.capabilities", side_effect=RuntimeError("private details")):
                    response = requests.get(url, headers={"Authorization": "Bearer test"})
                    self.assertEqual(response.status_code, 503)
                    self.assertNotIn("private details", response.text)
            finally:
                server.shutdown()
                server.server_close()
                jobs.close()
