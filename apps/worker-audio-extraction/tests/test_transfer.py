"""Transfer regressions run without GPU/model imports."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import requests
from audio_extraction.transfer import download_audio, persist_output


def response(chunks=(), status=200):
    result = Mock()
    result.__enter__ = Mock(return_value=result)
    result.__exit__ = Mock(return_value=False)
    result.iter_content.return_value = iter(chunks)
    result.status_code = status
    if status >= 400:
        result.raise_for_status.side_effect = requests.HTTPError(response=result)
    return result


class TransferTests(unittest.TestCase):
    @patch('audio_extraction.transfer.time.sleep')
    @patch('audio_extraction.transfer.requests.get')
    def test_interrupted_body_restarts_and_removes_partial_file(self, get, sleep):
        def broken():
            yield b'partial'
            raise requests.exceptions.ChunkedEncodingError('broken stream')
        first = response()
        first.iter_content.return_value = broken()
        get.side_effect = [first, response([b'complete'])]
        with tempfile.TemporaryDirectory() as directory:
            path = download_audio('https://example.org/audio.opus', directory=directory)
            self.assertEqual(Path(path).read_bytes(), b'complete')
            self.assertEqual(len(list(Path(directory).iterdir())), 1)
        self.assertEqual(get.call_count, 2)
        first.__exit__.assert_called_once()

    @patch('audio_extraction.transfer.time.sleep')
    @patch('audio_extraction.transfer.requests.get')
    def test_retry_limit_and_cleanup(self, get, sleep):
        def failed_response():
            result = response()
            result.iter_content.side_effect = requests.Timeout('timeout')
            return result
        get.side_effect = [failed_response() for _ in range(4)]
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(requests.Timeout):
                download_audio('https://example.org/audio.opus', directory=directory)
            self.assertEqual(list(Path(directory).iterdir()), [])
        self.assertEqual(get.call_count, 4)

    @patch('audio_extraction.transfer.requests.get')
    def test_not_found_is_not_retried(self, get):
        get.return_value = response(status=404)
        with self.assertRaises(requests.HTTPError):
            download_audio('https://example.org/audio.opus')
        get.assert_called_once()

    @patch('audio_extraction.transfer.time.sleep')
    @patch('audio_extraction.transfer.requests.post')
    def test_persistence_retries_same_idempotent_output(self, post, sleep):
        post.side_effect = [response(status=503), response()]
        sink = {'url': 'https://example.org/result', 'token': 'secret'}
        persist_output(sink, {'tracks': {}})
        self.assertEqual(post.call_count, 2)
        self.assertEqual(post.call_args_list[0], post.call_args_list[1])
        self.assertEqual(post.call_args.kwargs['json'],
                         {'type': 'TRANSCRIPT_OUTPUT', 'body': {'tracks': {}}})

    @patch('audio_extraction.transfer.requests.post')
    def test_persistence_failure_redacts_capability(self, post):
        post.return_value = response(status=401)
        with self.assertRaisesRegex(RuntimeError, '^Result persistence failed \\(HTTPError\\)$'):
            persist_output({'url': 'https://example.org/secret', 'token': 'secret'}, {})
        post.assert_called_once()

    @patch('audio_extraction.transfer.time.sleep')
    def test_real_http_truncated_content_length_recovers(self, sleep):
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        from threading import Thread

        class TruncatedFirstResponse(BaseHTTPRequestHandler):
            calls = 0

            def do_GET(self):
                type(self).calls += 1
                self.send_response(200)
                self.send_header('Content-Length', '10')
                self.end_headers()
                self.wfile.write(b'123' if self.calls == 1 else b'0123456789')
                self.close_connection = True

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(('127.0.0.1', 0), TruncatedFirstResponse)
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as directory:
                path = download_audio(f'http://127.0.0.1:{server.server_port}/audio.opus', directory=directory)
                self.assertEqual(Path(path).read_bytes(), b'0123456789')
                self.assertEqual(len(list(Path(directory).iterdir())), 1)
            self.assertEqual(TruncatedFirstResponse.calls, 2)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
