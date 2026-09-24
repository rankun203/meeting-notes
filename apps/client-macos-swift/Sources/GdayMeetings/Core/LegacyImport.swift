import Foundation

extension MeetingStore {
    /// Copies selected legacy recordings. Never opens the Rust library for writing.
    @discardableResult func importLegacyLibrary(url: URL) throws -> Int {
        guard libraryWritable else { throw MeetingError.message("The library is read-only because loading failed.") }
        let access = url.startAccessingSecurityScopedResource(); defer { if access { url.stopAccessingSecurityScopedResource() } }
        let fm = FileManager.default
        let recordings = fm.fileExists(atPath: url.appendingPathComponent("recordings").path) ? url.appendingPathComponent("recordings") : url
        let children = try fm.contentsOfDirectory(at: recordings, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        let folders = fm.fileExists(atPath: recordings.appendingPathComponent("metadata.json").path) ? [recordings] : children
        var imported = 0
        let legacyRoot = recordings.lastPathComponent == "recordings" ? recordings.deletingLastPathComponent() : url
        var personMap: [String: UUID] = [:]
        let peopleFolder = legacyRoot.appendingPathComponent("people")
        if fm.fileExists(atPath: peopleFolder.path) {
            for folder in try fm.contentsOfDirectory(at: peopleFolder, includingPropertiesForKeys: [.isSymbolicLinkKey]) {
                guard try folder.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink != true else { continue }
                let profileURL = folder.appendingPathComponent("profile.json")
                guard fm.fileExists(atPath: profileURL.path), let profile = try JSONSerialization.jsonObject(with: Data(contentsOf: profileURL)) as? [String: Any], let name = profile["name"] as? String else { continue }
                let personID = addPerson(name: name)
                if var person = people.first(where: { $0.id == personID }) { person.notes = profile["notes"] as? String ?? ""; updatePerson(person) }
                personMap[folder.lastPathComponent] = personID
            }
        }

        for folder in folders {
            let properties = try folder.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard properties.isDirectory == true, properties.isSymbolicLink != true else { continue }
            let metadataURL = folder.appendingPathComponent("metadata.json")
            guard fm.fileExists(atPath: metadataURL.path) else { continue }
            guard let metadata = try JSONSerialization.jsonObject(with: Data(contentsOf: metadataURL)) as? [String: Any] else { continue }
            var meeting = Meeting(title: metadata["name"] as? String ?? folder.lastPathComponent)
            meeting.notes = metadata["notes"] as? String ?? ""
            meeting.duration = metadata["duration_secs"] as? Double ?? 0
            for name in metadata["tags"] as? [String] ?? [] {
                let id = tags.first(where: { $0.name == name })?.id ?? addTag(name: name)
                meeting.tagIDs.append(id)
            }
            if let date = metadata["created_at"] as? String {
                let formatter = ISO8601DateFormatter(); formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
                meeting.createdAt = formatter.date(from: date) ?? ISO8601DateFormatter().date(from: date) ?? Date()
            }
            let transcriptURL = folder.appendingPathComponent("transcript.json")
            if fm.fileExists(atPath: transcriptURL.path), let transcript = try JSONSerialization.jsonObject(with: Data(contentsOf: transcriptURL)) as? [String: Any], let segments = transcript["segments"] as? [[String: Any]] {
                let speakerIndex = transcript["speaker_embeddings"] as? [String: [String: Any]] ?? [:]
                meeting.transcript = segments.map { segment in
                    let label = segment["speaker"] as? String ?? "Speaker"
                    let legacyPersonID = segment["person_id"] as? String ?? speakerIndex[label]?["person_id"] as? String
                    let personID = legacyPersonID.flatMap { personMap[$0] }
                    if let personID, !meeting.personIDs.contains(personID) { meeting.personIDs.append(personID) }
                    let name = personID.flatMap { id in people.first(where: { $0.id == id })?.name } ?? label
                    return TranscriptSegment(start: segment["start"] as? Double ?? 0, end: segment["end"] as? Double ?? 0, speaker: name, text: segment["text"] as? String ?? "")
                }
            }
            let markdownURL = folder.appendingPathComponent("summary.md")
            let summaryURL = folder.appendingPathComponent("summary.json")
            if fm.fileExists(atPath: markdownURL.path) { meeting.summary = try String(contentsOf: markdownURL, encoding: .utf8) }
            else if fm.fileExists(atPath: summaryURL.path) {
                let raw = try Data(contentsOf: summaryURL)
                let summary = try JSONSerialization.jsonObject(with: raw)
                meeting.summary = (summary as? [String: Any])?["summary"] as? String ?? String(data: raw, encoding: .utf8) ?? ""
            }
            let destination = directory(for: meeting.id)
            try fm.createDirectory(at: destination, withIntermediateDirectories: true)
            do {
                for file in try fm.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey]) {
                    let values = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                    guard values.isRegularFile == true, values.isSymbolicLink != true,
                          ["wav", "mp3", "m4a", "caf", "flac", "ogg", "opus"].contains(file.pathExtension.lowercased()) else { continue }
                    try fm.copyItem(at: file, to: destination.appendingPathComponent(file.lastPathComponent))
                    meeting.audioFiles.append(file.lastPathComponent)
                }
                try insertImportedMeeting(meeting)
                imported += 1
            } catch {
                try? fm.removeItem(at: destination)
                throw MeetingError.message("Imported \(imported) meetings before an error reading \(folder.lastPathComponent): \(error.localizedDescription)")
            }
        }
        guard imported > 0 else { throw MeetingError.message("No legacy meetings were found. Choose the recordings folder, a meeting folder, or the previous application's data folder.") }
        statusMessage = "Imported \(imported) meetings"
        return imported
    }
}
