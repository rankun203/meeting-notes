# file-drop

Temporary file parking server. Upload once, download once, auto-expire.

Designed for ephemeral file transfer between services — e.g., parking audio files for a GPU worker to download.

## Quick start

```bash
# Direct
cargo run -- --api-key YOUR_SECRET_KEY

# Docker Compose
API_KEY=YOUR_SECRET_KEY docker compose up
```

## API

### Upload a file

```bash
curl -X POST 'http://localhost:8199/upload?filename=meeting.opus' \
  -H "Authorization: Bearer YOUR_SECRET_KEY" \
  --data-binary @meeting.opus
```

Response:
```json
{
  "id": "ec4ae724-9201-42ee-8447-0c84cb1efed2",
  "url": "/d/ec4ae724-9201-42ee-8447-0c84cb1efed2",
  "filename": "meeting.opus",
  "size": 8421376,
  "size_human": "8.0 MB",
  "expires_in_secs": 600
}
```

API key can also be passed as a query parameter: `?api_key=YOUR_SECRET_KEY`

### Download a file

```bash
curl -o meeting.opus http://localhost:8199/d/ec4ae724-9201-42ee-8447-0c84cb1efed2
```

No API key required for downloads. File is **deleted immediately** after one successful download.

### Storage info

```bash
curl http://localhost:8199/info
```

Response:
```json
{
  "storage_dir": "/data/storage",
  "file_count": 1,
  "total_size_bytes": 8421376,
  "total_size_human": "8.0 MB",
  "free_space_bytes": 304546349056,
  "free_space_human": "283.6 GB",
  "max_file_size_bytes": 104857600,
  "max_file_size_human": "100.0 MB",
  "allowed_extensions": ["mp3", "opus"],
  "expiry_secs": 600
}
```

### Health check

```bash
curl http://localhost:8199/health
# {"status":"ok"}
```

## Error responses

| Status | When |
|--------|------|
| 400 | Bad extension, file too large, empty file, missing filename |
| 401 | Missing or invalid API key |
| 404 | File not found or already downloaded |

Example:
```json
{"error": "File extension 'exe' not allowed. Allowed: [\"mp3\", \"opus\"]"}
```

## CLI options

| Flag | Default | Description |
|------|---------|-------------|
| `--api-key` | (required) | Secret key for uploads |
| `--port` | `8199` | Port to listen on |
| `--host` | `0.0.0.0` | Host to bind to |
| `--storage-dir` | `./storage` | Directory for parked files |
| `--max-size` | `104857600` (100MB) | Max file size in bytes |
| `--ext` | `mp3,opus` | Allowed extensions (comma-separated) |
| `--expiry-secs` | `600` (10min) | Auto-delete after this many seconds |

## Behavior

- Files are deleted after **one download** or after **expiry** (default 10 min), whichever comes first
- Uploads stream to disk — large files don't consume RAM
- Size limit enforced both via `Content-Length` header (early reject) and mid-stream (kills upload if exceeded)
- Storage info printed to logs after every upload/download/expiry
