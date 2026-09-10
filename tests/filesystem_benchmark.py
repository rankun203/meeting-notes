# /// script
# requires-python = ">=3.11"
# ///
"""Benchmark an isolated JSON-only copy of an existing library (no credentials).
uv run --no-project tests/filesystem_benchmark.py --source /path/to/data
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--binary', default='target/release/meeting-notes-daemon', type=Path)
    parser.add_argument('--artifacts', default='target/filesystem-benchmark', type=Path)
    parser.add_argument('--cycles', default=3, type=int)
    parser.add_argument('--scale', default=1, type=int)
    args = parser.parse_args()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='mn-benchmark-') as temporary:
        root = Path(temporary)
        for section, names in [('recordings', {'metadata.json', 'transcript.json', 'summary.json', 'todos.json'}),
                               ('people', {'profile.json', 'embeddings.json'}), ('conversations', None)]:
            for source in (args.source / section).rglob('*.json'):
                if names is not None and source.name not in names:
                    continue
                destination = root / source.relative_to(args.source)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
                if source.name == 'metadata.json':
                    metadata = json.loads(destination.read_text())
                    metadata['audio_extraction'] = None
                    metadata['state'] = 'stopped'
                    destination.write_text(json.dumps(metadata))
        if (args.source / 'tags.json').exists():
            shutil.copy2(args.source / 'tags.json', root / 'tags.json')
        # Hard links exist only within this disposable copy. Scale the library
        # without writing gigabytes of duplicate transcript data or touching source.
        originals = list((root / 'recordings').iterdir())
        for repeat in range(1, args.scale):
            for original in originals:
                if not original.is_dir() or not (original / 'metadata.json').exists():
                    continue
                destination = original.with_name(original.name + f'_scale{repeat}')
                destination.mkdir()
                for source in original.iterdir():
                    if source.name == 'metadata.json':
                        metadata = json.loads(source.read_text())
                        metadata['session_id'] = destination.name
                        (destination / source.name).write_text(json.dumps(metadata))
                    elif source.is_file():
                        os.link(source, destination / source.name)
        baseline = {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*.json')}
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        log = (args.artifacts / 'daemon.log').open('w')
        process = subprocess.Popen([str(args.binary.resolve()), 'serve', '--web-ui', '--port', str(port), '--data-dir', str(root)], stdout=log, stderr=subprocess.STDOUT)
        timings = {}

        def api(path):
            start = time.monotonic()
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/api{path}', timeout=120) as response:
                data = json.load(response)
            timings.setdefault(path.split('?')[0], []).append((time.monotonic() - start) * 1000)
            return data

        def footprint(label):
            result = subprocess.check_output(['footprint', '-p', str(process.pid)], text=True)
            (args.artifacts / f'footprint-{label}.txt').write_text(result)
            line = next(line for line in result.splitlines() if 'phys_footprint:' in line)
            print(label, line.strip(), flush=True)
            return line.strip()

        start = time.monotonic()
        try:
            deadline = start + 120
            while True:
                try:
                    sessions = api('/sessions?limit=10000')['sessions']
                    break
                except Exception:
                    if time.monotonic() > deadline:
                        raise
                    time.sleep(.1)
            startup_ms = (time.monotonic() - start) * 1000
            memory = {'startup': footprint('startup')}
            transcripts = [s['id'] for s in sessions if s['transcript_available']]
            people = api('/people')['people']
            if people:
                api('/people/' + people[0]['id'] + '/sessions')
                api('/people/' + people[0]['id'] + '/sessions')
            conversations = sorted((root / 'conversations').glob('*.json'), key=lambda p: p.stat().st_size, reverse=True)
            for cycle in range(args.cycles):
                for sid in transcripts:
                    api(f'/sessions/{sid}/transcript')
                api('/conversations')
                if conversations:
                    api('/conversations/' + conversations[0].stem)
                memory[f'cycle{cycle+1}'] = footprint(f'cycle{cycle+1}')
            assert all(p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest() == sha for p, sha in baseline.items()), 'Source JSON changed during read-only benchmark'
            def stats(values):
                values = sorted(values)
                return {'count':len(values), 'median_ms':round(values[len(values)//2],1), 'p95_ms':round(values[min(len(values)-1,int(len(values)*.95))],1), 'max_ms':round(max(values),1)}
            all_transcripts = [t for path, samples in timings.items() if path.endswith('/transcript') for t in samples]
            result = {'sessions':len(sessions), 'transcripts':len(transcripts), 'startup_including_full_list_ms':round(startup_ms,1), 'memory':memory,
                      'transcript_get':stats(all_transcripts), 'conversation_get': {path:stats(samples) for path,samples in timings.items() if '/conversations' in path},
                      'person_lookup':{path:stats(samples) for path,samples in timings.items() if '/people/' in path}, 'source_hashes_unchanged':True}
            (args.artifacts / 'results.json').write_text(json.dumps(result, indent=2))
            print(json.dumps(result, indent=2), flush=True)
        finally:
            process.send_signal(signal.SIGTERM)
            process.wait(timeout=20)
            log.close()
            assert process.returncode == 0

if __name__ == '__main__':
    main()
