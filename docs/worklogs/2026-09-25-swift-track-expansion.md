---
date: 2026-09-25
title: Staged player track expansion
status: implemented
---

## Problem

Track disclosure appeared immediately. User requested the transport move up first, then smoothly reveal the track section, retaining final dimensions.

## Implemented solution

`MeetingPlayerBar` expands a clipped viewport using a 250 ms ease-in-out curve, then reveals full-size rows with a 180 ms ease-in-out fade and 8 pt slide. Collapse reverses the sequence. Rows retain their existing height and spacing throughout. Hidden rows exclude pointer and accessibility actions. Reduce Motion bypasses the sequence. Completion tokens invalidate superseded toggles and resets on meeting change/disappearance.

## Reasoning

Use animation completion rather than a timer to reveal rows only after space is available, following the sidebar approach. Keep the full-size row layout inside the animated viewport to avoid compressing content. This follows [HIG Motion](https://developer.apple.com/design/human-interface-guidelines/motion) and the app's existing Reduce Motion policy. No deployment-target or deprecated API changes.

## Validation

- Passed: production/preview build and `make install-macos`; native preview expand, collapse, expand again; hidden controls disappear from accessibility; final expanded screenshot retains player and row geometry.
- Untested: frame-by-frame animation smoothness (native snapshots show settled states), rapid mid-animation reversal, Reduce Motion runtime, small windows and older macOS.
- Existing Command Line Tools linker search-path warnings remain. No real recording or network action performed.

## Technical debt

Hidden track views remain mounted to preserve intrinsic dimensions through the animation. This retains small view/state overhead while collapsed; accepted for stable transitions. If many-track profiling reveals a cost, lazily mount rows during the space-opening phase without changing viewport dimensions.
