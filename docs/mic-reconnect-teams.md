# Resilient mic recording during Teams meetings

## Problem
When a user joins a Microsoft Teams meeting on macOS, the built-in microphone becomes unavailable to other audio apps. The error from cpal is: "The requested device is no longer available. For example, it has been unplugged."

The mic works fine before joining Teams, and works again after restarting the computer (or restarting coreaudiod).

## Root cause
Teams installs a HAL (Hardware Abstraction Layer) audio plugin at:
```
/Library/Audio/Plug-Ins/HAL/MSTeamsAudioDevice.driver
```

When joining a call (especially with screen sharing + audio), this plugin dynamically creates/destroys virtual I/O devices, which triggers a Core Audio **device graph reconfiguration**. This invalidates all existing `AudioDeviceID` integer handles. The physical microphone is still present — it just gets assigned a new device ID.

### Key technical details
- `AudioDeviceID` values are transient integers, not stable identifiers. Use `kAudioDevicePropertyDeviceUID` (CFString) for persistent identification.
- Core Audio batches property change notifications, so apps can miss intermediate states.
- cpal 0.15.3 monitors `kAudioDevicePropertyDeviceIsAlive` per-stream and fires `StreamError::DeviceNotAvailable` via the error callback when the device dies.
- cpal does NOT monitor `kAudioHardwarePropertyDevices` (device list changes) or `kAudioHardwarePropertyDefaultInputDevice` at the host level.

## Solution: auto-reconnect

### How it works
1. **Detection:** MicSource's cpal error callback sets an `AtomicBool` flag (`device_lost`) when `StreamError::DeviceNotAvailable` fires.
2. **Recovery:** The session manager's file-size ticker (runs every 2s) checks `recorder.has_device_lost_sources()`. When true, it dispatches reconnection on a `spawn_blocking` thread.
3. **Reconnection:** `restart_lost_sources()` calls `source.stop()` (drops old stream) then `source.start(sender)` (re-enumerates devices, finds mic with new ID, builds fresh stream). The writer channel stays alive throughout — just a brief gap in audio data.
4. **Notification:** On success, an info notice is emitted to the frontend. On failure (after 3 attempts), an error notice with recovery instructions.

### Files modified
- `src/audio/mic.rs` — `device_lost: Arc<AtomicBool>`, error callback detection, `is_device_lost()`
- `src/audio/source.rs` — `is_device_lost()` on AudioSource trait (default: false)
- `src/audio/recorder.rs` — `has_device_lost_sources()`, `restart_lost_sources()`
- `src/session/mod.rs` — file-size ticker auto-recovery logic

### Constraints
- The reconnection (Core Audio calls) must happen on a blocking thread, not the async runtime.
- The session write lock must be dropped before `spawn_blocking` and re-acquired after.
- The old stream must be fully dropped before building a new one.

## Manual recovery
If auto-reconnect fails:
1. Stop and restart recording from the UI
2. Or restart Core Audio: `sudo launchctl kickstart -kp system/com.apple.audio.coreaudiod`
3. Nuclear option: remove Teams' audio driver: `sudo rm -rf /Library/Audio/Plug-Ins/HAL/MSTeamsAudioDevice.driver` (requires Teams reinstall to restore)

## References
- cpal issue #373: device change notifications (open since 2019)
- cpal issue #704: silent device fallback
- Teams HAL plugin: https://hochwald.net/post/resolving-audio-issues-microsoft-teams-macos
- eqMac #976: Teams virtual device conflicts
