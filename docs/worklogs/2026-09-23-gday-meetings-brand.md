---
date: 2026-09-23
title: Rename the product to Gday Meetings
status: complete
---

**Problem:** Client, server, packaging and deployment documentation used inconsistent product names.

**Implemented solution:** Updated active product branding to Gday Meetings across client UI, macOS bundle and permission descriptions, server UI/auth/MCP, documentation and import skill references. Renamed the Rust package/binary to `gday-meetings-client`, the server package to `@gday-meetings/server`, and image names to `gday-meetings-server` and `gday-meetings-worker-audio-extraction`. Updated build/install scripts, logging filters, tests, release workflow and integration placeholders together. The bundle is now `Gday Meetings.app`; the development log is `gday-meetings.log`.

**Reasoning:** Product names and component roles should agree. Historical worklogs/releases remain accurate records. Existing storage and authentication identifiers stay stable so a branding change does not create an empty library or invalidate credentials. The actual GitHub repository URL remains unchanged; no remote repository was renamed or image published.

**Technical debt:** Retained the internal `org.rankun.meeting-notes` bundle/data identity, browser analytics storage key and worker Compose project/volume names to preserve existing state. These retain old spelling internally. Any future removal requires explicit data/volume migration and macOS permission handling; they must not be changed by a text replacement. Already installed/generated old bundles are not removed automatically; quit the old app before replacing it with Gday Meetings. Existing OAuth client registrations can retain their saved display name until registration is renewed.

**Notes:** `make build` passed, including plist and ad-hoc signature verification. Client tests: 45 passed, one external-fixture test ignored. Server tests: 33 passed. Shell syntax and diff whitespace checks passed. Existing running client and recordings were not modified. No release or deployment performed.
