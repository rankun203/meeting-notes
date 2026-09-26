import Foundation
import Testing

@testable import GdayMeetings

/// Virtual time for debounce and backoff. Attempts still run on a real serial
/// queue; `advance` drains it after each timer so tests stay deterministic.
private final class VirtualScheduler: RecoveryScheduling, @unchecked Sendable {
    private let lock = NSLock()
    private var items: [(id: Int, time: TimeInterval, work: () -> Void)] = []
    private var nextID = 0
    private var clock: TimeInterval = 0
    private var requested: [TimeInterval] = []

    var now: TimeInterval { lock.withLock { clock } }
    var delays: [TimeInterval] { lock.withLock { requested } }

    func schedule(after delay: TimeInterval, _ work: @escaping () -> Void) -> () -> Void {
        let id: Int = lock.withLock {
            nextID += 1
            items.append((nextID, clock + delay, work))
            requested.append(delay)
            return nextID
        }
        return { [weak self] in self?.lock.withLock { self?.items.removeAll { $0.id == id } } }
    }

    func advance(by interval: TimeInterval, draining queue: DispatchQueue?) {
        let end = now + interval
        while true {
            let next: (id: Int, time: TimeInterval, work: () -> Void)? = lock.withLock {
                guard let next = items.filter({ $0.time <= end }).min(by: { $0.time < $1.time }) else {
                    clock = end
                    return nil
                }
                items.removeAll { $0.id == next.id }
                clock = next.time
                return next
            }
            guard let next else { return }
            next.work()
            queue?.sync {}
        }
    }
}

private final class FakeSession {
    let id: Int
    let generation: Int
    var tornDown = false
    init(id: Int, generation: Int) {
        self.id = id
        self.generation = generation
    }
}

private struct TransientFailure: Error {}
private struct PermissionRevoked: Error {}

/// Records every start/stop and lets each test decide an attempt's outcome.
private final class Harness: @unchecked Sendable {
    let scheduler = VirtualScheduler()
    let queue = DispatchQueue(label: "test.recovery")
    let lock = NSLock()
    var attempts = 0
    var started: [FakeSession] = []
    var installed: [FakeSession] = []
    var states: [CaptureSourceState] = []
    var permanentFailures = 0
    var outcome: (_ generation: Int, _ attempt: Int) throws -> Void = { _, _ in }
    lazy var recovery: CaptureSourceRecovery<FakeSession> = {
        let recovery = CaptureSourceRecovery<FakeSession>(
            scheduler: scheduler, attemptQueue: queue,
            start: { [unowned self] generation in
                let attempt: Int = self.lock.withLock {
                    defer { self.attempts += 1 }
                    return self.attempts
                }
                try self.outcome(generation, attempt)
                let session = FakeSession(id: attempt + 1, generation: generation)
                self.lock.withLock { self.started.append(session) }
                return session
            },
            stop: { [unowned self] session in self.lock.withLock { session.tornDown = true } },
            isPermanent: { $0 is PermissionRevoked })
        recovery.onInstalled = { [unowned self] session in self.lock.withLock { self.installed.append(session) } }
        recovery.onStateChange = { [unowned self] state in self.lock.withLock { self.states.append(state) } }
        recovery.onPermanentFailure = { [unowned self] _ in self.lock.withLock { self.permanentFailures += 1 } }
        return recovery
    }()

    /// Installs a pre-started session, as AudioCapture does at recording start.
    @discardableResult func installInitial() -> FakeSession {
        let session = FakeSession(id: 0, generation: 0)
        recovery.install(session)
        return session
    }

    func advance(_ seconds: TimeInterval) { scheduler.advance(by: seconds, draining: queue) }
}

struct CaptureSourceRecoveryTests {
    @Test func recoversAfterRepeatedFailuresBeyondThirtySeconds() {
        let harness = Harness()
        let initial = harness.installInitial()
        harness.outcome = { [unowned harness] _, _ in
            if harness.scheduler.now < 35 { throw TransientFailure() }
        }
        harness.recovery.routeChanged()
        #expect(harness.recovery.state == .reconnecting)
        harness.advance(60)
        #expect(harness.recovery.state == .running)
        #expect(initial.tornDown)
        #expect(harness.installed.count == 1)
        #expect(harness.installed.first?.tornDown == false)
        // Debounce, then doubling backoff that caps at 5 s and keeps retrying.
        let delays = harness.scheduler.delays
        #expect(Array(delays.prefix(7)) == [0.3, 0.25, 0.5, 1, 2, 4, 5])
        #expect(delays.max() == 5)
        #expect(delays.dropFirst(6).allSatisfy { $0 == 5 })
        #expect(harness.states == [.reconnecting, .running])
    }

    @Test func routeChangeWhileReconnectingResetsBackoff() {
        let harness = Harness()
        harness.installInitial()
        harness.outcome = { _, _ in throw TransientFailure() }
        harness.recovery.routeChanged()
        harness.advance(20)
        #expect(harness.scheduler.delays.last == 5)
        let before = harness.scheduler.delays.count
        harness.outcome = { _, _ in }
        harness.recovery.routeChanged()
        harness.advance(0.3)
        #expect(Array(harness.scheduler.delays.dropFirst(before)) == [0.3])
        #expect(harness.recovery.state == .running)
        // A later failure starts again from the shortest backoff.
        harness.outcome = { _, _ in throw TransientFailure() }
        harness.recovery.routeChanged()
        harness.advance(0.3)
        #expect(harness.scheduler.delays.last == 0.25)
    }

    @Test func supersededAttemptIsTornDownAndNeverInstalled() {
        let harness = Harness()
        harness.installInitial()
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        harness.outcome = { generation, _ in
            if generation == 1 {
                entered.signal()
                release.wait()
            }
        }
        harness.recovery.routeChanged()
        harness.scheduler.advance(by: 0.3, draining: nil)
        entered.wait()
        // A newer route arrives while attempt 1 is inside a native call.
        harness.recovery.routeChanged()
        release.signal()
        harness.queue.sync {}
        let stale = harness.started.first { $0.generation == 1 }
        #expect(stale?.tornDown == true)
        #expect(harness.installed.isEmpty)
        harness.advance(0.3)
        #expect(harness.installed.map(\.generation) == [2])
        #expect(harness.installed.first?.tornDown == false)
        #expect(harness.recovery.state == .running)
    }

    @Test func lateCallbacksFromReplacedSessionsAreIgnored() {
        let harness = Harness()
        harness.installInitial()
        harness.recovery.routeChanged()
        harness.advance(0.3)
        #expect(harness.installed.count == 1)
        let count = harness.scheduler.delays.count
        harness.recovery.sessionInterrupted(generation: 0)
        harness.recovery.routeChanged(generation: 0)
        #expect(harness.recovery.state == .running)
        #expect(harness.scheduler.delays.count == count)
        harness.recovery.sessionInterrupted(generation: harness.installed[0].generation)
        #expect(harness.recovery.state == .reconnecting)
    }

    @Test func stopDuringDebounceNeverRestarts() {
        let harness = Harness()
        let initial = harness.installInitial()
        harness.recovery.routeChanged()
        #expect(harness.recovery.stop(deadline: .now() + 1))
        harness.recovery.routeChanged()
        harness.recovery.sessionInterrupted()
        harness.advance(60)
        #expect(harness.started.isEmpty)
        #expect(initial.tornDown)
        #expect(harness.recovery.state == .stopped)
    }

    @Test func stopDuringBackoffNeverRestarts() {
        let harness = Harness()
        harness.installInitial()
        harness.outcome = { _, _ in throw TransientFailure() }
        harness.recovery.routeChanged()
        harness.advance(0.4)
        #expect(harness.recovery.state == .reconnecting)
        #expect(harness.recovery.lastAttemptError is TransientFailure)
        harness.outcome = { _, _ in }
        #expect(harness.recovery.stop(deadline: .now() + 1))
        harness.advance(60)
        #expect(harness.started.isEmpty)
        #expect(harness.recovery.state == .stopped)
    }

    @Test func stopReturnsWithinBoundWhileAttemptIsStuck() {
        let harness = Harness()
        harness.installInitial()
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        harness.outcome = { _, _ in
            entered.signal()
            release.wait()
        }
        harness.recovery.routeChanged()
        harness.scheduler.advance(by: 0.3, draining: nil)
        entered.wait()
        let began = Date()
        let completed = harness.recovery.stop(deadline: .now() + 0.2)
        let elapsed = Date().timeIntervalSince(began)
        #expect(!completed)
        #expect(elapsed < 1)
        // The abandoned attempt eventually returns and cleans up after itself.
        release.signal()
        harness.queue.sync {}
        #expect(harness.started.count == 1)
        #expect(harness.started.first?.tornDown == true)
        #expect(harness.installed.isEmpty)
        harness.recovery.routeChanged()
        harness.advance(60)
        #expect(harness.started.count == 1)
        #expect(harness.recovery.state == .stopped)
    }

    @Test func permanentFailureStopsOnlyThatSource() {
        let microphone = Harness()
        let system = Harness()
        microphone.installInitial()
        system.installInitial()
        microphone.outcome = { _, _ in throw PermissionRevoked() }
        system.outcome = { _, attempt in if attempt == 0 { throw TransientFailure() } }
        microphone.recovery.routeChanged()
        system.recovery.routeChanged()
        microphone.advance(60)
        system.advance(60)
        #expect(microphone.recovery.state == .failed)
        #expect(microphone.permanentFailures == 1)
        #expect(microphone.recovery.permanentFailure is PermissionRevoked)
        #expect(microphone.scheduler.delays == [0.3])
        // A failed source ignores later route changes instead of retrying.
        microphone.recovery.routeChanged()
        microphone.advance(60)
        #expect(microphone.scheduler.delays == [0.3])
        #expect(system.recovery.state == .running)
        #expect(system.installed.count == 1)
    }

    @Test func automaticVoiceProcessingFollowsEachRebuild() {
        for policy in [VoiceProcessingPolicy.automatic, .on, .off] {
            let harness = Harness()
            harness.installInitial()
            var speakers = true
            var decisions: [Bool] = []
            harness.outcome = { _, _ in decisions.append(policy.enabled(speakerRoute: { speakers })) }
            // Speakers → headphones → speakers.
            for route in [true, false, true] {
                speakers = route
                harness.recovery.routeChanged()
                harness.advance(1)
            }
            switch policy {
            case .automatic: #expect(decisions == [true, false, true])
            case .on: #expect(decisions == [true, true, true])
            case .off: #expect(decisions == [false, false, false])
            }
        }
    }

    /// Each rebuilt session triggers the next rebuild before delivering audio,
    /// as a device selection's own configuration change did. The loop guard
    /// turns the 0.3 s flapping into a backoff that caps at 5 s.
    @Test func rebuildLoopBacksOffUntilAudioArrives() {
        let harness = Harness()
        harness.installInitial()
        harness.recovery.routeChanged(generation: 0)
        harness.advance(0.3)
        for _ in 0..<8 {
            guard let latest = harness.installed.last else { break }
            harness.recovery.routeChanged(generation: latest.generation)
            harness.advance(10)
        }
        #expect(harness.scheduler.delays == [0.3, 0.3, 0.5, 1, 2, 4, 5, 5, 5])
        #expect(harness.installed.count == 9)
        // Audio from the latest session clears the guard: the next route change is prompt.
        harness.recovery.sessionDelivered()
        harness.recovery.routeChanged(generation: harness.installed.last?.generation)
        #expect(harness.scheduler.delays.last == 0.3)
    }

    @Test func deliveringSessionsKeepThePromptDebounce() {
        let harness = Harness()
        harness.installInitial()
        for _ in 0..<5 {
            harness.recovery.sessionDelivered()
            harness.recovery.routeChanged()
            harness.advance(0.3)
        }
        #expect(harness.scheduler.delays == [0.3, 0.3, 0.3, 0.3, 0.3])
        #expect(harness.installed.count == 5)
    }

    @Test func deliveryReportsOutsideRunningAreIgnored() {
        let harness = Harness()
        harness.installInitial()
        harness.recovery.routeChanged()
        // A buffer from a session that is being replaced does not clear the guard.
        harness.recovery.sessionDelivered()
        harness.advance(0.3)
        harness.recovery.routeChanged()
        harness.advance(1)
        harness.recovery.routeChanged()
        #expect(harness.scheduler.delays == [0.3, 0.3, 0.5])
    }

    @Test func selectedMicrophoneFallsBackOnceAndRetriesAfterReconnect() {
        var fallback = SelectedMicrophoneFallback(connected: true)
        #expect(fallback.allowsSelected(connected: true))
        #expect(!fallback.allowsSelected(connected: false))
        // A bind, start, or delivery failure uses the default input from now on.
        fallback.selectedFailed()
        #expect(fallback.failed)
        #expect(!fallback.allowsSelected(connected: true))
        // Device-list noise (capture's own aggregates) never rebuilds.
        var noise: [Bool] = []
        for _ in 0..<5 { noise.append(fallback.connectionChanged(connected: true, usingSelected: false)) }
        #expect(noise.allSatisfy { !$0 })
        // Disconnecting while on the default input needs no rebuild.
        let disconnectedUnused = fallback.connectionChanged(connected: false, usingSelected: false)
        #expect(!disconnectedUnused)
        // Reconnecting gives the device another chance.
        let reconnected = fallback.connectionChanged(connected: true, usingSelected: false)
        #expect(reconnected)
        #expect(!fallback.failed)
        #expect(fallback.allowsSelected(connected: true))
        // Losing the device in use rebuilds onto the default input.
        let disconnectedInUse = fallback.connectionChanged(connected: false, usingSelected: true)
        #expect(disconnectedInUse)
    }

    @Test func ownConfigurationChangeDoesNotRebuild() {
        let started = MicrophoneConfigurationChange(running: true, device: 109, sampleRate: 48_000, channels: 1)
        #expect(!started.requiresRebuild(since: started))
        var stopped = started
        stopped.running = false
        #expect(stopped.requiresRebuild(since: started))
        var otherDevice = started
        otherDevice.device = 180
        #expect(otherDevice.requiresRebuild(since: started))
        var otherRate = started
        otherRate.sampleRate = 24_000
        #expect(otherRate.requiresRebuild(since: started))
        var otherChannels = started
        otherChannels.channels = 2
        #expect(otherChannels.requiresRebuild(since: started))
    }

    @Test func emptySourceMessageNamesTheSource() {
        let microphone = CaptureSourceError.noAudio(microphone: true, systemAudio: false, otherTrackSaved: true)
        let parts = LibraryView.alertParts(microphone.localizedDescription)
        #expect(parts.title == "Microphone recorded no audio.")
        #expect(parts.message.hasPrefix("The System Audio track was saved. "))
        #expect(!parts.message.contains("kept"))
        let system = CaptureSourceError.noAudio(microphone: false, systemAudio: true, otherTrackSaved: false)
        #expect(LibraryView.alertParts(system.localizedDescription).title == "System Audio recorded no audio.")
        #expect(!system.localizedDescription.contains("saved"))
        let both = CaptureSourceError.noAudio(microphone: true, systemAudio: true, otherTrackSaved: false)
        #expect(
            LibraryView.alertParts(both.localizedDescription).title == "Microphone and System Audio recorded no audio.")
        #expect(both.isNoAudio)
        #expect(!CaptureSourceError.microphoneAccessDenied.isNoAudio)
    }

    @Test func reconnectingLevelsResetMetersAndStatus() {
        let level = RecordingSourceLevel(enabled: true, hasSamples: true, reconnecting: true, rmsDB: -10)
        #expect(level.statusText == "Reconnecting…")
        #expect(level.level == 0)
        #expect(RecordingSourceLevel(reconnecting: true).statusText == "Not recording")
        var levels = RecordingLevels(
            microphone: RecordingSourceLevel(enabled: true, reconnecting: true),
            system: RecordingSourceLevel(enabled: true))
        #expect(RecordingWorkspaceView.reconnectingStatus(levels) == "Reconnecting microphone…")
        levels.system.reconnecting = true
        #expect(RecordingWorkspaceView.reconnectingStatus(levels) == "Reconnecting microphone and system audio…")
        levels.microphone.reconnecting = false
        #expect(RecordingWorkspaceView.reconnectingStatus(levels) == "Reconnecting system audio…")
        levels.system.reconnecting = false
        #expect(RecordingWorkspaceView.reconnectingStatus(levels) == nil)
    }
}
