---
date: 2026-09-25
title: People and Tags menu target
---

## Problem
People & Tags retained a text-height native borderless menu trigger after the earlier target-size pass.

## Implemented solution
Use the same button-backed menu treatment as player menus, with a minimum 28-point width/height, 10-point horizontal padding, explicit decorative chevron and full rectangular hit shape. Shared ActionButtonStyle draws hover feedback across the padded target.

## Reasoning
Size the actionable label itself instead of increasing hover opacity. Preserve the existing adaptive metadata row and native menu behavior.

## Technical debt
None introduced. Existing toolchain linker search-path warnings remain separate.

## Validation
Preview build passed, retaining the two known linker search-path warnings and adding no deprecation warnings. Selected a synthetic meeting and opened People & Tags; native menu showed People/Preview Person and Tags/Preview, and Escape dismissed it. Edge hit-testing, hover pixel measurement and appearance/size variants were not verified.
