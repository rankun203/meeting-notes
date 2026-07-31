#!/usr/bin/env python3
"""Pull Teams meeting transcripts out of a teams.microsoft.com HAR capture.

The Recap tab fetches the transcript twice:

  1. GET .../drives/{driveId}/items/{itemId}/cdnmedia/transcripts
     The real file. Chrome's HAR export mangles it (it is served as an
     opaque binary body, and the exporter round-trips it through UTF-8),
     so this copy is unrecoverable from a .har.

  2. GET https://substrate.office.com/api/beta/me/WorkingSetFiles/?$filter=...
     Same document inlined as a JSON *string* under
     ItemProperties.Default.TranscriptJson. Being plain JSON, it survives
     the export byte-for-byte. This is the copy we read.

Usage:
    python3 har_transcript.py capture.har outdir/
"""

import json
import re
import sys
from pathlib import Path


def offset_to_seconds(offset: str) -> float:
    """'00:01:23.4567' -> 83.4567"""
    h, m, s = offset.split(":")
    return int(h) * 3600 + int(m) * 60 + float(s)


def fmt_timestamp(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    h, m = divmod(m, 60)
    return f"{h}:{m:02d}:{s:02d}" if h else f"{m}:{s:02d}"


def find_transcripts(har_path: Path):
    """Yield (title, metadata, transcript_doc) for every transcript in the HAR."""
    har = json.loads(har_path.read_text())
    seen = set()

    for entry in har["log"]["entries"]:
        if "WorkingSetFiles" not in entry["request"]["url"]:
            continue
        body = entry["response"]["content"].get("text")
        if not body:
            continue
        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            continue

        for item in payload.get("value", []):
            props = item.get("ItemProperties", {}).get("Default", {})
            raw = props.get("TranscriptJson")
            if not raw:
                continue
            doc = json.loads(raw)
            title = item.get("Visualization", {}).get("Title") or "transcript"
            # The same transcript is fetched under several $filter queries.
            key = (title, len(doc.get("entries", [])))
            if key in seen:
                continue
            seen.add(key)
            yield title, props, doc


def to_markdown(title: str, props: dict, doc: dict) -> str:
    lines = [f"# {title}", ""]
    start, end = props.get("RecordingStartDateTime"), props.get("RecordingEndDateTime")
    if start:
        lines += [f"- Recorded: {start} - {end}"]
    lines += [
        f"- Call ID: {props.get('MeetingCallId')}",
        f"- Segments: {len(doc['entries'])}",
        "",
        "---",
        "",
    ]

    # Collapse runs of consecutive segments from the same speaker.
    speaker = None
    for seg in doc["entries"]:
        who = seg.get("speakerDisplayName") or "Unknown"
        stamp = fmt_timestamp(offset_to_seconds(seg["startOffset"]))
        if who != speaker:
            lines += ["", f"**{who}** ({stamp})", ""]
            speaker = who
        lines.append(seg["text"])

    return "\n".join(lines).strip() + "\n"


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2

    har_path, outdir = Path(sys.argv[1]), Path(sys.argv[2])
    outdir.mkdir(parents=True, exist_ok=True)

    found = 0
    for title, props, doc in find_transcripts(har_path):
        found += 1
        slug = re.sub(r"[^\w-]+", "-", title).strip("-").lower()

        json_path = outdir / f"{slug}.json"
        # Compact separators reproduce the original file byte-for-byte.
        json_path.write_text(
            json.dumps(doc, ensure_ascii=False, separators=(",", ":")),
            encoding="utf-8",
        )

        md_path = outdir / f"{slug}.md"
        md_path.write_text(to_markdown(title, props, doc), encoding="utf-8")

        print(f"{title}: {len(doc['entries'])} segments")
        print(f"  {json_path} ({json_path.stat().st_size} bytes)")
        print(f"  {md_path}")

    if not found:
        print("No transcript found. The Recap tab's Transcript view must have "
              "been opened while recording the HAR.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
