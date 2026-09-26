import CryptoKit
import Foundation

/// Provider metadata (language and model lists) saved per provider, so views can
/// show it without contacting the provider. Each entry records the non-secret
/// configuration it describes; a different endpoint makes the entry unusable.
@MainActor final class ProviderMetadataCache<Value: Codable & Equatable> {
    struct Entry: Codable, Equatable {
        let providerID: UUID
        let fingerprint: String
        let value: Value
        let fetchedAt: Date
    }
    private struct File: Codable {
        var version = 1
        var entries: [Entry]
    }
    private let url: URL
    /// A read-only library (for example one saved by a newer app version) must not gain files.
    private let canWrite: () -> Bool
    private(set) var entries: [UUID: Entry] = [:]

    init(directory: URL, fileName: String, canWrite: @escaping () -> Bool = { true }) {
        url = directory.appendingPathComponent(fileName)
        self.canWrite = canWrite
        // An unreadable cache only means the metadata must be loaded again.
        if let data = try? Data(contentsOf: url), let file = try? JSONDecoder().decode(File.self, from: data),
            file.version == 1
        {
            entries = Dictionary(file.entries.map { ($0.providerID, $0) }) { _, last in last }
        }
    }

    /// Hashes configuration fields so the file names no endpoint or credential.
    nonisolated static func fingerprint(_ fields: [String]) -> String {
        SHA256.hash(data: Data(fields.joined(separator: "\n").utf8)).map { String(format: "%02x", $0) }.joined()
    }

    func entry(providerID: UUID, fingerprint: String) -> Entry? {
        entries[providerID].flatMap { $0.fingerprint == fingerprint ? $0 : nil }
    }

    /// Saves the entry and drops entries for providers that no longer exist.
    func store(_ entry: Entry, keeping providerIDs: Set<UUID>) {
        entries[entry.providerID] = entry
        entries = entries.filter { providerIDs.contains($0.key) }
        let file = File(entries: entries.values.sorted { $0.providerID.uuidString < $1.providerID.uuidString })
        guard canWrite() else { return }
        // The in-memory entry stays usable if the write fails; it is loaded again next time.
        try? JSONEncoder().encode(file).write(to: url, options: .atomic)
    }
}
