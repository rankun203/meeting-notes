---
name: teams-transcript-import
description: Import a Microsoft Teams meeting transcript into the meeting-notes app as a transcript-only session (no audio). Use when the user provides a teams.microsoft.com HAR capture, a Stream transcript JSON, or a .vtt/.docx downloaded from the Teams Recap tab, and wants it to show up in meeting-notes. Also use when asked to find or recover a Teams transcript from a HAR file.
---

# Import a Teams transcript into meeting-notes

Teams transcripts have real speaker names and timestamps but no audio. The app
supports that: sessions with a transcript and no audio files render, summarize,
and jump to timestamps normally — the player just isn't shown.

## Step 1 — Get the transcript document

The goal is the Stream transcript JSON:
`{"$schema": "http://stream.office.com/schemas/transcript.json", "entries": [...]}`

Each entry has `text`, `speakerDisplayName`, `startOffset`, `endOffset`.

Try these in order:

**A. The user already has a file.** The Recap tab's Download button gives
`.docx` or `.vtt`. A `.vtt` is fine but loses the speaker IDs Teams keeps in the
JSON — prefer the JSON if it's available. If you only have `.vtt`, convert it to
the entries shape yourself before Step 2.

**B. From a HAR capture.** Run the bundled extractor:

```bash
python3 .claude/skills/teams-transcript-import/har_transcript.py capture.har outdir/
```

It writes `<slug>.json` (the transcript document) and `<slug>.md` (readable).

## Step 2 — If the HAR extractor finds nothing

HAR captures vary — Microsoft changes endpoints, and what's in the file depends
on which tabs the user opened. Don't give up; go looking. Load the HAR and grep
response bodies rather than URLs, since the useful copy is often inlined in an
unrelated-looking response.

```python
import json, base64
har = json.load(open("capture.har"))
for i, e in enumerate(har["log"]["entries"]):
    c = e["response"]["content"]
    t = c.get("text")
    if not t:
        continue
    body = base64.b64decode(t) if c.get("encoding") == "base64" else t.encode("utf8", "replace")
    for needle in (b"TranscriptJson", b"speakerDisplayName", b"spokenLanguageTag", b"WEBVTT"):
        if needle in body:
            print(i, needle, e["request"]["url"][:120])
```

Ignore hits in `*.onecdn.static.microsoft` — those are the app's own JS bundles
shipping the schema, not meeting data.

**Two known copies, and only one survives a HAR export:**

| Where | Notes |
|---|---|
| `.../drives/{driveId}/items/{itemId}/cdnmedia/transcripts` | The real file. Served as an opaque binary body, so Chrome's HAR exporter round-trips it through UTF-8 and fills it with `U+FFFD`. **Unrecoverable** — if you see thousands of `\xef\xbf\xbd`, stop and use the other copy. |
| `substrate.office.com/api/beta/me/WorkingSetFiles/?$filter=...` | Same document inlined as a JSON string under `ItemProperties.Default.TranscriptJson`. Plain JSON, so it survives byte-for-byte. **This is the one to read.** |

To confirm you recovered the whole file, compare its compact-serialized UTF-8
byte length against the `size` reported for the transcript in the
`?$expand=media/transcripts` response — they should match exactly.

If neither copy is present, the user needs to re-capture with the Recap tab's
**Transcript** view open, or just use the Download button (option A).

## Step 3 — Import

```bash
python3 scripts/import_teams_transcript.py TRANSCRIPT.json \
    --name "Meeting name" \
    --started-at 2026-07-28T07:11:51Z \
    --language zh \
    --dry-run
```

Always `--dry-run` first and show the user the speaker mapping.

- `--started-at` sets the session ID and sort order. Take it from
  `RecordingStartDateTime` in the WorkingSetFiles response, or the meeting time.
- `--language` should be the transcript's `spokenLanguageTag` (`zh-cn` → `zh`).
- `--tags` must name tags that already exist in `tags.json`; unknown ones are
  silently dropped. Tag notes are fed to the summarizer, so picking the right
  tag measurably improves the AI overview.

## Step 4 — Fix the speaker mapping

Teams writes `"Surname, Given"`; the People library uses short display names.
The importer matches when a person's whole name is contained in the Teams name
(`Frank` ⊆ `{gu, frank}`), which is deliberately conservative — it won't map
`Will Chen` onto `Chen, Peng`.

Nicknames and initials won't match on their own — a person stored as initials
or a short handle shares no token with their Teams name. Look them up rather
than guessing: read `~/.local/share/org.rankun.meeting-notes/people/*/profile.json`
and search both `name` and `notes`, since the `notes` field often records the
full name behind a handle. Then force the mapping:

```bash
--map "Surname, Given=p_xxxxxxxxxxxx"
```

Resolve every ID from the People library at import time. Never carry one over
from a previous run or from this document — they are specific to one library.

Leave genuinely unknown speakers unlinked. They keep their Teams name, and the
web UI shows them as unconfirmed so the user can assign them in one click.
Don't invent People entries for them without asking.

Then re-run without `--dry-run`, and tell the user to restart the daemon — it
scans `recordings/` for `metadata.json` only at startup.

## What the import writes

`recordings/{session_id}/` containing `metadata.json`, `metadata.md`,
`transcript.json`, `transcript.md`. No audio, no `extraction_raw.json`, and
`sources: []`.

`speaker_embeddings` entries are written with an empty `embedding` — FilesDb
reads that map (not the segments) to build the person→sessions index and count
unconfirmed speakers, so every speaker needs an entry even though there are no
voice samples to store.
