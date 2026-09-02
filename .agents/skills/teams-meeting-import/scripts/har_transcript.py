#!/usr/bin/env python3
"""Extract Teams meeting transcripts from a teams.microsoft.com HAR capture.

Usage:
    uv run --no-project har_transcript.py capture.har outdir/
"""

import json
import re
import sys
from pathlib import Path


def offset_to_seconds(offset: str) -> float:
    """Convert a Teams timestamp such as '00:01:23.4567' to seconds."""
    hours, minutes, seconds = offset.split(":")
    return int(hours) * 3600 + int(minutes) * 60 + float(seconds)


def fmt_timestamp(seconds: float) -> str:
    minutes, seconds = divmod(int(seconds), 60)
    hours, minutes = divmod(minutes, 60)
    return f"{hours}:{minutes:02d}:{seconds:02d}" if hours else f"{minutes}:{seconds:02d}"


def find_transcripts(har_path: Path):
    """Yield (title, metadata, transcript_doc) for transcripts in a HAR."""
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
            key = (title, len(doc.get("entries", [])))
            if key in seen:
                continue
            seen.add(key)
            yield title, props, doc


def to_markdown(title: str, props: dict, doc: dict) -> str:
    lines = [f"# {title}", ""]
    start = props.get("RecordingStartDateTime")
    end = props.get("RecordingEndDateTime")
    if start:
        lines.append(f"- Recorded: {start} - {end}")
    lines += [
        f"- Call ID: {props.get('MeetingCallId')}",
        f"- Segments: {len(doc['entries'])}",
        "",
        "---",
        "",
    ]

    speaker = None
    for segment in doc["entries"]:
        who = segment.get("speakerDisplayName") or "Unknown"
        stamp = fmt_timestamp(offset_to_seconds(segment["startOffset"]))
        if who != speaker:
            lines += ["", f"**{who}** ({stamp})", ""]
            speaker = who
        lines.append(segment["text"])

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
        print(
            "No transcript found. Open the Recap tab's Transcript view while "
            "recording the HAR.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
