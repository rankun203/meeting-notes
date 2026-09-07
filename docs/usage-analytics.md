# Usage analytics

Settings → Usage analytics configures PostHog without restarting the daemon.
Keep `posthog_project_token` in `{data_dir}/secrets.json`; no rename is needed.
The literal `posthog.project.token` key is also accepted on load and saved back
as snake_case. Do not configure both keys. This is a PostHog **project token**,
not a personal API key. Existing provider-key updates preserve it.

`posthog_enabled` defaults to true, but tracking stays off without a token.
`posthog_host` defaults to `https://us.i.posthog.com`; select EU Cloud for an EU
project. Only these two cloud ingestion hosts are supported. The admin form
preserves a blank token input, offers explicit removal, and never reads it back.
Secrets retain the existing file permission of 0600. Other tabs refresh the
configuration every minute; the server stops accepting events immediately when
disabled. Restart an already running daemon once to load this new code.

## Events and useful PostHog views

| Event | Breakdown / use |
| --- | --- |
| `feature_used` | `feature`, `outcome`: rank recording, upload, transcription/summary requests, edits, people, tags, TODOs and settings usage |
| `app_opened`, `view_opened`, `meeting_opened` | Entry and navigation patterns; view is sessions/people/settings |
| `recording_form_opened` | Compare recording setup visits to successful `recording_start` |
| `playback_started`, `playback_paused`, `playback_completed` | Playback usage and completion, with position/duration in seconds |
| `playback_seeked`, `playback_speed_changed`, `playback_track_toggled` | How users listen; seek source and speed |
| `content_tab_opened` | Transcript vs summary usage |
| `transcript_exported`, `summary_exported` | Export intent by format |
| `chat_message_sent` | Accepted chat requests by backend (not stream completion) |

Start with a Trends insight on `feature_used`, filter `outcome = success`, and
break down by `feature`. Compare total events and unique users. Create a funnel
from `recording_form_opened` → `feature_used` filtered to `recording_start` →
`feature_used` filtered to `recording_stop` → `playback_started`. Use Paths for
the sequence of explicit events, or compare content tab usage before/after a
redesign. Export events mean an export was requested, not that a download was
saved. Transcription/summary success means the server accepted the job, not that
the asynchronous pipeline completed. These events cover the web UI; CLI calls,
automatic pipeline actions, and auto-stop are not counted as user clicks.

## Data and delivery

The browser generates a random persistent UUID; it represents a browser profile,
not a verified human. Clearing browser storage resets it. A separate UUID groups
activity in each tab and rotates after 30 minutes of inactivity. If storage is
blocked the ID is kept in memory. Do Not Track and Global Privacy Control disable
sending. Browsers without `crypto.randomUUID` skip tracking.

The server allowlists event names and property values. It removes arbitrary
properties and rejects non-UUID identities. No audio, transcript, summary, chat
text, names, filenames, meeting IDs, URLs, search queries, IP property, or DOM
capture is sent. Person profiles and GeoIP enrichment are disabled. No replay or
third-party browser script is loaded.

Delivery uses the [PostHog batch API](https://posthog.com/docs/api/capture) with
a bounded 128-event queue, batches of up to 32, and a five-second timeout. Events
may be dropped on overload, network failure, or shutdown; they are not retried or
persisted. The daemon never waits for PostHog in recording or playback actions.
Tests use synthetic tokens and mocked transport; they do not pollute live data.
