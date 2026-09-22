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

## Compatibility and recovery limits

For the signed-out direct RunPod path, a capabilities 404 selects legacy file-drop;
authentication or service errors do not silently disable persistence. Legacy file-drop now retains downloads
until expiry (24 hours by default), allowing the worker to retry interrupted reads.
Explicit `--expiry-secs` deployment arguments still override the new default.

For legacy client-submitted RunPod jobs, a lost acknowledgement still leaves a durable
task recoverable on daemon restart even without a RunPod job ID. Such a task can remain pending when
the provider never accepted the request; inspect it before manually retrying to
avoid duplicate GPU work. Lookup failures preserve task references for another
restart. Task failure status only changes for an explicit terminal RunPod result,
not an unavailable or expired status response.

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
