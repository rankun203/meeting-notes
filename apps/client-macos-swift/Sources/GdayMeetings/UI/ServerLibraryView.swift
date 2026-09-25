import SwiftUI

struct ServerLibraryView: View {
    @EnvironmentObject private var store: MeetingStore
    @ObservedObject private var server = GdayServerService.shared
    @ViewState private var query = ""
    @ViewState private var results: [ServerMeeting] = []
    @ViewState private var searching = false
    @ViewState private var searched = false
    @ViewState private var imported: Set<String> = []
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Server Library").font(.largeTitle)
            if !server.connected {
                ContentUnavailableView {
                    Label("Connect Your Server", systemImage: "network")
                } description: {
                    Text("Sign in to your Gday server in Settings to search its meeting library.")
                } actions: {
                    SettingsLink()
                }
            }
            else {
                HStack {
                    TextField("Search server meetings", text: $query).onSubmit(search)
                    Button("Search", systemImage: "magnifyingglass", action: search).disabled(searching)
                    if searching { ProgressView().controlSize(.small) }
                }
                List(results) { result in
                    DisclosureGroup {
                        Text(result.transcript.isEmpty ? "No transcript available." : result.transcript).textSelection(
                            .enabled)
                        Button(imported.contains(result.id) ? "Imported" : "Import Transcript to My Library") {
                            let id = store.createMeeting(title: result.title)
                            if var meeting = store.meetings.first(where: { $0.id == id }) {
                                meeting.transcript = [
                                    TranscriptSegment(speaker: "Server transcript", text: result.transcript)
                                ]
                                store.updateMeeting(meeting)
                                imported.insert(result.id)
                            }
                        }.disabled(imported.contains(result.id))
                    } label: {
                        Text(result.title).font(.headline)
                    }
                }.overlay {
                    if searched && results.isEmpty && !searching { ContentUnavailableView.search(text: query) }
                }
            }
        }.padding(20)
    }
    private func search() {
        guard !searching else { return }
        searching = true
        Task {
            defer {
                searching = false
                searched = true
            }
            do { results = try await server.search(query: query) }
            catch {
                store.errorMessage = error.localizedDescription
            }
        }
    }
}
