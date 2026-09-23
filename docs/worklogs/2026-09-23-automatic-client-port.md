---
date: 2026-09-23
title: Choose an available client port by default
status: complete
---

**Problem:** Default launches failed when port 33487 was occupied. Logging a requested port of zero also gave CLI users an unusable URL.

**Implemented solution:** Default the client port to zero, letting the OS allocate and reserve an available port. Read the bound listener's address for startup/UI logs and browser launching. Preserve explicit port arguments. Update Finder/default and explicit-port assertions, usage documentation and fixed-port manual test prerequisites.

**Reasoning:** Binding port zero avoids port-probing races and works consistently for Finder and make start. Explicit fixed ports remain useful for integrations and fail if occupied.

**Technical debt:** Single-instance/reopen handling remains deferred. A new port does not prevent separately launched clients from opening the same data directory; quit the previous client before starting another against that library. A future desktop lifecycle change should lock the library and reopen the existing instance's URL. Browser-local state is origin-specific and can vary across assigned ports; persisted meetings/settings remain on disk. Integrations needing stable addresses must specify a fixed port.

**Notes:** 46 client tests passed, one external-fixture test ignored. Release app build and signature/plist checks passed. An isolated-library smoke test held/confirmed port 33487 occupied, launched without --port, fetched the UI at the actual URL logged by the client (52834), and stopped gracefully. Existing user client/data untouched. Diff checked; no release or deployment.
