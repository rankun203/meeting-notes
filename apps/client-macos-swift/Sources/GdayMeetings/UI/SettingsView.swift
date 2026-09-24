import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: MeetingStore
    @ObservedObject private var server = GdayServerService.shared
    @AppStorage("gdayServerURL") private var serverURL = ""
    @ViewState private var signingIn = false
    @ViewState private var error: String?
    private var audioSettingsLocked: Bool { store.recordingID != nil || store.isStartingRecording || store.isFinalizingRecording }

    private func setting<T>(_ path: WritableKeyPath<AppSettings, T>) -> Binding<T> {
        Binding(get: { store.settings[keyPath: path] }, set: { store.settings[keyPath: path] = $0; store.saveSettings() })
    }
    var body: some View {
        // HIG: a persistent tab selection groups settings by task in the standard
        // Settings scene; labeled native form controls support keyboard/VoiceOver.
        // https://developer.apple.com/design/human-interface-guidelines/settings
        TabView {
            Form {
                Section("Audio Sources") {
                    Toggle("Record microphone", isOn: setting(\.captureMicrophone)).disabled(audioSettingsLocked)
                    Toggle("Record system audio", isOn: setting(\.captureSystemAudio)).disabled(audioSettingsLocked)
                    // HIG Privacy: explain the requested resources in the context of their use.
                    // https://developer.apple.com/design/human-interface-guidelines/privacy
                    Text("When needed, macOS requests access to the audio sources you enable when recording starts. System Audio records other apps’ audio without sharing or recording your screen.").font(.caption).foregroundStyle(.secondary)
                }
                Section("Microphone Processing") {
                    // Apple voice processing provides noise suppression and gain control;
                    // its ducking can affect unrelated apps, so this is an explicit choice.
                    // https://developer.apple.com/videos/play/wwdc2023/10235/
                    Toggle("Microphone voice processing", isOn: setting(\.microphoneVoiceProcessing))
                        .disabled(!store.settings.captureMicrophone || audioSettingsLocked)
                    Text("Apple noise suppression and gain control. May reduce other apps’ volume; echo removal depends on the audio route. Headphones give the most reliable separation.").font(.caption).foregroundStyle(.secondary)
                    if audioSettingsLocked { Text("Audio source and processing changes are available after recording stops.").font(.caption).foregroundStyle(.secondary) }
                }
                Section("Recording Format") {
                    Picker("Save audio as", selection: setting(\.recordingFormat)) {
                        Text("Opus (Recommended)").tag(RecordingFormat.opus)
                        Text("M4A (AAC)").tag(RecordingFormat.m4a)
                        Text("WAV").tag(RecordingFormat.wav)
                    }.disabled(audioSettingsLocked)
                    Text("Audio is captured as temporary uncompressed PCM, then saved in this format after recording stops. If conversion fails, the original PCM recording is kept.").font(.caption).foregroundStyle(.secondary)
                }
                Section("After Recording") { Toggle("Automatically transcribe recordings", isOn: setting(\.autoTranscribe)) }
            }.tabItem { Label("Recording", systemImage: "mic") }
            Form {
                Section("Gday Server") {
                    TextField("Server URL", text: $serverURL).textContentType(.URL)
                    if server.connected {
                        LabeledContent("Signed in", value: server.email ?? "Connected")
                        Button("Sign Out") { Task { do { try await server.signOut() } catch { self.error = error.localizedDescription } } }
                    } else {
                        Button(signingIn ? "Signing In…" : "Sign In with Browser") {
                            signingIn = true
                            Task { defer { signingIn = false }; do { try await server.signIn(origin: serverURL) } catch { self.error = error.localizedDescription } }
                        }.disabled(signingIn || serverURL.isEmpty)
                    }
                    Text(server.connected ? "Transcription uses your signed-in Gday server. Sign out to use the compatible service below." : "Connect to your Gday server to transcribe audio using your account.").font(.caption).foregroundStyle(.secondary)
                }
                Section("Compatible Transcription Service") {
                    TextField("API Base URL", text: setting(\.transcriptionBaseURL))
                    TextField("Model", text: setting(\.transcriptionModel))
                    SecureField("API Key", text: setting(\.transcriptionAPIKey))
                }
            }.tabItem { Label("Transcription", systemImage: "text.bubble") }
            Form {
                Section("Language Model") {
                    TextField("API Base URL", text: setting(\.llmBaseURL))
                    TextField("Model", text: setting(\.llmModel))
                    SecureField("API Key", text: setting(\.llmAPIKey))
                    TextField("Summary Instructions", text: setting(\.summarizationPrompt), axis: .vertical).lineLimit(3...6)
                    Text("Summaries and chat send the selected meeting’s transcript and notes to this OpenAI-compatible service. You can use a local service.").font(.caption).foregroundStyle(.secondary)
                }
            }.tabItem { Label("Intelligence", systemImage: "sparkles") }
        }
        .formStyle(.grouped).padding(16).frame(width: 590, height: 600)
        .alert("Connection Failed", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) { Button("OK") { error = nil } } message: { Text(error ?? "") }
    }
}
