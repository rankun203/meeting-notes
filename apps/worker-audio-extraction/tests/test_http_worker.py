"""Local transport tests execute real HTTP and SQLite without downloading models."""
import json
import sqlite3
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import requests
from audio_extraction.config import pipeline_options
from audio_extraction.http_worker import Jobs, create_server, validate

EVENT = {"input": {"tracks": [{"audio_url": "http://server:3000/d/audio.opus", "track_name": "mic", "source_type": "mic"}], "diarize": False}}


def finished(jobs, job_id):
    for _ in range(200):
        result = jobs.status(job_id)
        if result["status"] in {"COMPLETED", "FAILED"}:
            return result
        time.sleep(.01)
    raise AssertionError("job did not finish")


class LocalWorkerTests(unittest.TestCase):
    def test_cpu_defaults_and_overrides(self):
        with patch.dict('os.environ', {"WHISPER_DEVICE": "cpu"}, clear=True):
            self.assertEqual(pipeline_options()["compute_type"], "int8")
            self.assertEqual(pipeline_options()["model_size"], "small")
            self.assertEqual(pipeline_options()["batch_size"], 1)
        with patch.dict('os.environ', {"WHISPER_DEVICE": "cpu", "WHISPER_MODEL_SIZE": "tiny", "WHISPER_BATCH_SIZE": "2"}, clear=True):
            self.assertEqual(pipeline_options()["model_size"], "tiny")
            self.assertEqual(pipeline_options()["batch_size"], 2)
        with patch.dict('os.environ', {}, clear=True):
            self.assertEqual(pipeline_options()["device"], "cuda")

    def test_authenticated_async_http_idempotency_and_persistence(self):
        started, release = threading.Event(), threading.Event()
        def handler(event):
            self.assertEqual(event, EVENT)
            started.set()
            release.wait(5)
            return {"tracks": {"mic": {"segments": [{"text": "test"}]}}, "model": "fixture"}
        with tempfile.TemporaryDirectory() as directory:
            jobs = Jobs(directory, handler, max_pending=1)
            server = create_server('127.0.0.1', 0, 'test-secret', jobs)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            base = f'http://127.0.0.1:{server.server_port}'
            headers = {"Authorization": "Bearer test-secret", "Idempotency-Key": "task-1"}
            try:
                self.assertEqual(requests.post(base+'/run', json=EVENT).status_code, 401)
                bad = requests.post(base+'/run', headers=headers, json={"input":{"tracks":[]}})
                self.assertEqual(bad.status_code, 400)
                response = requests.post(base+'/run', headers=headers, json=EVENT)
                self.assertEqual(response.status_code, 202)
                job_id = response.json()['id']
                self.assertTrue(started.wait(2))
                self.assertEqual(requests.get(base+'/status/'+job_id, headers=headers).json()['status'], 'IN_PROGRESS')
                self.assertEqual(requests.post(base+'/run', headers=headers, json=EVENT).json()['id'], job_id)
                different = {"input": {**EVENT['input'], "language": "fr"}}
                self.assertEqual(requests.post(base+'/run', headers=headers, json=different).status_code, 400)
                self.assertEqual(requests.post(base+'/run', headers={"Authorization":"Bearer test-secret"}, json=EVENT).status_code, 429)
                self.assertEqual(requests.get(base+'/status/missing', headers=headers).status_code, 404)
                release.set()
                self.assertEqual(finished(jobs, job_id)['output']['model'], 'fixture')
            finally:
                release.set()
                server.shutdown()
                server.server_close()
                thread.join()
                jobs.close()
            jobs = Jobs(directory, lambda _: self.fail('completed job must not rerun'))
            self.assertEqual(jobs.status(job_id)['status'], 'COMPLETED')
            self.assertEqual(jobs.submit(EVENT, 'task-1')['id'], job_id)
            jobs.close()

    def test_interrupted_job_requeues_and_failure_hides_secrets(self):
        with tempfile.TemporaryDirectory() as directory:
            jobs = Jobs(directory, lambda _: None)
            jobs.close()
            db = sqlite3.connect(str(Path(directory)/'jobs.sqlite'))
            with db:
                db.execute("INSERT INTO jobs VALUES (?,?,?,?,?,?,?)", ('interrupted', None, 'hash', json.dumps(EVENT), 'IN_PROGRESS', None, time.time()))
            db.close()
            def fail(_):
                raise RuntimeError('https://server/callback?secret=never-leak')
            jobs = Jobs(directory, fail)
            result = finished(jobs, 'interrupted')
            self.assertEqual(result['status'], 'FAILED')
            self.assertNotIn('never-leak', json.dumps(result))
            jobs.close()

    def test_callback_exhaustion_keeps_output_for_polling_after_restart(self):
        output = {"tracks": {"mic": {"segments": [{"text": "Recovered transcript"}]}}, "model": "fixture"}
        event = {"input": {**EVENT['input'], "result_sink": {"url": "http://server/callback", "token": "private-sink-token"}}}
        def extract(inp):
            self.assertNotIn('result_sink', inp['input'])
            return output
        with tempfile.TemporaryDirectory() as directory:
            with patch('audio_extraction.transfer.requests.post', side_effect=requests.ConnectionError('private-sink-token')) as callback, patch('audio_extraction.transfer.time.sleep'):
                jobs = Jobs(directory, extract)
                job_id = jobs.submit(event, 'callback-task')['id']
                self.assertEqual(finished(jobs, job_id)['output'], output)
                # Closing joins the inference thread, including all callback attempts.
                jobs.close()
                self.assertEqual(callback.call_count, 4)
                self.assertEqual(callback.call_args.kwargs['json'], {"type": "TRANSCRIPT_OUTPUT", "body": output})
            jobs = Jobs(directory, lambda _: self.fail('successful inference must not repeat'))
            server = create_server('127.0.0.1', 0, 'test-secret', jobs)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                response = requests.get(f'http://127.0.0.1:{server.server_port}/status/{job_id}', headers={"Authorization": "Bearer test-secret"})
                self.assertEqual(response.status_code, 200)
                self.assertEqual(response.json(), {"id": job_id, "status": "COMPLETED", "output": output})
                self.assertEqual(jobs.submit(event, 'callback-task')['id'], job_id)
                self.assertNotIn('private-sink-token', response.text)
            finally:
                server.shutdown()
                server.server_close()
                thread.join()
                jobs.close()

    def test_retention_and_exclusive_volume_ownership(self):
        with tempfile.TemporaryDirectory() as directory:
            jobs = Jobs(directory, lambda _: {"ok": True}, retention=10)
            try:
                with self.assertRaises(RuntimeError):
                    Jobs(directory, lambda _: None)
                first = jobs.submit(EVENT, 'old-job')['id']
                finished(jobs, first)
                with jobs.lock:
                    jobs.db.execute("UPDATE jobs SET updated=? WHERE id=?", (time.time()-20, first))
                    jobs.db.commit()
                second = jobs.submit(EVENT, 'new-job')['id']
                self.assertIsNone(jobs.status(first))
                self.assertEqual(finished(jobs, second)['status'], 'COMPLETED')
            finally:
                jobs.close()

    def test_track_limit_matches_server(self):
        event = {"input": {"tracks": [{**EVENT['input']['tracks'][0], 'track_name': str(i)} for i in range(32)]}}
        validate(event)
        event['input']['tracks'].append({**event['input']['tracks'][0], 'track_name': 'extra'})
        with self.assertRaises(ValueError):
            validate(event)

    def test_missing_auth_token_rejected(self):
        with self.assertRaises(ValueError):
            create_server('127.0.0.1', 0, '', None)
