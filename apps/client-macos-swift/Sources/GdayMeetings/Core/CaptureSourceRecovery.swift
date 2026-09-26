import Foundation

/// Schedules debounce and backoff timers. Tests inject a virtual clock so
/// recovery timing is verified without real sleeps.
protocol RecoveryScheduling: AnyObject {
    /// Runs `work` after `delay` seconds. The returned closure cancels it.
    func schedule(after delay: TimeInterval, _ work: @escaping () -> Void) -> () -> Void
}

final class QueueRecoveryScheduler: RecoveryScheduling {
    private let queue: DispatchQueue
    init(queue: DispatchQueue) { self.queue = queue }
    func schedule(after delay: TimeInterval, _ work: @escaping () -> Void) -> () -> Void {
        let item = DispatchWorkItem(block: work)
        queue.asyncAfter(deadline: .now() + delay, execute: item)
        return { item.cancel() }
    }
}

enum CaptureSourceState: Equatable { case running, reconnecting, failed, stopped }

/// Keeps one capture source (microphone engine or system tap) alive for the
/// whole recording. Route, format, and delivery problems rebuild the source;
/// only a classifier-approved permanent error stops retrying it.
///
/// - Every rebuild request bumps `generation`. An attempt that finishes after a
///   newer request, or after `stop`, tears down what it built and installs nothing.
/// - Attempts run one at a time on `attemptQueue`, never on the caller's thread,
///   so a slow Core Audio call cannot block route notifications or Stop & Save.
/// - A stuck native call is abandoned, not cancelled: `stop` waits only until its
///   deadline. The attempt keeps its thread until the call returns, then sees the
///   stale generation and releases its own resources. Isolating capture in a
///   disposable helper process would bound this fully; that is deferred.
final class CaptureSourceRecovery<Session>: @unchecked Sendable {
    typealias State = CaptureSourceState

    struct Timing {
        var debounce: TimeInterval = 0.3
        var initialBackoff: TimeInterval = 0.25
        var maximumBackoff: TimeInterval = 5
    }

    private let timing: Timing
    private let scheduler: RecoveryScheduling
    private let attemptQueue: DispatchQueue
    private let startSession: (_ generation: Int) throws -> Session
    private let stopSession: (Session) -> Void
    private let isPermanent: (Error) -> Bool
    private let lock = NSLock()
    private let inFlight = DispatchGroup()
    private var current: Session?
    private var generation = 0
    private var installedGeneration = 0
    private var backoff: TimeInterval
    private var cancelPending: (() -> Void)?
    private var failure: Error?
    private var lastError: Error?
    private var stateValue = State.stopped

    /// Called outside the lock after each state transition.
    var onStateChange: ((State) -> Void)?
    /// Called after a rebuilt session is installed; not called for the initial one.
    var onInstalled: ((Session) -> Void)?
    /// Called once when `isPermanent` classifies an attempt's error.
    var onPermanentFailure: ((Error) -> Void)?

    init(
        timing: Timing = Timing(), scheduler: RecoveryScheduling, attemptQueue: DispatchQueue,
        start: @escaping (_ generation: Int) throws -> Session, stop: @escaping (Session) -> Void,
        isPermanent: @escaping (Error) -> Bool
    ) {
        self.timing = timing
        self.scheduler = scheduler
        self.attemptQueue = attemptQueue
        startSession = start
        stopSession = stop
        self.isPermanent = isPermanent
        backoff = timing.initialBackoff
    }

    var state: State { locked { stateValue } }
    /// The most recent attempt error while reconnecting, for status detail.
    var lastAttemptError: Error? { locked { stateValue == .reconnecting ? lastError : nil } }
    var permanentFailure: Error? { locked { failure } }

    /// Adopts a session the caller started before recording began. Its callbacks
    /// must report `generation` 0.
    func install(_ session: Session) {
        locked {
            current = session
            installedGeneration = generation
            stateValue = .running
        }
    }

    /// A default device or engine configuration changed. Debounced; resets
    /// backoff so a new route is tried promptly even after long outages.
    /// A session-scoped report (`generation` set) is ignored once that session
    /// has been replaced.
    func routeChanged(generation reported: Int? = nil) {
        guard accepts(reported) else { return }
        request(resetBackoff: true)
    }

    /// The installed session stopped delivering or reported a recoverable fault.
    /// `nil` means whichever session is installed now (the delivery watchdog).
    func sessionInterrupted(generation reported: Int? = nil) {
        let accepted = locked { stateValue == .running && (reported ?? installedGeneration) == installedGeneration }
        if accepted { request(resetBackoff: false) }
    }

    private func accepts(_ reported: Int?) -> Bool {
        guard let reported else { return true }
        return locked { stateValue == .running && reported == installedGeneration }
    }

    private func request(resetBackoff: Bool) {
        let changed: Bool = locked {
            guard stateValue == .running || stateValue == .reconnecting else { return false }
            generation += 1
            if resetBackoff { backoff = timing.initialBackoff }
            let wasRunning = stateValue == .running
            stateValue = .reconnecting
            scheduleLocked(after: timing.debounce, generation: generation)
            return wasRunning
        }
        if changed { onStateChange?(.reconnecting) }
    }

    private func scheduleLocked(after delay: TimeInterval, generation scheduled: Int) {
        cancelPending?()
        cancelPending = scheduler.schedule(after: delay) { [weak self] in self?.fire(scheduled) }
    }

    private func fire(_ scheduled: Int) {
        let accepted: Bool = locked {
            guard scheduled == generation, stateValue == .reconnecting else { return false }
            cancelPending = nil
            inFlight.enter()
            return true
        }
        guard accepted else { return }
        attemptQueue.async { [self] in
            defer { inFlight.leave() }
            attempt(scheduled)
        }
    }

    private func attempt(_ scheduled: Int) {
        let previous: Session?? = locked {
            guard scheduled == generation, stateValue == .reconnecting else { return nil }
            defer { current = nil }
            return .some(current)
        }
        guard let previous else { return }
        // Release the old device before opening the new default one.
        if let previous { stopSession(previous) }
        do {
            let session = try startSession(scheduled)
            let installed: Bool = locked {
                guard scheduled == generation, stateValue == .reconnecting else { return false }
                current = session
                installedGeneration = scheduled
                stateValue = .running
                backoff = timing.initialBackoff
                lastError = nil
                return true
            }
            guard installed else {
                stopSession(session)
                return
            }
            onInstalled?(session)
            onStateChange?(.running)
        }
        catch {
            if isPermanent(error) {
                let first: Bool = locked {
                    guard scheduled == generation, stateValue == .reconnecting else { return false }
                    stateValue = .failed
                    failure = error
                    return true
                }
                if first {
                    onStateChange?(.failed)
                    onPermanentFailure?(error)
                }
                return
            }
            locked {
                guard scheduled == generation, stateValue == .reconnecting else { return }
                lastError = error
                let delay = backoff
                backoff = min(backoff * 2, timing.maximumBackoff)
                scheduleLocked(after: delay, generation: scheduled)
            }
        }
    }

    /// Cancels pending retries and tears down the current session. Returns
    /// `false` if the deadline passed while an attempt or teardown was still
    /// blocked; that work is abandoned and cleans up after itself.
    @discardableResult func stop(deadline: DispatchTime) -> Bool {
        let (session, wasActive): (Session?, Bool) = locked {
            let wasActive = stateValue != .stopped
            stateValue = .stopped
            generation += 1
            cancelPending?()
            cancelPending = nil
            defer { current = nil }
            return (current, wasActive)
        }
        if wasActive { onStateChange?(.stopped) }
        guard inFlight.wait(timeout: deadline) == .success else {
            // The attempt owns the queue. Queue this teardown behind it so the
            // session is still released when the stuck call returns.
            if let session { attemptQueue.async { self.stopSession(session) } }
            return false
        }
        guard let session else { return true }
        inFlight.enter()
        attemptQueue.async { [self] in
            defer { inFlight.leave() }
            stopSession(session)
        }
        return inFlight.wait(timeout: deadline) == .success
    }

    private func locked<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }
}
