import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: MeetingStore
    @AppStorage("settingsTab") private var settingsTab = "recording"
    private var audioSettingsLocked: Bool {
        store.recordingID != nil || store.isStartingRecording || store.isFinalizingRecording
    }

    private func setting<T>(_ path: WritableKeyPath<AppSettings, T>) -> Binding<T> {
        Binding(
            get: { store.settings[keyPath: path] },
            set: {
                store.settings[keyPath: path] = $0
                store.saveSettings()
            })
    }
    var body: some View {
        // HIG: a persistent tab selection groups settings by task in the standard
        // Settings scene; labeled native form controls support keyboard/VoiceOver.
        // https://developer.apple.com/design/human-interface-guidelines/settings
        TabView(selection: $settingsTab) {
            Form {
                Section {
                    Text(
                        "Choose the default audio sources and format for new recordings. You can change them in New Recording."
                    )
                    .foregroundStyle(.secondary)
                }
                Section("Audio Sources") {
                    Toggle("Microphone", isOn: setting(\.captureMicrophone)).disabled(audioSettingsLocked)
                    Toggle("System Audio", isOn: setting(\.captureSystemAudio)).disabled(audioSettingsLocked)
                    // HIG Privacy: explain the requested resources in the context of their use.
                    // https://developer.apple.com/design/human-interface-guidelines/privacy
                    Text(
                        "Microphone and system audio each require your permission. macOS asks for access the first time you record each source."
                    ).font(.caption).foregroundStyle(.secondary)
                }
                Section("Voice Processing") {
                    Toggle("Turn On Voice Processing Automatically", isOn: setting(\.automaticVoiceProcessing))
                        .disabled(audioSettingsLocked)
                    Text(
                        "Turns on when audio plays through speakers or the microphone picks up system audio. Reduces echo and background noise in the microphone track, and may lower other apps’ volume."
                    ).font(.caption).foregroundStyle(.secondary)
                }
                Section("Audio Format") {
                    Picker("Audio Format", selection: setting(\.recordingFormat)) {
                        Text("Opus (Recommended)").tag(RecordingFormat.opus)
                        Text("M4A (AAC)").tag(RecordingFormat.m4a)
                        Text("WAV").tag(RecordingFormat.wav)
                    }.disabled(audioSettingsLocked)
                    Text(
                        "Recordings are saved in this format when you stop. If conversion fails, the original audio is kept."
                    ).font(.caption).foregroundStyle(.secondary)
                }
                Section("Transcription") {
                    MeetingLanguagePicker(title: "Default Language", selection: setting(\.defaultLanguage))
                    Toggle("Automatically Transcribe Recordings", isOn: setting(\.autoTranscribe))
                }
            }.tabItem { Label("Recording", systemImage: "mic") }.tag("recording")
            ServiceProvidersView()
                .tabItem { Label("Service Providers", systemImage: "server.rack") }
                .tag("providers")
            Form {
                Section("Transcription") {
                    providerPicker(
                        "Provider", capability: .transcription, selection: setting(\.transcriptionProviderID))
                    Text("Transcription sends recording audio to the selected provider.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }.tabItem { Label("Transcription", systemImage: "text.bubble") }.tag("transcription")
            Form {
                Section("Summaries") {
                    providerPicker("Provider", capability: .summarization, selection: setting(\.summaryProviderID))
                    TextField("Summary Instructions", text: setting(\.summarizationPrompt), axis: .vertical)
                        .lineLimit(3...6)
                    Text("Summaries and chat send the selected transcript and notes to this provider.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }.tabItem { Label("Summaries", systemImage: "sparkles") }.tag("summaries")
        }
        .formStyle(.grouped).padding(16).frame(width: 780, height: 650)
    }

    private func providerPicker(
        _ title: String, capability: ProviderCapability, selection: Binding<UUID?>
    ) -> some View {
        Picker(title, selection: selection) {
            Text("None").tag(nil as UUID?)
            ForEach(
                store.settings.serviceProviders.filter {
                    availableForDefault($0, capability: capability)
                }
            ) { provider in
                Text(provider.name).tag(Optional(provider.id))
            }
            if let selected = selection.wrappedValue,
                !store.settings.serviceProviders.contains(where: {
                    $0.id == selected && availableForDefault($0, capability: capability)
                })
            {
                Text("Provider Unavailable").tag(Optional(selected))
            }
        }
    }

    private func availableForDefault(_ provider: ServiceProvider, capability: ProviderCapability) -> Bool {
        ProviderConfigurationEligibility.canSelect(
            provider, for: capability, providers: store.settings.serviceProviders)
    }
}
