import SwiftUI

/// Language choices come from the selected transcription provider. Retaining the
/// stored value keeps offline recording independent of service availability.
struct MeetingLanguagePicker: View {
    @EnvironmentObject private var store: MeetingStore
    var title = "Language"
    @Binding var selection: String
    var providerID: UUID?
    var compact = false
    @ViewState private var showInformation = false

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
        HStack(spacing: 4) {
            if compact {
                languagePicker.labelsHidden().frame(width: 120)
            }
            else {
                languagePicker
            }
            Button {
                showInformation.toggle()
            } label: {
                Image(systemName: informationSymbol)
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Language Information")
            .help("About transcription language")
            .popover(isPresented: $showInformation) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Transcription Language").font(.headline)
                    Text("Choose the language spoken in this meeting.")
                    status
                }
                .font(.callout)
                .padding(16)
                .frame(width: 280, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            }
        }
        .task(id: store.languageIdentity(for: selectedProviderID)) {
            await store.loadProviderLanguages(providerID: selectedProviderID)
        }
    }

    private var informationSymbol: String {
        if case .failed = state { return "exclamationmark.circle" }
        return "info.circle"
    }

    private var languagePicker: some View {
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
    }

    @ViewBuilder private var status: some View {
        if selectedProviderID == nil {
            Text("Select a transcription provider in Settings to change the language.")
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
