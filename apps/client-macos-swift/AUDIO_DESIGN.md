# Native audio design and validation

This client must record a meeting hosted by another app, keep microphone and remote participants separate, and build using only Apple's Command Line Tools. That differs from a VoIP app that owns both ends of the call's audio graph.

## Capture and processing choices

Use **ScreenCaptureKit for system audio** and **AVAudioEngine for microphone capture**. Keep each source in its own file. ScreenCaptureKit supplies sample buffers with timing and format metadata; its synchronization clock can be related to other capture clocks. Microphone capture through ScreenCaptureKit arrived in macOS 15, whereas this client's deployment target is macOS 14.2. ScreenCaptureKit microphone capture also does not, by itself, document echo cancellation. [Apple's capture sample](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos), [synchronization clock](https://developer.apple.com/documentation/screencapturekit/scstream/synchronizationclock), [WWDC24 microphone capture](https://developer.apple.com/videos/play/wwdc2024/10088/).

**Apple voice processing is an optional microphone mode.** It supplies echo cancellation, noise suppression, and automatic gain control. AVAudioEngine enables it while stopped and configures both input and output I/O. It requires device rendering, so offline/manual rendering is not an equivalent substitute. Keep the engine's output silent: replaying captured meeting audio or monitoring the microphone would create feedback or duplicate playback. [WWDC19 AVAudioEngine](https://developer.apple.com/videos/play/wwdc2019/510/).

Voice processing can reduce other apps' playback volume. Configure minimum ducking and leave advanced ducking off to minimize interference. The unprocessed capture option remains the default because this recorder does not own the call's playback graph. Apple's documentation describes voice-processing capabilities, but it does not establish reliable cancellation of every unrelated app's speaker output across every device route. Do not label the result “echo-free.” Headphones avoid the speaker-to-microphone acoustic path; speakerphone quality requires real hardware testing. Voice Isolation is a system-controlled mic mode, not a second custom denoiser to stack blindly on top. [WWDC23 voice processing and ducking](https://developer.apple.com/videos/play/wwdc2023/10235/).

Core Audio process taps are another supported system-audio capture option on macOS 14.2+. They require tap/aggregate-device lifecycle management and do not themselves provide noise suppression or acoustic echo cancellation. ScreenCaptureKit's stream lifecycle is used here rather than introducing a second capture backend. [Apple's Core Audio tap sample](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps).

## Track and file invariants

- Obtain permissions before starting capture; permission-dialog delays must not become track offsets.
- Use capture timestamps to retain gaps and align sources. Never assume callback arrival times or buffer counts alone establish synchronization.
- Preserve each source's actual sample rate and channel count. Stereo channels are not two independent speakers; source tracks and diarized identities are different concepts.
- Keep microphone monitoring off and exclude this app's playback from system capture.
- Keep file I/O away from hardware render callbacks. Own any buffer memory that outlives a callback, bound queued work, and surface write/format/route failures.
- Drain pending writes before closing files. A partial recording with an explicit failure is preferable to silently claiming a complete recording.
- Retain the original local audio. Perform lossy conversion only for playback/export/service compatibility, without repeatedly transcoding the same intermediate file.

## Transcription and transcoding

Use Apple's AVFoundation codecs, not an external `ffmpeg` executable. Preserve separate inputs when submitting a server task; report the prepared file's actual channels. Large PCM or unsupported containers are converted to a separate AAC/M4A file. A successful export must have completed successfully, contain readable audio, and satisfy the receiving service's size constraints.

Direct compatible transcription splits long tracks into bounded AAC excerpts and restores each segment's original timeline offset. Hard chunk boundaries can affect recognition of words crossing the boundary; a future overlap/deduplication strategy must be validated before adoption. Server-side workers retain responsibility for resampling, recognition, and diarization. Neither stereo layout nor acoustic echo cancellation substitutes for speaker diarization.

Server upload checkpoints preserve the exact uploaded inputs and attempt key, so retries do not silently create a new transcription job. Immutable archive checkpoints retain their converted bytes and hashes, allowing readback verification without deleting local originals.

## Validation

Automated checks use synthetic audio and temporary libraries. They can establish correct file formats, sample values, channels, timeline arithmetic, persistence, and HTTP contracts. They cannot establish acoustic echo cancellation quality, microphone permissions, Bluetooth behavior, or real speakerphone performance.

Before claiming a route is validated, exercise the following on physical hardware:

| Scenario | Check |
| --- | --- |
| Built-in mic + speakers, processing off/on | Remote speech leakage into mic, near-end intelligibility, simultaneous speech, startup convergence, and playback ducking. |
| Wired/USB headset | Clean separate tracks, no feedback, sample-rate and channel correctness. |
| Bluetooth headset | Input/output profile changes, bandwidth changes, recording continuity, and explicit handling of disconnection. |
| Output/input route changes during capture | No crash, no silently wrong timing, retained partial recording and clear recovery instructions. |
| Quiet system audio or muted microphone | Distinguish silence from missing callbacks; do not infer permission denial from silence alone. |
| Long meeting | Bounded memory, stable track alignment, valid final containers, conversion size limits, and resumable server processing. |
| App quit, device loss, sleep, disk failure | Finalization or an actionable failure; previously saved material remains readable. |

Do not use synthetic test success as evidence that all of these hardware scenarios have passed.
