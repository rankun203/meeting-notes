import AVFoundation
import Accelerate
import CoreAudio

/// HIG Privacy: request protected resources only when recording is requested.
/// https://developer.apple.com/design/human-interface-guidelines/privacy
// Start/stop are serialized by MeetingStore. The engine tap only invokes the
// locked writer/report methods; system-writer state belongs to queue. Profile
// snapshots synchronize with that queue; failure state is protected by stateLock.
final class AudioCapture: NSObject, @unchecked Sendable {
    private var engine: AVAudioEngine?
    private var systemCapture: SystemAudioCapture?
    private var microphoneWriter: TimedAudioWriter?
    private var systemWriter: TimedAudioWriter?
    private let queue = DispatchQueue(label: "com.gdaymeetings.macos.system-audio")
    private var configurationObserver: NSObjectProtocol?
    private var epoch: TimeInterval = 0
    private var voiceProcessing = false
    private var microphoneTapInstalled = false
    private var failure: Error?
    private let stateLock = NSLock()
    private var healthTimer: DispatchSourceTimer?
    private var expectedMicrophone = false
    private var expectedSystem = false
    private var levels = RecordingLevels()
    private var lastMicSample: TimeInterval = 0
    private var lastSystemSample: TimeInterval = 0
    private var meterTimer: DispatchSourceTimer?
    private var meterDeliveryPending = false
    var onLevels: ((RecordingLevels, @escaping () -> Void) -> Void)?
    var onHealth: ((String) -> Void)?
    var onFailure: ((Error) -> Void)?

    var profile: RecordingProfile {
        let systemProfile = queue.sync { systemWriter?.profile }
        return RecordingProfile(
            microphoneVoiceProcessing: voiceProcessing,
            tracks: [microphoneWriter?.profile, systemProfile].compactMap { $0 })
    }
    func start(directory: URL, microphoneEnabled: Bool, systemEnabled: Bool, voiceProcessingEnabled: Bool = false)
        async throws -> [String]
    {
        guard microphoneEnabled || systemEnabled else {
            throw MeetingError.message("Enable microphone or system audio in Settings before recording.")
        }
        expectedMicrophone = microphoneEnabled
        expectedSystem = systemEnabled
        levels.microphone.enabled = microphoneEnabled
        levels.system.enabled = systemEnabled
        var files: [String] = []
        do {
            let cancellationGeneration = await RecordingPermissions.currentCancellationGeneration
            try await RecordingPermissions.request(microphone: microphoneEnabled)
            try await RecordingPermissions.checkCancellation(since: cancellationGeneration)
            if systemEnabled {
                let capture = SystemAudioCapture(queue: queue)
                systemCapture = capture
                capture.onFailure = { [weak self] error in self?.report(error) }
                capture.onBuffer = { [weak self] buffer, timestamp in
                    guard let self, let writer = self.systemWriter, timestamp >= self.epoch else { return }
                    try writer.append(buffer, hostSeconds: timestamp)
                    self.measure(buffer, microphone: false)
                }
                // Trigger audio-only consent before starting the microphone. Frames
                // before the common recording epoch are discarded, not persisted.
                try capture.start()
                try await RecordingPermissions.checkCancellation(since: cancellationGeneration)
            }
            epoch = CMClockGetTime(CMClockGetHostTimeClock()).seconds
            if microphoneEnabled {
                let engine = AVAudioEngine()
                self.engine = engine
                let input = engine.inputNode
                // Preserve the physical microphone rate before VoiceProcessingIO can
                // expose a multichannel aggregate default. Its channels are not a
                // supported mapping of processed speech to be averaged or truncated.
                let microphoneDeviceFormat = input.outputFormat(forBus: 0)
                guard microphoneDeviceFormat.sampleRate > 0, microphoneDeviceFormat.channelCount > 0 else {
                    throw MeetingError.message("No microphone input is available.")
                }
                // Enable only while stopped. Both hardware I/O nodes participate; never feed
                // captured system audio or microphone monitoring back to the speakers.
                // https://developer.apple.com/videos/play/wwdc2019/510/
                if voiceProcessingEnabled {
                    do { try input.setVoiceProcessingEnabled(true) }
                    catch {
                        let cause = error as NSError
                        throw MeetingError.message(
                            "Could not enable Apple microphone voice processing (\(cause.domain) \(cause.code)): \(cause.localizedDescription)"
                        )
                    }
                    voiceProcessing = input.isVoiceProcessingEnabled
                    guard voiceProcessing else {
                        throw MeetingError.message(
                            "Apple voice processing is unavailable on this audio route. Disable it in Settings or choose another device."
                        )
                    }
                    // Other applications count as other audio. Minimum reduces but does not
                    // promise to eliminate ducking; do not claim external-app AEC guarantees.
                    // https://developer.apple.com/videos/play/wwdc2023/10235/
                    input.voiceProcessingOtherAudioDuckingConfiguration =
                        AVAudioVoiceProcessingOtherAudioDuckingConfiguration(
                            enableAdvancedDucking: false, duckingLevel: .min)
                    input.isVoiceProcessingAGCEnabled = true
                }
                let format: AVAudioFormat
                if voiceProcessing {
                    // Request a mono processed-speech client from the Audio Unit itself.
                    // installTap's explicit format configures this otherwise-unconnected
                    // output bus; no application-side selection/downmix of aggregate
                    // channels is performed. Raw capture keeps its device channel layout.
                    // https://developer.apple.com/documentation/avfaudio/avaudionode/installtap(onbus:buffersize:format:block:)
                    guard
                        let speechFormat = AVAudioFormat(
                            standardFormatWithSampleRate: microphoneDeviceFormat.sampleRate, channels: 1)
                    else {
                        throw MeetingError.message("Could not configure the mono voice-processing client format.")
                    }
                    format = speechFormat
                }
                else {
                    format = input.outputFormat(forBus: 0)
                }
                if voiceProcessing {
                    // Voice I/O needs an active hardware output. Render silence, never the
                    // captured meeting: this establishes I/O without monitoring or feedback.
                    let silence = AVAudioSourceNode { _, _, _, buffers in
                        for buffer in UnsafeMutableAudioBufferListPointer(buffers) {
                            if let data = buffer.mData { memset(data, 0, Int(buffer.mDataByteSize)) }
                        }
                        return noErr
                    }
                    engine.attach(silence)
                    // VoiceProcessingIO requires its client input and output formats
                    // to match. The mixer's implicit output format can differ from the
                    // input client's aggregate default, causing initialization -10875.
                    // Connect the silent source directly to hardware I/O with the exact
                    // microphone client format; the Audio Unit handles the device format.
                    // https://developer.apple.com/documentation/avfaudio/avaudioionode/setvoiceprocessingenabled(_:)
                    engine.connect(silence, to: engine.outputNode, format: format)
                    let outputFormat = engine.outputNode.inputFormat(forBus: 0)
                    guard outputFormat == format else {
                        throw MeetingError.message(
                            "Apple voice processing requires matching client formats. Microphone: \(format); output: \(outputFormat)."
                        )
                    }
                }
                epoch = CMClockGetTime(CMClockGetHostTimeClock()).seconds
                try prepareSystemWriter(directory: directory)
                let url = directory.appendingPathComponent("microphone.wav")
                let writer = try TimedAudioWriter(
                    url: url, format: format, epoch: epoch, voiceProcessed: voiceProcessing)
                microphoneWriter = writer
                // An ordinary input tap is deliberately used rather than a realtime sink.
                // Its samples are copied into Core Audio's bounded asynchronous writer.
                input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, time in
                    guard time.isHostTimeValid else {
                        self?.report(MeetingError.message("Microphone returned no host-clock timestamp."))
                        return
                    }
                    do {
                        try writer.append(buffer, hostSeconds: AVAudioTime.seconds(forHostTime: time.hostTime))
                        self?.measure(buffer, microphone: true)
                    }
                    catch { self?.report(error) }
                }
                microphoneTapInstalled = true
                if voiceProcessing {
                    let negotiatedInput = input.outputFormat(forBus: 0)
                    let negotiatedOutput = engine.outputNode.inputFormat(forBus: 0)
                    guard negotiatedInput == format, negotiatedOutput == format else {
                        throw MeetingError.message(
                            "Apple voice processing did not accept the mono client format. Input: \(negotiatedInput); output: \(negotiatedOutput). The unprocessed recording option remains available."
                        )
                    }
                }
                engine.prepare()
                do { try engine.start() }
                catch {
                    let cause = error as NSError
                    let mode = voiceProcessing ? "voice-processed" : "unprocessed"
                    let output = voiceProcessing ? "; output client \(engine.outputNode.inputFormat(forBus: 0))" : ""
                    throw MeetingError.message(
                        "Could not start \(mode) microphone capture (\(cause.domain) \(cause.code)). Input client \(format)\(output). \(cause.localizedDescription)"
                    )
                }
                guard engine.isRunning else {
                    throw MeetingError.message("The microphone engine could not start on this route.")
                }
                // A route change can change sample rate/channel count and stop the engine.
                // Preserve the partial meeting, then require a deliberate new recording.
                // https://developer.apple.com/documentation/avfaudio/avaudioengineconfigurationchangenotification
                configurationObserver = NotificationCenter.default.addObserver(
                    forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
                ) { [weak self] _ in
                    self?.report(
                        MeetingError.message(
                            "The audio device or format changed. Your partial recording was saved. Check the input/output device and start a new recording."
                        ))
                }
                files.append(url.lastPathComponent)
            }
            if systemEnabled {
                if !microphoneEnabled { try prepareSystemWriter(directory: directory) }
                files.append("system.wav")
            }
            try systemCapture?.observeOutputRoute()
            let timer = DispatchSource.makeTimerSource(queue: queue)
            timer.schedule(deadline: .now() + 10, repeating: 10)
            timer.setEventHandler { [weak self] in
                guard let self else { return }
                let micFrames = self.microphoneWriter?.capturedFrames ?? 0
                let systemFrames = self.systemWriter?.capturedFrames ?? 0
                var statuses: [String] = []
                if self.expectedMicrophone {
                    statuses.append(
                        micFrames > 0
                            ? (self.voiceProcessing
                                ? "Microphone receiving · Apple voice processing"
                                : "Microphone receiving · unprocessed") : "Microphone: no audio samples received")
                }
                if self.expectedSystem {
                    statuses.append(
                        systemFrames > 0
                            ? "System audio receiving · separate track"
                            : "System audio: no samples yet; the source may be silent")
                }
                self.onHealth?(statuses.joined(separator: " · "))
            }
            healthTimer = timer
            timer.resume()
            let meterTimer = DispatchSource.makeTimerSource(queue: queue)
            meterTimer.schedule(deadline: .now(), repeating: .milliseconds(100), leeway: .milliseconds(20))
            meterTimer.setEventHandler { [weak self] in
                guard let self else { return }
                self.stateLock.lock()
                let pending = self.meterDeliveryPending
                if !pending { self.meterDeliveryPending = true }
                self.stateLock.unlock()
                guard !pending else { return }
                let finish = { [weak self] in
                    guard let self else { return }
                    self.stateLock.lock()
                    self.meterDeliveryPending = false
                    self.stateLock.unlock()
                }
                if let publish = self.onLevels {
                    publish(self.levelSnapshot(), finish)
                }
                else {
                    finish()
                }
            }
            self.meterTimer = meterTimer
            meterTimer.resume()
            let startupFailure = currentFailure()
            if let startupFailure { throw startupFailure }
            return files
        }
        catch {
            try? await stop()
            throw error
        }
    }
    func stop() async throws {
        meterTimer?.cancel()
        meterTimer = nil
        healthTimer?.cancel()
        healthTimer = nil
        if let configurationObserver {
            NotificationCenter.default.removeObserver(configurationObserver)
            self.configurationObserver = nil
        }
        var stopError: Error?
        // Unregister system route listeners before tearing down VoiceProcessingIO,
        // whose own aggregate removal must not look like a mid-recording route loss.
        if let systemCapture {
            do { try systemCapture.stop() }
            catch { stopError = error }
        }
        systemCapture = nil
        if microphoneTapInstalled {
            engine?.inputNode.removeTap(onBus: 0)
            microphoneTapInstalled = false
        }
        engine?.stop()
        engine = nil
        // Flush only after delivery has stopped; disposal drains the async ring buffer.
        queue.sync {
            do { try systemWriter?.finish() }
            catch { stopError = error }
        }
        do { try microphoneWriter?.finish() }
        catch { stopError = error }
        let captureFailure = currentFailure()
        if let captureFailure { throw captureFailure }
        if let stopError { throw stopError }
        let systemFrames = queue.sync { systemWriter?.capturedFrames ?? 0 }
        if (expectedMicrophone && (microphoneWriter?.capturedFrames ?? 0) == 0) || (expectedSystem && systemFrames == 0)
        {
            throw MeetingError.message(
                "A selected audio source delivered no samples. Available tracks were saved. Check microphone and system-audio permissions and the selected devices; system silence can also produce no samples."
            )
        }
    }
    private func measure(_ buffer: AVAudioPCMBuffer, microphone: Bool) {
        let measured = RecordingSourceLevel.measure(buffer)
        let now = ProcessInfo.processInfo.systemUptime
        stateLock.lock()
        defer { stateLock.unlock() }
        if microphone {
            levels.microphone = measured
            lastMicSample = now
        }
        else {
            levels.system = measured
            lastSystemSample = now
        }
    }
    private func levelSnapshot() -> RecordingLevels {
        let now = ProcessInfo.processInfo.systemUptime
        stateLock.lock()
        defer { stateLock.unlock() }
        var snapshot = levels
        if now - lastMicSample > 1 { snapshot.microphone.stale = true }
        if now - lastSystemSample > 1 { snapshot.system.stale = true }
        return snapshot
    }
    private func currentFailure() -> Error? {
        stateLock.lock()
        defer { stateLock.unlock() }
        return failure
    }
    private func report(_ error: Error) {
        stateLock.lock()
        let first = failure == nil
        if first { failure = error }
        stateLock.unlock()
        if first { onFailure?(error) }
    }
    private func prepareSystemWriter(directory: URL) throws {
        guard let capture = systemCapture, let format = capture.format else { return }
        try queue.sync {
            systemWriter = try TimedAudioWriter(
                url: directory.appendingPathComponent("system.wav"), format: format, epoch: epoch)
        }
    }

}

struct RecordingLevels: Equatable {
    var microphone = RecordingSourceLevel()
    var system = RecordingSourceLevel()
}
struct RecordingSourceLevel: Equatable {
    var enabled = false
    var hasSamples = false
    var stale = false
    var rmsDB: Double = -120
    var peakDB: Double = -120
    var level: Double { enabled && hasSamples && !stale ? min(1, max(0, (rmsDB + 60) / 60)) : 0 }
    var statusText: String {
        if !enabled { return "Not recording" }
        if !hasSamples { return "Waiting for audio" }
        if stale { return "No recent audio" }
        return rmsDB < -60 ? "Quiet" : "Receiving audio"
    }
    static func measure(_ buffer: AVAudioPCMBuffer) -> Self {
        // Scalar snapshots only: no sample history, audio copies, or UI work here.
        guard buffer.format.commonFormat == .pcmFormatFloat32, buffer.frameLength > 0 else {
            return Self(enabled: true)
        }
        var sumSquares: Float = 0
        var maximum: Float = 0
        var sampleCount = 0
        let frameCount = Int(buffer.frameLength)
        let channels = Int(buffer.format.channelCount)
        for audio in UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList) {
            guard let data = audio.mData else { continue }
            let count = min(
                Int(audio.mDataByteSize) / MemoryLayout<Float>.size,
                frameCount * (buffer.format.isInterleaved ? channels : 1))
            guard count > 0 else { continue }
            var energy: Float = 0
            var peak: Float = 0
            vDSP_svesq(data.assumingMemoryBound(to: Float.self), 1, &energy, vDSP_Length(count))
            vDSP_maxmgv(data.assumingMemoryBound(to: Float.self), 1, &peak, vDSP_Length(count))
            sumSquares += energy
            maximum = max(maximum, peak)
            sampleCount += count
        }
        guard sampleCount > 0, sumSquares.isFinite, maximum.isFinite else { return Self(enabled: true) }
        return Self(
            enabled: true, hasSamples: true,
            rmsDB: 20 * log10(max(1e-6, sqrt(Double(sumSquares) / Double(sampleCount)))),
            peakDB: 20 * log10(max(1e-6, Double(maximum))))
    }
}
