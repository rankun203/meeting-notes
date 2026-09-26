import Foundation
import Testing

@testable import GdayMeetings

/// Opt-in integration test. Never runs as part of ordinary local or CI tests.
/// Uses only generated speech, an isolated library, and credentials supplied for testing.
@MainActor struct ProviderLiveTests {
    @Test(.enabled(if: ProcessInfo.processInfo.environment["GDAY_PROVIDER_LIVE_TEST"] == "1"))
    func syntheticSpeechThroughFiledropAndRunPod() async throws {
        let package = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent()
        let contents = try String(contentsOf: package.appendingPathComponent(".env"), encoding: .utf8)
        var values: [String: String] = [:]
        for line in contents.components(separatedBy: .newlines) {
            let line = line.trimmingCharacters(in: .whitespaces)
            guard !line.hasPrefix("#"), let split = line.firstIndex(of: "=") else { continue }
            values[String(line[..<split]).trimmingCharacters(in: .whitespaces)] =
                String(line[line.index(after: split)...]).trimmingCharacters(in: .whitespaces)
                .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
        }
        // Do not use expectation macros on secret values: failure output can print operands.
        guard let runpodURL = values["RUNPOD_ENDPOINT_URL"], let runpodKey = values["RUNPOD_API_KEY"],
            let filedropURL = values["FILE_DROP_URL"], let filedropKey = values["FILE_DROP_API_KEY"]
        else {
            throw ServiceError("The live-test credential file is incomplete.")
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "gday-provider-live-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let speech = root.appendingPathComponent("synthetic-meeting.aiff")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/say")
        process.arguments = [
            "-o", speech.path,
            "This is a test meeting. We agreed to finish the report on Friday. Alex will review the notes.",
        ]
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw ServiceError("Couldn't generate test speech.") }
        let store = MeetingStore(dataDirectory: root.appendingPathComponent("library"))
        var upload = ServiceProvider(kind: .filedrop)
        upload.endpoint = filedropURL
        upload.apiKey = filedropKey
        upload.enabledCapabilities = [.fileTransfer]
        var runpod = ServiceProvider(kind: .runpod)
        runpod.endpoint = runpodURL
        runpod.apiKey = runpodKey
        runpod.enabledCapabilities = [.transcription]
        runpod.uploadProviderID = upload.id
        store.settings.defaultLanguage = "en"
        store.settings.serviceProviders = [upload, runpod]
        store.settings.transcriptionProviderID = runpod.id
        _ = try await ProviderConnectionChecker.check(upload)
        _ = try await ProviderConnectionChecker.check(runpod)
        let imported = try await store.importAudioFiles([speech])
        let id = try #require(imported.first)
        await store.transcribe(id: id)
        // Surface only the app's sanitized error, never provider configuration.
        if let error = store.errorMessage { throw ServiceError(error) }
        let meeting = try #require(store.meetings.first { $0.id == id })
        #expect(meeting.transcriptionAttempt == nil)
        #expect(!meeting.transcript.isEmpty)
        let text = meeting.transcript.map(\.text).joined(separator: " ").lowercased()
        #expect(text.contains("friday"))
        #expect(meeting.transcript.allSatisfy { $0.start >= 0 && $0.end >= $0.start })
        #expect(FileManager.default.fileExists(atPath: speech.path))
    }
}
