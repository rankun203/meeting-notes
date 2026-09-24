import SwiftUI
import AppKit
import UniformTypeIdentifiers

private enum LibraryDestination: Hashable { case meetings, people, tags, server }

struct LibraryView: View {
    @EnvironmentObject private var store: MeetingStore
    @ViewState private var destination: LibraryDestination? = .meetings
    @ViewState private var selectedMeeting: UUID?
    @ViewState private var selectedPerson: UUID?
    @ViewState private var selectedTag: UUID?
    @ViewState private var search = ""
    @ViewState private var deleting: Meeting?

    private var filteredMeetings: [Meeting] {
        store.meetings.filter { meeting in
            search.isEmpty || ([meeting.title, meeting.notes, meeting.summary] + meeting.transcript.map(\.text)).joined(separator: " ").localizedCaseInsensitiveContains(search)
        }.sorted { $0.createdAt > $1.createdAt }
    }

    private var emptyMeetings: some View {
        let title = search.isEmpty ? "No Meetings" : "No Results"
        let description = search.isEmpty ? "Record a meeting or import audio to get started." : "Try another search."
        return ContentUnavailableView { Label(title, systemImage: "waveform") } description: { Text(description) }
    }

    var body: some View {
        // HIG: a sidebar expresses the hierarchy; an intermediate list selects content.
        // Native split views preserve resizing, keyboard navigation and system appearance.
        // https://developer.apple.com/design/human-interface-guidelines/sidebars
        NavigationSplitView {
            List(selection: $destination) {
                Label("Meetings", systemImage: "waveform").tag(LibraryDestination.meetings)
                Label("People", systemImage: "person.2").tag(LibraryDestination.people)
                Label("Tags", systemImage: "tag").tag(LibraryDestination.tags)
                Label("Server Library", systemImage: "network").tag(LibraryDestination.server)
            }
            .navigationTitle("Gday Meetings")
            .navigationSplitViewColumnWidth(min: 150, ideal: 180)
        } content: {
            switch destination {
            case .server: Text("Search your connected Gday server library.").foregroundStyle(.secondary).padding().navigationTitle("Server Library")
            case .people: PeopleView(selection: $selectedPerson)
            case .tags: TagsView(selection: $selectedTag)
            default:
                List(selection: $selectedMeeting) {
                    ForEach(filteredMeetings) { meeting in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(meeting.title).font(.headline).lineLimit(1)
                                if store.recordingID == meeting.id { Label("Recording", systemImage: "record.circle").foregroundStyle(.red).labelStyle(.iconOnly) }
                            }
                            Text(meeting.createdAt, format: .dateTime.month().day().hour().minute()).font(.caption).foregroundStyle(.secondary)
                            if !meeting.summary.isEmpty { Text(meeting.summary).lineLimit(2).font(.caption).foregroundStyle(.secondary) }
                        }.padding(.vertical, 4).tag(meeting.id)
                        .contextMenu {
                            Button("Export Meeting…") { MeetingPanels.export(meeting, store: store) }
                            Button("Delete Meeting…", role: .destructive) { deleting = meeting }
                                .disabled(store.recordingID == meeting.id)
                        }
                    }
                }
                .searchable(text: $search, prompt: "Search meetings and transcripts")
                .navigationTitle("Meetings")
                .overlay { if filteredMeetings.isEmpty { emptyMeetings } }
                .navigationSplitViewColumnWidth(min: 220, ideal: 280)
            }
        } detail: {
            if destination == .meetings, let id = selectedMeeting, store.meetings.contains(where: { $0.id == id }) {
                MeetingDetailView(meetingID: id).id(id)
            } else if destination == .server {
                ServerLibraryView()
            } else if destination == .people, let id = selectedPerson, let person = store.people.first(where: { $0.id == id }) {
                ContextDetailView(title: person.name, personID: id, tagID: nil).id(id)
            } else if destination == .tags, let id = selectedTag, let tag = store.tags.first(where: { $0.id == id }) {
                ContextDetailView(title: tag.name, personID: nil, tagID: id).id(id)
            } else {
                ContentUnavailableView("Gday Meetings", systemImage: "waveform", description: Text("Your recordings, transcripts, and meeting notes in one place."))
            }
        }
        // HIG: toolbar actions apply to the current content and use familiar symbols.
        // https://developer.apple.com/design/human-interface-guidelines/toolbars
        .toolbar {
            ToolbarItemGroup {
                Button {
                    if !NSWorkspace.shared.open(store.dataDirectory) {
                        store.errorMessage = "Could not open the meetings folder in Finder."
                    }
                } label: { Label("Open Meetings Folder", systemImage: "folder") }
                    .help("Open the meetings storage folder in Finder")
                Button { selectedMeeting = store.createMeeting(title: "Untitled Meeting"); destination = .meetings } label: { Label("New Meeting", systemImage: "square.and.pencil") }.help("Create a meeting")
                Button { MeetingPanels.importAudio(store) } label: { Label("Import Audio", systemImage: "square.and.arrow.down") }.help("Import an audio or video file")
                Button {
                    Task {
                        if store.recordingID == nil { await store.startRecording(); selectedMeeting = store.recordingID; destination = .meetings }
                        else { await store.stopRecording() }
                    }
                } label: { Label(store.recordingID == nil ? "Record" : "Stop Recording", systemImage: store.recordingID == nil ? "record.circle" : "stop.circle.fill") }
                    .tint(store.recordingID == nil ? nil : .red).help(store.recordingID == nil ? "Start recording" : "Stop and save recording")
            }
        }
        // HIG Feedback: show ongoing capture status passively, close to the content.
        // Critical capture actions remain in the toolbar, never only at the bottom.
        // https://developer.apple.com/design/human-interface-guidelines/feedback
        .safeAreaInset(edge: .top) {
            if store.recordingID != nil && !store.captureHealth.isEmpty {
                HStack(alignment: .top) {
                    Label(store.captureHealth, systemImage: "waveform")
                        .font(.callout).textSelection(.enabled)
                    Spacer()
                }.padding(10).background(.bar)
            }
        }
        .safeAreaInset(edge: .bottom) {
            if store.isBusy || !store.statusMessage.isEmpty || store.recordingID != nil {
                HStack(spacing: 8) {
                    if store.isBusy { ProgressView().controlSize(.small) }
                    if store.recordingID != nil { Label("Recording", systemImage: "record.circle.fill").foregroundStyle(.red) }
                    Text(store.statusMessage).font(.caption).lineLimit(2)
                    Spacer()
                }.padding(8).background(.bar)
            }
        }
        .alert("Unable to Complete Action", isPresented: Binding(get: { store.errorMessage != nil }, set: { if !$0 { store.errorMessage = nil } })) {
            Button("OK") { store.errorMessage = nil }
        } message: { Text(store.errorMessage ?? "") }
        .confirmationDialog("Delete \(deleting?.title ?? "meeting")?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
            Button("Delete Meeting", role: .destructive) { if let meeting = deleting { store.deleteMeeting(id: meeting.id); if selectedMeeting == meeting.id { selectedMeeting = nil } }; deleting = nil }
        } message: { Text("This deletes the meeting and its saved audio. This cannot be undone.") }
    }
}

@MainActor
enum MeetingPanels {
    static func importAudio(_ store: MeetingStore) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.audio, .movie]
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.prompt = "Import"
        if panel.runModal() == .OK {
            for url in panel.urls { do { _ = try store.importAudio(url: url) } catch { store.errorMessage = error.localizedDescription } }
        }
    }
    static func importLegacy(_ store: MeetingStore) {
        let panel = NSOpenPanel(); panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.prompt = "Import Library"
        panel.message = "Choose your existing Gday Meetings data folder. Meetings and audio are copied into the Swift app."
        if panel.runModal() == .OK, let url = panel.url { do { let count = try store.importLegacyLibrary(url: url); store.statusMessage = "Imported \(count) meetings" } catch { store.errorMessage = error.localizedDescription } }
    }
    static func importArchive(_ store: MeetingStore) {
        let panel = NSOpenPanel(); panel.allowedContentTypes = [.json]; panel.prompt = "Import"
        if panel.runModal() == .OK, let url = panel.url { do { try store.importArchive(url: url) } catch { store.errorMessage = error.localizedDescription } }
    }
    static func export(_ meeting: Meeting, store: MeetingStore) {
        let panel = NSSavePanel(); panel.allowedContentTypes = [.json, UTType(filenameExtension: "md") ?? .plainText]; panel.allowsOtherFileTypes = true; panel.nameFieldStringValue = meeting.title + ".json"
        panel.message = "Export meeting text as JSON or Markdown (.md). Audio files are not included."
        if panel.runModal() == .OK, let url = panel.url { do { try store.exportMeeting(id: meeting.id, to: url) } catch { store.errorMessage = error.localizedDescription } }
    }
}
