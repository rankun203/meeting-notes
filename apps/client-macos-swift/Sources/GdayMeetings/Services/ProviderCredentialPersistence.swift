import CryptoKit
import Darwin
import Foundation

/// Bind a secret to its destination as well as its provider. A failed disk write
/// must never pair a new destination's API key with the previously saved URL.
enum ProviderCredentialPersistence {
    static func account(for provider: ServiceProvider) -> String {
        let destination = provider.kind.rawValue + "\n" + provider.endpoint
        let digest = SHA256.hash(data: Data(destination.utf8)).map { String(format: "%02x", $0) }.joined()
        return "provider-\(provider.id.uuidString)-\(digest)"
    }

    static func writeSettings(_ data: Data, to destination: URL) throws {
        let temporary = destination.deletingLastPathComponent().appendingPathComponent(
            ".settings-\(UUID().uuidString).tmp")
        defer { try? FileManager.default.removeItem(at: temporary) }
        try data.write(to: temporary, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: temporary.path)
        guard Darwin.rename(temporary.path, destination.path) == 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
    }

    static func save(
        previous: [String: String], next: [String: String],
        write: (String, String) throws -> Void,
        remove: (String) throws -> Void,
        persistSettings: () throws -> Void
    ) throws {
        let changed = Set(previous.keys).union(next.keys).filter { previous[$0] != next[$0] }.sorted()
        var attempted: [String] = []
        do {
            for account in changed {
                // Include the failing operation: Keychain adapters may update the
                // canonical item before a later cleanup operation reports failure.
                attempted.append(account)
                if let value = next[account] {
                    try write(account, value)
                }
                else {
                    try remove(account)
                }
            }
            try persistSettings()
        }
        catch {
            let original = error
            var restoreFailed = false
            for account in attempted.reversed() {
                do {
                    if let value = previous[account] {
                        try write(account, value)
                    }
                    else {
                        try remove(account)
                    }
                }
                catch { restoreFailed = true }
            }
            if restoreFailed {
                throw ServiceError(
                    "Couldn't save provider settings or restore all saved API keys. Reenter the affected keys before using those providers."
                )
            }
            throw original
        }
    }
}
