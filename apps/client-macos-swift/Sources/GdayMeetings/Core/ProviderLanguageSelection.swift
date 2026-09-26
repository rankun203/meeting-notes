import Foundation

extension MeetingStore {
    func languageIdentity(for providerID: UUID?) -> ProviderLanguageIdentity? {
        guard let provider = settings.serviceProviders.first(where: { $0.id == providerID }) else { return nil }
        return ProviderLanguageIdentity(provider: provider)
    }

    /// Reads cached state only. Pickers call this, so it must never send a request.
    func languageState(for providerID: UUID?) -> ProviderLanguageState {
        guard let identity = languageIdentity(for: providerID) else { return .idle }
        if let transient = providerLanguageStates[identity] { return transient }
        if let entry = cachedLanguages(identity) {
            return .loaded(entry.value, fetchedAt: entry.fetchedAt)
        }
        return .idle
    }

    private func cachedLanguages(_ identity: ProviderLanguageIdentity)
        -> ProviderMetadataCache<ProviderLanguageCatalog>.Entry?
    {
        providerLanguageCache.entry(providerID: identity.providerID, fingerprint: identity.fingerprint)
    }

    /// Loads the provider's current list. Call only from an explicit action, such as
    /// Load Languages: for RunPod this starts a worker job that can incur charges.
    func refreshProviderLanguages(providerID: UUID?) async {
        guard let provider = settings.serviceProviders.first(where: { $0.id == providerID }) else { return }
        _ = try? await resolveProviderLanguages(provider, refresh: true)
    }

    private func resolveProviderLanguages(_ provider: ServiceProvider, refresh: Bool) async throws
        -> ProviderLanguageCatalog
    {
        guard let identity = languageIdentity(for: provider.id),
            settings.serviceProviders.first(where: { $0.id == provider.id }) == provider
        else {
            throw ServiceError("The provider settings changed. Load its languages again.")
        }
        if !refresh, let entry = cachedLanguages(identity) {
            return entry.value
        }
        if let task = providerLanguageTasks[identity] {
            let catalog = try await task.value
            guard languageIdentity(for: provider.id) == identity else {
                throw ServiceError("The provider settings changed. Load its languages again.")
            }
            return catalog
        }
        providerLanguageStates = providerLanguageStates.filter { $0.key.providerID != provider.id }
        providerLanguageStates[identity] = .loading
        let loader = providerLanguageLoader
        let task = Task { try await loader(provider) }
        providerLanguageTasks[identity] = task
        defer { providerLanguageTasks.removeValue(forKey: identity) }
        do {
            let catalog = try await task.value
            guard languageIdentity(for: provider.id) == identity else {
                providerLanguageStates.removeValue(forKey: identity)
                throw ServiceError("The provider settings changed. Load its languages again.")
            }
            providerLanguageCache.store(
                .init(
                    providerID: identity.providerID, fingerprint: identity.fingerprint, value: catalog,
                    fetchedAt: Date()),
                keeping: Set(settings.serviceProviders.map(\.id)))
            providerLanguageStates.removeValue(forKey: identity)
            return catalog
        }
        catch {
            if languageIdentity(for: provider.id) == identity {
                providerLanguageStates[identity] = .failed(error.localizedDescription)
            }
            else {
                providerLanguageStates.removeValue(forKey: identity)
            }
            throw error
        }
    }

    /// Transcribe is an explicit action that already starts provider work. It uses the
    /// saved list and loads one only when none exists.
    func validateTranscriptionLanguage(_ language: String, for provider: ServiceProvider) async throws {
        guard TranscriptionLanguage.isExplicit(language) else {
            throw ServiceError("Choose a language for this meeting before transcribing.")
        }
        let catalog = try await resolveProviderLanguages(provider, refresh: false)
        guard catalog.languages.contains(where: { $0.code == language }) else {
            throw ServiceError(
                "\(provider.name) does not support this meeting's language. Choose a language listed by the provider, or load its languages again."
            )
        }
    }
}
