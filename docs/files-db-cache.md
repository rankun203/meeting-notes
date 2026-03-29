# FilesDb: In-Memory Cache for Session Data

## Principle: User folder is the single source of truth

All session data lives as plain files in the user's data directory:

```
~/.local/share/org.rankun.meeting-notes/recordings/
  {session_id}/
    metadata.json           # session config, state, sources
    system_audio.opus       # recorded audio track
    system_microphone.opus  # recorded audio track
    transcript.json         # transcription result + speaker attributions
    extraction_raw.json     # raw extraction service response
    *.waveform.json         # generated waveform data (per track)
```

These files are human-readable, portable, and can be edited externally. The daemon never stores authoritative data anywhere else. If the cache is lost (process restart), it's rebuilt by scanning the files.

## What FilesDb does

`FilesDb` (`src/filesdb.rs`) is a **read cache with write-through** for transcript data. It exists purely for performance — to avoid reading and parsing `transcript.json` from disk on every API request.

### What it caches

- **Transcript data**: The full parsed `serde_json::Value` of each session's `transcript.json`, keyed by session ID.
- **Derived indexes**: A `person_id → Set<session_id>` reverse index, built from `speaker_embeddings` in each transcript.
- **Derived counts**: Per-session count of unconfirmed speakers (speakers without a `person_id`).

### What it does NOT cache

- Session metadata (managed by `SessionManager` which has its own in-memory state)
- Audio files (served directly from disk via `tower_http::ServeFile`)
- Waveform data (has its own file-level caching in `waveform.rs`)
- People data (managed by `PeopleManager`)

## Data flow

### Startup (cold cache)

```
Daemon starts
  → SessionManager.load_from_disk()    # loads metadata.json for each session
  → FilesDb.load_from_disk()           # scans for transcript.json in each session dir
    → For each transcript.json found:
      1. Read file from disk
      2. Parse JSON
      3. Extract speaker_embeddings → build person_id index
      4. Count unconfirmed speakers
      5. Store in memory
```

After startup, the cache is warm. All transcript reads are served from memory.

### Read path

```
GET /api/sessions/{id}/transcript
  → FilesDb.get_transcript(id)
  → Return cloned Value from HashMap (no disk I/O)

GET /api/people/{id}/sessions
  → FilesDb.get_person_session_ids(person_id)
  → Return Vec<session_id> from index (no disk I/O, no file scanning)

GET /api/sessions (list)
  → SessionManager.list_sessions()           # session metadata from memory
  → FilesDb.unconfirmed_speakers(id)         # per-session count from cache
```

### Write path (write-through)

Every write goes to **disk first, then cache**. If the disk write fails, the cache is not updated. This ensures the files always reflect the latest state.

```
Transcription completes (run_transcription_pipeline):
  → FilesDb.put_transcript(session_id, data)
    1. serde_json::to_string_pretty(data) → write to transcript.json on disk
    2. Parse speaker_embeddings → rebuild person_id index for this session
    3. Store parsed Value in cache

Speaker attribution updated (update_attribution):
  → FilesDb.get_transcript(id)               # read current from cache
  → Mutate the Value (update person_id, person_name, etc.)
  → FilesDb.put_transcript(id, mutated_data) # write-through to disk + cache

Transcript deleted (delete_transcript):
  → FilesDb.remove_transcript(id)            # remove from cache + index
  → fs::remove_file(transcript.json)         # remove from disk

Session deleted (delete_session):
  → FilesDb.remove_transcript(id)            # clean cache first
  → SessionManager.delete_session(id)        # removes entire session directory
```

### Write ordering guarantee

```
put_transcript:
  1. Write to disk    ← if this fails, error returned, cache unchanged
  2. Update cache     ← only happens after successful disk write
  3. Update indexes   ← derived from the same data just cached
```

If the process crashes between step 1 and 2, the file is on disk but the cache is stale. On next startup, `load_from_disk()` rebuilds the cache from the files — no data loss.

## Cache invalidation

The cache uses **explicit invalidation** — no file watchers, no TTLs, no polling.

This works because all writes to `transcript.json` go through the daemon:
- Transcription pipeline → `put_transcript`
- Attribution updates → `get_transcript` + mutate + `put_transcript`
- Deletion → `remove_transcript`

No external process is expected to modify these files while the daemon is running. If a user manually edits a `transcript.json` while the daemon is running, the cache will be stale until restart. This is an acceptable trade-off for the simplicity of not running a file watcher.

## Memory usage

Each `transcript.json` is stored as a parsed `serde_json::Value`. For a typical meeting (1-2 hours, 100-500 transcript segments), this is roughly 200KB-2MB per session.

| Sessions | Estimated memory |
|----------|-----------------|
| 10       | 2-20 MB         |
| 100      | 20-200 MB       |
| 1000     | 200 MB - 2 GB   |

For a desktop app with dozens to low hundreds of sessions, this is well within acceptable bounds. If it becomes a concern, a future optimization could cache only the indexes and serve transcript content from disk on demand.

## Concurrency

- Uses `tokio::sync::RwLock` — multiple concurrent reads don't block each other
- Writes acquire an exclusive lock briefly (just a HashMap insert)
- The transcription pipeline writes once per session; attribution updates are infrequent
- No contention in practice since reads vastly outnumber writes

## What happens if files are modified externally

| Scenario | Behavior |
|----------|----------|
| File edited while daemon running | Cache is stale until restart |
| File deleted while daemon running | Cache returns stale data; disk 404 on file serve |
| New file added while daemon running | Not visible until restart |
| Daemon restart | Full cache rebuild from disk — all changes picked up |

The files on disk are always the source of truth. The cache is a performance optimization that is rebuilt from scratch on every daemon start.
