import Foundation
import Testing

@testable import GdayMeetings

struct RecordingLogExportTests {
    /// OSLogStore scoped to this process reads back a capture log line without
    /// entitlements, which the Help menu export relies on.
    @Test func exportIncludesCaptureEntriesFromThisProcess() throws {
        let marker = UUID().uuidString
        let start = Date().addingTimeInterval(-5)
        CaptureLog.capture.notice("Export test \(marker, privacy: .public)")
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = try RecordingLogExport.export(since: start, to: directory)
        let text = try String(contentsOf: url, encoding: .utf8)
        #expect(text.contains(marker))
        #expect(text.contains(":capture] notice"))
    }
}
