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
/// - Loop guard: a session replaced before it delivered any audio did not fix
///   anything. From the `loopThreshold`th such rebuild in a row, requests wait
///   out a doubling backoff instead of the debounce, and route changes stop
///   resetting it, until a session reports `sessionDelivered`. A rebuild that
///   triggers the next one (as a device selection's own configuration change
///   did) therefore slows to one attempt per 5 seconds instead of flapping.
final class CaptureSourceRecovery<Session>: @unchecked Sendable {
    typealias State = CaptureSourceState
    /// Undelivered rebuilds in a row before requests back off.
    static var loopThreshold: Int { 2 }

    struct Timing {
        var debounce: TimeInterval = 0.3
        var initialBackoff: TimeInterval = 0.25
        var maximumBackoff: TimeInterval = 5
    }

    private let timing: Timing
    /// Names the source in log lines.
    private let label: String
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
    /// The installed session has delivered audio.
    private var delivered = false
    private var undeliveredRebuilds = 0
    private var loopBackoff: TimeInterval

    /// Called outside the lock after each state transition.
    var onStateChange: ((State) -> Void)?
    /// Called after a rebuilt session is installed; not called for the initial one.
    var onInstalled: ((Session) -> Void)?
    /// Called once when `isPermanent` classifies an attempt's error.
    var onPermanentFailure: ((Error) -> Void)?

    init(
        label: String = "source", timing: Timing = Timing(), scheduler: RecoveryScheduling,
        attemptQueue: DispatchQueue, start: @escaping (_ generation: Int) throws -> Session,
        stop: @escaping (Session) -> Void, isPermanent: @escaping (Error) -> Bool
    ) {
        self.label = label
        self.timing = timing
        self.scheduler = scheduler
        self.attemptQueue = attemptQueue
        startSession = start
        stopSession = stop
        self.isPermanent = isPermanent
        backoff = timing.initialBackoff
        loopBackoff = timing.initialBackoff
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
            delivered = false
        }
        CaptureLog.recovery.notice("\(self.label, privacy: .public): initial session installed")
    }

    /// The installed session delivered its first audio. Clears the loop guard.
    /// Callers report once per session, not per buffer.
    func sessionDelivered() {
        let cleared: (generation: Int, rebuilds: Int)? = locked {
            guard stateValue == .running, !delivered else { return nil }
            delivered = true
            defer {
                undeliveredRebuilds = 0
                loopBackoff = timing.initialBackoff
            }
            return (installedGeneration, undeliveredRebuilds)
        }
        guard let cleared else { return }
        CaptureLog.recovery.notice(
            "\(self.label, privacy: .public): generation \(cleared.generation) delivering audio after \(cleared.rebuilds) undelivered rebuilds"
        )
    }

    /// A default device or engine configuration changed. Debounced; resets
    /// backoff so a new route is tried promptly even after long outages.
    /// A session-scoped report (`generation` set) is ignored once that session
    /// has been replaced.
    func routeChanged(generation reported: Int? = nil) {
        guard accepts(reported) else { return }
        request(resetBackoff: true, cause: "route change")
    }

    /// The installed session stopped delivering or reported a recoverable fault.
    /// `nil` means whichever session is installed now (the delivery watchdog).
    func sessionInterrupted(generation reported: Int? = nil) {
        let accepted = locked { stateValue == .running && (reported ?? installedGeneration) == installedGeneration }
        if accepted { request(resetBackoff: false, cause: "interruption") }
    }

    private func accepts(_ reported: Int?) -> Bool {
        guard let reported else { return true }
        return locked { stateValue == .running && reported == installedGeneration }
    }

    private func request(resetBackoff: Bool, cause: String) {
        let scheduled: (wasRunning: Bool, delay: TimeInterval, undelivered: Int, generation: Int)? = locked {
            guard stateValue == .running || stateValue == .reconnecting else { return nil }
            generation += 1
            let wasRunning = stateValue == .running
            // Only replacing an installed session counts; requests that arrive
            // while an attempt is pending just move it.
            if wasRunning { undeliveredRebuilds = delivered ? 0 : undeliveredRebuilds + 1 }
            var delay = timing.debounce
            if undeliveredRebuilds >= Self.loopThreshold {
                delay = max(timing.debounce, loopBackoff)
                if wasRunning { loopBackoff = min(loopBackoff * 2, timing.maximumBackoff) }
            }
            else if resetBackoff {
                backoff = timing.initialBackoff
            }
            stateValue = .reconnecting
            scheduleLocked(after: delay, generation: generation)
            return (wasRunning, delay, undeliveredRebuilds, generation)
        }
        guard let scheduled else { return }
        if scheduled.undelivered >= Self.loopThreshold {
            CaptureLog.recovery.error(
                "\(self.label, privacy: .public): \(cause, privacy: .public) after \(scheduled.undelivered) rebuilds without audio; backing off \(scheduled.delay, format: .fixed(precision: 2)) s (generation \(scheduled.generation))"
            )
        }
        else {
            CaptureLog.recovery.notice(
                "\(self.label, privacy: .public): \(cause, privacy: .public); rebuilding in \(scheduled.delay, format: .fixed(precision: 2)) s (generation \(scheduled.generation))"
            )
        }
        if scheduled.wasRunning { onStateChange?(.reconnecting) }
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
                delivered = false
                return true
            }
            guard installed else {
                CaptureLog.recovery.notice(
                    "\(self.label, privacy: .public): attempt \(scheduled) superseded; discarding it")
                stopSession(session)
                return
            }
            CaptureLog.recovery.notice("\(self.label, privacy: .public): attempt \(scheduled) installed")
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
                    CaptureLog.recovery.error(
                        "\(self.label, privacy: .public): attempt \(scheduled) failed permanently: \(error.localizedDescription, privacy: .public)"
                    )
                    onStateChange?(.failed)
                    onPermanentFailure?(error)
                }
                return
            }
            let delay: TimeInterval? = locked {
                guard scheduled == generation, stateValue == .reconnecting else { return nil }
                lastError = error
                let delay = backoff
                backoff = min(backoff * 2, timing.maximumBackoff)
                scheduleLocked(after: delay, generation: scheduled)
                return delay
            }
            CaptureLog.recovery.error(
                "\(self.label, privacy: .public): attempt \(scheduled) failed: \(error.localizedDescription, privacy: .public); retry in \(delay ?? -1, format: .fixed(precision: 2)) s"
            )
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
            CaptureLog.recovery.error("\(self.label, privacy: .public): stop abandoned a blocked attempt")
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
