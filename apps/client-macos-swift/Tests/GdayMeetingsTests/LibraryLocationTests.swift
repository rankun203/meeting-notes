import Foundation
import Testing

@testable import GdayMeetings

struct LibraryLocationTests {
    @Test func migrationPreservesOriginalAndNeverMergesLibraries() throws {
        let files = FileManager.default
        let home = files.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? files.removeItem(at: home) }
        let legacy = home.appendingPathComponent("Library/Application Support/Gday Meetings Swift")
        try files.createDirectory(at: legacy, withIntermediateDirectories: true)
        let original = Data("existing library and recording".utf8)
        try original.write(to: legacy.appendingPathComponent("recording.wav"))
        try LibraryLocation.migrateLegacyLibrary(home: home)
        let destination = LibraryLocation.directory(home: home)
        #expect(destination.path == home.path + "/.local/share/com.gdaymeetings.macos")
        #expect(try Data(contentsOf: destination.appendingPathComponent("recording.wav")) == original)
        #expect(try Data(contentsOf: legacy.appendingPathComponent("recording.wav")) == original)
        let newer = Data("new recording".utf8)
        try newer.write(to: destination.appendingPathComponent("recording.wav"))
        try original.write(to: legacy.appendingPathComponent("other.wav"))
        try LibraryLocation.migrateLegacyLibrary(home: home)
        #expect(try Data(contentsOf: destination.appendingPathComponent("recording.wav")) == newer)
        #expect(!files.fileExists(atPath: destination.appendingPathComponent("other.wav").path))
    }

    @Test func freshInstallDoesNotInventLegacyLibrary() throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try LibraryLocation.migrateLegacyLibrary(home: home)
        #expect(!FileManager.default.fileExists(atPath: LibraryLocation.directory(home: home).path))
    }
}
