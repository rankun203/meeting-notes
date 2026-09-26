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
        #expect(VoiceProcessingPolicy(override: nil) == .automatic)
        #expect(VoiceProcessingPolicy(override: true) == .on)
        #expect(VoiceProcessingPolicy(override: false) == .off)
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
