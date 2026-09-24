import AVFoundation
import Foundation
import Testing
@testable import GdayMeetings

struct AudioConversionTests {
    @Test(arguments: [1, 2]) func aacConversionPreservesDurationAndChannels(channels: Int) async throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".caf")
        defer { try? FileManager.default.removeItem(at: file) }
        let format = try #require(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: AVAudioChannelCount(channels)))
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 48000))
        buffer.frameLength = 48000
        for channel in 0..<channels {
            let samples = try #require(buffer.floatChannelData?[channel])
            for frame in 0..<48000 { samples[frame] = Float(sin(Double(frame) * 2 * .pi * Double(440 + 220 * channel) / 48000) * 0.1) }
        }
        var output: AVAudioFile? = try AVAudioFile(forWriting: file, settings: format.settings)
        try output?.write(from: buffer); output = nil
        let originalSize = try file.resourceValues(forKeys: [.fileSizeKey]).fileSize!
        let prepared = try await prepareServerAudio(file)
        defer { if prepared.temporary { try? FileManager.default.removeItem(at: prepared.url) } }
        #expect(prepared.temporary)
        #expect(prepared.url.pathExtension == "m4a")
        #expect(prepared.channels == channels)
        let encoded = try AVAudioFile(forReading: prepared.url)
        #expect(encoded.processingFormat.sampleRate == 48000)
        #expect(abs(Double(encoded.length) / encoded.processingFormat.sampleRate - 1) < 0.1)
        #expect(try prepared.url.resourceValues(forKeys: [.fileSizeKey]).fileSize! < originalSize)
        #expect(try file.resourceValues(forKeys: [.fileSizeKey]).fileSize == originalSize)
        let decoded = try #require(AVAudioPCMBuffer(pcmFormat: encoded.processingFormat, frameCapacity: AVAudioFrameCount(encoded.length)))
        try encoded.read(into: decoded)
        for channel in 0..<channels {
            let samples = try #require(decoded.floatChannelData?[channel])
            let energy = (0..<Int(decoded.frameLength)).reduce(0.0) { $0 + Double(samples[$1] * samples[$1]) }
            #expect(energy > 10, "Each encoded channel retains its signal")
        }
    }
}
