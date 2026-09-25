---
date: 2026-09-25
title: Menu and preview drag affordances
status: implemented
---

## Problem

The prior actionable-control pass left sample drag sources without feedback, and playback-speed/track menus had insufficiently distinct hover feedback.

## Implemented solution

Added outlined hover feedback to both preview sample labels and the speed/All Tracks menus, using a stronger semantic fill and subtle outline within existing bounds. Samples expose a combined accessibility label and drag instruction. Native menu interaction and file drag behavior remain intact. No dimensions, padding, fonts or positions changed.

## Reasoning

Apply the app's UI design policy and Apple Buttons HIG feedback principle to menu triggers and drag sources as well as buttons. Background/overlay drawing does not allocate layout space. Samples remain drag sources rather than pretending to perform a click action.

## Validation

Release compile, preview packaging, installer build and diff whitespace check passed. Rebuilt preview launched; All Tracks opened and dismissed its native menu successfully. Existing Command Line Tools linker-path warnings remain. Hover appearance and drag delivery are not yet verified through native automation; do not treat this as full target-size compliance. No live audio, uploads or credentials accessed.

## Technical debt

Existing compact sample/menu target sizing retained under the user's unchanged-layout constraint. Measure native menu hit regions and allocate additional space only in a subsequent layout pass if necessary; hover decoration alone does not enlarge native menu hit regions.
