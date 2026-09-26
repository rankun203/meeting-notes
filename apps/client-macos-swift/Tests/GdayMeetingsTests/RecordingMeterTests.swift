import AVFoundation
import Foundation
import Testing

@testable import GdayMeetings

struct RecordingMeterTests {
    @Test func activityHistoryKeepsTenSecondsAndSeparatesSources() {
        var history = RecordingActivityHistory()
        let levels = RecordingLevels(
            microphone: RecordingSourceLevel(enabled: true, hasSamples: true, rmsDB: -30),
            system: RecordingSourceLevel(enabled: true, hasSamples: true, rmsDB: -15))
        history.append(levels, at: 0)
        history.append(RecordingLevels(), at: 0.2)
        #expect(history.bars(microphone: true).last == 0.5)
        #expect(history.bars(microphone: false).last == 0.75)
        history.append(RecordingLevels(), at: 9)
        #expect(history.bars(microphone: true).max() == 0.5)
        history.append(RecordingLevels(), at: 10.2)
        #expect(history.bars(microphone: true).allSatisfy { $0 == 0 })
        for tick in 0..<200 { history.append(levels, at: 20 + Double(tick) / 100) }
        #expect(history.samples.count <= 50)
        history.append(levels, at: .nan)
        #expect(history.samples.count <= 50)
        history.append(RecordingLevels(), at: 1)
        #expect(history.samples.isEmpty)
    }

    @Test func scrollOffsetIsContinuousAcrossBucketRolloverAndBoundsStalls() {
        let before = 50 - RecordingActivityHistory.scrollFraction(since: 100, at: 100.199)
        let after = 49 - RecordingActivityHistory.scrollFraction(since: 100.2, at: 100.201)
        #expect(abs((before - after) - 0.01) < 0.000001)
        #expect(RecordingActivityHistory.scrollFraction(since: 100, at: 99) == 0)
        #expect(RecordingActivityHistory.scrollFraction(since: 100, at: 110) == 2)
    }

    @Test func completedActivityBarsOnlyShiftAndNeverChangeHeight() {
        var history = RecordingActivityHistory()
        func levels(_ value: Double) -> RecordingLevels {
            RecordingLevels(microphone: RecordingSourceLevel(enabled: true, hasSamples: true, rmsDB: value))
        }
        history.append(levels(-42), at: 0.01)
        history.append(levels(-12), at: 0.11)
        history.append(levels(-36), at: 0.21)
        let first = history.bars(microphone: true)
        #expect(first.last == 0.8)
        history.append(levels(-3), at: 0.31)
        #expect(history.bars(microphone: true) == first)
        history.append(levels(-24), at: 0.43)
        let second = history.bars(microphone: true)
        #expect(Array(second.dropLast()) == Array(first.dropFirst()))
        #expect(second.last == 0.95)
        history.append(levels(-18), at: 0.89)  // Missing bucket stays silent, never regroups old samples.
        #expect(history.bars(microphone: true)[46] == 0.8)
        #expect(history.bars(microphone: true)[47] == 0.95)
        #expect(history.bars(microphone: true).last == 0)
    }

    @Test(arguments: [false, true])
    func measuresPlanarAndInterleavedStereo(interleaved: Bool) throws {
        let format = try #require(
            AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: 2, interleaved: interleaved))
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
