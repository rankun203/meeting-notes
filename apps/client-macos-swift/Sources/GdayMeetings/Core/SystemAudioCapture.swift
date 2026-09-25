import AVFoundation
import AudioCaptureBridge
import CoreAudio

/// Core Audio's IOProc is hard realtime, including the block API's synchronous
/// dispatch queue. Only the preallocated C ring runs there; this queue consumes it.
/// https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps
// Control methods are serialized by AudioCapture; buffer/failure handlers and
// reusable PCM storage belong to queue. Hardware sees only the opaque C ring.
final class SystemAudioCapture: @unchecked Sendable {
    private let queue: DispatchQueue
    private var tap: AudioObjectID = kAudioObjectUnknown
    private var aggregate: AudioObjectID = kAudioObjectUnknown
    private var ioProc: AudioDeviceIOProcID?
    private var ring: OpaquePointer?
    private var timer: DispatchSourceTimer?
    private var running = false
    private var listeners: [(AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertyListenerBlock)] = []
    private var buffer: AVAudioPCMBuffer?
    private var originalFormat = AudioStreamBasicDescription()
    private(set) var format: AVAudioFormat!
    var onBuffer: ((AVAudioPCMBuffer, TimeInterval) throws -> Void)?
    var onFailure: ((Error) -> Void)?
    private let maxFrames: UInt32 = 8192
    private var failed = false  // consumer queue only

    init(queue: DispatchQueue) { self.queue = queue }

    /// Called off the main actor, only after the person's explicit Record action.
    /// Starting a tap aggregate is the public first-use audio-consent trigger.
    /// There is no screen permission, display enumeration, or screen fallback.
    func start() throws {
        do {
            var pid = getpid()
            var process = AudioObjectID(kAudioObjectUnknown)
            var size = UInt32(MemoryLayout<AudioObjectID>.size)
            var address = AudioObjectPropertyAddress(
                mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject, mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain)
            try check(
                AudioObjectGetPropertyData(
                    AudioObjectID(kAudioObjectSystemObject), &address, UInt32(MemoryLayout<pid_t>.size), &pid, &size,
                    &process), "Identify this app's audio process")
            guard process != kAudioObjectUnknown else {
                throw MeetingError.message(
                    "Core Audio could not identify this app for exclusion from system audio. Try recording again.")
            }
            let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [process])
            description.name = "Gday Meetings system audio"
            description.isPrivate = true
            description.muteBehavior = .unmuted
            try check(AudioHardwareCreateProcessTap(description, &tap), "Create system-audio tap")
            originalFormat = try readFormat()
            let flags = originalFormat.mFormatFlags
            guard originalFormat.mFormatID == kAudioFormatLinearPCM,
                flags & kAudioFormatFlagIsFloat != 0,
                flags & kAudioFormatFlagIsBigEndian == 0,
                originalFormat.mBitsPerChannel == 32,
                originalFormat.mChannelsPerFrame == 2,
                originalFormat.mSampleRate > 0
            else {
                throw MeetingError.message(
                    "The system-audio tap returned an unsupported format. Select a standard stereo output device and try again."
                )
            }
            let interleaved = flags & kAudioFormatFlagIsNonInterleaved == 0
            guard originalFormat.mBytesPerFrame == (interleaved ? 8 : 4),
                let outputFormat = AVAudioFormat(
                    commonFormat: .pcmFormatFloat32, sampleRate: originalFormat.mSampleRate, channels: 2,
                    interleaved: true),
                let outputBuffer = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: maxFrames),
                let ring = GdayAudioRingCreate(2, interleaved, maxFrames, 64)
            else {
                throw MeetingError.message("Could not allocate the bounded system-audio capture buffer.")
            }
            self.ring = ring
            format = outputFormat
            buffer = outputBuffer
            var uidReference: Unmanaged<CFString>?
            size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
            address.mSelector = kAudioTapPropertyUID
            try check(
                AudioObjectGetPropertyData(tap, &address, 0, nil, &size, &uidReference),
                "Read system-audio tap identity")
            guard let uidReference else { throw MeetingError.message("The system-audio tap has no identifier.") }
            let uid = uidReference.takeRetainedValue()
            let composition: [String: Any] = [
                kAudioAggregateDeviceUIDKey: "com.gdaymeetings.macos.tap.\(UUID().uuidString)",
                kAudioAggregateDeviceNameKey: "Gday Meetings Audio Capture",
                kAudioAggregateDeviceIsPrivateKey: true,
                kAudioAggregateDeviceTapAutoStartKey: false,
                kAudioAggregateDeviceTapListKey: [[kAudioSubTapUIDKey: uid, kAudioSubTapDriftCompensationKey: true]],
            ]
            // Auto-start TRUE waits for a tapped application to play audio. Keep it
            // false: silence must not hang Start Recording.
            try check(
                AudioHardwareCreateAggregateDevice(composition as CFDictionary, &aggregate),
                "Create private audio capture device")
            try check(
                AudioDeviceCreateIOProcID(aggregate, GdayAudioIOProc, UnsafeMutableRawPointer(ring), &ioProc),
                "Prepare audio capture callback")
            try check(AudioDeviceStart(aggregate, ioProc), "Start system-audio capture")
            running = true
            let timer = DispatchSource.makeTimerSource(queue: queue)
            timer.schedule(deadline: .now(), repeating: .milliseconds(10), leeway: .milliseconds(2))
            timer.setEventHandler { [weak self] in self?.drain() }
            self.timer = timer
            timer.resume()
            try observe(tap, selector: kAudioTapPropertyFormat)
            try observe(aggregate, selector: kAudioDevicePropertyDeviceIsAlive)
        }
        catch {
            try? stop()
            throw error
        }
    }

    // Register after configuring VoiceProcessingIO, which may create an aggregate
    // itself. Subsequent user output-route changes end this recording explicitly.
    func observeOutputRoute() throws {
        try observe(AudioObjectID(kAudioObjectSystemObject), selector: kAudioHardwarePropertyDefaultOutputDevice)
    }

    private func readFormat() throws -> AudioStreamBasicDescription {
        var result = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioTapPropertyFormat, mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        try check(AudioObjectGetPropertyData(tap, &address, 0, nil, &size, &result), "Read system-audio format")
        return result
    }
    private func observe(_ object: AudioObjectID, selector: AudioObjectPropertySelector) throws {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        let block: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            self?.fail(
                MeetingError.message(
                    "The system-audio device or format changed. Available audio will be saved; start a new recording on the current device."
                ))
        }
        try check(AudioObjectAddPropertyListenerBlock(object, &address, queue, block), "Observe system-audio device")
        listeners.append((object, address, block))
    }
    private func drain() {
        guard let ring, let buffer, let destination = buffer.mutableAudioBufferList.pointee.mBuffers.mData else {
            return
        }
        var frames: UInt32 = 0
        var hostTime: UInt64 = 0
        // Bound each consumer pass as well; stop runs after hardware delivery ends.
        for _ in 0..<64 {
            guard
                GdayAudioRingRead(ring, destination.assumingMemoryBound(to: Float.self), maxFrames, &frames, &hostTime)
            else { break }
            buffer.frameLength = frames
            do { try onBuffer?(buffer, AVAudioTime.seconds(forHostTime: hostTime)) }
            catch { fail(error) }
        }
        switch GdayAudioRingFailure(ring) {
        case 0: break
        case 1:
            fail(
                MeetingError.message(
                    "System-audio capture could not keep up with incoming audio. Available audio will be saved; close busy applications before recording again."
                ))
        case 3: fail(MeetingError.message("System audio lost its host-clock timestamp. Available audio will be saved."))
        default:
            fail(
                MeetingError.message(
                    "The system-audio buffer format changed or exceeded its supported size. Available audio will be saved."
                ))
        }
    }
    private func fail(_ error: Error) {
        guard !failed else { return }
        failed = true
        onFailure?(error)
    }
    func stop() throws {
        // Stop and unregister before releasing callback context. No timing sleeps
        // substitute for Core Audio's lifecycle synchronization.
        var first: Error?
        func remember(_ status: OSStatus, _ operation: String) {
            if status != noErr && first == nil {
                first = MeetingError.message("\(operation) failed (Core Audio \(status)).")
            }
        }
        for (object, var address, block) in listeners {
            remember(AudioObjectRemovePropertyListenerBlock(object, &address, queue, block), "Remove audio observer")
        }
        listeners.removeAll()
        if running {
            remember(AudioDeviceStop(aggregate, ioProc), "Stop system audio")
            running = false
        }
        var callbackUnregistered = true
        if let ioProc {
            let status = AudioDeviceDestroyIOProcID(aggregate, ioProc)
            callbackUnregistered = status == noErr
            remember(status, "Remove audio callback")
            self.ioProc = nil
        }
        timer?.cancel()
        timer = nil
        queue.sync {
            drain()
            onBuffer = nil
            onFailure = nil
        }
        if aggregate != kAudioObjectUnknown {
            remember(AudioHardwareDestroyAggregateDevice(aggregate), "Remove capture device")
            aggregate = kAudioObjectUnknown
        }
        if tap != kAudioObjectUnknown {
            remember(AudioHardwareDestroyProcessTap(tap), "Remove audio tap")
            tap = kAudioObjectUnknown
        }
        queue.sync {
            // An unregister failure provides no safe guarantee that HAL has released
            // its raw context. Deliberately retain this bounded allocation until
            // process exit rather than risk a realtime use-after-free.
            if let ring, callbackUnregistered { GdayAudioRingDestroy(ring) }
            self.ring = nil
            buffer = nil
        }
        if let first { throw first }
    }
    private func check(_ status: OSStatus, _ operation: String) throws {
        guard status == noErr else {
            throw MeetingError.message(
                "\(operation) failed (Core Audio \(status)). If macOS denied audio recording, allow Gday Meetings in System Settings → Privacy & Security → Screen & System Audio Recording, then try again. No screen recording is requested."
            )
        }
    }
}
