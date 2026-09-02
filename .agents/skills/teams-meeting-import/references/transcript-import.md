# Import a Teams transcript into meeting-notes

Teams transcripts have real speaker names and timestamps but no audio. The app supports transcript-only sessions: they render, summarize, and jump to timestamps normally, while omitting the player.

## Get the transcript document

The preferred input is the Stream transcript JSON:

```json
{"$schema":"http://stream.office.com/schemas/transcript.json","entries":[]}
```

Each entry has `text`, `speakerDisplayName`, `startOffset`, and `endOffset`.

Try these inputs in order:

1. **A file the user already has.** The Recap tab's Download button provides `.docx` or `.vtt`. A `.vtt` is usable but loses the speaker IDs Teams keeps in JSON, so prefer JSON when available. Convert `.vtt` into the entries shape before importing it. Extract the transcript content from `.docx` while preserving speaker names and timestamps as far as the document permits.
2. **A HAR capture.** Run the bundled extractor from the repository root:

   ```bash
   uv run --no-project .agents/skills/teams-meeting-import/scripts/har_transcript.py capture.har outdir/
   ```

   It writes `<slug>.json` for import and `<slug>.md` for review.

## If the HAR extractor finds nothing

HAR captures vary, and what they contain depends on which tabs the user opened. Inspect response bodies rather than relying only on URLs because the useful copy is often embedded in an unrelated-looking response.

Search decoded response bodies for `TranscriptJson`, `speakerDisplayName`, `spokenLanguageTag`, and `WEBVTT`. Ignore hits in `*.onecdn.static.microsoft`; those are application JavaScript bundles containing the schema, not meeting data.

Two known copies behave differently in HAR exports:

| Location | Handling |
|---|---|
| `.../drives/{driveId}/items/{itemId}/cdnmedia/transcripts` | Chrome may round-trip this opaque binary response through UTF-8, filling it with `U+FFFD`. If it contains thousands of `\xef\xbf\xbd` sequences, treat it as unrecoverable. |
| `substrate.office.com/api/beta/me/WorkingSetFiles/?$filter=...` | The same document can appear as a JSON string under `ItemProperties.Default.TranscriptJson`. This plain JSON copy survives HAR export and is the preferred source. |

To confirm the whole file was recovered, compare its compact-serialized UTF-8 byte length with the transcript `size` reported by the `?$expand=media/transcripts` response. They should match exactly.

If neither usable copy is present, ask the user to capture again with the Recap tab's **Transcript** view open or use the Recap Download button.

## Import

Run the repository importer with `--dry-run` first:

```bash
python3 scripts/import_teams_transcript.py TRANSCRIPT.json \
    --name "Meeting name" \
    --started-at 2026-07-28T07:11:51Z \
    --language zh \
    --dry-run
```

Show the user the proposed speaker mapping before the real import.

- `--started-at` sets the session ID and sort order. Take it from `RecordingStartDateTime` in the WorkingSetFiles response or from the meeting time.
- `--language` should reflect the transcript's `spokenLanguageTag`, such as `zh-cn` to `zh`.
- `--tags` must refer to tags already present in `tags.json`; unknown tags are silently dropped. Tag notes are fed to the summarizer, so choose only clearly relevant existing tags.

## Resolve speaker mappings

Teams commonly writes `Surname, Given`, while the People library uses short display names. The importer matches when a person's whole name is contained in the Teams name. This deliberately conservative behavior avoids mapping people who merely share a surname.

Nicknames and initials may not match. Look them up rather than guessing: read `~/.local/share/org.rankun.meeting-notes/people/*/profile.json` and search both `name` and `notes`, because notes often record the full name behind a handle. Force a confirmed mapping with:

```bash
--map "Surname, Given=p_xxxxxxxxxxxx"
```

Resolve every ID from the current People library. Never reuse one from a prior run or from this reference. Leave genuinely unknown speakers unlinked; they retain their Teams name and appear as unconfirmed in the UI. Do not create People entries without asking.

After approval of the mapping, rerun without `--dry-run`. Tell the user to restart the daemon because it scans `recordings/` for `metadata.json` only at startup.

## Expected output

The import writes `recordings/{session_id}/metadata.json`, `metadata.md`, `transcript.json`, and `transcript.md`. It writes no audio, no `extraction_raw.json`, and uses `sources: []`.

Every speaker needs a `speaker_embeddings` entry with an empty `embedding`. FilesDb reads that map, rather than the segments, to build the person-to-sessions index and count unconfirmed speakers even when there are no voice samples.
