import AVFoundation
import AudioToolbox

struct AudioTrackProfile: Codable, Equatable {
    var filename: String
    var sampleRate: Double
    var channels: UInt32
    var voiceProcessed: Bool
}
struct RecordingProfile: Codable, Equatable {
    var microphoneVoiceProcessing: Bool
    var tracks: [AudioTrackProfile]
    var timeline = "Host clock; missing intervals padded with silence"
}

/// Apple documents ExtAudioFileWriteAsync as a bounded internal ring-buffer handoff;
/// warm it outside callbacks and dispose to flush. No filesystem work occurs in append.
/// https://developer.apple.com/documentation/audiotoolbox/extaudiofilewriteasync(_:_:_:)
/// AVAudioEngine regular taps are not realtime render blocks (WWDC19, 510).
final class TimedAudioWriter {
    private var file: ExtAudioFileRef?
    private let format: AVAudioFormat
    private let silence: AVAudioPCMBuffer
    private let lock = NSLock()
    private var framesWritten: Int64 = 0
    private var sourceFrames: Int64 = 0
    var capturedFrames: Int64 { lock.lock(); defer { lock.unlock() }; return sourceFrames }
    private let epoch: TimeInterval
    private var failure: Error?
    let profile: AudioTrackProfile

    init(url: URL, format: AVAudioFormat, epoch: TimeInterval, voiceProcessed: Bool = false) throws {
        self.format = format; self.epoch = epoch
        profile = AudioTrackProfile(filename: url.lastPathComponent, sampleRate: format.sampleRate, channels: format.channelCount, voiceProcessed: voiceProcessed)
        guard let silence = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4096) else { throw MeetingError.message("Could not allocate audio silence buffer.") }
        self.silence = silence
        silence.frameLength = silence.frameCapacity
        for buffer in UnsafeMutableAudioBufferListPointer(silence.mutableAudioBufferList) { if let data = buffer.mData { memset(data, 0, Int(buffer.mDataByteSize)) } }
        // Integer PCM has predictable size and interoperates with the transcription service.
        var fileFormat = AudioStreamBasicDescription(mSampleRate: format.sampleRate, mFormatID: kAudioFormatLinearPCM, mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked, mBytesPerPacket: 2 * format.channelCount, mFramesPerPacket: 1, mBytesPerFrame: 2 * format.channelCount, mChannelsPerFrame: format.channelCount, mBitsPerChannel: 16, mReserved: 0)
        try Self.check(ExtAudioFileCreateWithURL(url as CFURL, kAudioFileWAVEType, &fileFormat, nil, AudioFileFlags.eraseFile.rawValue, &file))
        do {
            var client = format.streamDescription.pointee
            try Self.check(ExtAudioFileSetProperty(file!, kExtAudioFileProperty_ClientDataFormat, UInt32(MemoryLayout<AudioStreamBasicDescription>.size), &client))
            try Self.check(ExtAudioFileWriteAsync(file!, 0, nil))
        } catch { if let file { ExtAudioFileDispose(file) }; file = nil; throw error }
    }
    deinit { if let file { ExtAudioFileDispose(file) } }

    /// Host-time anchoring pads startup skew and dropped intervals instead of collapsing time.
    /// A format change is terminal; it must not reinterpret frames at the old rate.
    func append(_ buffer: AVAudioPCMBuffer, hostSeconds: TimeInterval) throws {
        lock.lock(); defer { lock.unlock() }
        if let failure { throw failure }
        guard let file else { return }
        do {
            guard buffer.format == format else { throw MeetingError.message("The audio format changed. The partial recording was saved; start a new recording with the new device.") }
            let target = try Self.targetFrame(hostSeconds: hostSeconds, epoch: epoch, sampleRate: format.sampleRate)
            var gap = target - framesWritten
            // Sub-millisecond clock quantisation is normal; do not insert jitter-sized holes.
            if gap > Int64(format.sampleRate * 0.002) {
                guard gap <= Int64(format.sampleRate * 30) else { throw MeetingError.message("Audio capture was interrupted for over 30 seconds. The partial recording was saved.") }
                while gap > 0 {
                    let count = UInt32(min(gap, 4096))
                    try Self.check(ExtAudioFileWriteAsync(file, count, silence.audioBufferList))
                    framesWritten += Int64(count); gap -= Int64(count)
                }
            }
            let overlap = max(0, framesWritten - target)
            if overlap > Int64(format.sampleRate * 0.002) {
                // Drop overlapping source frames; copying here is safe in a regular non-RT tap.
                let skipped = min(Int64(buffer.frameLength), overlap)
                guard skipped < buffer.frameLength else { return }
                let count = buffer.frameLength - AVAudioFrameCount(skipped)
                guard let trimmed = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: count) else { throw MeetingError.message("Could not allocate aligned audio buffer.") }
                trimmed.frameLength = count
                let bytesPerFrame = Int(format.streamDescription.pointee.mBytesPerFrame)
                for (source, destination) in zip(UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList), UnsafeMutableAudioBufferListPointer(trimmed.mutableAudioBufferList)) {
                    if let src = source.mData, let dst = destination.mData { memcpy(dst, src.advanced(by: Int(skipped) * bytesPerFrame), Int(count) * bytesPerFrame) }
                }
                try Self.check(ExtAudioFileWriteAsync(file, count, trimmed.audioBufferList))
                framesWritten += Int64(count); sourceFrames += Int64(count)
            } else {
                try Self.check(ExtAudioFileWriteAsync(file, buffer.frameLength, buffer.audioBufferList))
                framesWritten += Int64(buffer.frameLength); sourceFrames += Int64(buffer.frameLength)
            }
        } catch { failure = error; throw error }
    }
    func finish() throws {
        lock.lock(); defer { lock.unlock() }
        var disposal: OSStatus = noErr
        if let file { disposal = ExtAudioFileDispose(file); self.file = nil }
        if let failure { throw failure }
        try Self.check(disposal)
    }
    static func targetFrame(hostSeconds: TimeInterval, epoch: TimeInterval, sampleRate: Double) throws -> Int64 {
        let value = (hostSeconds - epoch) * sampleRate
        guard value.isFinite, sampleRate > 0, abs(value) < Double(Int64.max) else { throw MeetingError.message("Audio capture returned an invalid timestamp.") }
        return max(0, Int64(value.rounded()))
    }
    private static func check(_ status: OSStatus) throws {
        guard status == noErr else { throw MeetingError.message("Audio recording failed (Core Audio \(status)). The disk may be full or unable to keep up; the partial recording is retained.") }
    }
}
