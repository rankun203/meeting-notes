import CryptoKit
import Foundation

/// Disposable, versioned envelopes. Source metadata avoids hashing/reading an
/// entire recording on a cache hit. Never write alongside a user's audio file.
struct WaveformCache: Sendable {
    static let shared = WaveformCache(directory: FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("com.gdaymeetings.macos/waveforms-v1", isDirectory: true))
    let directory: URL

    private struct Identity: Codable, Equatable {
        let path: String
        let size: Int
        let modified: Date
        let created: Date?
        init(_ url: URL) throws {
            // URL resource values may be cached on the URL instance across a
            // source rewrite. Stat afresh for every identity comparison.
            let values = try FileManager.default.attributesOfItem(atPath: url.path)
            guard let size = values[.size] as? NSNumber, let modified = values[.modificationDate] as? Date else {
                throw CocoaError(.fileReadUnknown)
            }
            self.path = url.standardizedFileURL.path; self.size = size.intValue
            self.modified = modified; self.created = values[.creationDate] as? Date
        }
    }
    private struct Entry: Codable { let identity: Identity; let waveform: AudioWaveform }

    func entryURL(for source: URL) -> URL {
        let key = SHA256.hash(data: Data(source.standardizedFileURL.path.utf8)).map { String(format: "%02x", $0) }.joined()
        return directory.appendingPathComponent(key + ".json")
    }

    func cached(_ source: URL) async -> AudioWaveform? {
        await Task.detached(priority: .utility) { load(source) }.value
    }

    private func load(_ source: URL) -> AudioWaveform? {
        guard let identity = try? Identity(source),
              let values = try? entryURL(for: source).resourceValues(forKeys: [.fileSizeKey]),
              let size = values.fileSize, size <= 100_000,
              let data = try? Data(contentsOf: entryURL(for: source)),
              let entry = try? JSONDecoder().decode(Entry.self, from: data), entry.identity == identity,
              entry.waveform.duration.isFinite, entry.waveform.duration > 0,
              !entry.waveform.peaks.isEmpty, entry.waveform.peaks.count <= 4096,
              entry.waveform.peaks.allSatisfy({ $0.isFinite && $0 >= 0 && $0 <= 1 }) else { return nil }
        return entry.waveform
    }

    func waveform(source: URL, readable: URL) async throws -> AudioWaveform {
        if let cached = await cached(source) { return cached }
        let identity = try Identity(source)
        let waveform = try await AudioWaveform.read(readable)
        try Task.checkCancellation()
        // A changed file must never receive an envelope for its old contents.
        guard try Identity(source) == identity else { throw CocoaError(.fileReadUnknown) }
        await Task.detached(priority: .utility) {
            do {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                let data = try JSONEncoder().encode(Entry(identity: identity, waveform: waveform))
                try data.write(to: entryURL(for: source), options: .atomic)
                // Bound disposable cache growth to 512 small envelopes, oldest first.
                let files = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.contentModificationDateKey])
                    .filter { $0.pathExtension == "json" }
                if files.count > 512 {
                    let ordered = files.sorted {
                        ((try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast) <
                        ((try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast)
                    }
                    for url in ordered.prefix(files.count - 512) { try? FileManager.default.removeItem(at: url) }
                }
            } catch { /* A disposable cache failure must not prevent playback. */ }
        }.value
        return waveform
    }
}
