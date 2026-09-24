import Foundation
import Combine
import AVFoundation

@MainActor
final class MeetingStore: ObservableObject {
    @Published var contextualChats: [String: [ChatMessage]] = [:]
    @Published var meetings: [Meeting] = []
    @Published var people: [Person] = []
    @Published var tags: [MeetingTag] = []
    @Published var settings = AppSettings()
    @Published var recordingID: UUID?
    @Published var presentsRecordingSetup = false
    @Published var isBusy = false
    @Published var errorMessage: String?
    @Published var recordingPermissionNeeded: RecordingPermission?
    @Published var captureHealth = ""
    @Published var statusMessage = ""
    @Published var recordingStartedAt: Date?
    @Published var isFinalizingRecording = false
    @Published private(set) var isStartingRecording = false
    @Published var recordingLevels = RecordingLevels()
    let dataDirectory: URL
    private var recorder: AudioCapture?
    private var captureTransition = false
    private var activeRecordingFormat: RecordingFormat = .opus
    private var canSave = true
    private var lastSavedLibrary = MeetingLibrary()
    var libraryWritable: Bool { canSave }
    private let usesKeychain: Bool
    var recordingDuration: TimeInterval { recordingStartedAt.map { Date().timeIntervalSince($0) } ?? 0 }

    init(dataDirectory: URL? = nil) {
        let dataDirectory = dataDirectory ?? ProcessInfo.processInfo.environment["GDAY_SWIFT_DATA_DIR"].map { URL(fileURLWithPath: $0, isDirectory: true) }
        usesKeychain = dataDirectory == nil
        self.dataDirectory = dataDirectory ?? LibraryLocation.directory()
        do {
            if dataDirectory == nil { try LibraryLocation.migrateLegacyLibrary() }
            try FileManager.default.createDirectory(at: self.dataDirectory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            let libraryURL = self.dataDirectory.appendingPathComponent("library.json")
            if FileManager.default.fileExists(atPath: libraryURL.path) {
                let library = try JSONDecoder().decode(MeetingLibrary.self, from: Data(contentsOf: libraryURL))
                guard library.version == 1 else { throw MeetingError.message("This library was created by a newer version of Gday Meetings.") }
                meetings = library.meetings; people = library.people; tags = library.tags; contextualChats = library.contextualChats
            }
            let settingsURL = self.dataDirectory.appendingPathComponent("settings.json")
            if FileManager.default.fileExists(atPath: settingsURL.path) { settings = try JSONDecoder().decode(AppSettings.self, from: Data(contentsOf: settingsURL)) }
            lastSavedLibrary = MeetingLibrary(contextualChats: contextualChats, meetings: meetings, people: people, tags: tags)
            if usesKeychain {
                do {
                    settings.llmAPIKey = try KeychainStore.get("llm-api-key") ?? ""
                    settings.transcriptionAPIKey = try KeychainStore.get("transcription-api-key") ?? ""
                } catch { errorMessage = error.localizedDescription }
            }
        } catch {
            canSave = false
            errorMessage = "Could not open the local library. Existing files have been preserved. \(error.localizedDescription)"
        }
    }
    @discardableResult private func save() -> Bool {
        guard canSave else { errorMessage = "Library is read-only because loading failed. Restore library.json before saving changes."; return false }
        do {
            let data = try JSONEncoder().encode(MeetingLibrary(contextualChats: contextualChats, meetings: meetings, people: people, tags: tags))
            try data.write(to: dataDirectory.appendingPathComponent("library.json"), options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: dataDirectory.appendingPathComponent("library.json").path)
            lastSavedLibrary = MeetingLibrary(contextualChats: contextualChats, meetings: meetings, people: people, tags: tags)
            return true
        } catch {
            meetings = lastSavedLibrary.meetings; people = lastSavedLibrary.people; tags = lastSavedLibrary.tags; contextualChats = lastSavedLibrary.contextualChats
            errorMessage = "Could not save changes: \(error.localizedDescription)"
            return false
        }
    }
    func saveContextChat(key: String, messages: [ChatMessage]) {
        guard canSave else { return }
        contextualChats[key] = messages; save()
    }
    func saveSettings() {
        guard canSave else { return }
        do {
            if usesKeychain {
                try KeychainStore.set(settings.llmAPIKey, for: "llm-api-key")
                try KeychainStore.set(settings.transcriptionAPIKey, for: "transcription-api-key")
            }
            try JSONEncoder().encode(settings).write(to: dataDirectory.appendingPathComponent("settings.json"), options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: dataDirectory.appendingPathComponent("settings.json").path)
        } catch { errorMessage = error.localizedDescription }
    }
    func insertImportedMeeting(_ meeting: Meeting) throws {
        guard canSave else { throw MeetingError.message("The library is read-only because loading failed.") }
        meetings.insert(meeting, at: 0)
        guard save() else { throw MeetingError.message(errorMessage ?? "Could not save imported meeting.") }
    }
    @discardableResult func createMeeting(title: String = "Untitled Meeting") -> UUID {
        guard canSave else { return UUID() }
        let meeting = Meeting(title: title)
        meetings.insert(meeting, at: 0); save(); return meeting.id
    }
    func updateMeeting(_ meeting: Meeting) { guard canSave else { return }; if let i = meetings.firstIndex(where: { $0.id == meeting.id }) { meetings[i] = meeting; save() } }
    func deleteMeeting(id: UUID) {
        guard canSave else { return }
        guard recordingID != id else { errorMessage = "Stop recording before deleting this meeting."; return }
        do {
            let original = meetings
            meetings.removeAll { $0.id == id }
            guard save() else { return }
            let folder = directory(for: id)
            do {
            if FileManager.default.fileExists(atPath: folder.path) { _ = try FileManager.default.trashItem(at: folder, resultingItemURL: nil) }
            } catch { meetings = original; save(); throw error }
        } catch { errorMessage = error.localizedDescription }
    }
    @discardableResult func addPerson(name: String) -> UUID { guard canSave else { return UUID() }; let person = Person(name: name); people.append(person); save(); return person.id }
    func updatePerson(_ person: Person) { guard canSave else { return }; if let i = people.firstIndex(where: { $0.id == person.id }) { people[i] = person; save() } }
    func deletePerson(id: UUID) { guard canSave else { return }; people.removeAll { $0.id == id }; contextualChats.removeValue(forKey: Self.contextChatKey(personID: id)); for i in meetings.indices { meetings[i].personIDs.removeAll { $0 == id } }; save() }
    @discardableResult func addTag(name: String, color: String = "blue") -> UUID { guard canSave else { return UUID() }; let tag = MeetingTag(name: name, color: color); tags.append(tag); save(); return tag.id }
    func updateTag(_ tag: MeetingTag) { guard canSave else { return }; if let i = tags.firstIndex(where: { $0.id == tag.id }) { tags[i] = tag; save() } }
    func deleteTag(id: UUID) { guard canSave else { return }; tags.removeAll { $0.id == id }; contextualChats.removeValue(forKey: Self.contextChatKey(tagID: id)); for i in meetings.indices { meetings[i].tagIDs.removeAll { $0 == id } }; save() }
    func directory(for id: UUID) -> URL { dataDirectory.appendingPathComponent(id.uuidString, isDirectory: true) }
    func audioURLs(for meeting: Meeting) -> [URL] { meeting.audioFiles.filter { URL(fileURLWithPath: $0).lastPathComponent == $0 && !$0.contains("..") }.map { directory(for: meeting.id).appendingPathComponent($0) }.filter { FileManager.default.fileExists(atPath: $0.path) } }
    func audioURL(for meeting: Meeting) -> URL? { audioURLs(for: meeting).first }

    func startRecording(title: String? = nil) async {
        guard recordingID == nil, !isBusy, canSave else { return }
        isStartingRecording = true
        defer { isStartingRecording = false }
        recordingLevels = RecordingLevels(microphone: RecordingSourceLevel(enabled: settings.captureMicrophone), system: RecordingSourceLevel(enabled: settings.captureSystemAudio))
        recordingPermissionNeeded = nil
        isBusy = true; captureTransition = true
        activeRecordingFormat = settings.recordingFormat
        let suppliedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let meeting = Meeting(title: suppliedTitle.isEmpty ? Date().formatted(date: .abbreviated, time: .shortened) : suppliedTitle)
        do {
            try FileManager.default.createDirectory(at: directory(for: meeting.id), withIntermediateDirectories: true)
            let capture = AudioCapture()
            capture.onLevels = { [weak self] levels, delivered in Task { @MainActor in defer { delivered() }; if self?.recordingID == meeting.id && self?.isFinalizingRecording == false { self?.recordingLevels = levels } } }
            capture.onHealth = { [weak self] message in Task { @MainActor in if self?.recordingID == meeting.id { self?.captureHealth = message } } }
            capture.onFailure = { [weak self] error in Task { @MainActor in
                guard let self else { return }
                self.errorMessage = error.localizedDescription
                while self.captureTransition { try? await Task.sleep(nanoseconds: 100_000_000) }
                await self.stopRecording(transcribeAfter: false)
            } }
            let files = try await capture.start(directory: directory(for: meeting.id), microphoneEnabled: settings.captureMicrophone, systemEnabled: settings.captureSystemAudio, voiceProcessingEnabled: settings.microphoneVoiceProcessing)
            var recorded = meeting; recorded.audioFiles = files; recorded.recordingProfile = capture.profile
            meetings.insert(recorded, at: 0)
            guard save() else { try? await capture.stop(); throw MeetingError.message(errorMessage ?? "Could not save recording metadata.") }
            recorder = capture; recordingID = meeting.id; recordingStartedAt = Date()
            captureHealth = [settings.captureMicrophone ? (settings.microphoneVoiceProcessing ? "Microphone: Apple voice processing" : "Microphone: unprocessed") : nil, settings.captureSystemAudio ? "System audio: separate track" : nil].compactMap { $0 }.joined(separator: " · ")
            statusMessage = "Recording"
        } catch {
            if error is CancellationError { statusMessage = "Recording cancelled" }
            else if let permission = RecordingPermissions.permission(for: error) { recordingPermissionNeeded = permission }
            else { errorMessage = error.localizedDescription }
        }
        isBusy = false; captureTransition = false
    }
    func stopRecording(transcribeAfter: Bool = true) async {
        guard let id = recordingID else { return }
        guard !captureTransition else { return }
        captureTransition = true
        isBusy = true
        isFinalizingRecording = true
        let duration = recordingDuration
        var stopFailed = false
        do { try await recorder?.stop() } catch { errorMessage = error.localizedDescription; stopFailed = true }
        let profile = recorder?.profile
        recorder = nil
        captureHealth = ""
        if let index = meetings.firstIndex(where: { $0.id == id }) {
            meetings[index].duration = duration; meetings[index].recordingProfile = profile
            if !save() { stopFailed = true }
        }
        if !stopFailed && activeRecordingFormat != .wav {
            statusMessage = "Saving \(activeRecordingFormat.rawValue.uppercased()) audio…"
            do { try await finalizeRecordingAudio(id: id, format: activeRecordingFormat) }
            catch { errorMessage = "Audio compression failed; the original WAV recording was retained. \(error.localizedDescription)"; stopFailed = true }
        }
        recordingID = nil; recordingStartedAt = nil
        recordingLevels = RecordingLevels()
        statusMessage = stopFailed ? "Recording interrupted; partial audio retained" : "Recording saved"
        captureTransition = false
        isBusy = false
        isFinalizingRecording = false
        if !stopFailed && transcribeAfter && settings.autoTranscribe { await transcribe(id: id) }
    }
    func finalizeRecordingAudio(id: UUID, format: RecordingFormat) async throws {
        guard canSave, format != .wav else { return }
        guard let meeting = meetings.first(where: { $0.id == id }) else { return }
        let originals = audioURLs(for: meeting)
        guard !originals.isEmpty, originals.count == meeting.audioFiles.count else { throw MeetingError.message("A recorded audio file is missing.") }
        var encoded: [URL] = []
        do {
            for source in originals {
                let destination = source.deletingPathExtension().appendingPathExtension(format.rawValue)
                try await RecordingEncoder.encode(source: source, destination: destination, format: format)
                encoded.append(destination)
            }
            guard let index = meetings.firstIndex(where: { $0.id == id }) else { throw MeetingError.message("The recording no longer exists.") }
            meetings[index].audioFiles = encoded.map(\.lastPathComponent)
            if var profile = meetings[index].recordingProfile {
                for i in profile.tracks.indices {
                    if let sourceIndex = originals.firstIndex(where: { $0.lastPathComponent == profile.tracks[i].filename }) {
                        profile.tracks[i].filename = encoded[sourceIndex].lastPathComponent
                    }
                }
                meetings[index].recordingProfile = profile
            }
            guard save() else { throw MeetingError.message(errorMessage ?? "Could not save compressed recording metadata.") }
        } catch {
            for file in encoded { try? FileManager.default.removeItem(at: file) }
            throw error
        }
        // Remove only this capture's PCM spools, after durable metadata points to
        // every finalized compressed track. Failed conversion leaves WAV recoverable.
        for file in originals { try? FileManager.default.removeItem(at: file) }
    }
    func finalizeForQuit() async {
        RecordingPermissions.cancelPendingStart()
        while captureTransition { try? await Task.sleep(nanoseconds: 100_000_000) }
        await stopRecording(transcribeAfter: false)
    }
    @discardableResult func importAudio(url: URL) throws -> UUID {
        guard canSave else { throw MeetingError.message("The library is read-only because loading failed.") }
        guard ["opus","ogg","wav","m4a","mp3","mp4","aiff","aif","caf","flac","aac","mov"].contains(url.pathExtension.lowercased()) else { throw MeetingError.message("Choose an audio or video file: Opus, Ogg, WAV, M4A, MP3, MP4, AIFF, CAF, FLAC, AAC, or MOV.") }
        let access = url.startAccessingSecurityScopedResource(); defer { if access { url.stopAccessingSecurityScopedResource() } }
        var meeting = Meeting(title: url.deletingPathExtension().lastPathComponent)
        let folder = directory(for: meeting.id)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let name = "imported.\(url.pathExtension.lowercased())"
        try FileManager.default.copyItem(at: url, to: folder.appendingPathComponent(name))
        meeting.audioFiles = [name]
        if let player = try? AVAudioPlayer(contentsOf: folder.appendingPathComponent(name)) { meeting.duration = player.duration }
        meetings.insert(meeting, at: 0); guard save() else { throw MeetingError.message(errorMessage ?? "Could not save imported audio.") }
        if ["opus", "ogg"].contains(url.pathExtension.lowercased()) {
            let importedID = meeting.id
            Task { [weak self] in
                do {
                    let metadata = try await AudioPlaybackPreparation.opusMetadata(folder.appendingPathComponent(name))
                    guard let self, let index = self.meetings.firstIndex(where: { $0.id == importedID }) else { return }
                    self.meetings[index].duration = metadata.duration; self.save()
                } catch { self?.errorMessage = "The imported audio was retained, but its Opus metadata could not be read. \(error.localizedDescription)" }
            }
        }
        return meeting.id
    }
    func importArchive(url: URL) throws {
        guard canSave else { throw MeetingError.message("The library is read-only because loading failed.") }
        let access = url.startAccessingSecurityScopedResource(); defer { if access { url.stopAccessingSecurityScopedResource() } }
        var meeting = try JSONDecoder().decode(Meeting.self, from: Data(contentsOf: url))
        meeting.id = UUID(); meeting.audioFiles = []; meeting.personIDs = []; meeting.tagIDs = []; meeting.serverTranscription = nil
        meetings.insert(meeting, at: 0); guard save() else { throw MeetingError.message(errorMessage ?? "Could not save imported meeting.") }
    }
    func exportMeeting(id: UUID, to url: URL) throws {
        guard var meeting = meetings.first(where: { $0.id == id }) else { throw MeetingError.message("Meeting no longer exists.") }
        meeting.serverTranscription = nil
        if url.pathExtension.lowercased() == "json" {
            let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try encoder.encode(meeting).write(to: url, options: .atomic)
        } else {
            let transcript = meeting.transcript.map { "[\(Int($0.start / 60)):\(String(format: "%02d", Int($0.start) % 60))] **\($0.speaker):** \($0.text)" }.joined(separator: "\n\n")
            let todos = meeting.todos.map { "- [\($0.isCompleted ? "x" : " ")] \($0.title)" }.joined(separator: "\n")
            try "# \(meeting.title)\n\n\(meeting.createdAt.formatted())\n\n## Summary\n\n\(meeting.summary)\n\n## Notes\n\n\(meeting.notes)\n\n## Action items\n\n\(todos)\n\n## Transcript\n\n\(transcript)\n".write(to: url, atomically: true, encoding: .utf8)
        }
    }
}
