import AVFoundation
import OpusFileBridge

/// Each reader belongs to one worker queue. Never call it from the render thread.
protocol StreamingAudioReading: AnyObject {
    var totalFrames: Int64 { get }
    func seek(frame: Int64) throws
    func read(into buffer: AVAudioPCMBuffer, frames: AVAudioFrameCount) throws
}

enum StreamingAudioReader {
    static let sampleRate: Double = 48000
    static let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 2)!
    static func open(_ url: URL) throws -> any StreamingAudioReading {
        if ["opus", "ogg"].contains(url.pathExtension.lowercased()) { return try OpusFileDecoder(url) }
        return try NativeAudioReader(url)
    }
}

/// libopusfile owns container indexing, pre-skip, header gain, seek preroll, and
/// end trimming. Only the requested PCM is decoded; no temporary audio file.
/// https://opus-codec.org/docs/opusfile_api-0.12/group__stream__seeking.html
final class OpusFileDecoder: StreamingAudioReading {
    private let file: OpaquePointer
    let channels: Int
    let totalFrames: Int64
    private var interleaved: [Float]
    private var atEnd = false
    var position: Int64 { atEnd ? totalFrames : gday_opus_position(file) }
    var bytesRead: UInt64 { gday_opus_bytes_read(file) }

    init(_ url: URL) throws {
        var error: Int32 = 0
        guard let file = url.withUnsafeFileSystemRepresentation({ gday_opus_open($0, &error) }) else {
            throw ServiceError("Cannot open Ogg Opus audio (libopusfile \(error)).")
        }
        self.file = file
        channels = Int(gday_opus_channels(file))
        totalFrames = gday_opus_frames(file)
        interleaved = [Float](repeating: 0, count: 5760 * channels)
    }
    deinit { gday_opus_close(file) }

    func seek(frame: Int64) throws {
        // op_pcm_seek accepts sample positions, not the one-past-end UI cursor.
        atEnd = frame >= totalFrames
        if atEnd { return }
        let result = gday_opus_seek(file, max(0, frame))
        guard result == 0 else { throw ServiceError("Cannot seek Opus audio (libopusfile \(result)).") }
    }

    func read(into buffer: AVAudioPCMBuffer, frames: AVAudioFrameCount) throws {
        precondition(buffer.format == StreamingAudioReader.format && frames <= buffer.frameCapacity)
        if atEnd { buffer.frameLength = 0; return }
        let output = buffer.floatChannelData!
        var written = 0
        while written < Int(frames) {
            let requested = min(5760, Int(frames) - written)
            let count = interleaved.withUnsafeMutableBufferPointer {
                gday_opus_read(file, $0.baseAddress, Int32(requested * channels))
            }
            // A hole is an explicit error, not permission to silently desync tracks.
            guard count >= 0 else { throw ServiceError("Opus audio is damaged or unreadable (libopusfile \(count)).") }
            if count == 0 { break }
            for frame in 0..<Int(count) {
                output[0][written + frame] = interleaved[frame * channels]
                output[1][written + frame] = interleaved[frame * channels + channels - 1]
            }
            written += Int(count)
        }
        buffer.frameLength = AVAudioFrameCount(written)
    }
}

/// File-backed incremental decoding for native formats. Conversion is stateful
/// across blocks and resets only on seek; never resample each block in isolation.
private final class NativeAudioReader: StreamingAudioReading {
    private let file: AVAudioFile
    private let converter: AVAudioConverter
    private let input: AVAudioPCMBuffer
    let totalFrames: Int64
    init(_ url: URL) throws {
        file = try AVAudioFile(forReading: url, commonFormat: .pcmFormatFloat32, interleaved: false)
        guard file.length > 0, file.processingFormat.sampleRate > 0,
              let converter = AVAudioConverter(from: file.processingFormat, to: StreamingAudioReader.format),
              let input = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: 8192) else {
            throw ServiceError("This audio file has no playable samples.")
        }
        self.converter = converter; self.input = input
        // Avoid adding priming silence when seeking; the converter handles rate
        // conversion with one continuous state per track.
        converter.primeMethod = .none
        totalFrames = Int64((Double(file.length) / file.processingFormat.sampleRate * 48000).rounded())
    }
    func seek(frame: Int64) throws {
        file.framePosition = min(file.length, max(0, Int64((Double(frame) / 48000 * file.processingFormat.sampleRate).rounded())))
        converter.reset()
    }
    func read(into buffer: AVAudioPCMBuffer, frames: AVAudioFrameCount) throws {
        precondition(frames == buffer.frameCapacity)
        buffer.frameLength = 0
        var conversionError: NSError?
        var readError: Error?
        let status = converter.convert(to: buffer, error: &conversionError) { [self] requested, state in
            do {
                guard file.framePosition < file.length else {
                    state.pointee = .endOfStream
                    return nil
                }
                try file.read(into: input, frameCount: min(requested, input.frameCapacity))
                state.pointee = input.frameLength > 0 ? .haveData : .endOfStream
                return input.frameLength > 0 ? input : nil
            } catch {
                readError = error; state.pointee = .endOfStream; return nil
            }
        }
        if let readError { throw readError }
        if status == .error { throw conversionError ?? ServiceError("Cannot decode this audio file.") }
    }
}
