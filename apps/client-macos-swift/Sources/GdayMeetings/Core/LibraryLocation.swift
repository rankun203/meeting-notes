import Foundation

enum LibraryLocation {
    static func directory(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> URL {
        home.appendingPathComponent(".local/share/com.gdaymeetings.macos", isDirectory: true)
    }

    /// Copy once, preserving the old library for rollback. Publish the complete copy
    /// atomically; never merge two libraries or leave an incomplete destination.
    static func migrateLegacyLibrary(home: URL = FileManager.default.homeDirectoryForCurrentUser) throws {
        let files = FileManager.default
        let destination = directory(home: home)
        let legacy = home.appendingPathComponent("Library/Application Support/Gday Meetings Swift", isDirectory: true)
        guard !files.fileExists(atPath: destination.path), files.fileExists(atPath: legacy.path) else { return }
        let parent = destination.deletingLastPathComponent()
        try files.createDirectory(at: parent, withIntermediateDirectories: true)
        let staging = parent.appendingPathComponent(".gday-migration-\(UUID().uuidString)", isDirectory: true)
        defer { try? files.removeItem(at: staging) }
        try files.copyItem(at: legacy, to: staging)
        try files.setAttributes([.posixPermissions: 0o700], ofItemAtPath: staging.path)
        try files.moveItem(at: staging, to: destination)
    }
}
