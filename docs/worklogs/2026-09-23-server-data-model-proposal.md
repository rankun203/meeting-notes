---
date: 2026-09-23
title: Meeting-centered Payload collection redesign
status: deferred
component: server
---

## Problem

The current server has two representations: newly processed meetings use Tasks and Outputs, while imported meetings retain much of their content in archiveMetadata/archiveArtifacts/archiveAudio. This preserves data but makes editing, filtering, permissions, and search harder. This worklog records the modeling recommendation requested in the conversation. The user explicitly deferred implementation while establishing the client/server/worker architecture.

## Proposed solution (not implemented)

Make Meetings the central record. Both local imports and new transcription should populate the same collections:

| Collection | Responsibility | Main fields/relationships |
| --- | --- | --- |
| Users | Canonical accounts and access | Email, role, disabled status. Server owns users; the authentication library owns OAuth sessions and grants. |
| Meetings | Identity, ownership, organization | Title, owner, recorded time, duration, participants, tags, source identity, current transcript and summary. |
| Recordings | CMS-managed audio uploads | Meeting, file, original name, checksum, duration, codec, channels, track/source type and timeline offset. Replaces audio-files. |
| Transcripts | Structured editable speech | Meeting, source artifact, language, segments, speaker mappings and revision metadata. |
| Meeting Documents | Notes and summaries | Meeting, NOTE/SUMMARY kind, title, content, author, source transcript revision. |
| Action Items | Follow-up work | Meeting, description, status, assignee, due date and supporting transcript reference. |
| People | Participants and speaker identities | Display name, aliases, optional linked user and restricted voice-profile information. |
| Tags | Consistent classification | Name, color and ownership/sharing scope. |
| Processing Runs | Execution history | Meeting, operation, input references, parameters, provider job ID, state, timestamps and retry relationship. Replaces tasks. |
| Artifacts | Immutable source material and results | Meeting, optional run, type, schema version, checksum and managed file. Generalizes outputs and imported archive files. |

### Relationships and storage

Processing inputs should reference recording IDs and exact checksums; resolve download URLs at submission time. Each recording belongs to a meeting created before uploading. An interrupted upload leaves a recoverable incomplete meeting. Preserve the 500,000,000-byte limit and preference for compressed formats. Store the child's meeting relationship once and use Payload Join fields for the reverse view instead of maintaining duplicate arrays of IDs.

Preserve raw worker responses, original transcript imports, Markdown attachments, waveform files and import manifests as immutable managed artifacts. Parse transcripts and summaries into editable records referencing their source. The API can retain the {type, body} shape while artifact bytes live in managed storage. Retire the large archive JSON bundle only after verified migration.

### Transcripts, identities and revisions

Segments need stable IDs, timestamps, text and speaker keys. Transcript-local speaker labels map to People; SPEAKER_00 across independent runs is not a global identity. Recording offsets establish the common meeting timeline. Initially use schema-validated segment JSON and a dedicated editor; avoid thousands of independently managed Payload documents or expanded admin array rows without a query requirement.

Use bounded Payload version history for human edits, accounting for full-document snapshot cost. Retranscription creates a candidate transcript. Meeting.currentTranscript selects the visible revision; a late result cannot silently replace human corrections. Summaries track their source transcript revision so stale generated content can be identified.

### Ownership and access (decision still open)

Current collections intentionally behave as one shared library for active users. Authentication alone does not establish ownership. Recommendation: private meetings with explicit viewer/editor shares; enforce parent permissions on every child collection, file download and MCP result. Scope People and Tags too. If team-owned libraries are required, introduce Workspaces and Memberships before migration; otherwise user ownership is the smaller starting point. The recommendation does not constitute approval to switch existing access policy automatically.

### Processing and provenance

Each Processing Run is one execution attempt, including queued, submitting, running, succeeded, failed and submission-unknown states. A retry links to its predecessor instead of erasing the failure. Record input revisions/checksums, operation, provider/model/options and provider response identity. Deduplicate callback outputs by run/type. Imported transcripts carry import provenance without fabricated processing runs.

Meetings use internal UUIDs. External identity is optional and namespaced by source and owning account/workspace, rather than globally requiring every meeting to have a client externalId. Import hashes/manifests establish copied content without blocking subsequent editing.

### Search and admin workflow

Search is a rebuildable projection of current transcript, selected summary, notes, tags and participant names. Superseded results should not compete with corrected content by default. Use the same permission-aware service from website and MCP, returning links and excerpts. Maintain portable initial queries on both SQLite and PostgreSQL.

Main admin navigation: Meetings, People, Tags, Action Items. Reach recordings, transcripts and documents through the meeting. Group processing runs and raw artifacts under operations.

## Reasoning

The boundaries follow different lifecycles: recordings and source artifacts are immutable, transcripts are corrected, action items progress, and runs describe execution. Separating them enables consistent imports, provenance and editing without making RunPod response retention part of storage. Payload relationships, reverse joins, upload collections and document versions provide the underlying mechanisms.

## Technical debt

Retained until this work is scheduled: dual archive/new-result representations; shared-library access without individual ownership; globally required external IDs; URL-based task inputs; raw archive JSON carrying first-class meeting data. These were accepted to preserve existing data and defer a substantial schema migration. Consequences are limited editing/search and future migration work. Remediation: approve ownership policy, introduce the target collections, backfill and verify existing content, make both creation paths use them, then remove old fields with committed SQLite and PostgreSQL migrations.

## Next steps

1. Decide user versus workspace ownership and sharing policy.
2. Specify schemas, deletion rules, validation and revision adoption behavior.
3. Implement and test migrations on both databases with existing fixtures.
4. Verify source artifacts and relationships before retiring old representations.
5. Perform local-meeting bulk migration after the schema work if sequencing permits. Preserve local backups; chat history remains out of scope.

References: [Payload joins](https://payloadcms.com/docs/fields/join), [versions](https://payloadcms.com/docs/versions/overview), [collection access](https://payloadcms.com/docs/access-control/collections).
