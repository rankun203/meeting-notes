import Foundation

/// Detects system audio (what the speakers play) leaking into the microphone by
/// correlating short-term level envelopes, not raw audio. Speaker playback that
/// reaches the microphone makes the microphone's loudness follow the system
/// track's loudness a few tens of milliseconds later; independent speech does not.
///
/// Cost is deliberately tiny: callers add one mean-square value per capture
/// buffer, which fills the ~50 Hz bins the buffer covers in a fixed ring; `evaluate` runs about once per
/// second and correlates ~300 bins across ~21 lags. Storage is allocated once in
/// `init`, so adding values never allocates. Not thread-safe; callers serialize.
struct EchoDetector {
    struct Configuration {
        /// Envelope bin width. A capture buffer fills every bin it overlaps, so
        /// longer buffers (for example Bluetooth call audio) still cover the timeline.
        var binDuration: TimeInterval = 0.02
        /// Correlation window, in seconds of microphone envelope.
        var window: TimeInterval = 6
        /// Largest microphone delay behind the system track that counts as echo.
        /// Covers output and input latency, including Bluetooth speakers.
        var maximumLag: TimeInterval = 0.4
        /// Normalized cross-correlation at or above this counts as a match. Set well
        /// above chance speech alignment (about 0.6 in tests): a false match latches
        /// processing on for the session, while a miss only leaves the route default.
        var threshold: Double = 0.75
        /// Consecutive matching evaluations before echo is reported.
        var requiredMatches = 3
        /// Consecutive non-matching active evaluations before echo is cleared.
        var requiredMisses = 5
        /// The system envelope must vary this much (standard deviation, dB) to be
        /// informative; steady noise or silence says nothing about leakage.
        var minimumSystemVariationDB: Double = 3
        /// The system track's loud bins (90th percentile) must reach this level.
        var minimumSystemLevelDB: Double = -50
        /// Levels are clamped here so digital silence doesn't dominate the statistics.
        var floorDB: Double = -80
        /// Fraction of bins in each window that must hold measured audio.
        var minimumCoverage: Double = 0.8
    }

    struct Evaluation: Equatable {
        /// Best normalized cross-correlation over the searched lags.
        var correlation: Double
        /// Microphone delay behind the system track at the best correlation.
        var lag: TimeInterval
        /// Whether echo is currently reported after this evaluation.
        var echoLikely: Bool
    }

    let configuration: Configuration
    private(set) var echoLikely = false
    private var matches = 0
    private var misses = 0
    private var system: EnvelopeRing
    private var microphone: EnvelopeRing
    private let windowBins: Int
    private let lagBins: Int
    /// Scratch for the percentile check, reused on every evaluation.
    private var sortScratch: [Double]

    init(configuration: Configuration = Configuration()) {
        self.configuration = configuration
        windowBins = max(2, Int((configuration.window / configuration.binDuration).rounded()))
        lagBins = max(0, Int((configuration.maximumLag / configuration.binDuration).rounded()))
        // Room for the lagged system window plus a second of lead so
        // evaluation can wait for both sources to fill the same bins.
        let capacity = windowBins + lagBins + Int((1 / configuration.binDuration).rounded()) + 8
        system = EnvelopeRing(capacity: capacity)
        microphone = EnvelopeRing(capacity: capacity)
        sortScratch = [Double](repeating: 0, count: windowBins + lagBins)
    }

    /// Adds one system-audio buffer's mean square, starting at its capture host time.
    mutating func addSystem(meanSquare: Double, hostTime: TimeInterval, duration: TimeInterval) {
        add(meanSquare, hostTime: hostTime, duration: duration, to: &system)
    }

    /// Adds one microphone buffer's mean square, starting at its capture host time.
    mutating func addMicrophone(meanSquare: Double, hostTime: TimeInterval, duration: TimeInterval) {
        add(meanSquare, hostTime: hostTime, duration: duration, to: &microphone)
    }

    private func add(_ meanSquare: Double, hostTime: TimeInterval, duration: TimeInterval, to ring: inout EnvelopeRing)
    {
        let value = level(meanSquare)
        let first = bin(hostTime)
        // Bound the span so a bogus duration cannot loop over the whole ring.
        let last = min(bin(hostTime + max(0, duration) - 1e-6), first + 16)
        for index in first...max(first, last) { ring.add(level: value, bin: index) }
    }

    /// Forgets consecutive-match state and history, for example after the
    /// microphone engine changes processing and the old envelope no longer applies.
    mutating func reset() {
        system.clear()
        microphone.clear()
        matches = 0
        misses = 0
        echoLikely = false
    }

    /// Correlates the latest complete window. Returns `nil` when there is not
    /// enough audio or the system track is too quiet or steady to be informative;
    /// those evaluations leave the reported state unchanged.
    mutating func evaluate() -> Evaluation? {
        guard let latestSystem = system.latest, let latestMicrophone = microphone.latest else { return nil }
        // The newest bin may still be filling; stop one bin before the older source's newest.
        let end = min(latestSystem, latestMicrophone) - 1
        let start = end - windowBins + 1
        // Wait for a full window; coverage below only tolerates short gaps inside it.
        guard let firstSystem = system.earliest, let firstMicrophone = microphone.earliest,
            start - lagBins >= max(system.oldest, microphone.oldest, firstSystem), start >= firstMicrophone
        else { return nil }
        guard coverage(of: microphone, from: start, through: end) >= configuration.minimumCoverage,
            coverage(of: system, from: start - lagBins, through: end) >= configuration.minimumCoverage,
            systemIsActive(from: start - lagBins, through: end)
        else { return nil }

        var best = -Double.infinity
        var bestLag = 0
        for lag in 0...lagBins {
            guard let correlation = correlation(microphoneStart: start, end: end, lag: lag) else { continue }
            if correlation > best {
                best = correlation
                bestLag = lag
            }
        }
        guard best.isFinite else { return nil }
        if best >= configuration.threshold {
            matches += 1
            misses = 0
            if matches >= configuration.requiredMatches { echoLikely = true }
        }
        else {
            misses += 1
            matches = 0
            if misses >= configuration.requiredMisses { echoLikely = false }
        }
        return Evaluation(
            correlation: best, lag: Double(bestLag) * configuration.binDuration, echoLikely: echoLikely)
    }

    // MARK: - Statistics

    private func level(_ meanSquare: Double) -> Double {
        guard meanSquare.isFinite, meanSquare > 0 else { return configuration.floorDB }
        return max(configuration.floorDB, 10 * log10(meanSquare))
    }

    private func bin(_ hostTime: TimeInterval) -> Int { Int((hostTime / configuration.binDuration).rounded(.down)) }

    private func coverage(of ring: EnvelopeRing, from start: Int, through end: Int) -> Double {
        var measured = 0
        for index in start...end where ring.value(at: index) != nil { measured += 1 }
        return Double(measured) / Double(end - start + 1)
    }

    private mutating func systemIsActive(from start: Int, through end: Int) -> Bool {
        var count = 0
        var sum = 0.0
        var squares = 0.0
        for index in start...end {
            guard let value = system.value(at: index) else { continue }
            sortScratch[count] = value
            count += 1
            sum += value
            squares += value * value
        }
        guard count > 1 else { return false }
        let mean = sum / Double(count)
        let deviation = (max(0, squares / Double(count) - mean * mean)).squareRoot()
        guard deviation >= configuration.minimumSystemVariationDB else { return false }
        // In-place partial sort of the reused scratch; no allocation.
        sortScratch.withUnsafeMutableBufferPointer { $0[0..<count].sort() }
        let loud = sortScratch[min(count - 1, Int(Double(count) * 0.9))]
        return loud >= configuration.minimumSystemLevelDB
    }

    /// Pearson correlation of the microphone window against the system window
    /// `lag` bins earlier, over bins where both sources were measured.
    private func correlation(microphoneStart start: Int, end: Int, lag: Int) -> Double? {
        var count = 0.0
        var sumX = 0.0
        var sumY = 0.0
        var sumXX = 0.0
        var sumYY = 0.0
        var sumXY = 0.0
        for index in start...end {
            guard let y = microphone.value(at: index), let x = system.value(at: index - lag) else { continue }
            count += 1
            sumX += x
            sumY += y
            sumXX += x * x
            sumYY += y * y
            sumXY += x * y
        }
        guard count >= Double(windowBins) * configuration.minimumCoverage else { return nil }
        let covariance = sumXY - sumX * sumY / count
        let varianceX = sumXX - sumX * sumX / count
        let varianceY = sumYY - sumY * sumY / count
        // A flat microphone envelope (muted or constant noise) cannot follow the system.
        guard varianceX > 1e-9, varianceY > 1e-9 else { return 0 }
        return covariance / (varianceX * varianceY).squareRoot()
    }
}

/// Fixed-capacity envelope history keyed by absolute bin number. Several buffers
/// in one bin are averaged in the power domain; bins without audio stay empty.
private struct EnvelopeRing {
    private var levels: [Double]
    private var power: [Double]
    private var counts: [Int]
    private var bins: [Int]
    private(set) var latest: Int?
    /// The first bin added since the ring was created or cleared.
    private(set) var earliest: Int?
    let capacity: Int

    init(capacity: Int) {
        self.capacity = capacity
        levels = [Double](repeating: 0, count: capacity)
        power = [Double](repeating: 0, count: capacity)
        counts = [Int](repeating: 0, count: capacity)
        bins = [Int](repeating: Int.min, count: capacity)
    }

    /// The oldest bin that can still be stored.
    var oldest: Int { (latest ?? 0) - capacity + 1 }

    mutating func add(level: Double, bin: Int) {
        if let latest, bin <= latest - capacity { return }  // Too old to keep.
        let slot = ((bin % capacity) + capacity) % capacity
        if bins[slot] != bin {
            bins[slot] = bin
            power[slot] = 0
            counts[slot] = 0
        }
        power[slot] += pow(10, level / 10)
        counts[slot] += 1
        levels[slot] = 10 * log10(power[slot] / Double(counts[slot]))
        latest = max(latest ?? bin, bin)
        earliest = min(earliest ?? bin, bin)
    }

    func value(at bin: Int) -> Double? {
        let slot = ((bin % capacity) + capacity) % capacity
        return bins[slot] == bin ? levels[slot] : nil
    }

    mutating func clear() {
        for slot in 0..<capacity { bins[slot] = Int.min }
        latest = nil
        earliest = nil
    }
}
