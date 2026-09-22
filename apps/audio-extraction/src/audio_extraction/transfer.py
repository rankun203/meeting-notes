"""Retry complete transfers, including failures while consuming streamed bodies."""

import logging
import os
import tempfile
import time

import requests

logger = logging.getLogger(__name__)
ATTEMPTS = 4
RETRY_STATUSES = {408, 429, 500, 502, 503, 504}


def _retryable(exc: requests.RequestException) -> bool:
    if isinstance(exc, requests.HTTPError):
        return exc.response is not None and exc.response.status_code in RETRY_STATUSES
    return isinstance(exc, (requests.ConnectionError, requests.Timeout,
                            requests.exceptions.ChunkedEncodingError))


def download_audio(url: str, suffix: str = ".audio", directory: str | None = None) -> str:
    """Download from byte zero on retry; never expose partial files to the decoder."""
    for attempt in range(ATTEMPTS):
        path = None
        complete = False
        try:
            with requests.get(url, timeout=(15, 60), stream=True) as response:
                response.raise_for_status()
                with tempfile.NamedTemporaryFile(delete=False, suffix=suffix, dir=directory) as tmp:
                    path = tmp.name
                    for chunk in response.iter_content(chunk_size=65536):
                        tmp.write(chunk)
                complete = True
                return path
        except requests.RequestException as exc:
            if attempt == ATTEMPTS - 1 or not _retryable(exc):
                raise
            # Do not log URLs or exception messages: they may contain signed credentials.
            logger.warning("Audio download interrupted (%s); retry %d/%d",
                           type(exc).__name__, attempt + 2, ATTEMPTS)
        finally:
            # A successful return leaves ownership with the caller/job temp directory.
            if path is not None and not complete:
                os.unlink(path)
        time.sleep(2 ** attempt)
    raise AssertionError("unreachable")


def persist_output(sink: dict, output: dict) -> None:
    """Idempotent sink must upsert TRANSCRIPT_OUTPUT before acknowledging success."""
    for attempt in range(ATTEMPTS):
        try:
            with requests.post(
                sink["url"], headers={"Authorization": f"Bearer {sink['token']}"},
                json={"type": "TRANSCRIPT_OUTPUT", "body": output}, timeout=(15, 60),
            ) as response:
                response.raise_for_status()
            return
        except requests.RequestException as exc:
            if attempt == ATTEMPTS - 1 or not _retryable(exc):
                # Avoid leaking the callback capability through RunPod's error response.
                raise RuntimeError(f"Result persistence failed ({type(exc).__name__})") from None
            logger.warning("Result persistence interrupted (%s); retry %d/%d",
                           type(exc).__name__, attempt + 2, ATTEMPTS)
            time.sleep(2 ** attempt)
