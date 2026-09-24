import AVFoundation
import Foundation
import Testing
@testable import GdayMeetings

struct RecordingMeterTests {
    @Test(arguments: [false, true])
    func measuresPlanarAndInterleavedStereo(interleaved: Bool) throws {
        let format = try #require(AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: 2, interleaved: interleaved))
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        buffer.frameLength = 480
        for audio in UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList) {
            let samples = try #require(audio.mData?.assumingMemoryBound(to: Float.self))
            for index in 0..<(Int(audio.mDataByteSize) / 4) { samples[index] = 0.25 }
        }
        let meter = RecordingSourceLevel.measure(buffer)
        #expect(meter.hasSamples)
        #expect(abs(meter.rmsDB + 12.0412) < 0.001)
        #expect(abs(meter.peakDB + 12.0412) < 0.001)
        #expect(meter.statusText == "Receiving audio")
    }
    @Test func quietDisabledAndStaleAreDistinct() throws {
        let format = try #require(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1))
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        buffer.frameLength = 480
        let samples = try #require(buffer.floatChannelData)
        for index in 0..<480 { samples[0][index] = 0 }
        var meter = RecordingSourceLevel.measure(buffer)
        #expect(meter.statusText == "Quiet")
        #expect(meter.level == 0)
        meter.stale = true
        #expect(meter.statusText == "No recent audio")
        #expect(RecordingSourceLevel().statusText == "Not recording")
        #expect(RecordingSourceLevel(enabled: true).statusText == "Waiting for audio")
    }
}
