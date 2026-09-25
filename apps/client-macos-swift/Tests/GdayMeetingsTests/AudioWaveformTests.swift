import AVFoundation
import Foundation
import Testing

@testable import GdayMeetings

struct AudioWaveformTests {
    @Test func tenHourOverviewHasBoundedWorkAndPersistentCache() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        // Sparse 10-hour PCM file: exercises real seeks without writing hours of
        // synthetic samples or depending on a wall-clock performance threshold.
        let source = directory.appendingPathComponent("long.wav")
        let frames: UInt32 = 8000 * 36000
        let bytes = frames * 2
        var header = Data()
        func text(_ value: String) { header.append(Data(value.utf8)) }
        func u16(_ value: UInt16) {
            var v = value.littleEndian
            withUnsafeBytes(of: &v) { header.append(contentsOf: $0) }
        }
        func u32(_ value: UInt32) {
            var v = value.littleEndian
            withUnsafeBytes(of: &v) { header.append(contentsOf: $0) }
        }
        text("RIFF")
        u32(bytes + 36)
        text("WAVEfmt ")
        u32(16)
        u16(1)
        u16(1)
        u32(8000)
        u32(16000)
        u16(2)
        u16(16)
        text("data")
        u32(bytes)
        try header.write(to: source)
        let handle = try FileHandle(forWritingTo: source)
        try handle.truncate(atOffset: UInt64(bytes) + 44)
        try handle.close()
        let windows = (0..<1200).map { AudioWaveform.sampleWindow(length: Int64(frames), bucket: $0, count: 1200) }
        #expect(windows.reduce(0) { $0 + $1.count } == 1_228_800)
        #expect(windows.allSatisfy { $0.start >= 0 && $0.start + $0.count <= Int64(frames) })
        let cache = WaveformCache(directory: directory.appendingPathComponent("cache"))
        let clock = ContinuousClock()
        let start = clock.now
        let first = try await cache.waveform(source: source, readable: source)
        let generationTime = start.duration(to: clock.now)
        #expect(first.duration == 36000)
        #expect(first.peaks.count == 1200)
        #expect(first.peaks.allSatisfy { $0 == 0 })
        let hitStart = clock.now
        let hit = try await cache.waveform(source: source, readable: directory.appendingPathComponent("does-not-exist"))
        print("10h sparse PCM overview: \(generationTime); cached: \(hitStart.duration(to: clock.now))")
        #expect(hit == first)
        try Data("broken".utf8).write(to: cache.entryURL(for: source))
        #expect(await cache.cached(source) == nil)
        _ = try await cache.waveform(source: source, readable: source)
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSinceNow: 60)], ofItemAtPath: source.path)
        #expect(await cache.cached(source) == nil)
    }

    @Test func peaksKeepSilenceAndOppositePhaseChannels() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID()).wav")
        defer { try? FileManager.default.removeItem(at: url) }
        let format = AVAudioFormat(standardFormatWithSampleRate: 8000, channels: 2)!
        do {
            let file = try AVAudioFile(forWriting: url, settings: format.settings)
            let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 16000)!
            buffer.frameLength = 16000
            for index in 0..<16000 {
                let value: Float = index >= 8000 && index < 12000 ? 0.5 : 0
                buffer.floatChannelData![0][index] = value
                buffer.floatChannelData![1][index] = -value
            }
            try file.write(from: buffer)
        }
        let waveform = try await AudioWaveform.read(url, bucketCount: 20)
        #expect(waveform.peaks.count == 20)
        #expect(waveform.duration == 2)
        #expect(waveform.peak(from: 0, to: 1) == 0)
        #expect(waveform.peak(from: 1, to: 1.5) == 0.5)
        #expect(waveform.peak(from: 1.5, to: 2) == 0)
        #expect(waveform.peak(from: 2, to: 3) == 0)
        #expect(waveform.peak(from: -1, to: 0) == 0)
        let bounded = try await AudioWaveform.read(url, bucketCount: 100_000)
        #expect(bounded.peaks.count == 4096)
    }
}
