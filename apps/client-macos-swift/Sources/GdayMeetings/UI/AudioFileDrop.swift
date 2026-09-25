import SwiftUI
import UniformTypeIdentifiers

/// HIG Drag and drop: highlight the destination and copy the source files.
/// https://developer.apple.com/design/human-interface-guidelines/drag-and-drop
struct AudioFileDrop: ViewModifier {
    @EnvironmentObject private var store: MeetingStore
    var meetingID: UUID?
    @ViewState private var targeted = false

    func body(content: Content) -> some View {
        content
            .contentShape(Rectangle())
            .overlay {
                if targeted {
                    RoundedRectangle(cornerRadius: 8).stroke(Color.accentColor, lineWidth: 2)
                        .overlay(alignment: .top) {
                            Text(meetingID == nil ? "Import as meetings" : "Add audio tracks")
                                .font(.callout).padding(8).background(.regularMaterial, in: Capsule())
                                .padding(8)
                        }.allowsHitTesting(false)
                }
            }
            .onDrop(of: [.fileURL], isTargeted: $targeted) { providers in
                guard !providers.isEmpty else { return false }
                let destination = meetingID
                // Start each provider load inside the drop callback. Store ordered
                // results on the main actor before performing one atomic import.
                let batch = AudioDropBatch(count: providers.count) { result in
                    Task { @MainActor in
                        do { _ = try await store.importAudioFiles(result.get(), into: destination) }
                        catch {
                            store.errorMessage = error.localizedDescription
                        }
                    }
                }
                for (index, provider) in providers.enumerated() {
                    _ = provider.loadObject(ofClass: URL.self) { url, error in
                        Task { @MainActor in
                            if let url {
                                batch.receive(.success(url), at: index)
                            }
                            else {
                                batch.receive(
                                    .failure(error ?? MeetingError.message("Could not read the dropped file.")),
                                    at: index)
                            }
                        }
                    }
                }
                return true
            }
    }
}

@MainActor
private final class AudioDropBatch {
    var results: [Result<URL, Error>?]
    let completion: (Result<[URL], Error>) -> Void
    init(count: Int, completion: @escaping (Result<[URL], Error>) -> Void) {
        results = Array(repeating: nil, count: count)
        self.completion = completion
    }
    func receive(_ result: Result<URL, Error>, at index: Int) {
        guard results[index] == nil else { return }
        results[index] = result
        guard results.allSatisfy({ $0 != nil }) else { return }
        completion(Result { try results.map { try $0!.get() } })
    }
}
