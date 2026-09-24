import SwiftUI

struct MeetingDetailView: View {
    @EnvironmentObject private var store: MeetingStore
    @EnvironmentObject private var playback: MeetingPlayback
    @ObservedObject private var server = GdayServerService.shared
    let meetingID: UUID
    @ViewState private var tab = 0
    @ViewState private var chatDraft = ""
    @ViewState private var todoDraft = ""
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
            VStack(alignment: .leading, spacing: 22) {
                meetingHeader(meeting)
                if store.recordingID == meetingID {
                    RecordingWorkspaceView(meetingID: meetingID)
                } else if !meeting.audioFiles.isEmpty {
                    recordingOverview(meeting)
                }
                // HIG: standard segmented navigation maintains a predictable content
                // hierarchy and keyboard accessibility without custom hit targets.
                // https://developer.apple.com/design/human-interface-guidelines/segmented-controls
                Picker("Meeting content", selection: $tab) {
                    Text("Transcript").tag(0); Text("Notes").tag(1); Text("Summary").tag(2); Text("To-Dos").tag(3); Text("Chat").tag(4)
                }.pickerStyle(.segmented).labelsHidden().accessibilityLabel("Meeting content")
                meetingContent(meeting)
            }
            .padding(24)
            .modifier(AudioFileDrop(meetingID: meetingID))
            .navigationTitle(meeting.title)
            .toolbar {
                Menu {
                    Button(meeting.serverTranscription == nil ? "Transcribe Recording" : "Resume Transcription", systemImage: "text.bubble") { Task { await store.transcribe(id: meetingID) } }
                        .disabled(store.isBusy || meeting.audioFiles.isEmpty || store.recordingID == meetingID)
                    Divider()
                    Button("Export Meeting Text…", systemImage: "square.and.arrow.up") { MeetingPanels.export(meeting, store: store) }
                    Button("Archive to Server", systemImage: "icloud.and.arrow.up") { Task { await store.archiveToServer(id: meetingID) } }
                        .disabled(!server.connected || store.isBusy || store.recordingID == meetingID)
                } label: { Label("Meeting Actions", systemImage: "ellipsis.circle") }
                .help("Transcribe, export, or archive this meeting")
            }
            // Reading or editing a meeting never changes the app-owned playback.
            .onAppear { if store.recordingID == meetingID { tab = 1 } }
            .onChange(of: store.recordingID) { _, id in if id == meetingID { tab = 1 } }
        }
    }

    private func meetingHeader(_ meeting: Meeting) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            TextField("Meeting title", text: text(\.title))
                .font(.largeTitle.weight(.semibold)).textFieldStyle(.plain)
                .accessibilityLabel("Meeting title")
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) {
                    meetingDate(meeting).fixedSize()
                    Spacer(minLength: 8)
                    associationsMenu
                }
                VStack(alignment: .leading, spacing: 8) {
                    meetingDate(meeting)
                    associationsMenu
                }
            }.font(.callout).foregroundStyle(.secondary)
            if !meeting.personIDs.isEmpty || !meeting.tagIDs.isEmpty {
                Text(associationSummary(meeting)).font(.callout).foregroundStyle(.secondary)
            }
        }
    }

    private func meetingDate(_ meeting: Meeting) -> some View {
        HStack(spacing: 10) {
            Text(meeting.createdAt, format: .dateTime.month(.abbreviated).day().year().hour().minute())
            if meeting.duration > 0 && store.recordingID != meetingID {
                Text(formatTime(meeting.duration)).monospacedDigit().accessibilityLabel("Duration \(formatTime(meeting.duration))")
            }
        }
    }

    private var associationsMenu: some View {
        Menu {
            Section("People") {
                ForEach(store.people) { person in
                    Toggle(person.name, isOn: Binding(get: { self.meeting?.personIDs.contains(person.id) ?? false }, set: { selected in change { if selected { $0.personIDs.append(person.id) } else { $0.personIDs.removeAll { $0 == person.id } } } }))
                }
                if store.people.isEmpty { Text("Add people in the sidebar") }
            }
            Section("Tags") {
                ForEach(store.tags) { tag in
                    Toggle(tag.name, isOn: Binding(get: { self.meeting?.tagIDs.contains(tag.id) ?? false }, set: { selected in change { if selected { $0.tagIDs.append(tag.id) } else { $0.tagIDs.removeAll { $0 == tag.id } } } }))
                }
                if store.tags.isEmpty { Text("Add tags in the sidebar") }
            }
        } label: { Label("People & Tags", systemImage: "person.2") }
        .menuStyle(.borderlessButton).fixedSize()
    }


    private func recordingOverview(_ meeting: Meeting) -> some View {
        // HIG Playing Audio: start playback only after an intentional action.
        // Library browsing does not replace or pause the current recording.
        // https://developer.apple.com/design/human-interface-guidelines/playing-audio
        HStack(spacing: 14) {
            Image(systemName: "waveform")
                .font(.title2).foregroundStyle(.tint)
                .frame(width: 52, height: 52)
                .background(Color.accentColor.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text("Meeting Recording").font(.headline)
                Text(audioSourceSummary(meeting)).font(.callout).foregroundStyle(.secondary)
            }
            Spacer(minLength: 12)
            Button {
                if playback.meetingID == meetingID { playback.togglePlayPause() }
                else { playback.play(meeting: meeting, files: store.audioURLs(for: meeting)) }
            } label: {
                Label(playbackActionTitle, systemImage: playback.meetingID == meetingID && playback.isPlaying ? "pause.fill" : "play.fill")
            }
            .buttonStyle(.borderedProminent)
            .disabled(playback.isPlaybackBlocked || (playback.meetingID == meetingID && playback.isLoading) || store.audioURLs(for: meeting).isEmpty)
            .help(playback.isPlaybackBlocked ? "Playback is unavailable while recording" : "Listen to this meeting")
        }
        .padding(16)
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 14))
    }

    private var playbackActionTitle: String {
        guard playback.meetingID == meetingID else { return "Play" }
        if playback.isLoading { return "Loading…" }
        if playback.isPlaying { return "Pause" }
        return "Play"
    }

    private func audioSourceSummary(_ meeting: Meeting) -> String {
        let names = meeting.audioFiles.map { file in
            let name = URL(fileURLWithPath: file).deletingPathExtension().lastPathComponent
            return name == "microphone" ? "Microphone" : name == "system" ? "System Audio" : name
        }
        return names.joined(separator: " · ")
    }

    @ViewBuilder
    private func meetingContent(_ meeting: Meeting) -> some View {
        switch tab {
        case 0: transcript(meeting)
        case 1:
            VStack(alignment: .leading, spacing: 10) {
                if store.recordingID == meetingID {
                    HStack {
                        Text("Meeting Notes").font(.headline)
                        Spacer()
                        Text("Saved as you type").font(.caption).foregroundStyle(.secondary)
                    }
                }
                editor("Notes", binding: text(\.notes))
            }
        case 2:
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Summary").font(.headline)
                    Spacer()
                    Button(meeting.summary.isEmpty ? "Generate Summary" : "Regenerate Summary", systemImage: "sparkles") { Task { await store.summarize(id: meetingID) } }
                        .disabled(store.isBusy || (meeting.transcript.isEmpty && meeting.notes.isEmpty))
                }
                editor("Summary", binding: text(\.summary))
            }
        case 3: todos(meeting)
        default: chat(meeting)
        }
    }

    private func associationSummary(_ meeting: Meeting) -> String {
        let names: [String] = store.people.filter { meeting.personIDs.contains($0.id) }.map(\.name)
        let tags: [String] = store.tags.filter { meeting.tagIDs.contains($0.id) }.map { "#" + $0.name }
        return (names + tags).joined(separator: " · ")
    }
    private func editor(_ label: String, binding: Binding<String>) -> some View {
        // HIG accessibility: standard editable text, semantic fonts and system colors
        // respect contrast and assistive technologies without custom event handling.
        // https://developer.apple.com/design/human-interface-guidelines/accessibility
        TextEditor(text: binding).font(.body).accessibilityLabel(label)
            .padding(8).background(.background, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color(nsColor: .separatorColor).opacity(0.6)))
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
                                Button(formatTime(segment.start)) { playback.play(meeting: meeting, files: store.audioURLs(for: meeting), at: segment.start) }.buttonStyle(.link).monospacedDigit().help("Play from this point").disabled(playback.isPlaybackBlocked || store.audioURLs(for: meeting).isEmpty)
                                TextField("Speaker", text: Binding(get: { self.meeting?.transcript.first(where: { $0.id == segment.id })?.speaker ?? "" }, set: { value in change { meeting in if let index = meeting.transcript.firstIndex(where: { $0.id == segment.id }) { meeting.transcript[index].speaker = value } } })).font(.headline).textFieldStyle(.plain)
                            }
                            TextField("Transcript", text: Binding(get: { self.meeting?.transcript.first(where: { $0.id == segment.id })?.text ?? "" }, set: { value in change { meeting in if let index = meeting.transcript.firstIndex(where: { $0.id == segment.id }) { meeting.transcript[index].text = value } } }), axis: .vertical).textFieldStyle(.plain)
                            Divider()
                        }
                    }
                }.padding(4)
            }.overlay {
                // Expanded playback can leave little vertical space. Keep the
                // empty-state action reachable using standard scrolling.
                if meeting.transcript.isEmpty { ScrollView { emptyTranscript(meeting).fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity) } }
            }
        }
    }
    private func emptyTranscript(_ meeting: Meeting) -> some View {
        ContentUnavailableView {
            Label("Recording transcript", systemImage: "text.bubble")
        } description: {
            Text(store.recordingID == meetingID ? "Take notes while recording. Transcription is available after the audio is saved." : "No transcript yet.")
        } actions: {
            if !meeting.audioFiles.isEmpty && store.recordingID != meetingID {
                Button(meeting.serverTranscription == nil ? "Transcribe Recording" : "Resume Transcription") { Task { await store.transcribe(id: meetingID) } }.disabled(store.isBusy)
            }
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

private func formatTime(_ seconds: Double) -> String { let value = seconds.isFinite ? max(0, Int(min(seconds, Double(Int.max / 2)))) : 0; return String(format: "%d:%02d", value / 60, value % 60) }
