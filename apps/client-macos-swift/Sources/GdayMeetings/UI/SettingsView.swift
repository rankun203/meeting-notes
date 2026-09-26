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
            DefaultsSettingsView()
                .tabItem { Label("Defaults", systemImage: "slider.horizontal.3") }
                .tag("defaults")
            DataPrivacyView()
                .tabItem { Label("Data Privacy", systemImage: "hand.raised") }
                .tag("privacy")
        }
        .formStyle(.grouped).padding(16).frame(width: 780, height: 650)
        .onAppear {
            // The Transcription and Summaries tabs became Defaults; a saved
            // selection of either would otherwise show no tab.
            if settingsTab == "transcription" || settingsTab == "summaries" { settingsTab = "defaults" }
        }
    }
}
