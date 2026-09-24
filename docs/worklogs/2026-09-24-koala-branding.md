---
date: 2026-09-24
title: Adopt the approved koala logo
status: implemented
---

**Problem:** The selected logo existed only as a generated preview; app bundles and web interfaces lacked the approved identity.

**Implemented solution:** Saved the exact approved winking koala master under docs/branding. Exported an ICNS for native packaging and 256px PNGs for independently deployed client/server assets. Added the bundle icon resource, client sidebar/favicon, server homepage/favicon and Payload logo/navigation icon. Regenerated Payload's import map and copied public assets into the server's runtime Docker image. Documented the source and export process, and displayed the logo in the root README.

**Reasoning:** Preserve the selected artwork exactly, using size/format conversion only. Component-local exports avoid cross-component dependencies at build or deployment time. Packaging the macOS resource before signing keeps bundle verification intact.

**Technical debt:** Generated PNG copies and ICNS must be refreshed together if the master changes; export instructions document that maintenance step. The native status item retains its monochrome waveform because the colored tile cannot serve as a legible template icon. A purpose-designed koala silhouette remains future design work.

**Notes:** TypeScript, import-map generation, client JavaScript/shell syntax and diff checks passed. The 256px export was visually inspected. Native build and server regression results recorded below. Testing/build output uses a separate app bundle under /private/tmp/gday-brand-build; the recording client on port 33487 is untouched. No installation or deployment performed.

Validation complete: separate native bundle build and signature/plist verification passed; all 33 server tests passed. The stored master matches the user-selected file byte-for-byte. Server TypeScript validation includes the regenerated brand import map. Docker asset copy was reviewed; no container image was built or published.
