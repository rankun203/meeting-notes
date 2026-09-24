---
date: 2026-09-24
title: Focused recording setup and live source feedback
status: implemented-tests-passed
---

## Problem

Recording began directly from a toolbar action with configuration elsewhere, and the active experience lacked immediate source feedback. The user requested a polished native flow with clear setup, an elapsed timer, real microphone/system levels, and a persistent listening experience.

## Implemented solution

- Added `RecordingSetupView`: a scoped New Recording sheet with title, microphone/system choices, secondary voice-processing/format options, Cancel, and one Start Recording action. Draft changes persist only when Start is chosen. Showing the sheet does not access protected audio resources.
- Added `RecordingWorkspaceView(meetingID:)`: restrained recording status, monospaced elapsed time, two real source meters, Stop & Save, and disclosure-only technical details. Saving displays an indeterminate indicator and disables further stop actions; no unsupported pause or synthetic waveform was added.
- Added explicit `MeetingStore.isStartingRecording`, optional meeting title input, and live scalar source snapshots. Source states distinguish disabled, awaiting first samples, quiet samples, active samples, and stale delivery.
- `AudioCapture` computes Float32 RMS/peak over existing buffers using Accelerate. A 100 ms timer publishes at most one in-flight UI update; acknowledgements drop redundant updates while the main actor is occupied. No audio-copy arrays, meter history, or per-buffer UI dispatch is introduced.
- Preserved the verified mono voice-processing graph, separate system capture, permission session, host-clock alignment, and Opus finalization.

## Reasoning

[Apple HIG Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets) supports a single scoped task with explicit completion/cancel actions. [HIG Feedback](https://developer.apple.com/design/human-interface-guidelines/feedback) supports passive, accurate status integrated into the working interface; meter colors are supplemented by text and accessibility values. [HIG Progress indicators](https://developer.apple.com/design/human-interface-guidelines/progress-indicators) supports an indeterminate saving indicator when completion duration is unknown. These principles are cited next to the corresponding views.

## Technical debt

- Meters represent buffer RMS and peak levels, not loudness-standard measurements or persisted waveform history. This is intentional for inexpensive recording confidence; users cannot inspect historical transients in this view. Add an explicitly scoped, bounded historical visualization only if a future editing workflow requires it.
- Silence and lack of callbacks cannot diagnose every device/permission failure. The UI distinguishes them without claiming a cause and retains existing explicit capture errors. Future device diagnostics should use actual route/device state rather than infer permission failure from silence.
- The setup sheet drafts a subset of existing global defaults. Cancel discards its edits; Start writes them before requesting capture. A future per-meeting recording-preset model could separate one-off choices from defaults if users need that distinction.

## Validation

Added deterministic meter tests for planar and interleaved stereo RMS/peak, quiet input, disabled input, pending samples, and stale callbacks. Root's integrated run passed all 44 tests in 15 suites, and the Command Line Tools release build and bundle signature verification succeeded. UI fixture review is tracked in the root design worklog. This subtask performed no live capture, playback, or operating-system settings changes.

## Integration refinement

The setup sheet now consumes newly produced startup/permission errors immediately after the attempted start and presents them inline beside its retry action, avoiding a parent alert obscured by a modal sheet. Cancelling the system picker simply returns to the preserved draft. The live card uses tighter spacing and a red Stop & Save button, keeping more space available for notes. Meter accessibility omits meaningless decibel values for disabled, pending, or stale sources.
