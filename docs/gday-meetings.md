# Client/server integration

The CMS now lives in [apps/server](../apps/server) as Meeting Notes Server. It defaults to SQLite and supports PostgreSQL. See [deployment](deployment.md) for local CPU/GPU and cloud options. Existing gday protocol identifiers and stored login paths are retained.

## Deploy and connect

1. Deploy apps/server using the deployment guide. Use localhost for fully local execution or a reachable HTTPS origin for cloud deployment.
2. Build/deploy the updated `apps/worker-audio-extraction` worker. Older workers ignore the
   result sink and cannot guarantee durable outputs.
3. Configure the local worker or RunPod endpoint **on the server**. In this daemon's
   web UI open **Settings → Services**, enter the server origin and click
   **Login to Meeting Notes Server**. Sign in and approve recording read/write access.
4. Transcribe or retry a meeting. Audio uploads under your user grant; the server submits
   the worker job and stores the transcript. Local RunPod credentials are not needed
   while signed in. Existing direct RunPod settings are used when signed out.

Login uses standard OpenID Connect discovery, public native client registration and
PKCE. The daemon validates the signed ID token, stores access/refresh tokens only in
`{data-dir}/gday-auth.json` (owner-only permissions), and refreshes access tokens when
needed (saving a rotated refresh token when the provider issues one). Login must start from the local loopback web UI. An HTTPS server origin is
required except for local development at localhost/127.0.0.1. Sign out removes local
credentials and requests remote revocation; it does not cancel server tasks.

Before submission, the daemon saves the uploaded audio URLs, execution options and
idempotency key in session metadata. If a response is lost, a retry or daemon restart
submits the exact same request and recovers the original task instead of duplicating
processing. Once acknowledged, only the durable task location remains in that metadata.
The worker delivers `TRANSCRIPT_OUTPUT` to the server. RunPod requires a successful
callback before returning; local HTTP mode saves output before delivery so the server
can also recover it by polling. Client task polling and startup recovery retrieve the
server's copy even after provider results expire. API lookup failures
retain recovery metadata; explicit task failure permits a fresh transcription attempt.

Tasks record input audio, typed outputs and execution state. CMS task outputs can be
downloaded as JSON. User access/refresh tokens never appear in browser responses or
session metadata. Preserve access to the owning account/server while tasks are
outstanding. If sign-in expires or is revoked, sign in again and retry the meeting.

## Copy existing local meetings

Migration needs the deployed server URL and your OAuth login. Building the app or
previewing the plan does not transfer any data. Deploy the migration-capable server
and daemon, sign in to the intended server account under **Settings → Services**, then
review **Copy existing meetings to Meeting Notes Server**.

The preview lists the number of local meetings ready to copy, their audio size, and
blocked meetings with the reason each cannot be copied. Resolve those issues or start
with the ready meetings; blocked meetings are skipped. Click **Copy existing meetings
to Meeting Notes Server** to transfer recordings and their existing results. This imports existing
content without automatically transcribing it again.

Progress shows the current meeting, processed count, and per-meeting outcomes:
**Copied**, **Already copied**, or **Failed** with an error. Keep the daemon running
during the transfer. Reopening settings reconnects to its progress. After completion,
resolve any errors and use **Refresh preview** and **Retry copying meetings**; already
imported meetings are recognized rather than duplicated. A progress-request error
retries automatically while the run is active.

Local recordings and result files remain as a backup. Verify the copied meetings,
audio, and outputs in the deployed CMS before relying on that copy; this flow never
deletes local files. Signing out does not undo completed imports.

## Standalone transcription and recovery

Server uploads, task creation and task retrieval always require the signed-in user's
OAuth grant. There is no shared-key server mode, capability-based fallback, externally
submitted server job, or client task-status PATCH. The server owns worker execution and status.

The separately configured standalone file-drop + direct RunPod workflow remains
available when signed out. It uploads only to that file-drop service and submits directly
to RunPod; it never probes or creates server tasks. Do not configure the standalone
file-drop URL to point at the server. Its input files expire after 24 hours by default, and
its results are subject to RunPod response retention.

Previously saved server task references are retained and now require login to the owning
server/account. Their old authentication-mode flag is ignored. A pending server task
cannot be silently replaced by standalone transcription when signed out. OAuth/API
errors preserve the durable reference for a later authenticated retry.

Deployment of these changes is separate from source verification: rebuilding only
the desktop cannot fix transfer behavior inside an old worker or file-drop server.

## Local provider contract test

The default Rust suite includes a signed mock-provider test that exercises local HTTP
login/callback, token validation and refresh, upload, lost submission acknowledgement,
restart, and transcript creation. An additional ignored test exercises the **actual**
server authentication implementation and platform authorization without production accounts.

From the repository root, with apps/server dependencies installed, run the disposable fixture:

```bash
node apps/server/node_modules/tsx/dist/cli.mjs \
  apps/client-macos-rust/tests/fixtures/gday-provider.mts
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
