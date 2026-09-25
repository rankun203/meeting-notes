import AVFoundation
import AudioCaptureBridge
import Testing

/// Synthetic input only: these tests never start a HAL device or request permission.
struct AudioCaptureBridgeTests {
    private func buffer(channels: UInt32 = 2, interleaved: Bool = false, frames: UInt32 = 3) throws -> AVAudioPCMBuffer
    {
        let format = try #require(
            AVAudioFormat(
                commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: channels, interleaved: interleaved))
        let value = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames))
        value.frameLength = frames
        let list = UnsafeMutableAudioBufferListPointer(value.mutableAudioBufferList)
        for (index, item) in list.enumerated() {
            let samples = try #require(item.mData).assumingMemoryBound(to: Float.self)
            for sample in 0..<(Int(item.mDataByteSize) / MemoryLayout<Float>.size) {
                let channel = interleaved ? sample % Int(channels) : index
                let frame = interleaved ? sample / Int(channels) : sample
                samples[sample] = Float((channel + 1) * 100 + frame)
            }
        }
        return value
    }

    private func timestamp(_ host: UInt64) -> AudioTimeStamp {
        var value = AudioTimeStamp()
        value.mHostTime = host
        value.mFlags = .hostTimeValid
        return value
    }

    @Test(arguments: [false, true]) func stereoLayoutAndTimestampSurviveWrapping(interleaved: Bool) throws {
        let ring = try #require(GdayAudioRingCreate(2, interleaved, 3, 3))
        defer { GdayAudioRingDestroy(ring) }
        let input = try buffer(interleaved: interleaved)
        var output = [Float](repeating: -1, count: 6)
        // Repeated push/pop crosses the physical ring end many times.
        for iteration in 0..<17 {
            var time = timestamp(UInt64(1000 + iteration * 63))
            #expect(GdayAudioRingPush(ring, input.audioBufferList, &time))
            var frames: UInt32 = 0
            var host: UInt64 = 0
            #expect(GdayAudioRingRead(ring, &output, 3, &frames, &host))
            #expect(frames == 3)
            #expect(host == time.mHostTime)
            #expect(output == [100, 200, 101, 201, 102, 202])
            #expect(!GdayAudioRingRead(ring, &output, 3, &frames, &host))
        }
        #expect(GdayAudioRingFailure(ring) == 0)
    }

    @Test func overflowDoesNotOverwriteQueuedSamples() throws {
        let ring = try #require(GdayAudioRingCreate(1, false, 3, 2))
        defer { GdayAudioRingDestroy(ring) }
        let input = try buffer(channels: 1)
        var accepted: [UInt64] = []
        // Fill the bounded queue, then verify a failed push never overwrites pending data.
        for value in 1...4 {
            var time = timestamp(UInt64(value))
            if GdayAudioRingPush(ring, input.audioBufferList, &time) {
                accepted.append(UInt64(value))
            }
            else {
                break
            }
        }
        #expect(accepted == [1, 2])
        #expect(GdayAudioRingFailure(ring) == 1)
        var output = [Float](repeating: 0, count: 3)
        for expected in accepted {
            var frames: UInt32 = 0
            var host: UInt64 = 0
            #expect(GdayAudioRingRead(ring, &output, 3, &frames, &host))
            #expect(host == expected)
            #expect(frames == 3)
            #expect(output == [100, 101, 102])
        }
    }

    @Test func wrongChannelLayoutFailsWithoutEnqueueing() throws {
        let ring = try #require(GdayAudioRingCreate(2, false, 3, 2))
        defer { GdayAudioRingDestroy(ring) }
        let input = try buffer(channels: 1)
        var time = timestamp(50)
        #expect(!GdayAudioRingPush(ring, input.audioBufferList, &time))
        #expect(GdayAudioRingFailure(ring) == 2)
        var output = [Float](repeating: 0, count: 6)
        var frames: UInt32 = 0
        var host: UInt64 = 0
        #expect(!GdayAudioRingRead(ring, &output, 3, &frames, &host))
    }

    @Test func missingHostTimestampFailsRatherThanInventingTime() throws {
        let ring = try #require(GdayAudioRingCreate(1, false, 3, 2))
        defer { GdayAudioRingDestroy(ring) }
        let input = try buffer(channels: 1)
        var time = AudioTimeStamp()
        time.mSampleTime = 123
        time.mFlags = .sampleTimeValid
        #expect(!GdayAudioRingPush(ring, input.audioBufferList, &time))
        #expect(GdayAudioRingFailure(ring) == 3)
    }

    @Test func oversizedBufferIsRejected() throws {
        let ring = try #require(GdayAudioRingCreate(1, false, 2, 2))
        defer { GdayAudioRingDestroy(ring) }
        let input = try buffer(channels: 1, frames: 3)
        var time = timestamp(50)
        #expect(!GdayAudioRingPush(ring, input.audioBufferList, &time))
        #expect(GdayAudioRingFailure(ring) == 2)
    }

    @Test func disabledHALBufferBecomesSilenceAndQueuedMemoryIsOwned() throws {
        let ring = try #require(GdayAudioRingCreate(1, true, 3, 2))
        defer { GdayAudioRingDestroy(ring) }
        var input = AudioBufferList(
            mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: 1, mDataByteSize: 12, mData: nil))
        var time = timestamp(100)
        #expect(GdayAudioRingPush(ring, &input, &time))
        var source: [Float] = [0.1, 0.2, 0.3]
        source.withUnsafeMutableBytes { bytes in
            input.mBuffers.mData = bytes.baseAddress
            time = timestamp(200)
            #expect(GdayAudioRingPush(ring, &input, &time))
            let samples = bytes.bindMemory(to: Float.self)
            for index in samples.indices { samples[index] = -1 }
        }
        #expect(source == [-1, -1, -1])  // HAL can reuse input memory after return.
        var output = [Float](repeating: -1, count: 3)
        var frames: UInt32 = 0
        var host: UInt64 = 0
        #expect(GdayAudioRingRead(ring, &output, 3, &frames, &host))
        #expect(output == [0, 0, 0])
        #expect(host == 100)
        #expect(GdayAudioRingRead(ring, &output, 3, &frames, &host))
        #expect(output == [0.1, 0.2, 0.3])
        #expect(host == 200)
    }

    @Test func shortDestinationKeepsPendingBufferForSafeDrain() throws {
        let ring = try #require(GdayAudioRingCreate(1, false, 3, 2))
        defer { GdayAudioRingDestroy(ring) }
        let input = try buffer(channels: 1)
        var time = timestamp(70)
        #expect(GdayAudioRingPush(ring, input.audioBufferList, &time))
        var output = [Float](repeating: -1, count: 3)
        var frames: UInt32 = 0
        var host: UInt64 = 0
        #expect(!GdayAudioRingRead(ring, &output, 2, &frames, &host))
        #expect(output == [-1, -1, -1])
        #expect(GdayAudioRingFailure(ring) == 2)
        #expect(GdayAudioRingRead(ring, &output, 3, &frames, &host))
        #expect(output == [100, 101, 102])
        #expect(host == 70)
        // Failure is terminal for new input even after pending samples are drained.
        #expect(!GdayAudioRingPush(ring, input.audioBufferList, &time))
        #expect(!GdayAudioRingRead(ring, &output, 3, &frames, &host))
    }
}
