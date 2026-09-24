import AVFoundation
import Foundation
import Testing
@testable import GdayMeetings

struct AudioCaptureTests {
    @Test func planarStereoSamplesWriteReadableWAV() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".wav")
        defer { try? FileManager.default.removeItem(at: url) }
        let format = try #require(AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: 2, interleaved: false))
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        buffer.frameLength = 480
        let channels = try #require(buffer.floatChannelData)
        for frame in 0..<480 { channels[0][frame] = 0.25; channels[1][frame] = -0.5 }
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

struct TimedAudioWriterTests {
    @Test func hostTimelinePadsGapsAndTrimsOverlaps() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".wav")
        defer { try? FileManager.default.removeItem(at: url) }
        let format = try #require(AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: 1, interleaved: false))
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        buffer.frameLength = 480
        let data = try #require(buffer.floatChannelData)
        for frame in 0..<480 { data[0][frame] = 0.5 }
        let writer = try TimedAudioWriter(url: url, format: format, epoch: 10)
        try writer.append(buffer, hostSeconds: 10.01) // 480 silent frames, then 480 source frames.
        try writer.append(buffer, hostSeconds: 10.015) // 240 overlapping frames trimmed.
        try writer.append(buffer, hostSeconds: 10.03) // 240 silent missing frames.
        try writer.finish()
        let file = try AVAudioFile(forReading: url)
        #expect(file.length == 1920)
        let decoded = try #require(AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: 1920))
        try file.read(into: decoded)
        let samples = try #require(decoded.floatChannelData)
        #expect(abs(samples[0][20]) < 0.0001)
        #expect(abs(samples[0][600] - 0.5) < 0.0001)
        #expect(abs(samples[0][1300]) < 0.0001)
        #expect(abs(samples[0][1500] - 0.5) < 0.0001)
    }
    @Test func changedFormatFailsAndStillFinalizesPartialFile() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".wav")
        defer { try? FileManager.default.removeItem(at: url) }
        let format = try #require(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1))
        let changed = try #require(AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2))
        let original = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        original.frameLength = 480
        let samples = try #require(original.floatChannelData)
        for index in 0..<480 { samples[0][index] = 0 }
        let wrong = try #require(AVAudioPCMBuffer(pcmFormat: changed, frameCapacity: 441)); wrong.frameLength = 441
        let writer = try TimedAudioWriter(url: url, format: format, epoch: 0)
        try writer.append(original, hostSeconds: 0)
        #expect(throws: (any Error).self) { try writer.append(wrong, hostSeconds: 0.01) }
        #expect(throws: (any Error).self) { try writer.finish() }
        #expect(try AVAudioFile(forReading: url).length == 480)
    }
    @Test func timelineRejectsInvalidClock() throws {
        #expect(try TimedAudioWriter.targetFrame(hostSeconds: 12.5, epoch: 10, sampleRate: 48000) == 120000)
        #expect(throws: (any Error).self) { try TimedAudioWriter.targetFrame(hostSeconds: .nan, epoch: 0, sampleRate: 48000) }
    }
}
