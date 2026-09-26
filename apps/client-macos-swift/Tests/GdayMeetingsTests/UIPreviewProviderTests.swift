import Foundation
import Testing

@testable import GdayMeetings

struct UIPreviewProviderTests {
    @Test func explicitConfigurationConnectsProviderDependency() throws {
        let providers = try UIPreview.testProviders(
            configuration: """
                # Test-only values
                RUNPOD_ENDPOINT_URL = "https://example.invalid/v2/test"
                RUNPOD_API_KEY='test-runpod-key'
                FILE_DROP_URL=https://files.example.invalid
                FILE_DROP_API_KEY=test-filedrop-key
                """)
        let runpod = try #require(providers.first { $0.kind == .runpod })
        let filedrop = try #require(providers.first { $0.kind == .filedrop })
        #expect(runpod.uploadProviderID == filedrop.id)
        #expect(runpod.supports(.transcription))
        #expect(runpod.supports(.diarization))
        #expect(filedrop.supports(.fileTransfer))
        #expect(runpod.endpoint == "https://example.invalid/v2/test")
        let encoded = String(decoding: try JSONEncoder().encode(providers), as: UTF8.self)
        #expect(!encoded.contains("test-runpod-key"))
        #expect(!encoded.contains("test-filedrop-key"))
    }

    @Test func missingConfigurationFailsWithoutIncludingValues() {
        do {
            _ = try UIPreview.testProviders(configuration: "FILE_DROP_URL=private-test-value")
            Issue.record("Expected a missing API key error.")
        }
        catch {
            #expect(error.localizedDescription.contains("FILE_DROP_API_KEY"))
            #expect(!error.localizedDescription.contains("private-test-value"))
        }
    }
}
