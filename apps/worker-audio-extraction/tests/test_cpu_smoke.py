"""Opt-in real inference against a short speech fixture (models download on first run)."""
import os
import unittest
from unittest.mock import patch


@unittest.skipUnless(os.environ.get('WORKER_CPU_SMOKE_AUDIO'), 'set WORKER_CPU_SMOKE_AUDIO to a short speech WAV/AIFF')
class CPUInferenceSmoke(unittest.TestCase):
    def test_real_transcription_and_word_alignment(self):
        from audio_extraction.config import pipeline_options
        from audio_extraction.pipeline import TranscriptionPipeline
        with patch.dict(os.environ, {'WHISPER_DEVICE': 'cpu', 'WHISPER_MODEL_SIZE': 'tiny', 'WHISPER_COMPUTE_TYPE': 'int8', 'WHISPER_BATCH_SIZE': '1'}):
            pipeline = TranscriptionPipeline(**pipeline_options())
        result = pipeline.process_track(os.environ['WORKER_CPU_SMOKE_AUDIO'], language='en', diarize=False)
        self.assertEqual(pipeline.device, 'cpu')
        self.assertTrue(result['segments'], 'expected speech transcript')
        words = [word for segment in result['segments'] for word in segment.get('words', [])]
        self.assertTrue(any('start' in word and 'end' in word for word in words), 'expected aligned word timestamps')
