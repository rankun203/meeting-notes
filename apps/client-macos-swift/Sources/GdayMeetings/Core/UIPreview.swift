import AVFoundation
import Foundation
import SwiftUI

/// Explicit opt-in: never load the normal library, credentials, or remote services.
enum UIPreview {
    static let enabled = ProcessInfo.processInfo.arguments.contains("--ui-preview")
        || Bundle.main.object(forInfoDictionaryKey: "GdayUIPreview") as? Bool == true

    static func requireLiveServices() throws {
        if enabled { throw ServiceError("Online services are disabled in UI Preview.") }
    }

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
        } catch { store.errorMessage = "Could not prepare UI Preview: \(error.localizedDescription)" }
        return store
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
    @EnvironmentObject private var store: MeetingStore
    @ViewBuilder let content: () -> Content
    @ViewState private var appearance = 0
    var body: some View {
        VStack(spacing: 0) {
            if UIPreview.enabled {
                HStack {
                    Label("UI Preview · Synthetic audio · Silent playback", systemImage: "eye")
                    Spacer()
                    if let meeting = store.meetings.first(where: { $0.audioFiles.count == 2 }) {
                        ForEach(Array(store.audioURLs(for: meeting).enumerated()), id: \.offset) { index, url in
                            Label("Sample \(index + 1)", systemImage: "doc")
                                .padding(.horizontal, 6)
                                .frame(minWidth: 28, minHeight: 28)
                                .contentShape(Rectangle())
                                .onDrag { NSItemProvider(object: url as NSURL) }
                                .modifier(ActionHover())
                                .accessibilityElement(children: .combine)
                                .accessibilityHint("Drag into the meetings list to import, or into a meeting to add a track")
                                .help("Drag this synthetic audio file into the list or a meeting")
                        }
                    }
                    Picker("Appearance", selection: $appearance) {
                        Text("System").tag(0)
                        Text("Light").tag(1)
                        Text("Dark").tag(2)
                    }.fixedSize()
                }.font(.caption).padding(8).background(.quaternary)
            }
            content()
        }
        .onChange(of: appearance) { _, selection in
            guard UIPreview.enabled else { return }
            // Use one AppKit appearance source for native controls and SwiftUI.
            // Removing preferredColorScheme left stale dark foregrounds until
            // window activation; nil here restores live system inheritance.
            NSApp.appearance = selection == 1 ? NSAppearance(named: .aqua)
                : selection == 2 ? NSAppearance(named: .darkAqua) : nil
        }
    }
}
