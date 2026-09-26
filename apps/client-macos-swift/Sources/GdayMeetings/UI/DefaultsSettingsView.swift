import SwiftUI

/// Settings → Defaults: one section per capability that chooses the provider
/// used for new work. Add a capability by adding one `CapabilityDefaultSection`
/// with its settings key path; stored keys stay in `AppSettings`.
struct DefaultsSettingsView: View {
    @EnvironmentObject private var store: MeetingStore

    private func setting<T>(_ path: WritableKeyPath<AppSettings, T>) -> Binding<T> {
        Binding(
            get: { store.settings[keyPath: path] },
            set: {
                store.settings[keyPath: path] = $0
                store.saveSettings()
            })
    }

    var body: some View {
        Form {
            CapabilityDefaultSection(
                capability: .transcription, selection: setting(\.transcriptionProviderID),
                caption: "Transcription sends recording audio to the selected provider.")
            CapabilityDefaultSection(
                capability: .summarization, selection: setting(\.summaryProviderID),
                caption: "Summaries and chat send the selected transcript and notes to this provider."
            ) {
                TextField("Summary Instructions", text: setting(\.summarizationPrompt), axis: .vertical)
                    .lineLimit(3...6)
            }
        }
    }
}

/// A provider picker for one capability, followed by capability-specific
/// settings and the caption that states what the provider receives.
struct CapabilityDefaultSection<Extra: View>: View {
    @EnvironmentObject private var store: MeetingStore
    @AppStorage("settingsTab") private var settingsTab = "recording"
    let capability: ProviderCapability
    @Binding var selection: UUID?
    let caption: String
    @ViewBuilder var extra: Extra

    init(
        capability: ProviderCapability, selection: Binding<UUID?>, caption: String,
        @ViewBuilder extra: () -> Extra = { EmptyView() }
    ) {
        self.capability = capability
        _selection = selection
        self.caption = caption
        self.extra = extra()
    }

    private var eligible: [ServiceProvider] {
        store.settings.serviceProviders.filter {
            ProviderConfigurationEligibility.canSelect($0, for: capability, providers: store.settings.serviceProviders)
        }
    }

    var body: some View {
        Section(capability.title) {
            let eligible = eligible
            // Keep the picker while a saved choice exists so it can be cleared,
            // even after its provider stops qualifying.
            if !eligible.isEmpty || selection != nil {
                Picker("Provider", selection: $selection) {
                    Text("None").tag(nil as UUID?)
                    ForEach(eligible) { provider in
                        Text(provider.name).tag(Optional(provider.id))
                    }
                    if let selected = selection, !eligible.contains(where: { $0.id == selected }) {
                        Text("Provider Unavailable").tag(Optional(selected))
                    }
                }
            }
            // HIG Writing: give an empty state a useful next action.
            if eligible.isEmpty {
                HStack {
                    Text("Add a provider and turn on \(capability.title) to choose it here.")
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Open Service Providers") { settingsTab = "providers" }
                }
            }
            extra
            Text(caption).font(.caption).foregroundStyle(.secondary)
        }
    }
}
