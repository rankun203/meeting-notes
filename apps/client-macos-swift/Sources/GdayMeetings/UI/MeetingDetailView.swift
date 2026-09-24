import SwiftUI
import AVKit

struct MeetingDetailView: View {
    @EnvironmentObject private var store: MeetingStore
    @ObservedObject private var server = GdayServerService.shared
    let meetingID: UUID
    @ViewState private var tab = 0
    @ViewState private var chatDraft = ""
    @ViewState private var todoDraft = ""
    @ViewState private var player: AVPlayer?
    @ViewState private var selectedTrack = -1
    @ViewState private var playbackGeneration = UUID()
    @ViewState private var loadingPlayback = false
    @ViewState private var speakerFrom = ""
    @ViewState private var speakerTo = ""

    private var meeting: Meeting? { store.meetings.first { $0.id == meetingID } }
    private func change(_ edit: (inout Meeting) -> Void) {
        guard var value = meeting else { return }; edit(&value); store.updateMeeting(value)
    }
    private func text(_ path: WritableKeyPath<Meeting, String>) -> Binding<String> {
        Binding(get: { meeting?[keyPath: path] ?? "" }, set: { value in change { $0[keyPath: path] = value } })
    }
    var body: some View {
        if let meeting {
            VStack(alignment: .leading, spacing: 14) {
                TextField("Meeting title", text: text(\.title)).font(.title).textFieldStyle(.plain).accessibilityLabel("Meeting title")
                HStack {
                    Text(meeting.createdAt, format: .dateTime).foregroundStyle(.secondary)
                    if store.recordingID == meetingID {
                        TimelineView(.periodic(from: .now, by: 1)) { timeline in
                            let elapsed = store.recordingStartedAt.map { timeline.date.timeIntervalSince($0) } ?? 0
                            Label(formatTime(elapsed), systemImage: "record.circle.fill").foregroundStyle(.red).monospacedDigit().accessibilityLabel("Recording duration \(formatTime(elapsed))")
                        }
                    } else if meeting.duration > 0 {
                        Text(formatTime(meeting.duration)).monospacedDigit().foregroundStyle(.secondary).accessibilityLabel("Duration \(formatTime(meeting.duration))")
                    }
                    Spacer()
                    Menu("People") {
                        ForEach(store.people) { person in
                            Toggle(person.name, isOn: Binding(get: { self.meeting?.personIDs.contains(person.id) ?? false }, set: { selected in change { if selected { $0.personIDs.append(person.id) } else { $0.personIDs.removeAll { $0 == person.id } } } }))
                        }
                        if store.people.isEmpty { Text("Add people in the sidebar") }
                    }
                    Menu("Tags") {
                        ForEach(store.tags) { tag in
                            Toggle(tag.name, isOn: Binding(get: { self.meeting?.tagIDs.contains(tag.id) ?? false }, set: { selected in change { if selected { $0.tagIDs.append(tag.id) } else { $0.tagIDs.removeAll { $0 == tag.id } } } }))
                        }
                        if store.tags.isEmpty { Text("Add tags in the sidebar") }
                    }
                }.font(.callout)
                if !meeting.personIDs.isEmpty || !meeting.tagIDs.isEmpty {
                    Text(associationSummary(meeting)).font(.caption).foregroundStyle(.secondary)
                }
                if !meeting.audioFiles.isEmpty && store.recordingID != meetingID {
                    HStack {
                        if meeting.audioFiles.count > 1 {
                            Picker("Audio track", selection: $selectedTrack) {
                                Text("All Tracks").tag(-1)
                                ForEach(Array(meeting.audioFiles.enumerated()), id: \.offset) { index, file in Text(file.hasPrefix("microphone") ? "Microphone" : file.hasPrefix("system") ? "System Audio" : file).tag(index) }
                            }.frame(maxWidth: 260)
                        }
                        if let player { AudioPlaybackView(player: player).frame(height: 44) }
                        else if loadingPlayback { ProgressView("Loading audio…").controlSize(.small) }
                    }
                }
                Picker("Meeting content", selection: $tab) {
                    Text("Transcript").tag(0); Text("Notes").tag(1); Text("Summary").tag(2); Text("To-Dos").tag(3); Text("Chat").tag(4)
                }.pickerStyle(.segmented)
                switch tab {
                case 0: transcript(meeting)
                case 1: editor("Notes", binding: text(\.notes))
                case 2:
                    VStack(alignment: .leading) {
                        Button("Generate Summary", systemImage: "sparkles") { Task { await store.summarize(id: meetingID) } }.disabled(store.isBusy || (meeting.transcript.isEmpty && meeting.notes.isEmpty))
                        editor("Summary", binding: text(\.summary))
                    }
                case 3: todos(meeting)
                default: chat(meeting)
                }
            }.padding(20)
            .navigationTitle(meeting.title)
            .toolbar {
                Button(meeting.serverTranscription == nil ? "Transcribe" : "Resume Transcription", systemImage: "text.bubble") { Task { await store.transcribe(id: meetingID) } }.disabled(store.isBusy || meeting.audioFiles.isEmpty || store.recordingID == meetingID)
                Menu {
                    Button("Export Meeting Text…") { MeetingPanels.export(meeting, store: store) }
                    Button("Archive to Server", systemImage: "icloud.and.arrow.up") { Task { await store.archiveToServer(id: meetingID) } }.disabled(!server.connected || store.isBusy || store.recordingID == meetingID)
                } label: { Label("Export and Archive", systemImage: "square.and.arrow.up") }.help("Export meeting text or archive audio and meeting data to the server")
            }
            .task(id: PlaybackSelection(meetingID: meetingID, audioFiles: meeting.audioFiles, track: selectedTrack, recording: store.recordingID == meetingID)) { await loadPlayer() }
            .onDisappear { playbackGeneration = UUID(); player?.pause(); player = nil }
        }
    }
    private func associationSummary(_ meeting: Meeting) -> String {
        let names: [String] = store.people.filter { meeting.personIDs.contains($0.id) }.map(\.name)
        let tags: [String] = store.tags.filter { meeting.tagIDs.contains($0.id) }.map { "#" + $0.name }
        return (names + tags).joined(separator: " · ")
    }
    @MainActor
    private func loadPlayer() async {
        let generation = UUID()
        playbackGeneration = generation
        let previousTime = player?.currentTime() ?? .zero
        player?.pause(); player = nil
        loadingPlayback = false
        guard let meeting, store.recordingID != meetingID else { return }
        let files = store.audioURLs(for: meeting)
        guard !files.isEmpty else { return }
        loadingPlayback = true
        defer { if playbackGeneration == generation { loadingPlayback = false } }
        do {
            let item: AVPlayerItem
            if selectedTrack >= 0 || files.count == 1 {
                let index = min(max(0, selectedTrack), files.count - 1)
                item = AVPlayerItem(url: files[index])
            } else {
                // Simultaneous capture tracks share a zero origin. Native AVFoundation
                // composition mixes them so listening includes every participant.
                let composition = AVMutableComposition()
                for file in files {
                    try Task.checkCancellation()
                    let asset = AVURLAsset(url: file)
                    let tracks = try await asset.loadTracks(withMediaType: .audio)
                    let duration = try await asset.load(.duration)
                    guard duration.isNumeric, CMTimeCompare(duration, .zero) > 0 else { continue }
                    for source in tracks {
                        guard let destination = composition.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) else {
                            throw MeetingError.message("Unable to prepare audio playback.")
                        }
                        try destination.insertTimeRange(CMTimeRange(start: .zero, duration: duration), of: source, at: .zero)
                    }
                }
                guard !composition.tracks.isEmpty else { throw MeetingError.message("The recording has no playable audio tracks.") }
                item = AVPlayerItem(asset: composition)
            }
            try Task.checkCancellation()
            guard playbackGeneration == generation, store.recordingID != meetingID else { return }
            let newPlayer = AVPlayer(playerItem: item)
            if previousTime.isNumeric { await newPlayer.seek(to: previousTime) }
            try Task.checkCancellation()
            guard playbackGeneration == generation, store.recordingID != meetingID else { return }
            player = newPlayer
        } catch is CancellationError {
            // Switching tracks or meetings cancels preparation without an alert.
        } catch {
            guard !Task.isCancelled, playbackGeneration == generation else { return }
            store.errorMessage = "Unable to load meeting audio: " + error.localizedDescription
        }
    }
    private func editor(_ label: String, binding: Binding<String>) -> some View {
        // HIG accessibility: standard editable text, semantic fonts and system colors
        // respect contrast and assistive technologies without custom event handling.
        // https://developer.apple.com/design/human-interface-guidelines/accessibility
        TextEditor(text: binding).font(.body).accessibilityLabel(label)
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(.separator))
    }
    private func transcript(_ meeting: Meeting) -> some View {
        VStack(alignment: .leading) {
            if !meeting.transcript.isEmpty {
                HStack {
                    Picker("Speaker", selection: $speakerFrom) {
                        Text("Choose speaker").tag("")
                        ForEach(Array(Set(meeting.transcript.map(\.speaker))).sorted(), id: \.self) { Text($0).tag($0) }
                    }
                    TextField("Rename speaker", text: $speakerTo)
                    Button("Apply") {
                        change { meeting in
                            for index in meeting.transcript.indices where meeting.transcript[index].speaker == speakerFrom { meeting.transcript[index].speaker = speakerTo }
                        }; speakerFrom = speakerTo; speakerTo = ""
                    }.disabled(speakerFrom.isEmpty || speakerTo.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(meeting.transcript) { segment in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Button(formatTime(segment.start)) { player?.seek(to: CMTime(seconds: segment.start, preferredTimescale: 600)); player?.play() }.buttonStyle(.link).help("Play from this point").disabled(player == nil || store.recordingID == meetingID)
                                TextField("Speaker", text: Binding(get: { self.meeting?.transcript.first(where: { $0.id == segment.id })?.speaker ?? "" }, set: { value in change { meeting in if let index = meeting.transcript.firstIndex(where: { $0.id == segment.id }) { meeting.transcript[index].speaker = value } } })).font(.headline).textFieldStyle(.plain)
                            }
                            TextField("Transcript", text: Binding(get: { self.meeting?.transcript.first(where: { $0.id == segment.id })?.text ?? "" }, set: { value in change { meeting in if let index = meeting.transcript.firstIndex(where: { $0.id == segment.id }) { meeting.transcript[index].text = value } } }), axis: .vertical).textFieldStyle(.plain)
                            Divider()
                        }
                    }
                }.padding(4)
            }.overlay { if meeting.transcript.isEmpty { ContentUnavailableView("No Transcript", systemImage: "text.bubble", description: Text("Record or import audio, then choose Transcribe. Configure your transcription service in Settings.")) } }
        }
    }
    private func todos(_ meeting: Meeting) -> some View {
        VStack {
            HStack {
                TextField("New to-do", text: $todoDraft).onSubmit(addTodo)
                Button("Add", action: addTodo).disabled(todoDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            List {
                ForEach(meeting.todos) { todo in
                    HStack {
                        Toggle(isOn: Binding(get: { self.meeting?.todos.first(where: { $0.id == todo.id })?.isCompleted ?? false }, set: { value in change { meeting in if let index = meeting.todos.firstIndex(where: { $0.id == todo.id }) { meeting.todos[index].isCompleted = value } } })) { Text("Completed").hidden() }.labelsHidden().accessibilityLabel("Mark \(todo.title) complete")
                        TextField("To-do", text: Binding(get: { self.meeting?.todos.first(where: { $0.id == todo.id })?.title ?? "" }, set: { value in change { meeting in if let index = meeting.todos.firstIndex(where: { $0.id == todo.id }) { meeting.todos[index].title = value } } })).strikethrough(todo.isCompleted)
                        Spacer()
                        Button("Delete To-Do", systemImage: "trash", role: .destructive) { change { $0.todos.removeAll { $0.id == todo.id } } }.labelStyle(.iconOnly).buttonStyle(.borderless)
                    }
                }
            }
        }
    }
    private func addTodo() {
        let title = todoDraft.trimmingCharacters(in: .whitespacesAndNewlines); guard !title.isEmpty else { return }
        change { $0.todos.append(MeetingTodo(title: title)) }; todoDraft = ""
    }
    private func chat(_ meeting: Meeting) -> some View {
        VStack {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        if meeting.chat.isEmpty { Text("Ask questions about this meeting. Your transcript and notes provide context.").foregroundStyle(.secondary) }
                        ForEach(meeting.chat) { message in
                            VStack(alignment: .leading, spacing: 5) {
                                Text(message.role == "user" ? "You" : "Gday").font(.headline)
                                Text(message.content).textSelection(.enabled)
                            }.frame(maxWidth: .infinity, alignment: .leading).id(message.id)
                        }
                    }.padding(6)
                }.onChange(of: meeting.chat.count) { _, _ in if let id = meeting.chat.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
            }
            HStack(alignment: .bottom) {
                TextField("Ask about this meeting", text: $chatDraft, axis: .vertical).lineLimit(1...5).onSubmit(sendChat)
                Button("Send", systemImage: "arrow.up", action: sendChat).disabled(store.isBusy || chatDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }
    private func sendChat() {
        let text = chatDraft.trimmingCharacters(in: .whitespacesAndNewlines); guard !text.isEmpty else { return }
        chatDraft = ""; Task { await store.sendChat(id: meetingID, message: text) }
    }
}

private struct PlaybackSelection: Hashable { let meetingID: UUID; let audioFiles: [String]; let track: Int; let recording: Bool }

private func formatTime(_ seconds: Double) -> String { let value = seconds.isFinite ? max(0, Int(min(seconds, Double(Int.max / 2)))) : 0; return String(format: "%d:%02d", value / 60, value % 60) }

// AVKit's native controls provide standard playback shortcuts and accessibility.
struct AudioPlaybackView: NSViewRepresentable {
    let player: AVPlayer
    func makeNSView(context: Context) -> AVPlayerView { let view = AVPlayerView(); view.controlsStyle = .inline; view.player = player; return view }
    func updateNSView(_ view: AVPlayerView, context: Context) { view.player = player }
}
