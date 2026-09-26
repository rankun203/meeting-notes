import AVFoundation
import Foundation
import SwiftUI

/// Uses a temporary library, silent playback, and no Keychain access.
enum UIPreview {
    static let enabled =
        ProcessInfo.processInfo.arguments.contains("--ui-preview")
        || Bundle.main.object(forInfoDictionaryKey: "GdayUIPreview") as? Bool == true

    @MainActor static func makeStore() -> MeetingStore {
        guard enabled else { return MeetingStore() }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("Gday-UI-Preview-\(UUID())")
        let store = MeetingStore(dataDirectory: directory)
        do {
            for title in ["Synthetic single track", "Synthetic conversation"] {
                var meeting = Meeting(title: title)
                let folder = store.directory(for: meeting.id)
                try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                meeting.audioFiles = title.contains("single") ? ["microphone.wav"] : ["microphone.wav", "system.wav"]
                meeting.duration = 60
                for (index, name) in meeting.audioFiles.enumerated() {
                    try writeFixture(to: folder.appendingPathComponent(name), source: index)
                }
                try store.insertImportedMeeting(meeting)
            }
            _ = store.addPerson(name: "Preview Person")
            _ = store.addTag(name: "Preview")
            if let flag = ProcessInfo.processInfo.arguments.firstIndex(of: "--provider-test-env") {
                let arguments = ProcessInfo.processInfo.arguments
                guard arguments.indices.contains(flag + 1), !arguments[flag + 1].hasPrefix("--") else {
                    throw ServiceError("Add the configuration file path after --provider-test-env.")
                }
                let contents: String
                do { contents = try String(contentsOfFile: arguments[flag + 1], encoding: .utf8) }
                catch { throw ServiceError("Couldn’t read the provider test configuration file.") }
                let providers = try testProviders(configuration: contents)
                store.settings.serviceProviders = providers
                store.settings.transcriptionProviderID = providers.first { $0.kind == .runpod }?.id
            }
        }
        catch { store.errorMessage = "Could not prepare UI Preview: \(error.localizedDescription)" }
        return store
    }

    /// Parses only the explicitly supplied test file. Values are never logged or
    /// passed through a shell, and provider API keys stay in memory in Preview.
    static func testProviders(configuration: String) throws -> [ServiceProvider] {
        var values: [String: String] = [:]
        for line in configuration.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.hasPrefix("#"), let separator = trimmed.firstIndex(of: "=") else { continue }
            let key = String(trimmed[..<separator]).trimmingCharacters(in: .whitespaces)
            var value = String(trimmed[trimmed.index(after: separator)...]).trimmingCharacters(in: .whitespaces)
            if value.count >= 2, let first = value.first,
                first == "\"" || first == "'", value.last == first
            {
                value = String(value.dropFirst().dropLast())
            }
            values[key] = value
        }
        func required(_ key: String) throws -> String {
            guard let value = values[key], !value.isEmpty else {
                throw ServiceError("Add \(key) to the provider test configuration file.")
            }
            return value
        }
        var filedrop = ServiceProvider(kind: .filedrop)
        filedrop.endpoint = try required("FILE_DROP_URL")
        filedrop.apiKey = try required("FILE_DROP_API_KEY")
        filedrop.enabledCapabilities = [.fileTransfer]
        var runpod = ServiceProvider(kind: .runpod)
        runpod.endpoint = try required("RUNPOD_ENDPOINT_URL")
        runpod.apiKey = try required("RUNPOD_API_KEY")
        runpod.enabledCapabilities = [.transcription, .diarization]
        runpod.uploadProviderID = filedrop.id
        return [runpod, filedrop]
    }

    static func writeFixture(to url: URL, source: Int) throws {
        let rate = 8000.0
        let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1)!
        let file = try AVAudioFile(forWriting: url, settings: format.settings)
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 8000)!
        buffer.frameLength = 8000
        for second in 0..<60 {
            let samples = buffer.floatChannelData![0]
            for frame in 0..<8000 {
                let time = Double(second) + Double(frame) / rate
                let phase = (time + Double(source) * 4).truncatingRemainder(dividingBy: 12)
                let envelope = phase > 1 && phase < 7 ? pow(sin((phase - 1) / 6 * .pi), 2) : 0
                samples[frame] = Float(0.65 * envelope * sin(2 * .pi * 230 * time) * (0.65 + 0.35 * sin(time * 13)))
            }
            try file.write(from: buffer)
        }
    }
}

struct PreviewContainer<Content: View>: View {
    @ViewBuilder let content: () -> Content
    @ViewState private var appearance = 0
    private static func recordingLevel(at time: Double, offset: Double, reconnects: Bool = false)
        -> RecordingSourceLevel
    {
        let phase = (time + offset).truncatingRemainder(dividingBy: 7)
        let value = phase < 4 ? abs(sin(time * 5 + offset)) * 0.65 + 0.12 : 0
        // Simulate a 4-second device reconnect every 20 seconds to show that state.
        let reconnecting = reconnects && time.truncatingRemainder(dividingBy: 20) >= 16
        return RecordingSourceLevel(
            enabled: true, hasSamples: true, reconnecting: reconnecting, rmsDB: value * 60 - 60)
    }
    private static func recordingHistory(at time: Double) -> RecordingActivityHistory {
        var history = RecordingActivityHistory()
        let tick = floor(time * 10)
        for index in 0..<102 {
            let sampleTime = (tick - Double(101 - index)) / 10
            history.append(
                RecordingLevels(
                    microphone: recordingLevel(at: sampleTime, offset: 0),
                    system: recordingLevel(at: sampleTime, offset: 3, reconnects: true)), at: sampleTime)
        }
        return history
    }
    var body: some View {
        VStack(spacing: 0) {
            if UIPreview.enabled {
                HStack {
                    Label("UI Preview · Synthetic audio · Silent playback", systemImage: "eye")
                    Spacer()
                    Picker("Appearance", selection: $appearance) {
                        Text("System").tag(0)
                        Text("Light").tag(1)
                        Text("Dark").tag(2)
                    }.fixedSize()
                }.font(.caption).padding(8).background(.quaternary)
            }
            if UIPreview.enabled {
                DisclosureGroup("Recording visualization preview · synthetic levels") {
                    TimelineView(.periodic(from: .now, by: 0.1)) { _ in
                        let now = ProcessInfo.processInfo.systemUptime
                        let history = Self.recordingHistory(at: now)
                        let status = RecordingWorkspaceView.reconnectingStatus(
                            RecordingLevels(
                                microphone: Self.recordingLevel(at: now, offset: 0),
                                system: Self.recordingLevel(at: now, offset: 3, reconnects: true)))
                        VStack(alignment: .leading, spacing: 10) {
                            // Reserve the line so the simulated reconnect does not shift the meters.
                            Label(status ?? "Reconnecting system audio…", systemImage: "arrow.triangle.2.circlepath")
                                .font(.subheadline).foregroundStyle(.secondary).opacity(status == nil ? 0 : 1)
                                .accessibilityHidden(status == nil)
                            HStack(spacing: 26) {
                                RecordingSourceMeter(
                                    title: "Microphone", symbol: "mic.fill",
                                    source: Self.recordingLevel(
                                        at: now, offset: 0),
                                    saving: false, activity: history.bars(microphone: true),
                                    activityTime: history.bucketStart, tint: .accentColor)
                                RecordingSourceMeter(
                                    title: "System Audio", symbol: "speaker.wave.2.fill",
                                    source: Self.recordingLevel(
                                        at: now, offset: 3, reconnects: true),
                                    saving: false, activity: history.bars(microphone: false),
                                    activityTime: history.bucketStart, tint: .teal)
                            }
                        }.padding(18).frame(maxWidth: 500)
                    }
                }.padding(.horizontal, 12).padding(.vertical, 6)
            }
            content()
        }
        .onChange(of: appearance) { _, selection in
            guard UIPreview.enabled else { return }
            // Use one AppKit appearance source for native controls and SwiftUI.
            // Removing preferredColorScheme left stale dark foregrounds until
            // window activation; nil here restores live system inheritance.
            NSApp.appearance =
                selection == 1
                ? NSAppearance(named: .aqua)
                : selection == 2 ? NSAppearance(named: .darkAqua) : nil
        }
    }
}
