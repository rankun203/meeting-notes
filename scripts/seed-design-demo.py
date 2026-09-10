#!/usr/bin/env python3
"""Seed an empty, isolated data directory with fictional meetings for UI demos.
Run: uv run --no-project scripts/seed-design-demo.py /tmp/meeting-notes-demo
"""
import json
import sys
from demo_audio import write_demo_audio
from datetime import datetime, timedelta, timezone
from pathlib import Path

root = Path(sys.argv[1]).expanduser().resolve() if len(sys.argv) == 2 else None
if root is None:
    raise SystemExit('Usage: seed-design-demo.py EMPTY_DATA_DIRECTORY')
if root.exists() and any(root.iterdir()):
    raise SystemExit('Refusing to overwrite a nonempty data directory. Choose a new demo directory.')
root.mkdir(parents=True, exist_ok=True)

def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2))

write(root / 'secrets.json', {'posthog_enabled': False})
(root / 'secrets.json').chmod(0o600)
write(root / 'settings.json', {'auto_transcribe': False, 'auto_summarize': False})
for person_index, person_name in enumerate(['Alex Morgan', 'Sam Rivera', 'Jordan Lee']):
    person_id = f'demo-person-{person_index}'
    write(root / 'people' / person_id / 'profile.json', {
        'id': person_id, 'name': person_name, 'notes': 'Fictional participant for the design demo.',
        'starred': False, 'created_at': '2026-09-07T00:00:00Z', 'updated_at': '2026-09-07T00:00:00Z',
    })

meetings = [
    ('Monday product sync', ['product', 'weekly-sync']),
    ('A better first five minutes', ['design', 'onboarding']),
    ('Customer voices · September', ['research']),
    ('Engineering catch-up', ['engineering']),
    ('Brand direction & visual language', ['design']),
    ('The next chapter', ['strategy']),
]
summary = '''## The direction is clear
The team aligned on a calmer, more useful first-run experience. The priority is helping people reach their first meaningful moment quickly, with fewer decisions along the way. [00:00-00:40]

## What we decided
- **Lead with the conversation.** Put recording at the heart of the workspace, with one clear action to get started. Keep advanced settings available when needed. [00:40-01:20]
- **Make listening effortless.** Playback should stay visible while you read. Add direct access to the moments that matter, without losing your place. [01:20-02:10]
- **Give every file a home.** Keep recordings, transcripts, and exports together in a compact panel next to the meeting. [02:10-03:00]

## A moment worth keeping
> “The best tool lets you be more present in the meeting, and makes the important parts easy to find afterward.” [03:00-03:35]

## Next steps
- [ ] **Alex** — Prototype the recording flow before Thursday’s review. [03:35-04:10]
- [ ] **Sam** — Explore the persistent player on desktop and mobile. [04:10-04:50]
- [x] **Jordan** — Share the first round of research notes. [04:50-05:20]

## Still to explore
How might we make the relationship between a summary and its source audio feel immediate? Try highlighting the cited passage during playback and make every timestamp an invitation to listen. [05:20-06:10]

## For the next conversation
Review the prototype together. Focus on the transition from recording to reading, then check how quickly someone can return to a specific moment. [06:10-07:10]
'''
phrases = [
    'Let’s start with the experience we want someone to have. You open the app, and the next step should feel obvious.',
    'The feedback is consistent: people want to spend less time managing a meeting and more time being in it.',
    'I’d make recording the primary action. One clear button, with the more detailed settings a step away.',
    'Agreed. The same idea applies afterward. Listening back should be possible wherever you are in the notes.',
    'If the player stays visible, we can move between the summary and transcript without interrupting the audio.',
    'And the files can live in their own little panel. We should be able to see the recording, the transcript, and the exports together.',
    'The best tool lets you be more present in the meeting, and makes the important parts easy to find afterward.',
    'I’ll put together the recording prototype for Thursday. Let’s keep it focused on that first minute.',
    'I can explore the player on smaller screens. It needs to be easy to reach without covering the content.',
    'The research notes are already shared. There are a few examples in there that should help us make these decisions.',
    'We could highlight the summary passage as its source audio plays. That would make the relationship much clearer.',
    'Let’s review that as a team next week. I’d like us to test the whole journey, from the first click to finding a specific moment.',
]
for idx, (name, tags) in enumerate(meetings):
    sid = f'demo-meeting-{idx + 1}'
    folder = root / 'recordings' / sid
    folder.mkdir(parents=True)
    date = (datetime(2026,9,7,0,30,tzinfo=timezone.utc) - timedelta(days=idx)).isoformat()
    write(folder / 'metadata.json', {'session_id':sid, 'name':name, 'state':'stopped', 'language':'en', 'format':'wav', 'raw_sample_rate':8000, 'created_at':date, 'updated_at':date, 'duration_secs':480, 'sources':[], 'tags':tags, 'notes':'Keep the first interaction simple. Bring the mobile player exploration to Thursday’s review.' if idx == 0 else None})
    segments = [{'start':n*40, 'end':(n+1)*40, 'text':text, 'speaker':f'SPEAKER_{n%3:02}', 'person_name':['Alex Morgan','Sam Rivera','Jordan Lee'][n%3], 'person_id':f'demo-person-{n%3}', 'words':[]} for n,text in enumerate(phrases)]
    embeddings = {f'SPEAKER_{n:02}':{'person_id':f'demo-person-{n}', 'person_name':name, 'confidence':1, 'embedding':[]} for n,name in enumerate(['Alex Morgan','Sam Rivera','Jordan Lee'])}
    write(folder / 'transcript.json', {'language':'en','segments':segments,'speaker_embeddings':embeddings})
    write(folder / 'summary.json', {'content':summary})
    (folder / 'summary.md').write_text(summary)
    if idx in [0,1,3]:
        write_demo_audio(folder / 'recording.wav')
        if idx == 0:
            write_demo_audio(folder / 'microphone.wav', phase=1)
print(f'Created {len(meetings)} fictional meetings in {root}. Analytics and automatic processing are disabled.')
print(f'cargo run -- serve --web-ui --port 33490 --data-dir {root}')
