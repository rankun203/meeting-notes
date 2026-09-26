import Testing

@testable import GdayMeetings

struct ProviderConfigurationEligibilityTests {
    private func configuredProviders() throws -> [ServiceProvider] {
        try UIPreview.testProviders(
            configuration: """
                RUNPOD_ENDPOINT_URL=https://example.invalid/v2/test
                RUNPOD_API_KEY=test-key
                FILE_DROP_URL=https://files.example.invalid
                FILE_DROP_API_KEY=test-key
                """)
    }

    @Test func runpodRequiresItsOwnConnectionFields() throws {
        let providers = try configuredProviders()
        var runpod = providers[0]
        #expect(ProviderConfigurationEligibility.canSelect(runpod, for: .transcription, providers: providers))
        for endpoint in ["", "not-a-url", "https://example.invalid/v2/test/run"] {
            runpod.endpoint = endpoint
            #expect(!ProviderConfigurationEligibility.canSelect(runpod, for: .transcription, providers: providers))
        }
        runpod = providers[0]
        runpod.apiKey = "  "
        #expect(!ProviderConfigurationEligibility.canSelect(runpod, for: .transcription, providers: providers))
    }

    @Test func runpodRequiresConfiguredEnabledUploadProvider() throws {
        var providers = try configuredProviders()
        let runpod = providers[0]
        #expect(!ProviderConfigurationEligibility.canSelect(runpod, for: .transcription, providers: []))
        providers[1].enabledCapabilities = []
        #expect(!ProviderConfigurationEligibility.canSelect(runpod, for: .transcription, providers: providers))
        providers = try configuredProviders()
        providers[1].endpoint = ""
        #expect(!ProviderConfigurationEligibility.canSelect(providers[0], for: .transcription, providers: providers))
    }

    @Test func compatibleLocalSummaryProviderNeedsModelButNotAPIKey() {
        var provider = ServiceProvider(kind: .openAICompatible)
        provider.endpoint = "http://localhost:8080/v1"
        provider.enabledCapabilities = [.summarization]
        #expect(!ProviderConfigurationEligibility.canSelect(provider, for: .summarization, providers: []))
        provider.model = "local-model"
        #expect(ProviderConfigurationEligibility.canSelect(provider, for: .summarization, providers: []))
        provider.isEnabled = false
        #expect(!ProviderConfigurationEligibility.canSelect(provider, for: .summarization, providers: []))
    }
}
