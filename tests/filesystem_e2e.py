# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Isolated daemon/API/browser regression test.

Run: uv run --no-project tests/filesystem_e2e.py [--binary target/release/meeting-notes-daemon]
No production credentials, audio devices, or external AI services are used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import threading
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.new')
    temporary.write_text(json.dumps(value))
    temporary.replace(path)


def fixture(root, sid, index=0):
    directory = root / 'recordings' / sid
    write(directory / 'metadata.json', {
        'session_id': sid, 'name': f'Meeting {index:03}', 'state': 'stopped',
        'language': 'en', 'created_at': f'2026-09-{index % 28 + 1:02}T00:00:00Z',
        'updated_at': '2026-09-01T00:00:00Z', 'duration_secs': 60,
        'tags': ['work'], 'notes': None, 'extension': {'keep': True},
    })
    write(directory / 'transcript.json', {
        'segments': [{'start': 0, 'end': 60, 'text': f'Transcript {index:03}',
                      'speaker': 'speaker_0', 'person_id': 'person1', 'person_name': 'Test Person',
                      'words': [{'word': 'hello', 'start': 0, 'end': 1}]}],
        'speaker_embeddings': {'speaker_0': {'person_id': 'person1', 'confidence': 0.9}},
    })
    write(directory / 'summary.json', {'content': f'Summary {index:03}', 'extension': 'keep'})
    write(directory / 'todos.json', {'items': [{'text': 'Test action', 'completed': False, 'person_id': 'person1'}]})


def hashes(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for section in ['recordings', 'people', 'conversations']
            for p in (root / section).rglob('*.json')}


def wait_for(fn, timeout=10):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            result = fn()
            if result:
                return result
        except Exception as error:
            last = error
        time.sleep(0.1)
    raise AssertionError(f'Timed out: {last}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', default='target/release/meeting-notes-daemon')
    parser.add_argument('--artifacts', default='target/filesystem-e2e')
    args = parser.parse_args()
    binary = Path(args.binary).resolve()
    artifacts = Path(args.artifacts).resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='meeting-notes-e2e-') as tmp:
        root = Path(tmp)
        for index in range(75):
            fixture(root, f'session{index:03}', index)
        write(root / 'people/person1/profile.json', {
            'id': 'person1', 'name': 'Test Person', 'created_at': '2026-09-01T00:00:00Z',
            'updated_at': '2026-09-01T00:00:00Z', 'notes': 'Person notes', 'extension': 'keep',
        })
        write(root / 'tags.json', {'tags': [{'name': 'work', 'hidden': False}]})
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        origin = f'http://127.0.0.1:{port}'
        elapsed = []

        def api(path, method='GET', body=None, status=200):
            started = time.monotonic()
            request = urllib.request.Request(origin + '/api' + path, method=method,
                        data=None if body is None else json.dumps(body).encode(),
                        headers={'Content-Type': 'application/json'})
            try:
                response = urllib.request.urlopen(request, timeout=30)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                data = response.read()
                assert response.status == status, (path, response.status, data[:300])
            elapsed.append((path, (time.monotonic() - started) * 1000))
            return json.loads(data) if data else None

        prompts = []
        class MockLLM(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_POST(self):
                prompts.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.end_headers()
                payload = {'choices': [{'delta': {'content': 'Local test answer'}, 'finish_reason': 'stop'}]}
                self.wfile.write(('data: ' + json.dumps(payload) + '\n\ndata: [DONE]\n\n').encode())
        llm = ThreadingHTTPServer(('127.0.0.1', 0), MockLLM)
        threading.Thread(target=llm.serve_forever, daemon=True).start()
        write(root / 'secrets.json', {'api_keys': {'127.0.0.1': 'isolated-test-key'}, 'posthog_enabled': False})
        baseline = hashes(root)
        log = (artifacts / 'daemon.log').open('w')
        process = subprocess.Popen([str(binary), 'serve', '--web-ui', '--port', str(port), '--data-dir', str(root)],
                                   stdout=log, stderr=subprocess.STDOUT)
        started = time.monotonic()
        try:
            wait_for(lambda: api('/sessions?limit=1')['total'] == 75)
            startup_ms = (time.monotonic() - started) * 1000
            page1 = api('/sessions?limit=50&offset=0')
            page2 = api('/sessions?limit=50&offset=50')
            assert len(page1['sessions']) == 50 and len(page2['sessions']) == 25
            assert not ({s['id'] for s in page1['sessions']} & {s['id'] for s in page2['sessions']})
            assert len(api('/people/person1/sessions')['sessions']) == 75
            assert len(api('/people/person1/todos')['todos']) == 75
            assert api('/sessions/session000/transcript')['segments'][0]['text'] == 'Transcript 000'
            assert api('/sessions/session000/summary')['content'] == 'Summary 000'
            assert hashes(root) == baseline, 'Startup/read APIs changed source JSON'
            print('PASS: startup, pagination, source hashes, transcript/summary/person APIs', flush=True)

            directory = root / 'recordings/session000'
            transcript = json.loads((directory / 'transcript.json').read_text())
            transcript['segments'][0]['text'] = 'Externally revised transcript'
            transcript['speaker_embeddings']['speaker_0']['person_id'] = 'person2'
            write(directory / 'transcript.json', transcript)
            assert api('/sessions/session000/transcript')['segments'][0]['text'] == 'Externally revised transcript'
            assert len(api('/people/person1/sessions')['sessions']) == 74
            assert len(api('/people/person2/sessions')['sessions']) == 1
            metadata = json.loads((directory / 'metadata.json').read_text())
            metadata['name'] = 'Externally renamed meeting'
            metadata['notes'] = 'External notes'
            write(directory / 'metadata.json', metadata)
            assert api('/sessions/session000')['name'] == 'Externally renamed meeting'
            api('/sessions/session000', 'PATCH', {'notes': 'Stale draft', 'previous_notes': None}, status=409)
            api('/sessions/session000', 'PATCH', {'notes': 'New notes', 'previous_notes': 'External notes'})
            saved = json.loads((directory / 'metadata.json').read_text())
            assert saved['name'] == 'Externally renamed meeting' and saved['extension'] == {'keep': True}
            assert saved['notes'] == 'New notes'
            api('/sessions/session000/summary', 'PATCH', {'content': 'Updated summary'})
            assert json.loads((directory / 'summary.json').read_text())['extension'] == 'keep'
            api('/sessions/session000/todos/0', 'PATCH', {})
            assert api('/sessions/session000/todos')['items'][0]['completed'] is True
            print('PASS: external edits, person-index invalidation, stale-write rejection, preserved extensions, todos', flush=True)

            fixture(root, 'imported', 76)
            assert api('/sessions?limit=1')['total'] == 76
            (root / 'recordings/imported/transcript.json').unlink()
            api('/sessions/imported/transcript', status=404)
            (root / 'recordings/imported/metadata.json').unlink()
            assert api('/sessions?limit=1')['total'] == 75
            (directory / 'transcript.json').write_text('{partial')
            api('/sessions/session000/transcript', status=500)
            api('/people/person2/sessions', status=500)
            write(directory / 'transcript.json', transcript)
            assert len(api('/people/person2/sessions')['sessions']) == 1
            print('PASS: imports, deletions, partial JSON errors and recovery', flush=True)

            # Exercise normal API creation/deletion and parallel file updates.
            created = api('/sessions', 'POST', {'language': 'en', 'format': 'wav', 'sources': ['e2e:missing-source']}, status=201)
            api('/sessions/' + created['id'] + '/recording/start', 'POST', {}, status=400)
            assert api('/sessions/' + created['id'])['state'] == 'created'
            api('/sessions/' + created['id'], 'DELETE', status=204)
            with ThreadPoolExecutor(max_workers=8) as pool:
                list(pool.map(lambda n: api('/tags', 'POST', {'name': f'parallel_{n}'}), range(16)))
                list(pool.map(lambda _: api('/sessions/session000/todos/0', 'PATCH', {}), range(20)))
            assert len(api('/tags')['tags']) == 17
            assert api('/sessions/session000/todos')['items'][0]['completed'] is True
            print('PASS: session CRUD, safe recording-start failure, concurrent tags and todo updates', flush=True)

            conv = api('/conversations', 'POST', {'title': 'Test conversation'}, status=201)
            assert api('/conversations/' + conv['id'])['title'] == 'Test conversation'
            conv_path = root / 'conversations' / (conv['id'] + '.json')
            stored = json.loads(conv_path.read_text())
            stored['title'] = 'External conversation edit'
            write(conv_path, stored)
            assert api('/conversations/' + conv['id'])['title'] == 'External conversation edit'
            assert api('/conversations')['conversations'][0]['title'] == 'External conversation edit'
            print('PASS: conversation creation, loading and external edits', flush=True)

            # Use a local SSE provider to inspect the actual next-turn prompt.
            api('/settings', 'PUT', {'llm_host': f'http://127.0.0.1:{llm.server_port}', 'llm_model': 'test'})
            def chat(mentions):
                body = json.dumps({'content': 'What changed?', 'mentions': mentions}).encode()
                req = urllib.request.Request(origin + '/api/conversations/' + conv['id'] + '/messages',
                                             data=body, headers={'Content-Type': 'application/json'})
                with urllib.request.urlopen(req, timeout=30) as response:
                    events = response.read().decode()
                assert 'event: done' in events and 'event: error' not in events, events
            chat([{'kind': 'session', 'id': 'session000', 'label': 'Test meeting', 'context_mode': 'both'}])
            assert 'Externally revised transcript' in json.dumps(prompts[-1]), prompts[-1]
            transcript['segments'][0]['text'] = 'New source for next chat turn'
            write(directory / 'transcript.json', transcript)
            write(directory / 'summary.json', {'content': 'New summary for next chat turn'})
            chat([])
            prompt = json.dumps(prompts[-1])
            assert 'New source for next chat turn' in prompt and 'New summary for next chat turn' in prompt
            assert 'Externally revised transcript' not in prompt
            stored = json.loads(conv_path.read_text())
            assert len([m for m in stored['messages'] if m['role'] == 'assistant']) == 2
            assert all('words' not in chunk.get('segment', {}) for m in stored['messages'] if m['role'] == 'context_result' for chunk in m['chunks'])
            transcript['segments'][0]['text'] = 'Externally revised transcript'
            write(directory / 'transcript.json', transcript)
            write(directory / 'summary.json', {'content': 'Updated summary'})
            print('PASS: local SSE chat, persisted replies, refreshed follow-up context without word arrays', flush=True)

            from playwright.sync_api import sync_playwright
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel='chrome', headless=True)
                context = browser.new_context()
                context.add_init_script("""(() => {
                    const Native = window.WebSocket;
                    window.WebSocket = class extends Native {
                        constructor(...args) { super(...args); (window.__testSockets ||= []).push(this); }
                    };
                })();""")
                page = context.new_page()
                errors = []
                requests = []
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.on('request', lambda request: requests.append(request.url.removeprefix(origin)))
                navigation = time.monotonic()
                page.goto(origin + '/sessions/session000', wait_until='networkidle')
                page.get_by_text('Externally revised transcript', exact=True).wait_for()
                browser_load_ms = (time.monotonic() - navigation) * 1000
                page.screenshot(path=str(artifacts / 'session-desktop.png'), full_page=True)
                page.wait_for_timeout(2200)  # Include init and multiple poll ticks.
                assert requests.count('/api/people') == 0, 'Session display eagerly loaded people'
                with page.expect_response(lambda response: response.url == origin + '/api/people'):
                    page.get_by_role('button', name='Reassign', exact=True).first.click()
                page.locator('div.fixed.z-50').get_by_text('Test Person', exact=True).wait_for()
                assert requests.count('/api/people') == 1
                profile_path = root / 'people/person1/profile.json'
                profile = json.loads(profile_path.read_text())
                profile['name'] = 'Changed person in open picker'
                write(profile_path, profile)
                page.locator('div.fixed.z-50').get_by_text(profile['name'], exact=True).wait_for(timeout=10000)
                assert requests.count('/api/people') == 2, 'One person edit should refresh the open picker once'
                page.get_by_placeholder('Search or create person...').press('Escape')
                print('PASS: zero eager people requests, one picker request, live picker refresh', flush=True)
                transcript['segments'][0]['text'] = 'Browser observed disk change'
                write(directory / 'transcript.json', transcript)
                page.get_by_text('Browser observed disk change', exact=True).wait_for(timeout=10000)
                page.get_by_text('Summary', exact=True).first.click()
                page.get_by_text('Updated summary', exact=False).first.wait_for()
                write(directory / 'summary.json', {'content': 'Browser observed summary change'})
                page.get_by_text('Browser observed summary change', exact=False).first.wait_for(timeout=10000)
                # Miss the change event, then verify reconnect init reconciles data.
                page.evaluate("window.__testSockets.forEach(socket => socket.close())")
                write(directory / 'summary.json', {'content': 'Recovered after reconnect'})
                page.get_by_text('Recovered after reconnect', exact=False).first.wait_for(timeout=10000)
                # Do not replace a dirty draft or overwrite a simultaneous external edit.
                notes = page.get_by_placeholder('Add notes about this session...')
                notes.fill('Browser draft must survive')
                external = json.loads((directory / 'metadata.json').read_text())
                external['notes'] = 'Concurrent editor notes'
                write(directory / 'metadata.json', external)
                page.get_by_text('Notes changed on disk.', exact=False).wait_for(timeout=10000)
                assert notes.input_value() == 'Browser draft must survive'
                assert json.loads((directory / 'metadata.json').read_text())['notes'] == 'Concurrent editor notes'
                # A deep link must work even when its session is outside the first page.
                last = page2['sessions'][-1]
                page.goto(origin + '/sessions/' + last['id'], wait_until='networkidle')
                page.get_by_text(f"Transcript {int(last['id'][7:]):03}", exact=True).wait_for()
                # Rapid navigation must never install a late transcript for another session.
                page.goto(origin + '/sessions/session001')
                page.goto(origin + '/sessions/session002', wait_until='networkidle')
                page.get_by_text('Transcript 002', exact=True).wait_for()
                assert requests.count('/api/people') == 2, 'Navigation or session edits reloaded people'
                page.set_viewport_size({'width': 390, 'height': 844})
                page.screenshot(path=str(artifacts / 'session-mobile.png'), full_page=True)
                page.set_viewport_size({'width': 1280, 'height': 900})
                with page.expect_response(lambda response: response.url == origin + '/api/people'):
                    page.get_by_role('button', name='Open chat', exact=True).click()
                page.wait_for_timeout(1500)
                independent = ['/api/people', '/api/tags', '/api/settings', '/api/sessions?limit=100&offset=0']
                before = {path: requests.count(path) for path in independent}
                stored = json.loads(conv_path.read_text())
                stored['title'] = 'Conversation refresh without mention reload'
                with page.expect_response(lambda response: response.url == origin + '/api/conversations'):
                    write(conv_path, stored)
                page.wait_for_timeout(500)
                assert {path: requests.count(path) for path in independent} == before, 'Conversation edit refetched unrelated resources'
                print('PASS: navigation avoids people requests; conversation changes do not refetch mention data or settings', flush=True)
                assert not errors, errors
                browser.close()
            print('PASS: real Chrome desktop/mobile, off-page deep links, live transcript/summary refresh, no JS errors', flush=True)

            result = {'startup_ms': round(startup_ms, 1), 'browser_load_ms': round(browser_load_ms, 1),
                      'browser_people_requests': requests.count('/api/people'),
                      'api_max_ms': round(max(ms for _, ms in elapsed), 1),
                      'api_samples': [{'path': path, 'ms': round(ms, 1)} for path, ms in elapsed]}
            (artifacts / 'results.json').write_text(json.dumps(result, indent=2))
            print(json.dumps({k: v for k, v in result.items() if k != 'api_samples'}), flush=True)
        finally:
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            log.close()
            llm.shutdown()
            llm.server_close()
            assert process.returncode == 0, f'Daemon exited {process.returncode}; see {artifacts / "daemon.log"}'


if __name__ == '__main__':
    main()
