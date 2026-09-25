import AVFoundation
import Foundation

struct PreparedPlaybackAudio {
    let url: URL
    let temporary: Bool
}

enum AudioPlaybackPreparation {
    /// Compatibility conversion for transcription/service inputs. Interactive
    /// playback uses OpusFileDecoder/StreamingPlayback and never calls this path.
    /// The caller owns the temporary file and removes it after its consumer finishes.
    static func prepare(_ source: URL) async throws -> PreparedPlaybackAudio {
        guard ["opus", "ogg"].contains(source.pathExtension.lowercased()) else {
            return .init(url: source, temporary: false)
        }
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent(
            "gday-playback-\(UUID().uuidString).caf")
        do {
            try decodeOpus(source, to: destination)
            try Task.checkCancellation()
            return .init(url: destination, temporary: true)
        }
        catch {
            try? FileManager.default.removeItem(at: destination)
            throw error
        }
    }
    static func opusChannels(_ source: URL) throws -> Int { try OggOpusReader(source).channels }
    static func opusMetadata(_ source: URL) async throws -> (channels: Int, duration: Double) {
        let reader = try OggOpusReader(source)
        var final: Int64?
        var frames: Int64 = 0
        var lastPacketFrames: Int64 = 0
        while let packet = try reader.nextAudioPacket() {
            try Task.checkCancellation()
            lastPacketFrames = Int64(try opusPacketFrames(packet.data))
            frames += lastPacketFrames
            if let granule = packet.finalGranule { final = granule }
        }
        guard let final, final >= max(Int64(reader.preSkip), frames - lastPacketFrames), final <= frames, reader.sawEnd
        else { throw ServiceError("The Opus recording has no valid final duration.") }
        return (reader.channels, Double(final - Int64(reader.preSkip)) / 48000)
    }

    /// Core Audio supports the Opus codec but AVAsset doesn't open its Ogg container.
    /// Demux packets without buffering the recording; decode to a seekable temporary CAF.
    /// https://developer.apple.com/documentation/avfaudio/avaudioconverter
    /// https://www.rfc-editor.org/rfc/rfc7845 (pre-skip, output gain and final granule)
    private static func decodeOpus(_ source: URL, to destination: URL) throws {
        let reader = try OggOpusReader(source)
        var description = AudioStreamBasicDescription(
            mSampleRate: 48000, mFormatID: kAudioFormatOpus, mFormatFlags: 0, mBytesPerPacket: 0, mFramesPerPacket: 0,
            mBytesPerFrame: 0, mChannelsPerFrame: UInt32(reader.channels), mBitsPerChannel: 0, mReserved: 0)
        guard let compressed = AVAudioFormat(streamDescription: &description),
            let pcm = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: UInt32(reader.channels)),
            let converter = AVAudioConverter(from: compressed, to: pcm),
            let output = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: 5760)
        else { throw ServiceError("The native Opus decoder is unavailable on this Mac.") }
        converter.primeMethod = .none
        var file: AVAudioFile? = try AVAudioFile(forWriting: destination, settings: pcm.settings)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: destination.path)
        var skipped = 0
        var written: Int64 = 0
        var decoded: Int64 = 0
        while let packet = try reader.nextAudioPacket() {
            try Task.checkCancellation()
            let input = AVAudioCompressedBuffer(
                format: compressed, packetCapacity: 1, maximumPacketSize: max(1, packet.data.count))
            input.byteLength = UInt32(packet.data.count)
            input.packetCount = 1
            packet.data.copyBytes(to: input.data.assumingMemoryBound(to: UInt8.self), count: packet.data.count)
            input.packetDescriptions?.pointee = AudioStreamPacketDescription(
                mStartOffset: 0, mVariableFramesInPacket: UInt32(try opusPacketFrames(packet.data)),
                mDataByteSize: UInt32(packet.data.count))
            var supplied = false
            var error: NSError?
            output.frameLength = 0
            let status = converter.convert(to: output, error: &error) { _, state in
                if supplied {
                    state.pointee = .noDataNow
                    return nil
                }
                supplied = true
                state.pointee = .haveData
                return input
            }
            if status == .error { throw error ?? ServiceError("The Opus packet could not be decoded.") }
            decoded += Int64(output.frameLength)
            let discard = min(Int(output.frameLength), max(0, reader.preSkip - skipped))
            skipped += discard
            var count = Int(output.frameLength) - discard
            if let granule = packet.finalGranule {
                guard granule >= Int64(reader.preSkip) + written, granule <= decoded else {
                    throw ServiceError("The Opus final duration lies outside its final decoded packet.")
                }
                count = Int(granule - Int64(reader.preSkip) - written)
            }
            if count > 0 {
                guard let samples = output.floatChannelData else { throw ServiceError("Missing decoded Opus samples.") }
                let gain = Float(pow(10, Double(reader.outputGain) / 5120))
                for channel in 0..<reader.channels {
                    for frame in 0..<count { samples[channel][frame] = samples[channel][frame + discard] * gain }
                }
                output.frameLength = UInt32(count)
                try file?.write(from: output)
                written += Int64(count)
            }
        }
        guard written > 0, reader.sawEnd else { throw ServiceError("The Opus recording is empty or incomplete.") }
        file = nil
    }
    private static func opusPacketFrames(_ packet: Data) throws -> Int {
        guard let toc = packet.first else { throw ServiceError("An Opus packet is empty.") }
        let config = Int(toc >> 3)
        let frame: Int
        if config >= 16 {
            frame = 120 << (config & 3)
        }
        else if config >= 12 {
            frame = 480 << (config & 1)
        }
        else {
            frame = config & 3 == 3 ? 2880 : 480 << (config & 3)
        }
        let count: Int
        switch toc & 3 {
        case 0: count = 1
        case 1, 2: count = 2
        default:
            guard packet.count > 1 else { throw ServiceError("Invalid Opus frame count.") }
            count = Int(packet[1] & 63)
        }
        guard count > 0, frame * count <= 5760 else { throw ServiceError("Invalid Opus packet duration.") }
        return frame * count
    }
}

private final class OggOpusReader {
    struct Packet {
        let data: Data
        let finalGranule: Int64?
    }
    private(set) var channels = 0
    private(set) var preSkip = 0
    private(set) var outputGain: Int16 = 0
    private let file: FileHandle
    private var packets: [Packet] = []
    private var partial = Data()
    private var serial: UInt32?
    private var sequence: UInt32 = 0
    private(set) var sawEnd = false
    init(_ url: URL) throws {
        file = try FileHandle(forReadingFrom: url)
        guard let identification = try nextPacket()?.data, identification.count >= 19,
            identification.prefix(8) == Data("OpusHead".utf8), identification[8] <= 15,
            [1, 2].contains(identification[9]), identification[18] == 0
        else {
            throw ServiceError("Only mono/stereo Ogg Opus mapping family zero is supported.")
        }
        channels = Int(identification[9])
        preSkip = Int(identification[10]) | (Int(identification[11]) << 8)
        outputGain = Int16(bitPattern: UInt16(identification[16]) | (UInt16(identification[17]) << 8))
        guard let tags = try nextPacket()?.data, tags.prefix(8) == Data("OpusTags".utf8) else {
            throw ServiceError("The Opus comment header is missing.")
        }
    }
    deinit { try? file.close() }
    func nextAudioPacket() throws -> Packet? { try nextPacket() }
    private func nextPacket() throws -> Packet? {
        while packets.isEmpty {
            if sawEnd { return nil }
            try readPage()
        }
        return packets.removeFirst()
    }
    private func readExactly(_ count: Int) throws -> Data {
        var result = Data()
        while result.count < count {
            guard let chunk = try file.read(upToCount: count - result.count), !chunk.isEmpty else {
                throw ServiceError("The Ogg recording is truncated.")
            }
            result.append(chunk)
        }
        return result
    }
    private func readPage() throws {
        let header = try readExactly(27)
        guard header.prefix(4) == Data("OggS".utf8), header[4] == 0 else {
            throw ServiceError("Unsupported Ogg audio container.")
        }
        let laces = try readExactly(Int(header[26]))
        let body = try readExactly(laces.reduce(0) { $0 + Int($1) })
        var page = header + laces + body
        let expectedCRC = Self.uint32(header, 22)
        page.replaceSubrange(22..<26, with: [0, 0, 0, 0])
        var crc: UInt32 = 0
        for byte in page {
            crc ^= UInt32(byte) << 24
            for _ in 0..<8 { crc = crc & 0x8000_0000 != 0 ? (crc << 1) ^ 0x04c1_1db7 : crc << 1 }
        }
        guard crc == expectedCRC else { throw ServiceError("The Ogg recording failed its integrity check.") }
        let pageSerial = Self.uint32(header, 14)
        if serial == nil {
            serial = pageSerial
            guard header[5] & 2 != 0 else { throw ServiceError("Missing Ogg stream beginning.") }
        }
        guard serial == pageSerial, Self.uint32(header, 18) == sequence else {
            throw ServiceError("Chained or interrupted Ogg streams are not supported.")
        }
        sequence &+= 1
        guard (header[5] & 1 != 0) == !partial.isEmpty else { throw ServiceError("Invalid continued Ogg packet.") }
        let ended = header[5] & 4 != 0
        var granule: UInt64 = 0
        for index in 0..<8 { granule |= UInt64(header[6 + index]) << (index * 8) }
        var cursor = 0
        for (index, lace) in laces.enumerated() {
            let count = Int(lace)
            partial.append(body[cursor..<(cursor + count)])
            cursor += count
            guard partial.count <= 65_536 else { throw ServiceError("The Ogg packet exceeds the supported size.") }
            if count < 255 {
                let final = ended && index == laces.count - 1 ? Int64(bitPattern: granule) : nil
                packets.append(Packet(data: partial, finalGranule: final))
                partial.removeAll(keepingCapacity: true)
            }
        }
        if ended {
            guard partial.isEmpty else { throw ServiceError("The final Ogg packet is incomplete.") }
            if let trailing = try file.read(upToCount: 1), !trailing.isEmpty {
                throw ServiceError("Chained Ogg streams or trailing bytes are not supported.")
            }
            sawEnd = true
        }
    }
    static func uint32(_ data: Data, _ offset: Int) -> UInt32 {
        (0..<4).reduce(UInt32(0)) { $0 | (UInt32(data[offset + $1]) << ($1 * 8)) }
    }
}
