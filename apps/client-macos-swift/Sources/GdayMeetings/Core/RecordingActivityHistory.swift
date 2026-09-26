import Foundation

/// Fifty completed 200 ms bars. Once published, a bar's height never changes.
/// No audio is retained; only scalar levels from the existing meter delivery.
struct RecordingActivityHistory {
    struct Sample {
        let time: TimeInterval
        let microphone: Double
        let system: Double
    }
    private(set) var samples: [Sample] = []
    private var pending: Sample?
    private var lastTime: TimeInterval?
    var bucketStart: TimeInterval? { pending?.time }

    static func scrollFraction(since start: TimeInterval, at time: TimeInterval) -> Double {
        min(2, max(0, (time - start) * 5))
    }

    mutating func append(_ levels: RecordingLevels, at time: TimeInterval = ProcessInfo.processInfo.systemUptime) {
        guard time.isFinite else { return }
        if let lastTime, time < lastTime {
            samples = []
            pending = nil
        }
        lastTime = time
        let bucket = floor(time * 5) / 5
        if let pending, bucket > pending.time {
            samples.append(pending)
            self.pending = nil
        }
        samples.removeAll { $0.time < bucket - 10 }
        if samples.count > 50 { samples.removeFirst(samples.count - 50) }
        pending = Sample(
            time: bucket,
            microphone: max(pending?.microphone ?? 0, levels.microphone.level),
            system: max(pending?.system ?? 0, levels.system.level))
    }

    func bars(microphone: Bool) -> [Double] {
        var bars = [Double](repeating: 0, count: 50)
        guard let pending else { return bars }
        for sample in samples {
            let distance = Int(((pending.time - sample.time) * 5).rounded())
            guard (1...50).contains(distance) else { continue }
            bars[50 - distance] = microphone ? sample.microphone : sample.system
        }
        return bars
    }
}
