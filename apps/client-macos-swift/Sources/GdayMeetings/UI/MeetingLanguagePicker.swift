import SwiftUI

/// Language choices come from the selected transcription provider. Retaining the
/// stored value keeps offline recording independent of service availability.
struct MeetingLanguagePicker: View {
    @EnvironmentObject private var store: MeetingStore
    var title = "Language"
    @Binding var selection: String
    var providerID: UUID?

    private var selectedProviderID: UUID? { providerID ?? store.settings.transcriptionProviderID }
    private var state: ProviderLanguageState { store.languageState(for: selectedProviderID) }
    private var languages: [ProviderLanguage] {
        if case .loaded(let catalog) = state { return catalog.languages }
        return []
    }
    private var selectedName: String {
        guard TranscriptionLanguage.isExplicit(selection) else { return "Choose a Language" }
        return languages.first { $0.code == selection }?.name
            ?? Locale.current.localizedString(forIdentifier: selection) ?? selection
    }
    private var unsupported: Bool {
        guard case .loaded = state, TranscriptionLanguage.isExplicit(selection) else { return false }
        return !languages.contains { $0.code == selection }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Picker(title, selection: $selection) {
                ForEach(languages) { language in
                    Text(language.name).tag(language.code)
                }
                if !languages.contains(where: { $0.code == selection }) {
                    Text(unsupported ? "\(selectedName) (Unsupported)" : selectedName)
                        .tag(selection)
                        .disabled(true)
                }
            }
            .pickerStyle(.menu)
            .disabled(languages.isEmpty)
            status
        }
        .task(id: store.languageIdentity(for: selectedProviderID)) {
            await store.loadProviderLanguages(providerID: selectedProviderID)
        }
    }

    @ViewBuilder private var status: some View {
        if selectedProviderID == nil {
            Text("Language selection is available when a transcription provider is selected.")
                .font(.caption).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        else {
            switch state {
            case .idle, .loading:
                Text("Loading Languages…").font(.caption).foregroundStyle(.secondary)
            case .failed(let message):
                HStack(spacing: 6) {
                    Label("Languages Unavailable", systemImage: "exclamationmark.circle")
                        .foregroundStyle(.secondary).help(message)
                        .accessibilityHint(message)
                    Button("Retry") {
                        Task {
                            await store.loadProviderLanguages(providerID: selectedProviderID, force: true)
                        }
                    }
                    .buttonStyle(.link)
                    .accessibilityLabel("Retry Loading Languages")
                }.font(.caption)
            case .loaded:
                EmptyView()
            }
        }
    }
}
