import Foundation

extension MeetingStore {
    func languageIdentity(for providerID: UUID?) -> ProviderLanguageIdentity? {
        guard let provider = settings.serviceProviders.first(where: { $0.id == providerID }) else { return nil }
        let account =
            provider.kind == .gdayWebsite
            ? (GdayServerService.shared.email ?? "") + (GdayServerService.shared.origin ?? "") : ""
        return ProviderLanguageIdentity(provider: provider, account: account)
    }

    func languageState(for providerID: UUID?) -> ProviderLanguageState {
        guard let identity = languageIdentity(for: providerID) else { return .idle }
        return providerLanguageStates[identity] ?? .idle
    }

    func loadProviderLanguages(providerID: UUID?, force: Bool = false) async {
        guard let provider = settings.serviceProviders.first(where: { $0.id == providerID }) else { return }
        _ = try? await resolveProviderLanguages(provider, force: force)
    }

    private func resolveProviderLanguages(_ provider: ServiceProvider, force: Bool = false) async throws
        -> ProviderLanguageCatalog
    {
        guard let identity = languageIdentity(for: provider.id),
            settings.serviceProviders.first(where: { $0.id == provider.id }) == provider
        else {
            throw ServiceError("The provider settings changed. Load its languages again.")
        }
        if !force, case .loaded(let catalog) = providerLanguageStates[identity],
            let fetched = providerLanguageFetchedAt[identity], Date().timeIntervalSince(fetched) < 300
        {
            return catalog
        }
        if let task = providerLanguageTasks[identity] {
            let catalog = try await task.value
            guard languageIdentity(for: provider.id) == identity,
                settings.serviceProviders.first(where: { $0.id == provider.id }) == provider
            else {
                throw ServiceError("The provider settings changed. Load its languages again.")
            }
            return catalog
        }
        providerLanguageStates = providerLanguageStates.filter {
            $0.key.providerID != provider.id || $0.key == identity
        }
        providerLanguageStates[identity] = .loading
        let loader = providerLanguageLoader
        let task = Task { try await loader(provider) }
        providerLanguageTasks[identity] = task
        defer { providerLanguageTasks.removeValue(forKey: identity) }
        do {
            let catalog = try await task.value
            guard languageIdentity(for: provider.id) == identity else {
                throw ServiceError("The provider settings changed. Load its languages again.")
            }
            providerLanguageStates[identity] = .loaded(catalog)
            providerLanguageFetchedAt[identity] = Date()
            return catalog
        }
        catch {
            if languageIdentity(for: provider.id) == identity {
                providerLanguageStates[identity] = .failed(error.localizedDescription)
            }
            throw error
        }
    }

    func validateTranscriptionLanguage(_ language: String, for provider: ServiceProvider) async throws {
        guard TranscriptionLanguage.isExplicit(language) else {
            throw ServiceError("Choose a language for this meeting before transcribing.")
        }
        let catalog = try await resolveProviderLanguages(provider)
        guard catalog.languages.contains(where: { $0.code == language }) else {
            throw ServiceError(
                "\(provider.name) does not support this meeting's language. Choose a language listed by the provider.")
        }
    }
}
