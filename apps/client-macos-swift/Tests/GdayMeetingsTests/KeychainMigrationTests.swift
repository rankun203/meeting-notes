import Testing
@testable import GdayMeetings

private final class MemoryCredentialStorage: CredentialStorage {
    var values: [String: String] = [:]
    var failWrite = false
    var failRemoval: String?
    func read(account: String, service: String) throws -> String? { values[service + ":" + account] }
    func write(_ value: String, account: String, service: String) throws {
        if failWrite { throw ServiceError("Simulated write failure") }
        values[service + ":" + account] = value
    }
    func remove(account: String, service: String) throws {
        if failRemoval == service { throw ServiceError("Simulated removal failure") }
        values.removeValue(forKey: service + ":" + account)
    }
}

struct KeychainMigrationTests {
    private let current = CredentialIdentityMigration.service
    private let legacy = CredentialIdentityMigration.legacyService

    @Test func migratesAllCredentialAccountsAndDeletesLegacyCopies() throws {
        let storage = MemoryCredentialStorage()
        let migration = CredentialIdentityMigration(storage: storage)
        for account in ["gday-oauth", "llm-api-key", "transcription-api-key"] {
            storage.values[legacy + ":" + account] = "saved-" + account
            #expect(try migration.get(account) == "saved-" + account)
            #expect(storage.values[current + ":" + account] == "saved-" + account)
            #expect(storage.values[legacy + ":" + account] == nil)
            try migration.delete(account)
            #expect(try migration.get(account) == nil)
        }
    }

    @Test func canonicalValueIncludingEmptyKeyWins() throws {
        let storage = MemoryCredentialStorage()
        let migration = CredentialIdentityMigration(storage: storage)
        storage.values[current + ":key"] = ""
        storage.values[legacy + ":key"] = "old-secret"
        #expect(try migration.get("key") == "")
        try migration.delete("key")
        #expect(try migration.get("key") == nil)
    }

    @Test func failedMigrationWritePreservesLegacyCredential() throws {
        let storage = MemoryCredentialStorage()
        storage.values[legacy + ":key"] = "old-secret"
        storage.failWrite = true
        let migration = CredentialIdentityMigration(storage: storage)
        #expect(throws: (any Error).self) { try migration.get("key") }
        #expect(storage.values[legacy + ":key"] == "old-secret")
        #expect(storage.values[current + ":key"] == nil)
    }

    @Test func failedLegacyRemovalDoesNotDeleteCanonicalCredential() throws {
        let storage = MemoryCredentialStorage()
        storage.values[legacy + ":key"] = "old-secret"
        storage.values[current + ":key"] = "new-secret"
        storage.failRemoval = legacy
        let migration = CredentialIdentityMigration(storage: storage)
        #expect(throws: (any Error).self) { try migration.delete("key") }
        #expect(try migration.get("key") == "new-secret")
        storage.failRemoval = nil
        try migration.delete("key")
        #expect(try migration.get("key") == nil)
    }

    @Test func failedCanonicalDeletionCannotResurrectLegacyCredential() throws {
        let storage = MemoryCredentialStorage()
        storage.values[legacy + ":key"] = "old-secret"
        storage.values[current + ":key"] = "new-secret"
        storage.failRemoval = current
        let migration = CredentialIdentityMigration(storage: storage)
        #expect(throws: (any Error).self) { try migration.delete("key") }
        #expect(storage.values[legacy + ":key"] == nil)
        storage.failRemoval = nil
        try migration.delete("key")
        #expect(try migration.get("key") == nil)
    }
}
