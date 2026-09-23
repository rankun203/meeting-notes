---
date: 2026-09-23
title: Persistent macOS client logs
status: complete
---

**Problem:** Finder launches had no persistent log destination; development logging depended on the launcher and created an unbounded build-directory file.

**Implemented solution:** Added client-owned tracing output in `src/logging.rs` to `~/Library/Logs/Gday Meetings/client.YYYY-MM-DD.log`, with UTC daily rotation and up to 14 retained files using tracing-appender. Preserved stderr output and RUST_LOG filtering. New log directories use private Unix permissions. GDAY_MEETINGS_LOG_DIR permits isolated testing. Initialization failures warn on stderr without preventing startup. Rust panic diagnostics also enter the file sink. The launcher forwards the log directory and streams stderr through its FIFO without maintaining a duplicate file. Help/version invocations do not initialize logging.

**Reasoning:** Logging belongs to the executable so all launch methods behave consistently. The user's Library/Logs folder needs no system-wide service or elevated permissions. Synchronous writes avoid a background queue losing trailing diagnostics on unwind.

**Technical debt:** Daily retention limits file count, not byte size; verbose debugging can still create large daily files. Add a byte-size rotation policy if real usage warrants it. Synchronous file writes can block logging callers on slow disks; switch to a bounded background writer with explicit shutdown flushing if measurements show an impact. Native crashes/forced OS termination may omit final diagnostics; this does not replace macOS crash reports. Existing log directories retain their existing permissions. These tradeoffs avoid custom rotation and crash-handler complexity in this focused change.

**Notes:** 46 client tests passed, one external-provider test ignored. A new test checks append across reopen, retention, plain-text formatting, preservation of unrelated files and invalid-directory handling. Release app build, plist/signature validation, shell syntax and diff checks passed. LaunchServices smoke test with stdout/stderr discarded verified the default log folder and private permissions, HTTP startup and graceful SIGINT shutdown using an isolated temporary library; the user's running client/data were untouched. No release or deployment performed.
