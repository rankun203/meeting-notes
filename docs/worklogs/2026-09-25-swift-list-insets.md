---
date: 2026-09-25
title: Explicit list insets and short-list scrolling
status: mitigation-implemented
---

## Problem

The user observed a changing sidebar top gap and a partially clipped first meeting. The preview banner already occupies real VStack layout space, rather than overlaying content. The precise clipping trigger has not been reproduced.

## Implemented solution

Give the meeting list an explicit inset style and 10-point top content margin. Apply size-dependent bounce behavior to both navigation and meeting lists so short lists do not elastically overscroll. Long meeting lists remain scrollable.

## Reasoning

Use supported SwiftUI contentMargins and scrollBounceBehavior APIs to make scroll-content spacing explicit; avoid compensating offsets or resetting the user's scroll position. This is a targeted mitigation pending reproduction of the reported clipping, not a proven root-cause diagnosis. See [Apple layout adjustments](https://developer.apple.com/documentation/swiftui/layout-adjustments).

## Validation

Passed: preview build and native scroll attempts followed by sidebar collapse/expand; both first rows remained fully visible with consistent top spacing. Untested: the user's precise intermittent sequence and long-library scrolling in this build. No root-cause fix claimed. Build still emits the pre-existing warnings described below.

## Technical debt

The exact intermittent clipping trigger remains unresolved; capture a reproducible sequence if this persists and inspect native scroll insets during that sequence.

Build investigation also identified existing deprecated Security ACL APIs in ServiceSupport.swift, used to customize prompts for existing file-based Keychain credentials. No direct modern ACL-renaming replacement is available in the SecItem interface. Retained for this layout task to avoid silently changing credential storage/access behavior. Follow-up: move purpose explanation entirely into app UI, remove legacy ACL customization, and design/test migration to the data-protection Keychain with appropriate signing and legacy-read compatibility. Do not suppress these warnings. Existing linker search-path warnings also remain and need toolchain-path cleanup.
