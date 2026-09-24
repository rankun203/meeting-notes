import Accelerate
import AVFoundation
import Foundation

/// Fixed-size peak envelope, derived locally from decoded audio. All channels
/// contribute, so out-of-phase stereo cannot hide activity through downmixing.
struct AudioWaveform: Sendable, Equatable {
    let duration: Double
    let peaks: [Float]

    func peak(at time: Double) -> Float {
        guard duration > 0, time >= 0, time < duration, !peaks.isEmpty else { return 0 }
        return peaks[min(peaks.count - 1, Int(time / duration * Double(peaks.count)))]
    }

    static func read(_ url: URL, bucketCount: Int = 1200) async throws -> AudioWaveform {
        let work = Task.detached(priority: .utility) {
            let file = try AVAudioFile(forReading: url, commonFormat: .pcmFormatFloat32, interleaved: false)
            let format = file.processingFormat
            guard file.length > 0, format.sampleRate > 0,
                  let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 8192) else {
                throw MeetingError.message("No audio samples available for waveform.")
            }
            let count = min(max(1, bucketCount), 4096, Int(min(file.length, 4096)))
            var peaks = [Float](repeating: 0, count: count)
            var position: AVAudioFramePosition = 0
            while position < file.length {
                try Task.checkCancellation()
                try file.read(into: buffer)
                guard buffer.frameLength > 0, let channels = buffer.floatChannelData else { break }
                var offset = 0
                while offset < Int(buffer.frameLength) {
                    let frame = position + Int64(offset)
                    let bucket = min(count - 1, Int(Double(frame) * Double(count) / Double(file.length)))
                    let end = Int64(ceil(Double(bucket + 1) * Double(file.length) / Double(count)))
                    let length = min(Int(buffer.frameLength) - offset, max(1, Int(end - frame)))
                    for channel in 0..<Int(format.channelCount) {
                        var peak: Float = 0
                        vDSP_maxmgv(channels[channel].advanced(by: offset), 1, &peak, vDSP_Length(length))
                        if peak.isFinite { peaks[bucket] = max(peaks[bucket], min(1, peak)) }
                    }
                    offset += length
                }
                position += Int64(buffer.frameLength)
            }
            return AudioWaveform(duration: Double(file.length) / format.sampleRate, peaks: peaks)
        }
        return try await withTaskCancellationHandler(operation: { try await work.value }, onCancel: { work.cancel() })
    }
}

extension AudioWaveform {
    func peak(from start: Double, to end: Double) -> Float {
        guard duration > 0, start < duration, end > 0, end > start, !peaks.isEmpty else { return 0 }
        let lower = max(0, min(peaks.count - 1, Int(max(0, start) / duration * Double(peaks.count))))
        let upper = max(lower + 1, min(peaks.count, Int(ceil(min(duration, end) / duration * Double(peaks.count)))))
        return peaks[lower..<upper].max() ?? 0
    }
}
