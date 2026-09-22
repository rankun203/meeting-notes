# GdayMeetings integration

[GdayMeetings](https://github.com/rankun203/gday-meetings) replaces temporary
file-drop with a Payload CMS recordings platform. It defaults to SQLite and also
supports Postgres. Its repository includes deployment and meeting-search MCP setup.

## Deploy and connect

1. Deploy GdayMeetings using its README and Docker Compose configuration. Set its
   public server URL to an HTTPS address reachable by both the desktop and RunPod.
2. Build/deploy the updated `apps/audio-extraction` worker. Older workers ignore the
   result sink and cannot guarantee durable outputs.
3. Configure the RunPod endpoint and key **on the Gday server**. In this daemon's
   web UI open **Settings → Services**, enter the Gday origin and click
   **Login to Gday Meetings**. Sign in and approve recording read/write access.
4. Transcribe or retry a meeting. Audio uploads under your user grant; Gday submits
   the worker job and stores the transcript. Local RunPod credentials are not needed
   while signed in. Existing direct RunPod settings are used when signed out.

Login uses standard OpenID Connect discovery, public native client registration and
PKCE. The daemon validates the signed ID token, stores access/refresh tokens only in
`{data-dir}/gday-auth.json` (owner-only permissions), and refreshes access tokens when
needed (saving a rotated refresh token when the provider issues one). Login must start from the local loopback web UI. An HTTPS Gday origin is
required except for local development at localhost/127.0.0.1. Sign out removes local
credentials and requests remote revocation; it does not cancel server tasks.

Before submission, the daemon saves the uploaded audio URLs, execution options and
idempotency key in session metadata. If a response is lost, a retry or daemon restart
submits the exact same request and recovers the original task instead of duplicating
GPU work. Once acknowledged, only the durable task location remains in that metadata.
The worker writes `TRANSCRIPT_OUTPUT` before returning success. Gday task polling and
startup recovery retrieve it even after RunPod expires its copy. API lookup failures
retain recovery metadata; explicit task failure permits a fresh transcription attempt.

Tasks record input audio, typed outputs and execution state. CMS task outputs can be
downloaded as JSON. User access/refresh tokens never appear in browser responses or
session metadata. Preserve access to the owning Gday account/server while tasks are
outstanding. If sign-in expires or is revoked, sign in again and retry the meeting.

## Standalone transcription and recovery

Gday uploads, task creation and task retrieval always require the signed-in user's
OAuth grant. There is no shared-key Gday mode, capability-based fallback, externally
submitted Gday job, or client task-status PATCH. Gday owns worker execution and status.

The separately configured standalone file-drop + direct RunPod workflow remains
available when signed out. It uploads only to that file-drop service and submits directly
to RunPod; it never probes or creates Gday tasks. Do not configure the standalone
file-drop URL to point at Gday. Its input files expire after 24 hours by default, and
its results are subject to RunPod response retention.

Previously saved Gday task references are retained and now require login to the owning
server/account. Their old authentication-mode flag is ignored. A pending Gday task
cannot be silently replaced by standalone transcription when signed out. OAuth/API
errors preserve the durable reference for a later authenticated retry.

Deployment of these changes is separate from source verification: rebuilding only
the desktop cannot fix transfer behavior inside an old worker or file-drop server.

## Local provider contract test

The default Rust suite includes a signed mock-provider test that exercises local HTTP
login/callback, token validation and refresh, upload, lost submission acknowledgement,
restart, and transcript creation. An additional ignored test exercises the **actual**
Gday authentication implementation and platform authorization without production accounts.

In a terminal, run the disposable fixture from a Gday checkout with dependencies installed:

```bash
cd /path/to/gday-meetings
GDAY_REPO_DIR="$PWD" ./node_modules/.bin/tsx --tsconfig tsconfig.json \
  /path/to/meeting-notes/tests/fixtures/gday-provider.mts
```

It prints `READY http://127.0.0.1:PORT`. In the meeting-notes checkout run:

```bash
GDAY_TEST_ORIGIN=http://127.0.0.1:PORT \
  cargo test maintained_gday_provider_contract -- --ignored --nocapture
```

The fixture creates temporary SQLite databases and one synthetic user. It verifies
real discovery, native registration, account login, consent, signed ID-token validation,
and initial/refreshed access to the platform API. Stop it with Ctrl+C to delete its
temporary data. Never point this test at a deployed service.
