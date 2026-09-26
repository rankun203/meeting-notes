import AVFoundation
import Foundation
import Testing

@testable import GdayMeetings

struct AudioCaptureTests {
    @Test func planarStereoSamplesWriteReadableWAV() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".wav")
        defer { try? FileManager.default.removeItem(at: url) }
        let format = try #require(
            AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: 2, interleaved: false))
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        buffer.frameLength = 480
        let channels = try #require(buffer.floatChannelData)
        for frame in 0..<480 {
            channels[0][frame] = 0.25
            channels[1][frame] = -0.5
        }
        do {
            let writer = try TimedAudioWriter(url: url, format: format, epoch: 0)
            try writer.append(buffer, hostSeconds: 0)
            try writer.finish()
        }
        let file = try AVAudioFile(forReading: url)
        #expect(file.length == 480)
        #expect(file.fileFormat.channelCount == 2)
        #expect(file.fileFormat.sampleRate == 48000)
        let result = try #require(AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: 480))
        try file.read(into: result)
        let samples = try #require(result.floatChannelData)
        #expect(abs(samples[0][20] - 0.25) < 0.0001)
        #expect(abs(samples[1][20] + 0.5) < 0.0001)
    }
}
