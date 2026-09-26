import Foundation
import Testing

@testable import GdayMeetings

/// Synthetic 20 ms level envelopes; no audio is generated. Levels are power
/// (mean square) so echo and near-end speech add the way sound does.
struct EchoDetectorTests {
    private static let bin = 0.02
    private static let origin = 1_000.0

    /// Deterministic generator so thresholds are tested against fixed inputs.
    private struct SplitMix: RandomNumberGenerator {
        var state: UInt64
        mutating func next() -> UInt64 {
            state &+= 0x9E37_79B9_7F4A_7C15
            var z = state
            z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
            z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
            return z ^ (z >> 31)
        }
    }

    /// Speech-like power envelope: 100–300 ms syllables near -20 dB separated by
    /// 50–400 ms pauses near -70 dB.
    private static func speech(bins count: Int, seed: UInt64) -> [Double] {
        var random = SplitMix(state: seed)
        var levels: [Double] = []
        while levels.count < count {
            let syllable = Int.random(in: 5...15, using: &random)
            let loudness = Double.random(in: -26 ... -14, using: &random)
            for _ in 0..<syllable { levels.append(loudness + Double.random(in: -3...3, using: &random)) }
            let pause = Int.random(in: 3...20, using: &random)
            for _ in 0..<pause { levels.append(-70 + Double.random(in: -3...3, using: &random)) }
        }
        return levels.prefix(count).map { pow(10, $0 / 10) }
    }

    private static func noise(bins count: Int, level: Double, seed: UInt64) -> [Double] {
        var random = SplitMix(state: seed)
        return (0..<count).map { _ in pow(10, (level + Double.random(in: -2...2, using: &random)) / 10) }
    }

    /// System leaking into the microphone `delay` bins later at `gainDB`.
    private static func echo(of system: [Double], delay: Int, gainDB: Double) -> [Double] {
        let gain = pow(10, gainDB / 10)
        return system.indices.map { $0 >= delay ? system[$0 - delay] * gain : 0 }
    }

    private static func sum(_ a: [Double], _ b: [Double]) -> [Double] { zip(a, b).map { $0 + $1 } }

    /// Feeds both envelopes and evaluates once per simulated second, as capture does.
    private static func run(
        system: [Double]?, microphone: [Double], microphoneBlock: Int = 1,
        configuration: EchoDetector.Configuration = .init()
    ) -> (detector: EchoDetector, evaluations: [EchoDetector.Evaluation?]) {
        var detector = EchoDetector(configuration: configuration)
        var evaluations: [EchoDetector.Evaluation?] = []
        for index in microphone.indices {
            let time = origin + Double(index) * bin + 0.001
            if let system { detector.addSystem(meanSquare: system[index], hostTime: time, duration: bin) }
            // Larger microphone buffers deliver the average power of several bins at once.
            if (index + 1) % microphoneBlock == 0 {
                let first = index + 1 - microphoneBlock
                let power = microphone[first...index].reduce(0, +) / Double(microphoneBlock)
                detector.addMicrophone(
                    meanSquare: power, hostTime: origin + Double(first) * bin + 0.001,
                    duration: Double(microphoneBlock) * bin)
            }
            if (index + 1) % 50 == 0 { evaluations.append(detector.evaluate()) }
        }
        return (detector, evaluations)
    }

    @Test func delayedSystemCopyInMicrophoneIsDetected() throws {
        let count = 50 * 15
        let system = Self.speech(bins: count, seed: 1)
        let microphone = Self.sum(
            Self.echo(of: system, delay: 6, gainDB: -25), Self.noise(bins: count, level: -65, seed: 2))
        let result = Self.run(system: system, microphone: microphone)
        #expect(result.detector.echoLikely)
        let last = try #require(result.evaluations.compactMap { $0 }.last)
        #expect(last.correlation > 0.8)
        #expect(abs(last.lag - 0.12) <= 0.04)
        // Needs three consecutive matches after the 6-second window fills.
        let firstLikely = try #require(result.evaluations.firstIndex { $0?.echoLikely == true })
        #expect(firstLikely >= 8)
    }

    @Test(arguments: [2, 10, 18])
    func delayEstimateIsWithinTolerance(delay: Int) throws {
        let count = 50 * 12
        let system = Self.speech(bins: count, seed: UInt64(10 + delay))
        let microphone = Self.sum(
            Self.echo(of: system, delay: delay, gainDB: -30), Self.noise(bins: count, level: -68, seed: 3))
        let result = Self.run(system: system, microphone: microphone)
        let last = try #require(result.evaluations.compactMap { $0 }.last)
        #expect(abs(last.lag - Double(delay) * Self.bin) <= 0.04)
        #expect(result.detector.echoLikely)
    }

    @Test func largeMicrophoneBuffersStillCoverTheWindow() {
        let count = 50 * 15
        let system = Self.speech(bins: count, seed: 4)
        let microphone = Self.sum(
            Self.echo(of: system, delay: 5, gainDB: -25), Self.noise(bins: count, level: -65, seed: 5))
        // 60 ms buffers, as from a narrowband Bluetooth microphone.
        let result = Self.run(system: system, microphone: microphone, microphoneBlock: 3)
        #expect(result.detector.echoLikely)
    }

    @Test func independentSignalsAreNotDetected() {
        let count = 50 * 30
        let system = Self.speech(bins: count, seed: 6)
        let microphone = Self.sum(Self.speech(bins: count, seed: 7), Self.noise(bins: count, level: -65, seed: 8))
        let result = Self.run(system: system, microphone: microphone)
        let evaluated = result.evaluations.compactMap { $0 }
        #expect(!evaluated.isEmpty)
        #expect(!evaluated.contains { $0.echoLikely })
        // Chance alignment of syllables can briefly approach the threshold; the
        // three-match requirement keeps it from being reported.
        #expect(evaluated.allSatisfy { $0.correlation < 0.7 })
    }

    @Test func silentOrMissingSystemAudioIsNotEvaluated() {
        let count = 50 * 15
        let microphone = Self.speech(bins: count, seed: 9)
        let silent = Self.run(system: [Double](repeating: 0, count: count), microphone: microphone)
        #expect(silent.evaluations.allSatisfy { $0 == nil })
        #expect(!silent.detector.echoLikely)
        // Steady noise from system audio carries no speech envelope to follow.
        let steady = Self.run(system: Self.noise(bins: count, level: -40, seed: 10), microphone: microphone)
        #expect(steady.evaluations.allSatisfy { $0 == nil })
        let missing = Self.run(system: nil, microphone: microphone)
        #expect(missing.evaluations.allSatisfy { $0 == nil })
    }

    @Test func doubleTalkLowersCorrelationButStaysBounded() throws {
        let count = 50 * 15
        let system = Self.speech(bins: count, seed: 11)
        let leak = Self.echo(of: system, delay: 6, gainDB: -25)
        let noise = Self.noise(bins: count, level: -65, seed: 12)
        let echoOnly = Self.run(system: system, microphone: Self.sum(leak, noise))
        // Near-end speech about as loud as the leaked system audio.
        let nearEnd = Self.speech(bins: count, seed: 13).map { $0 * pow(10, -25.0 / 10) }
        let doubleTalk = Self.run(system: system, microphone: Self.sum(Self.sum(leak, noise), nearEnd))
        let clean = try #require(echoOnly.evaluations.compactMap { $0 }.last)
        let mixed = try #require(doubleTalk.evaluations.compactMap { $0 }.last)
        #expect(mixed.correlation < clean.correlation)
        #expect(mixed.correlation > 0.2 && mixed.correlation <= 1)
        #expect(abs(mixed.lag - 0.12) <= 0.06)
    }

    @Test func echoClearsAfterSustainedMisses() {
        var detector = EchoDetector()
        let count = 50 * 30
        let system = Self.speech(bins: count, seed: 14)
        let leak = Self.sum(Self.echo(of: system, delay: 6, gainDB: -25), Self.noise(bins: count, level: -65, seed: 15))
        let independent = Self.sum(Self.speech(bins: count, seed: 16), Self.noise(bins: count, level: -65, seed: 17))
        var states: [Bool] = []
        for index in 0..<count {
            let time = Self.origin + Double(index) * Self.bin + 0.001
            detector.addSystem(meanSquare: system[index], hostTime: time, duration: Self.bin)
            // Headphones plugged in halfway: the leak stops.
            let microphone = index < count / 2 ? leak[index] : independent[index]
            detector.addMicrophone(meanSquare: microphone, hostTime: time, duration: Self.bin)
            if (index + 1) % 50 == 0, detector.evaluate() != nil { states.append(detector.echoLikely) }
        }
        #expect(states.contains(true))
        #expect(states.last == false)
        detector.reset()
        #expect(!detector.echoLikely)
        #expect(detector.evaluate() == nil)
    }
}
