# Resilient mic recording during Teams meetings

## Problem
When a user joins a Microsoft Teams meeting on macOS, the built-in microphone becomes unavailable to apps using cpal (Core Audio HAL API). The error: "The requested device is no longer available. For example, it has been unplugged."

The mic works fine before joining Teams. Restarting the daemon doesn't help — only a computer restart (or coreaudiod restart) fixes it. Meanwhile, leading AI meeting notes applications continue recording fine.

## Root cause
Teams installs a HAL (Hardware Abstraction Layer) audio plugin at:
```
/Library/Audio/Plug-Ins/HAL/MSTeamsAudioDevice.driver
```

When joining a call (especially with screen sharing + audio), this plugin dynamically creates/destroys virtual I/O devices, triggering a Core Audio **device graph reconfiguration**. This invalidates all existing `AudioDeviceID` integer handles.

### Why cpal fails but other apps work
- **cpal** uses the low-level Core Audio HAL API (`AudioDeviceCreateIOProcID` + `AudioDeviceStart`). When device graph changes invalidate `AudioDeviceID` handles, the HAL API gives hard errors that persist even across fresh enumerations.
- **Leading AI meeting notes apps** use **AVAudioEngine** — Apple's higher-level audio API. AVAudioEngine's `inputNode` automatically tracks the system default input device and handles device graph changes gracefully.

## Solution: AVAudioEngine

We replaced cpal with AVAudioEngine for mic capture on macOS.

### How it works
1. **AVAudioEngine's `inputNode`** always tracks the system default input device — no manual device enumeration needed.
2. **Tap installed on inputNode** delivers PCM audio buffers via a callback block (similar to cpal's callback, but at a higher API level).
3. **Configuration change notification:** When Teams (or any app) modifies the audio device graph, AVAudioEngine posts `AVAudioEngineConfigurationChangeNotification` and stops itself. We observe this notification and **auto-restart the engine** — the tap persists across restarts, so audio resumes with only a brief gap.
4. **No AudioDeviceID dependency:** AVAudioEngine abstracts away device IDs entirely. The engine always routes to whatever the current system default is.

### Implementation (src/audio/mic.rs)
- Uses raw `objc2` message sending (same pattern as system_audio/macos.rs) to call AVAudioEngine APIs
- `block2` crate (0.5.x, compatible with objc2 0.5) for the ObjC block callbacks
- Non-interleaved PCM buffers from AVAudioPCMBuffer are interleaved before sending to the writer
- `NSNotificationCenter` observer for `AVAudioEngineConfigurationChangeNotification` auto-restarts the engine

### Dependencies added
```toml
# In [target.'cfg(target_os = "macos")'.dependencies]
block2 = "0.5"
objc2-foundation = { version = "0.2", features = ["NSNotification"] }  # added feature
```

### cpal retained for
- Device enumeration (`discover_sources()` in `src/audio/mod.rs`)
- Default source ID resolution (`default_source_ids()` in `src/session/mod.rs`)

## Fallback recovery
The session manager's file-size ticker also monitors for device-lost sources (via `AudioSource::is_device_lost()` trait method) and attempts reconnection as a safety net. This handles edge cases where AVAudioEngine's notification might not fire.

## Manual recovery
If all else fails:
1. Stop and restart recording from the UI
2. Nuclear option: remove Teams' audio driver: `sudo rm -rf /Library/Audio/Plug-Ins/HAL/MSTeamsAudioDevice.driver` (requires Teams reinstall to restore)
3. Note: `sudo launchctl kickstart -kp system/com.apple.audio.coreaudiod` is blocked by SIP on modern macOS

## References
- cpal issue #373: device change notifications (open since 2019)
- Teams HAL plugin: https://hochwald.net/post/resolving-audio-issues-microsoft-teams-macos
- eqMac #976: Teams virtual device conflicts
- AVAudioEngine config change: Apple Developer docs on AVAudioEngineConfigurationChangeNotification
