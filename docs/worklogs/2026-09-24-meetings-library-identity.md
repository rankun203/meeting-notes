---
date: 2026-09-24
task: meetings-library-identity
status: implemented
---

**Problem:** The product needed its owned `gdaymeetings.com` domain reflected in app identities and a stable, clearly named meetings library at `~/.local/share/com.gdaymeetings.macos/`. A disposable UI-test directory had caused confusion.

**Implemented solution:** Updated the native app's bundle identity and default meetings directory. Added staged copy-once migration from the former Application Support location, preserving originals and never merging an existing destination. The folder toolbar action follows this actual store location. Updated current documentation and internal queue labels. Separate agents updated credential migration, Rust identity compatibility, and deployment examples.

**Reasoning:** Keep incompatible Rust and Swift data formats separate, with `com.gdaymeetings.macos.rust` for the Rust client. Retain explicit legacy identifiers only for migration and historical records. Test-only directory overrides remain available and are not production defaults. No DNS or live deployment changes are implied.

**Technical debt:** The old meetings directory remains as a rollback copy, consuming duplicate disk space and diverging if an old app keeps writing. This avoids destructive migration; quit older app versions before upgrade and remove the old copy only after verifying migrated data. Future remediation: a reviewed migration-status/cleanup flow. The app identity change can require macOS recording permissions and Keychain access to be approved again; it cannot transparently transfer OS permissions.

**Notes:** Added isolated migration fixtures covering original preservation, non-overwrite/non-merge, and fresh installation. All 30 Swift tests across ten suites passed, including migration and credential failure paths. CLT release build, bundle identity, signature verification, and Finder installer succeeded. Live microphone/system recording and OS permission transfer are still unverified.
