import AVFoundation
import Accelerate
import Foundation

/// Fixed-size peak envelope, derived locally from decoded audio. All channels
/// contribute, so out-of-phase stereo cannot hide activity through downmixing.
struct AudioWaveform: Sendable, Equatable, Codable {
    let duration: Double
    let peaks: [Float]

    func peak(at time: Double) -> Float {
        guard duration > 0, time >= 0, time < duration, !peaks.isEmpty else { return 0 }
        return peaks[min(peaks.count - 1, Int(time / duration * Double(peaks.count)))]
    }

    static func read(_ url: URL, bucketCount: Int = 1200) async throws -> AudioWaveform {
        let work = Task.detached(priority: .utility) {
            if ["opus", "ogg"].contains(url.pathExtension.lowercased()) {
                let decoder = try OpusFileDecoder(url)
                let count = min(max(1, bucketCount), 4096, Int(min(decoder.totalFrames, 4096)))
                let buffer = AVAudioPCMBuffer(pcmFormat: StreamingAudioReader.format, frameCapacity: 1024)!
                var peaks = [Float](repeating: 0, count: count)
                for bucket in 0..<count {
                    try Task.checkCancellation()
                    let window = sampleWindow(length: decoder.totalFrames, bucket: bucket, count: count)
                    if decoder.position != window.start { try decoder.seek(frame: window.start) }
                    try decoder.read(into: buffer, frames: UInt32(window.count))
                    guard buffer.frameLength == window.count else {
                        throw MeetingError.message("Incomplete Opus waveform samples.")
                    }
                    for channel in 0..<2 {
                        var peak: Float = 0
                        vDSP_maxmgv(buffer.floatChannelData![channel], 1, &peak, vDSP_Length(buffer.frameLength))
                        if peak.isFinite { peaks[bucket] = max(peaks[bucket], min(1, peak)) }
                    }
                }
                return AudioWaveform(duration: Double(decoder.totalFrames) / 48000, peaks: peaks)
            }
            let file = try AVAudioFile(forReading: url, commonFormat: .pcmFormatFloat32, interleaved: false)
            let format = file.processingFormat
            guard file.length > 0, format.sampleRate > 0,
                let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 1024)
            else {
                throw MeetingError.message("No audio samples available for waveform.")
            }
            let count = min(max(1, bucketCount), 4096, Int(min(file.length, 4096)))
            var peaks = [Float](repeating: 0, count: count)
            // A bounded overview, not an exhaustive transient detector. Short
            // buckets are exact; long buckets sample their center. At the default
            // resolution this examines at most 1,228,800 frames per channel,
            // regardless of recording length. Codec seeking may add decoding work.
            for bucket in 0..<count {
                try Task.checkCancellation()
                let window = sampleWindow(length: file.length, bucket: bucket, count: count)
                if file.framePosition != window.start { file.framePosition = window.start }
                try file.read(into: buffer, frameCount: AVAudioFrameCount(window.count))
                guard buffer.frameLength == window.count, let channels = buffer.floatChannelData else {
                    throw MeetingError.message("Incomplete audio samples for waveform.")
                }
                for channel in 0..<Int(format.channelCount) {
                    var peak: Float = 0
                    vDSP_maxmgv(channels[channel], 1, &peak, vDSP_Length(buffer.frameLength))
                    if peak.isFinite { peaks[bucket] = max(peaks[bucket], min(1, peak)) }
                }
            }
            return AudioWaveform(duration: Double(file.length) / format.sampleRate, peaks: peaks)
        }
        return try await withTaskCancellationHandler(operation: { try await work.value }, onCancel: { work.cancel() })
    }

    static func sampleWindow(length: Int64, bucket: Int, count: Int) -> (start: Int64, count: Int64) {
        let start = Int64(ceil(Double(bucket) * Double(length) / Double(count)))
        let end = Int64(ceil(Double(bucket + 1) * Double(length) / Double(count)))
        let frames = min(1024, end - start)
        return (start + (end - start - frames) / 2, frames)
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
