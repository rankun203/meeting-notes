---
date: 2026-09-24
title: Remove inset tile framing from the macOS icon
status: completed
---

## Problem

Finder rendered the rounded, padded logo inside another tile, leaving a large gray border around the artwork.

## Implemented solution

Created a dedicated opaque square master at `docs/branding/gday-meetings-koala-macos.png`, rebuilt all ten standard ICNS representations, and updated the branding export instructions. The web logo and original artwork are preserved.

## Reasoning

Edge-to-edge teal removes the baked-in tile silhouette and padding, leaving the platform to apply its icon presentation. A separate master keeps platform framing out of the web assets.

## Technical debt

None.

## Notes

Used the built-in imagegen tool with the original logo as the edit target. Prompt: preserve the silver koala's identity, wink, tilted head, chat bubbles, materials and lighting; remove transparent margins, rounded tile edges, bevel and tile shadow; extend deep teal to all straight canvas edges and corners; keep ears inside the canvas and torso at the bottom; no text or icon mockup.

Visually inspected the generated artwork and verified square dimensions and opacity. Packaged using sips/iconutil and built a signed app at `/private/tmp/gday-icon-build/Gday Meetings.app`. Finder rendering after installation has not been verified. The currently running app was not replaced or restarted.
