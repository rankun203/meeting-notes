# GdayMeetings integration

[GdayMeetings](https://github.com/rankun203/gday-meetings) replaces temporary
file-drop with a Payload CMS recordings platform. It defaults to SQLite and also
supports Postgres. Its repository includes deployment and meeting-search MCP setup.

## Deploy and connect

1. Deploy GdayMeetings using its README and Docker Compose configuration. Set its
   public server URL to an HTTPS address reachable by both the desktop and RunPod.
2. Build/deploy the updated `apps/audio-extraction` worker. Older workers ignore the
   result sink and cannot guarantee durable outputs.
3. Build/restart this daemon. In existing transcription settings, set
   `file_drop_url` to the GdayMeetings origin and `file_drop_api_key` to its
   `GDAY_API_TOKEN`. Keep your RunPod endpoint and key configured for new jobs.
4. Retry failed transcriptions. Previously expired RunPod outputs cannot be
   reconstructed without rerunning the original local audio.

The daemon checks `/api/platform/capabilities`, uploads audio, creates a task, and
persists its task reference in local session metadata before submitting to RunPod.
The worker writes `TRANSCRIPT_OUTPUT` to a task-scoped callback before returning
success. Polling and startup recovery check that saved output before RunPod, so
results remain available when the desktop sleeps through RunPod response expiry.

Tasks record input audio, typed outputs, the acknowledged RunPod job ID, and
observed terminal worker failures. CMS task outputs can be downloaded as JSON.
The service token stays in settings; callback credentials are not saved in session
metadata. Preserve access to the previous platform when moving outstanding tasks.

## Compatibility and recovery limits

A capabilities 404 selects the legacy file-drop flow; authentication or service
errors do not silently disable persistence. Legacy file-drop now retains downloads
until expiry (24 hours by default), allowing the worker to retry interrupted reads.
Explicit `--expiry-secs` deployment arguments still override the new default.

If a submission acknowledgement is lost, its durable task remains recoverable on
daemon restart even without a RunPod job ID. Such a task can remain pending when
the provider never accepted the request; inspect it before manually retrying to
avoid duplicate GPU work. Lookup failures preserve task references for another
restart. Task failure status only changes for an explicit terminal RunPod result,
not an unavailable or expired status response.

Deployment of these changes is separate from source verification: rebuilding only
the desktop cannot fix transfer behavior inside an old worker or file-drop server.
