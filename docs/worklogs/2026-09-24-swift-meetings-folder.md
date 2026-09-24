---
date: 2026-09-24
task: swift-meetings-folder-toolbar
status: implemented
---

**Problem:** The native app had no toolbar shortcut to its meeting storage folder.

**Implemented solution:** Added an accessible “Open Meetings Folder” folder button before New Meeting in `LibraryView`. It opens the active library directory in Finder and uses the existing error alert if opening fails.

**Reasoning:** Use the store's actual directory, including development overrides, rather than duplicating its default path. The existing HIG toolbar citation applies to the native labeled symbol and tooltip.

**Technical debt:** None.

**Notes:** Release build and signature verification passed. Native UI smoke check confirmed the labeled toolbar button opens the active disposable QA library in Finder. Complete diff reviewed; no new test added for this small UI action.
