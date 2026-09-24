import AVFoundation
import Foundation
import Testing
@testable import GdayMeetings

struct AudioWaveformTests {
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
