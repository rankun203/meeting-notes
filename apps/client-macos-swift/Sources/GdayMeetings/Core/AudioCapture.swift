import AVFoundation
import Accelerate
import CoreAudio

/// HIG Privacy: request protected resources only when recording is requested.
/// https://developer.apple.com/design/human-interface-guidelines/privacy
// Start/stop are serialized by MeetingStore. Each source has a recovery
// controller that replaces its engine or tap on its own serial queue; writers,
// the epoch, and files stay fixed for the whole recording. Taps and IOProc
// consumers only call the locked writer/measure/report methods. Mutable shared
// state below the "stateLock" mark is protected by stateLock.
final class AudioCapture: NSObject, @unchecked Sendable {
    /// Stop & Save waits at most this long for native teardown before abandoning it.
    static let stopBound: TimeInterval = 3
    /// A source that delivered before and then goes quiet this long is rebuilt.
    static let deliveryWatchdog: TimeInterval = 3

    private var microphoneWriter: TimedAudioWriter?
    private var systemWriter: TimedAudioWriter?
    private let queue = DispatchQueue(label: "com.gdaymeetings.macos.system-audio")
    private var microphoneRecovery: CaptureSourceRecovery<MicrophoneSession>?
    private var systemRecovery: CaptureSourceRecovery<SystemAudioCapture>?
    private var routeListeners: [(AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertyListenerBlock)] = []
    private var knownInput: AudioObjectID?  // queue only after listeners register
    private var knownOutput: AudioObjectID?  // queue only after listeners register
    private var epoch: TimeInterval = 0
    private var policy = VoiceProcessingPolicy.automatic
    private var initialVoiceProcessing = false
    private var initialMicrophoneFormat: AVAudioFormat?
    private var initialSystemFormat: AVAudioFormat?
    private var healthTimer: DispatchSourceTimer?
    private var meterTimer: DispatchSourceTimer?
    private var recoveryTimer: DispatchSourceTimer?
    private var expectedMicrophone = false
    private var expectedSystem = false
    private var finished = false
    // stateLock
    private let stateLock = NSLock()
    private var failure: Error?
    private var teardownError: Error?
    private var levels = RecordingLevels()
    private var microphoneDelivery = SourceDelivery()
    private var systemDelivery = SourceDelivery()
    private var microphoneVoiceProcessing = false
    private var routeChanges: [RecordingRouteChange] = []
    private var meterDeliveryPending = false
    var onLevels: ((RecordingLevels, @escaping () -> Void) -> Void)?
    var onHealth: ((String) -> Void)?
    /// Terminal failures only: a writer error, or every selected source failed permanently.
    var onFailure: ((Error) -> Void)?

    private struct SourceDelivery {
        var everDelivered = false
        /// A rebuilt session has not delivered yet; the source still shows as reconnecting.
        var awaitingResume = false
        var sessionStart: TimeInterval = 0
        var last: TimeInterval = 0
    }

    var profile: RecordingProfile {
        let systemProfile = queue.sync { systemWriter?.profile }
        var microphoneProfile = microphoneWriter?.profile
        // An automatic-policy fallback can finish on an unprocessed engine after
        // the writer was created for a processed one.
        microphoneProfile?.voiceProcessed = initialVoiceProcessing
        var profile = RecordingProfile(
            microphoneVoiceProcessing: initialVoiceProcessing,
            tracks: [microphoneProfile, systemProfile].compactMap { $0 })
        profile.voiceProcessingPolicy = expectedMicrophone ? policy : nil
        profile.routeChanges = locked { routeChanges }
        return profile
    }

    func start(directory: URL, microphoneEnabled: Bool, systemEnabled: Bool, voiceProcessingEnabled: Bool? = nil)
        async throws -> [String]
    {
        guard microphoneEnabled || systemEnabled else {
            throw MeetingError.message("Choose Microphone or System Audio in New Recording.")
        }
        expectedMicrophone = microphoneEnabled
        expectedSystem = systemEnabled
        policy = VoiceProcessingPolicy(override: voiceProcessingEnabled)
        levels.microphone.enabled = microphoneEnabled
        levels.system.enabled = systemEnabled
        var files: [String] = []
        do {
            let cancellationGeneration = await RecordingPermissions.currentCancellationGeneration
            try await RecordingPermissions.request(microphone: microphoneEnabled)
            try await RecordingPermissions.checkCancellation(since: cancellationGeneration)
            if systemEnabled {
                let recovery = makeSystemRecovery()
                systemRecovery = recovery
                // Trigger audio-only consent before starting the microphone. Frames
                // before the common recording epoch are discarded, not persisted.
                let session = try makeSystemSession(generation: 0)
                recovery.install(session)
                initialSystemFormat = session.format
                try await RecordingPermissions.checkCancellation(since: cancellationGeneration)
            }
            epoch = hostNow()
            if microphoneEnabled {
                let recovery = makeMicrophoneRecovery()
                microphoneRecovery = recovery
                let url = directory.appendingPathComponent("microphone.wav")
                let session = try makeMicrophoneSession(generation: 0) { [self] format, processed in
                    // A voice-processing fallback reuses the first writer; it converts formats.
                    if let microphoneWriter { return microphoneWriter }
                    epoch = hostNow()
                    try prepareSystemWriter(directory: directory)
                    let writer = try TimedAudioWriter(url: url, format: format, epoch: epoch, voiceProcessed: processed)
                    microphoneWriter = writer
                    return writer
                }
                recovery.install(session)
                initialVoiceProcessing = session.voiceProcessing
                initialMicrophoneFormat = session.format
                files.append(url.lastPathComponent)
            }
            if systemEnabled {
                if !microphoneEnabled { try prepareSystemWriter(directory: directory) }
                files.append("system.wav")
            }
            let now = ProcessInfo.processInfo.systemUptime
            locked {
                microphoneVoiceProcessing = initialVoiceProcessing
                microphoneDelivery.sessionStart = now
                systemDelivery.sessionStart = now
            }
            recordInitialRoutes()
            try observeDefaultDevices()
            startTimers()
            if let startupFailure = currentFailure() { throw startupFailure }
            return files
        }
        catch {
            try? await stop()
            throw error
        }
    }

    func stop() async throws {
        guard !finished else { return }
        finished = true
        meterTimer?.cancel()
        meterTimer = nil
        healthTimer?.cancel()
        healthTimer = nil
        recoveryTimer?.cancel()
        recoveryTimer = nil
        // Remove route listeners first so no notification can restart capture.
        removeRouteListeners()
        let deadline = DispatchTime.now() + Self.stopBound
        let system = systemRecovery
        let microphone = microphoneRecovery
        // Waiting blocks a thread, so keep it off Swift's cooperative pool.
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            DispatchQueue.global(qos: .userInitiated).async {
                // Stop the tap before VoiceProcessingIO, whose aggregate removal
                // must not look like a route loss. Both share one deadline.
                system?.stop(deadline: deadline)
                microphone?.stop(deadline: deadline)
                continuation.resume()
            }
        }
        // End both tracks at the same host time, padding a source that was still
        // reconnecting. Finished writers ignore late callbacks from abandoned sessions.
        let stopHost = hostNow()
        var stopError: Error?
        queue.sync {
            do { try systemWriter?.finish(throughHostSeconds: stopHost) }
            catch { stopError = error }
        }
        do { try microphoneWriter?.finish(throughHostSeconds: stopHost) }
        catch { stopError = error }
        if let captureFailure = currentFailure() { throw captureFailure }
        if let stopError { throw stopError }
        if let teardownError = locked({ teardownError }) { throw teardownError }
        // Frames count across reconnects, so a source that delivered earlier and
        // was reconnecting at stop does not trigger this error.
        let systemFrames = queue.sync { systemWriter?.capturedFrames ?? 0 }
        if (expectedMicrophone && (microphoneWriter?.capturedFrames ?? 0) == 0) || (expectedSystem && systemFrames == 0)
        {
            throw MeetingError.message(
                "A selected audio source delivered no samples. Available tracks were saved. Check microphone and system-audio permissions and the selected devices; system silence can also produce no samples."
            )
        }
    }

    // MARK: - Recovery wiring

    private func makeMicrophoneRecovery() -> CaptureSourceRecovery<MicrophoneSession> {
        let recovery = CaptureSourceRecovery<MicrophoneSession>(
            scheduler: QueueRecoveryScheduler(queue: queue),
            attemptQueue: DispatchQueue(label: "com.gdaymeetings.macos.microphone-recovery"),
            start: { [weak self] generation in
                guard let self else { throw CancellationError() }
                return try self.rebuildMicrophone(generation: generation)
            },
            stop: { $0.teardown() },
            isPermanent: { ($0 as? CaptureSourceError) == .microphoneAccessDenied })
        recovery.onStateChange = { [weak self] state in self?.sourceStateChanged(state) }
        recovery.onInstalled = { [weak self] session in
            guard let self else { return }
            self.locked {
                self.microphoneVoiceProcessing = session.voiceProcessing
                self.microphoneDelivery.sessionStart = ProcessInfo.processInfo.systemUptime
                self.microphoneDelivery.awaitingResume = self.microphoneDelivery.everDelivered
            }
            if let format = session.format {
                self.recordRoute(source: "microphone", format: format, voiceProcessed: session.voiceProcessing)
            }
        }
        recovery.onPermanentFailure = { [weak self] error in self?.sourceFailedPermanently(error) }
        return recovery
    }

    private func makeSystemRecovery() -> CaptureSourceRecovery<SystemAudioCapture> {
        let recovery = CaptureSourceRecovery<SystemAudioCapture>(
            scheduler: QueueRecoveryScheduler(queue: queue),
            attemptQueue: DispatchQueue(label: "com.gdaymeetings.macos.system-audio-recovery"),
            start: { [weak self] generation in
                guard let self else { throw CancellationError() }
                return try self.makeSystemSession(generation: generation)
            },
            stop: { [weak self] capture in
                do { try capture.stop() }
                catch { self?.rememberTeardownError(error) }
            },
            // System audio has no revocable runtime permission that Core Audio
            // reports; keep retrying while the recording is active.
            isPermanent: { _ in false })
        recovery.onStateChange = { [weak self] state in self?.sourceStateChanged(state) }
        recovery.onInstalled = { [weak self] capture in
            guard let self else { return }
            self.locked {
                self.systemDelivery.sessionStart = ProcessInfo.processInfo.systemUptime
                self.systemDelivery.awaitingResume = self.systemDelivery.everDelivered
            }
            if let format = capture.format {
                self.recordRoute(source: "system", format: format, voiceProcessed: false)
            }
        }
        return recovery
    }

    private func makeSystemSession(generation: Int) throws -> SystemAudioCapture {
        let capture = SystemAudioCapture(queue: queue)
        capture.onFailure = { [weak self] error in self?.report(error) }
        capture.onInterrupted = { [weak self] in
            self?.systemRecovery?.sessionInterrupted(generation: generation)
        }
        capture.onBuffer = { [weak self] buffer, timestamp in
            guard let self, let writer = self.systemWriter, timestamp >= self.epoch else { return }
            try writer.append(buffer, hostSeconds: timestamp)
            self.measure(buffer, microphone: false)
        }
        try capture.start()
        return capture
    }

    /// Builds a fresh engine on the current default input for an existing writer.
    private func rebuildMicrophone(generation: Int) throws -> MicrophoneSession {
        // Revoked access cannot recover by retrying; other sources keep recording.
        guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else {
            throw CaptureSourceError.microphoneAccessDenied
        }
        guard let writer = microphoneWriter else { throw CancellationError() }
        return try makeMicrophoneSession(generation: generation) { _, _ in writer }
    }

    /// Automatic policy re-reads the output route on every build and falls back
    /// to unprocessed capture when the route rejects voice processing. An
    /// explicit On keeps failing so recovery retries with a clear status.
    private func makeMicrophoneSession(
        generation: Int, writer: (AVAudioFormat, Bool) throws -> TimedAudioWriter
    ) throws -> MicrophoneSession {
        let processed = policy.enabled()
        do { return try buildMicrophoneEngine(generation: generation, voiceProcessing: processed, writer: writer) }
        catch  where processed && policy == .automatic {
            return try buildMicrophoneEngine(generation: generation, voiceProcessing: false, writer: writer)
        }
    }

    private func buildMicrophoneEngine(
        generation: Int, voiceProcessing requested: Bool,
        writer makeWriter: (AVAudioFormat, Bool) throws -> TimedAudioWriter
    ) throws -> MicrophoneSession {
        let session = MicrophoneSession()
        do {
            try configure(session, generation: generation, voiceProcessing: requested, writer: makeWriter)
            return session
        }
        catch {
            session.teardown()
            throw error
        }
    }

    private func configure(
        _ session: MicrophoneSession, generation: Int, voiceProcessing requested: Bool,
        writer makeWriter: (AVAudioFormat, Bool) throws -> TimedAudioWriter
    ) throws {
        let engine = session.engine
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
        if requested {
            do { try input.setVoiceProcessingEnabled(true) }
            catch {
                let cause = error as NSError
                throw CaptureSourceError.voiceProcessing(
                    "Could not enable Apple microphone voice processing (\(cause.domain) \(cause.code)): \(cause.localizedDescription)"
                )
            }
            session.voiceProcessing = input.isVoiceProcessingEnabled
            guard session.voiceProcessing else {
                throw CaptureSourceError.voiceProcessing(
                    "Apple voice processing is unavailable on this audio route. Turn off Microphone Voice Processing in New Recording or choose another device."
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
        if session.voiceProcessing {
            // Request a mono processed-speech client from the Audio Unit itself.
            // installTap's explicit format configures this otherwise-unconnected
            // output bus; no application-side selection/downmix of aggregate
            // channels is performed. Raw capture keeps its device channel layout.
            // https://developer.apple.com/documentation/avfaudio/avaudionode/installtap(onbus:buffersize:format:block:)
            guard
                let speechFormat = AVAudioFormat(
                    standardFormatWithSampleRate: microphoneDeviceFormat.sampleRate, channels: 1)
            else {
                throw CaptureSourceError.voiceProcessing("Could not configure the mono voice-processing client format.")
            }
            format = speechFormat
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
                throw CaptureSourceError.voiceProcessing(
                    "Apple voice processing requires matching client formats. Microphone: \(format); output: \(outputFormat)."
                )
            }
        }
        else {
            format = input.outputFormat(forBus: 0)
        }
        session.format = format
        let writer = try makeWriter(format, session.voiceProcessing)
        // An ordinary input tap is deliberately used rather than a realtime sink.
        // Its samples are copied into Core Audio's bounded asynchronous writer,
        // which converts a rebuilt engine's format to the track format.
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, time in
            guard let self else { return }
            guard time.isHostTimeValid else {
                // Without host time the buffer cannot be placed; rebuild the engine.
                self.microphoneRecovery?.sessionInterrupted(generation: generation)
                return
            }
            do {
                try writer.append(buffer, hostSeconds: AVAudioTime.seconds(forHostTime: time.hostTime))
                self.measure(buffer, microphone: true)
            }
            catch { self.report(error) }
        }
        session.tapInstalled = true
        if session.voiceProcessing {
            let negotiatedInput = input.outputFormat(forBus: 0)
            let negotiatedOutput = engine.outputNode.inputFormat(forBus: 0)
            guard negotiatedInput == format, negotiatedOutput == format else {
                throw CaptureSourceError.voiceProcessing(
                    "Apple voice processing did not accept the mono client format. Input: \(negotiatedInput); output: \(negotiatedOutput). Turn off Microphone Voice Processing in New Recording and try again."
                )
            }
        }
        engine.prepare()
        do { try engine.start() }
        catch {
            let cause = error as NSError
            let mode = session.voiceProcessing ? "voice-processed" : "unprocessed"
            let output = session.voiceProcessing ? "; output client \(engine.outputNode.inputFormat(forBus: 0))" : ""
            throw MeetingError.message(
                "Could not start \(mode) microphone capture (\(cause.domain) \(cause.code)). Input client \(format)\(output). \(cause.localizedDescription)"
            )
        }
        guard engine.isRunning else {
            throw MeetingError.message("The microphone engine could not start on this route.")
        }
        // A device, sample-rate, or channel change stops this engine. Recovery
        // builds a fresh engine on the current default input for the same track.
        // https://developer.apple.com/documentation/avfaudio/avaudioengineconfigurationchangenotification
        session.configurationObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            self?.microphoneRecovery?.routeChanged(generation: generation)
        }
    }

    /// Follows the macOS default devices. Comparing device IDs ignores
    /// notifications caused by capture's own aggregate and voice-processing
    /// changes, which do not change the user's default devices.
    private func observeDefaultDevices() throws {
        knownInput = RecordingAudioRoute.defaultDevice(output: false)
        knownOutput = RecordingAudioRoute.defaultDevice(output: true)
        if expectedMicrophone {
            try observe(kAudioHardwarePropertyDefaultInputDevice) { [weak self] in
                self?.defaultDeviceChanged(output: false)
            }
        }
        try observe(kAudioHardwarePropertyDefaultOutputDevice) { [weak self] in self?.defaultDeviceChanged(output: true)
        }
    }

    private func observe(_ selector: AudioObjectPropertySelector, _ handler: @escaping () -> Void) throws {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        let block: AudioObjectPropertyListenerBlock = { _, _ in handler() }
        let object = AudioObjectID(kAudioObjectSystemObject)
        let status = AudioObjectAddPropertyListenerBlock(object, &address, queue, block)
        guard status == noErr else {
            throw MeetingError.message("Could not observe audio device changes (Core Audio \(status)).")
        }
        routeListeners.append((object, address, block))
    }

    private func removeRouteListeners() {
        for (object, var address, block) in routeListeners {
            AudioObjectRemovePropertyListenerBlock(object, &address, queue, block)
        }
        routeListeners.removeAll()
    }

    /// Runs on queue.
    private func defaultDeviceChanged(output: Bool) {
        let device = RecordingAudioRoute.defaultDevice(output: output)
        if output {
            guard device != knownOutput else { return }
            knownOutput = device
            systemRecovery?.routeChanged()
            // Voice processing couples to the output device, so a processed engine
            // must move with it. An unprocessed engine only rebuilds when automatic
            // mode now wants processing; headphone-to-headphone switches keep the mic running.
            let processed = locked { microphoneVoiceProcessing }
            if processed || (policy == .automatic && policy.enabled()) { microphoneRecovery?.routeChanged() }
        }
        else {
            guard device != knownInput else { return }
            knownInput = device
            microphoneRecovery?.routeChanged()
        }
    }

    private func sourceStateChanged(_ state: CaptureSourceState) {
        publishHealth()
        // Start padding right away so a resumed source never appends after a large backlog.
        if state == .reconnecting { queue.async { [weak self] in self?.superviseSources() } }
    }

    private func sourceFailedPermanently(_ error: Error) {
        publishHealth()
        let microphoneFailed = !expectedMicrophone || microphoneRecovery?.state == .failed
        let systemFailed = !expectedSystem || systemRecovery?.state == .failed
        if microphoneFailed && systemFailed { report(error) }
    }

    private func rememberTeardownError(_ error: Error) {
        // Recovery teardown errors are expected on a lost device; report only final-stop failures.
        guard finished else { return }
        locked { if teardownError == nil { teardownError = error } }
    }

    // MARK: - Timers, health, and metadata

    private func startTimers() {
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 10, repeating: 10)
        timer.setEventHandler { [weak self] in self?.publishHealth() }
        healthTimer = timer
        timer.resume()
        let recoveryTimer = DispatchSource.makeTimerSource(queue: queue)
        recoveryTimer.schedule(deadline: .now() + 1, repeating: 1, leeway: .milliseconds(100))
        recoveryTimer.setEventHandler { [weak self] in self?.superviseSources() }
        self.recoveryTimer = recoveryTimer
        recoveryTimer.resume()
        let meterTimer = DispatchSource.makeTimerSource(queue: queue)
        meterTimer.schedule(deadline: .now(), repeating: .milliseconds(100), leeway: .milliseconds(20))
        meterTimer.setEventHandler { [weak self] in
            guard let self else { return }
            let pending: Bool = self.locked {
                let pending = self.meterDeliveryPending
                if !pending { self.meterDeliveryPending = true }
                return pending
            }
            guard !pending else { return }
            let finish = { [weak self] in
                guard let self else { return }
                self.locked { self.meterDeliveryPending = false }
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
    }

    /// Runs on queue once per second: keeps reconnecting tracks at pace with the
    /// host clock, and rebuilds a source whose engine runs without delivering.
    private func superviseSources() {
        let host = hostNow()
        let now = ProcessInfo.processInfo.systemUptime
        let sources: [(CaptureSourceState?, TimedAudioWriter?, SourceDelivery, (() -> Void)?)] = [
            (
                microphoneRecovery?.state, microphoneWriter, locked { microphoneDelivery },
                microphoneRecovery.map { recovery in { recovery.sessionInterrupted() } }
            ),
            (
                systemRecovery?.state, systemWriter, locked { systemDelivery },
                systemRecovery.map { recovery in { recovery.sessionInterrupted() } }
            ),
        ]
        for (state, writer, delivery, interrupt) in sources {
            switch state {
            case .reconnecting?, .failed?:
                // Pad in one-second steps so the writer never flushes a long outage
                // at once. Stay 0.5 s behind now so a resumed buffer is not trimmed.
                do { try writer?.padSilence(throughHostSeconds: host - 0.5) }
                catch { report(error) }
            case .running?:
                if delivery.everDelivered, now - max(delivery.last, delivery.sessionStart) > Self.deliveryWatchdog {
                    interrupt?()
                }
            default: break
            }
        }
    }

    private func publishHealth() {
        guard let onHealth else { return }
        onHealth(healthText())
    }

    private func healthText() -> String {
        let (microphone, system) = locked { (microphoneDelivery, systemDelivery) }
        let processed = locked { microphoneVoiceProcessing }
        var statuses: [String] = []
        if expectedMicrophone, let recovery = microphoneRecovery {
            switch recovery.state {
            case .failed:
                statuses.append("Microphone stopped · Allow microphone access in System Settings")
            case .reconnecting:
                if policy == .on, case .voiceProcessing(_)? = recovery.lastAttemptError as? CaptureSourceError {
                    statuses.append("Reconnecting microphone… Voice Processing is unavailable on the current device")
                }
                else {
                    statuses.append("Reconnecting microphone…")
                }
            case .running where microphone.awaitingResume:
                statuses.append("Reconnecting microphone…")
            default:
                statuses.append(
                    (microphoneWriter?.capturedFrames ?? 0) > 0
                        ? (processed
                            ? "Microphone receiving · Apple voice processing" : "Microphone receiving · unprocessed")
                        : "Microphone: no audio samples received")
            }
        }
        if expectedSystem, let recovery = systemRecovery {
            if recovery.state == .reconnecting || (recovery.state == .running && system.awaitingResume) {
                statuses.append("Reconnecting system audio…")
            }
            else {
                statuses.append(
                    (systemWriter?.capturedFrames ?? 0) > 0
                        ? "System audio receiving · separate track"
                        : "System audio: no samples yet; the source may be silent")
            }
        }
        return statuses.joined(separator: " · ")
    }

    private func recordInitialRoutes() {
        if let format = initialMicrophoneFormat {
            recordRoute(source: "microphone", format: format, voiceProcessed: initialVoiceProcessing)
        }
        if let format = initialSystemFormat {
            recordRoute(source: "system", format: format, voiceProcessed: false)
        }
    }

    private func recordRoute(source: String, format: AVAudioFormat, voiceProcessed: Bool) {
        let device = RecordingAudioRoute.defaultDevice(output: source == "system").flatMap(
            RecordingAudioRoute.deviceName)
        let change = RecordingRouteChange(
            time: max(0, hostNow() - epoch), source: source, device: device, sampleRate: format.sampleRate,
            channels: format.channelCount, voiceProcessed: voiceProcessed)
        locked { routeChanges.append(change) }
    }

    private func measure(_ buffer: AVAudioPCMBuffer, microphone: Bool) {
        let measured = RecordingSourceLevel.measure(buffer)
        let now = ProcessInfo.processInfo.systemUptime
        let resumed: Bool = locked {
            if microphone {
                levels.microphone = measured
                defer { microphoneDelivery.awaitingResume = false }
                microphoneDelivery.everDelivered = true
                microphoneDelivery.last = now
                return microphoneDelivery.awaitingResume
            }
            levels.system = measured
            defer { systemDelivery.awaitingResume = false }
            systemDelivery.everDelivered = true
            systemDelivery.last = now
            return systemDelivery.awaitingResume
        }
        // Clear the reconnecting status as soon as audio arrives again.
        if resumed { publishHealth() }
    }

    private func levelSnapshot() -> RecordingLevels {
        let now = ProcessInfo.processInfo.systemUptime
        let microphoneState = microphoneRecovery?.state
        let systemState = systemRecovery?.state
        return locked {
            var snapshot = levels
            if now - microphoneDelivery.last > 1 { snapshot.microphone.stale = true }
            if now - systemDelivery.last > 1 { snapshot.system.stale = true }
            snapshot.microphone.reconnecting =
                microphoneState == .reconnecting || (microphoneState == .running && microphoneDelivery.awaitingResume)
            snapshot.system.reconnecting =
                systemState == .reconnecting || (systemState == .running && systemDelivery.awaitingResume)
            return snapshot
        }
    }

    private func currentFailure() -> Error? { locked { failure } }

    private func report(_ error: Error) {
        let first: Bool = locked {
            guard failure == nil else { return false }
            failure = error
            return true
        }
        if first { onFailure?(error) }
    }

    private func prepareSystemWriter(directory: URL) throws {
        guard let format = initialSystemFormat else { return }
        try queue.sync {
            systemWriter = try TimedAudioWriter(
                url: directory.appendingPathComponent("system.wav"), format: format, epoch: epoch)
        }
    }

    private func hostNow() -> TimeInterval { CMClockGetTime(CMClockGetHostTimeClock()).seconds }

    private func locked<T>(_ body: () -> T) -> T {
        stateLock.lock()
        defer { stateLock.unlock() }
        return body()
    }
}

/// One microphone engine. Recovery discards it and builds another rather than
/// restarting an engine whose device or format changed.
final class MicrophoneSession {
    let engine = AVAudioEngine()
    var format: AVAudioFormat?
    var voiceProcessing = false
    var configurationObserver: NSObjectProtocol?
    var tapInstalled = false

    /// Remove the observer first so teardown cannot trigger another rebuild.
    func teardown() {
        if let configurationObserver {
            NotificationCenter.default.removeObserver(configurationObserver)
            self.configurationObserver = nil
        }
        if tapInstalled {
            engine.inputNode.removeTap(onBus: 0)
            tapInstalled = false
        }
        engine.stop()
    }
}

enum CaptureSourceError: LocalizedError, Equatable {
    /// Microphone access was revoked during recording; retrying cannot recover it.
    case microphoneAccessDenied
    /// Voice processing failed on the current route. Automatic policy falls back.
    case voiceProcessing(String)

    var errorDescription: String? {
        switch self {
        case .microphoneAccessDenied:
            return "Microphone access is off. Available audio was saved. Allow microphone access in System Settings."
        case .voiceProcessing(let message):
            return message
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
    /// The source's device is being replaced; earlier levels no longer apply.
    var reconnecting = false
    var rmsDB: Double = -120
    var peakDB: Double = -120
    var level: Double { enabled && hasSamples && !stale && !reconnecting ? min(1, max(0, (rmsDB + 60) / 60)) : 0 }
    var statusText: String {
        if !enabled { return "Not recording" }
        if reconnecting { return "Reconnecting…" }
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
