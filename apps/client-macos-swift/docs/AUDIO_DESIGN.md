# Native audio design and validation

This client must record a meeting hosted by another app, keep microphone and remote participants separate, and build using only Apple's Command Line Tools. That differs from a VoIP app that owns both ends of the call's audio graph.

## Capture and processing choices

Use **Core Audio process taps for system audio** and **AVAudioEngine for microphone capture**. Keep each source in its own file. A private, unmuted global stereo tap captures other apps' outgoing audio without selecting a display or requesting screen-sharing access. The tap feeds a private HAL aggregate device; microphone audio stays in a separate engine and file. These APIs are available in the Command Line Tools SDK and support the client's macOS 14.2 deployment target without a virtual audio driver. [Apple's Core Audio tap sample](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps).

The packaged app supplies `NSAudioCaptureUsageDescription`. Starting capture through the tap aggregate triggers macOS's system-audio permission request when needed; microphone access has its own request and purpose string. No screen video is captured, and system audio no longer requires an available display or ScreenCaptureKit content-sharing session. Permission decisions remain controlled by macOS; neither a successful API return nor silent samples prove access was granted. [System-audio purpose string](https://developer.apple.com/documentation/bundleresources/information-property-list/nsaudiocaptureusagedescription), [Apple's audio-only permission guidance](https://support.apple.com/en-au/guide/mac-help/mchl2844ecab/mac).

**Apple voice processing is an optional microphone mode.** It supplies echo cancellation, noise suppression, and automatic gain control. AVAudioEngine enables it while stopped and configures both input and output I/O. It requires device rendering, so offline/manual rendering is not an equivalent substitute. Keep the engine's output silent: replaying captured meeting audio or monitoring the microphone would create feedback or duplicate playback. [WWDC19 AVAudioEngine](https://developer.apple.com/videos/play/wwdc2019/510/).

Voice processing can reduce other apps' playback volume. Configure minimum ducking and leave advanced ducking off to minimize interference. The unprocessed capture option remains the default because this recorder does not own the call's playback graph. Apple's documentation describes voice-processing capabilities, but it does not establish reliable cancellation of every unrelated app's speaker output across every device route. Do not label the result “echo-free.” Headphones avoid the speaker-to-microphone acoustic path; speakerphone quality requires real hardware testing. Voice Isolation is a system-controlled mic mode, not a second custom denoiser to stack blindly on top. [WWDC23 voice processing and ducking](https://developer.apple.com/videos/play/wwdc2023/10235/).

The Core Audio IOProc is a real-time callback. It copies Float32 input into a bounded, preallocated C ring and retains each buffer's input host timestamp; a separate consumer performs Swift processing and file writing. Callback arrival time includes scheduling delay and must not replace the input timestamp. Overflow or incompatible data fails explicitly rather than allowing unbounded memory growth or corrupting channel layout. The aggregate does not opt into waiting for a tapped application to start playing audio. Teardown stops IO before releasing callback memory, drains pending audio, and destroys the aggregate and tap. Taps do not provide noise suppression or acoustic echo cancellation. [Apple's real-time audio guidance](https://developer.apple.com/documentation/audiotoolbox/analyzing-audio-performance-with-instruments), [IOProc timing contract](https://developer.apple.com/documentation/coreaudio/audiodeviceioproc).

## Track and file invariants

- Keep permission/setup delays outside the recording timeline; use a shared capture epoch once sources are ready.
- Use capture timestamps to retain gaps and align sources. Never assume callback arrival times or buffer counts alone establish synchronization.
- Preserve each source's channel count and duration; retain capture format metadata. Opus storage uses a 48 kHz timeline with native sample-rate conversion. Stereo channels are not two independent speakers; source tracks and diarized identities are different concepts.
- Keep microphone monitoring off and exclude this app's playback from system capture.
- Keep file I/O away from hardware render callbacks. Own any buffer memory that outlives a callback, bound queued work, and surface write/format/route failures.
- Drain pending writes before closing files. A partial recording with an explicit failure is preferable to silently claiming a complete recording.
- Capture into recoverable PCM spools. Finalize each source into the selected Opus (default), M4A/AAC, or WAV format. Remove generated spools only after all encoded tracks and their library metadata are saved successfully. Retain PCM on failure. Never replace saved recordings during later playback/export/service conversion.

## Recording formats

New Opus recordings target **32 kbps mono / 64 kbps stereo**, with a 48 kHz encoding timeline. Request VBR when the native encoder exposes bitrate-strategy control; otherwise keep its default strategy. VBR targets are not exact file-size guarantees. Keep Apple's native encoder complexity because `AVAudioConverter` has no libopus-style 0–10 complexity setting. Capture retains the device's native sample rate and separate source tracks; conversion resamples as needed. Existing recordings are not re-encoded. [Apple's bitrate strategy](https://developer.apple.com/documentation/avfaudio/avaudioconverter/bitratestrategy), [Opus bitrate guidance](https://www.rfc-editor.org/rfc/rfc6716.html#section-2.1.1).

Opus and AAC use Apple's native encoders. Opus packets are written into the standard Ogg container with pre-skip, checksums, channel metadata, and final granule trimming, following [RFC 7845](https://www.rfc-editor.org/rfc/rfc7845) and [Ogg framing](https://www.xiph.org/ogg/doc/framing.html). Interactive Ogg Opus playback uses libopusfile to read and seek incrementally, feeding AVAudioEngine without a temporary decoded file. Server transcription receives the original Opus; compatible direct transcription uses decoded audio to prepare AAC excerpts. Native MP3 encoding is unavailable on the tested Mac; MP3 input remains supported. No external encoder is installed or required. Older supported macOS releases still need codec validation; unavailable conversion fails visibly and retains WAV.

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

## Streaming playback and waveform cache

`StreamingPlayback` owns file readers and the engine on a serial worker queue. Opus uses libopusfile's pre-skip, gain, end trimming, and seek preroll; native formats use AVAudioFile and continuous AVAudioConverter resampling. A fixed 16,384-frame stereo ring per track (about 341 ms at 48 kHz) feeds one AVAudioSourceNode and AVAudioUnitTimePitch. The render callback performs only bounded mixing and atomic cursor access: no file reads, decoding, locks, or allocations. Tracks share one consumer cursor; starvation emits silence without advancing media time. Playback progress publishes at 60 Hz from consumed source frames; output/time-pitch buffering can put the cursor slightly ahead of audible output. Seeking stops and resets the graph before replacing buffered samples. Device changes pause and surface a restart action.

Source and working PCM memory are bounded independently of meeting duration. libopusfile seeks using Ogg page granule positions without building a full PCM file or scanning every packet. Supported Opus input is single-link mono/stereo; chained or multichannel files fail visibly. The transport supports up to 32 tracks. See [pinned offline dependencies](../ThirdParty/README.md) for build and license management.

Waveform work runs independently of playback. Up to 1,200 evenly spaced buckets sample at most 1,024 frames each; channel peak magnitudes preserve opposite-phase stereo. This is an approximate overview and can miss brief transients between windows. Opus seeks include codec preroll, so its work exceeds the sampled PCM count but remains bounded by the number of buckets. Versioned JSON envelopes in the app cache validate source path, size, creation time, and modification time. Cached envelopes can display before audio preparation finishes. Service/transcription conversion still uses temporary compatible audio when necessary; that path is separate from playback.
