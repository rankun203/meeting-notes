---
date: 2026-09-24
task: recording-setup-sheet-overflow
status: implemented-and-packaged
---

## Problem

Expanding Recording options increased the setup sheet's intrinsic height beyond its available space. The unscrollable VStack clipped its header and action row. Earlier visual checks rendered only the collapsed setup state and missed this failure.

## Implemented solution

`RecordingSetupView` now has a bounded height with a flexible vertical scroll region for its form. The title and the Cancel/Start Recording actions remain outside that region. Footer helper text gets its own line so it cannot crowd the buttons, and startup progress stays beside them. A newly arriving startup error scrolls into view without animation. The scroll-view HIG reference is cited beside the layout.

## Reasoning

Scrolling the variable-length content accommodates expanded options and lengthy startup errors without moving completion actions offscreen. Merely increasing the sheet's fixed height would retain the failure on shorter windows. The existing startup, consent, and saving behavior is unchanged.

## Technical debt

Native UI automation still fails with “Sky Computer Use native pipe startup failed.” Offscreen component validation cannot prove live sheet sizing or trackpad/keyboard scrolling; this validation gap is retained temporarily until the connection is restored or a user confirms the corrected app's behavior. No additional production compatibility bridge or schema debt.

## Validation

`make install-macos` succeeded with the final source: Command Line Tools release build, plist validation, ad-hoc signing, strict signature verification, refreshed installer bundle, and successful Finder-open command. The running app is the separate `/Applications` copy; it was not overwritten or quit.

An independent agent rendered 12 expanded-options/error fixtures at 510×400, 510×480, and 510×600 in light/dark appearance, with top/bottom images for each. The short fixture had 358-point form content (494 with a long error) in a 224-point viewport; scrolling reached the format selector and full error while the header/footer stayed visible. Root inspected the short dark options and light error layouts.

Changed-case checks with the final source confirmed that an asynchronously arriving error automatically scrolls into view, startup progress stays beside the disabled buttons at 400-point height, and collapsed setup has an ideal fitting size of exactly 510×540. Root inspected all three renders; selected previews are retained in the [design directory](../design/macos-recording/README.md). These are isolated offscreen component checks, not live interaction. No real library, credentials, recording, playback, or network requests were used.
