"""Authenticated, durable single-inference-process local worker transport.

Use one process per data directory. SQLite preserves accepted inputs and results;
interrupted work re-enters the queue on restart (callbacks must be idempotent).
"""
import fcntl
import hashlib
import hmac
import json
import logging
import os
import sqlite3
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

logger = logging.getLogger(__name__)


class Jobs:
    def __init__(self, directory, handler, max_pending=100, retention=7 * 86400):
        Path(directory).mkdir(parents=True, exist_ok=True, mode=0o700)
        self.process_lock = open(Path(directory) / "worker.lock", "a")
        try:
            fcntl.flock(self.process_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.process_lock.close()
            raise RuntimeError("Worker data directory is already in use") from None
        self.db = sqlite3.connect(str(Path(directory) / "jobs.sqlite"), check_same_thread=False)
        os.chmod(Path(directory) / "jobs.sqlite", 0o600)
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, key TEXT UNIQUE, digest TEXT, input TEXT, status TEXT, result TEXT, updated REAL)")
        self.db.execute("UPDATE jobs SET status='IN_QUEUE' WHERE status='IN_PROGRESS'")
        self.db.commit()
        self.lock = threading.Lock()
        self.wake = threading.Event()
        self.stopping = threading.Event()
        self.handler = handler
        self.max_pending = max_pending
        self.retention = retention
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()

    def submit(self, event, key=None):
        serialized = json.dumps(event, sort_keys=True, separators=(",", ":"))
        digest = hashlib.sha256(serialized.encode()).hexdigest()
        with self.lock:
            self.db.execute("DELETE FROM jobs WHERE status IN ('COMPLETED','FAILED') AND updated < ?", (time.time()-self.retention,))
            if key:
                old = self.db.execute("SELECT id,digest,status FROM jobs WHERE key=?", (key,)).fetchone()
                if old:
                    if old[1] != digest:
                        raise ValueError("Idempotency-Key already used with different input")
                    return {"id": old[0], "status": old[2]}
            count = self.db.execute("SELECT COUNT(*) FROM jobs WHERE status IN ('IN_QUEUE','IN_PROGRESS')").fetchone()[0]
            if count >= self.max_pending:
                raise OverflowError("Worker queue is full")
            job_id = str(uuid.uuid4())
            self.db.execute("INSERT INTO jobs VALUES (?,?,?,?,?,?,?)", (job_id, key, digest, serialized, "IN_QUEUE", None, time.time()))
            self.db.commit()
        self.wake.set()
        return {"id": job_id, "status": "IN_QUEUE"}

    def status(self, job_id):
        with self.lock:
            row = self.db.execute("SELECT status,result FROM jobs WHERE id=?", (job_id,)).fetchone()
        if not row:
            return None
        value = {"id": job_id, "status": row[0]}
        if row[1] is not None:
            value["output" if row[0] == "COMPLETED" else "error"] = json.loads(row[1])
        return value

    def _run(self):
        while not self.stopping.is_set():
            with self.lock:
                row = self.db.execute("SELECT id,input FROM jobs WHERE status='IN_QUEUE' ORDER BY updated LIMIT 1").fetchone()
                if row:
                    self.db.execute("UPDATE jobs SET status='IN_PROGRESS',updated=? WHERE id=?", (time.time(), row[0]))
                    self.db.commit()
            if not row:
                self.wake.wait(1)
                self.wake.clear()
                continue
            try:
                event = json.loads(row[1])
                # Local transport owns delivery: extraction must return before callbacks.
                # RunPod still passes result_sink straight to its existing handler.
                sink = event["input"].pop("result_sink", None)
                output = self.handler(event)
                result, status = json.dumps(output, allow_nan=False), "COMPLETED"
            except Exception as error:
                # Exceptions can contain signed download/callback URLs; expose only type.
                logger.error("Job %s failed (%s)", row[0], type(error).__name__)
                result, status = json.dumps({"error_type": type(error).__name__, "error_message": "Worker processing failed; inspect worker configuration and logs"}), "FAILED"
            with self.lock:
                self.db.execute("UPDATE jobs SET status=?,result=?,input='{}',updated=? WHERE id=?", (status, result, time.time(), row[0]))
                self.db.commit()
            if status == "COMPLETED" and sink:
                # Commit first. Polling can recover output even if delivery fails or
                # the worker exits during the callback. Never discard computed output.
                try:
                    from audio_extraction.transfer import persist_output
                    persist_output(sink, output)
                except Exception as error:
                    logger.warning("Job %s callback failed (%s); output remains available through status",
                                   row[0], type(error).__name__)

    def close(self):
        self.stopping.set()
        self.wake.set()
        self.thread.join()
        self.db.close()
        self.process_lock.close()


def validate(event):
    if not isinstance(event, dict) or not isinstance(event.get("input"), dict):
        raise ValueError("Expected input object")
    inp = event["input"]
    tracks = inp.get("tracks")
    if not isinstance(tracks, list) or not 1 <= len(tracks) <= 32:
        raise ValueError("Expected 1 to 32 tracks")
    names = set()
    for track in tracks:
        if not isinstance(track, dict) or not all(isinstance(track.get(k), str) and track[k] for k in ("audio_url", "track_name", "source_type")):
            raise ValueError("Invalid track")
        if track["track_name"] in names:
            raise ValueError("Duplicate track name")
        names.add(track["track_name"])
        validate_url(track["audio_url"])
    if "result_sink" in inp:
        sink = inp["result_sink"]
        if not isinstance(sink, dict) or not isinstance(sink.get("token"), str) or not sink["token"]:
            raise ValueError("Invalid result sink")
        validate_url(sink.get("url", ""))


def validate_url(value):
    if not isinstance(value, str):
        raise ValueError("Invalid URL")
    parsed = urlsplit(value)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname or parsed.username or parsed.password or parsed.fragment:
        raise ValueError("Expected HTTP(S) URL without user credentials or fragment")


def create_server(host, port, token, jobs):
    if not token:
        raise ValueError("WORKER_API_TOKEN is required in HTTP mode")

    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            super().setup()
            self.connection.settimeout(15)

        def log_message(self, *_args):
            pass  # No request paths, bearer tokens, or signed URLs in access logs.

        def respond(self, code, value):
            body = json.dumps(value).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def authenticated(self):
            if not hmac.compare_digest(self.headers.get("Authorization", "").encode(), f"Bearer {token}".encode()):
                self.respond(401, {"error": "Unauthorized"})
                return False
            return True

        def do_GET(self):
            if not self.authenticated():
                return
            if self.path == "/health":
                self.respond(200, {"status": "ok", "transport": "http"})
            elif self.path.startswith("/status/"):
                value = jobs.status(self.path.removeprefix("/status/"))
                self.respond(200 if value else 404, value or {"error": "Unknown job"})
            else:
                self.respond(404, {"error": "Not found"})

        def do_POST(self):
            if not self.authenticated():
                return
            if self.path != "/run":
                self.respond(404, {"error": "Not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 1024 * 1024 or self.headers.get("Transfer-Encoding"):
                    self.respond(413, {"error": "Expected request body up to 1 MiB"})
                    return
                event = json.loads(self.rfile.read(length))
                validate(event)
                key = self.headers.get("Idempotency-Key")
                if key and len(key) > 256:
                    raise ValueError("Idempotency-Key too long")
                result = jobs.submit(event, key)
                self.respond(202, result)
            except OverflowError:
                self.respond(429, {"error": "Worker queue is full"})
            except (ValueError, TypeError):
                self.respond(400, {"error": "Invalid request or conflicting idempotency key"})

    return ThreadingHTTPServer((host, port), Handler)


def serve():
    token = os.environ.get("WORKER_API_TOKEN", "")
    if not token:
        raise ValueError("WORKER_API_TOKEN is required in HTTP mode")
    from audio_extraction.handler import handler
    jobs = Jobs(os.environ.get("WORKER_DATA_DIR", "/data"), handler,
                max_pending=int(os.environ.get("WORKER_MAX_PENDING", "100")),
                retention=int(os.environ.get("WORKER_RESULT_RETENTION_SECONDS", "604800")))
    server = create_server(os.environ.get("WORKER_HOST", "0.0.0.0"), int(os.environ.get("WORKER_PORT", "8000")), token, jobs)
    try:
        logger.info("Local HTTP worker listening on port %s", server.server_port)
        server.serve_forever()
    finally:
        server.server_close()
        jobs.close()
