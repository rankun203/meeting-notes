import CryptoKit
import Foundation

private struct ArchiveAudio: Codable {
    let filename: String
    let path: String
    let sha256: String
    let size: UInt64
    var url: URL?
}
private struct ArchiveCheckpoint: Codable {
    let origin: String
    let externalID: String
    let importKey: String
    let snapshot: Data
    var audio: [ArchiveAudio]
}

extension MeetingStore {
    func archiveToServer(id: UUID) async {
        guard !isBusy, recordingID == nil, libraryWritable, let meeting = meetings.first(where: { $0.id == id }) else { return }
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            let server = GdayServerService.shared
            guard let origin = server.origin else { throw ServiceError("Sign in to Gday Meetings Server in Settings.") }
            try await server.ensureArchiveAvailable()
            let folder = directory(for: id)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            let checkpointURL = folder.appendingPathComponent("server-archive.json")
            var checkpoint: ArchiveCheckpoint
            if FileManager.default.fileExists(atPath: checkpointURL.path) {
                checkpoint = try JSONDecoder().decode(ArchiveCheckpoint.self, from: Data(contentsOf: checkpointURL))
                guard checkpoint.origin == origin else { throw ServiceError("Sign in to \(checkpoint.origin) to resume this archive.") }
            } else {
                statusMessage = "Preparing the meeting archive…"
                var archivedMeeting = meeting; archivedMeeting.serverTranscription = nil
                let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]; encoder.dateEncodingStrategy = .iso8601
                func json<T: Encodable>(_ value: T) throws -> Any { try JSONSerialization.jsonObject(with: encoder.encode(value)) }
                let artifacts: [String: Any] = ["meeting.json": try json(archivedMeeting), "transcript.json": ["segments": try json(meeting.transcript)], "notes.md": meeting.notes, "summary.md": meeting.summary, "todos.json": try json(meeting.todos), "chat.json": try json(meeting.chat)]
                let metadata: [String: Any] = ["people": try json(people.filter { meeting.personIDs.contains($0.id) }), "tags": try json(tags.filter { meeting.tagIDs.contains($0.id) }), "client": "Gday Meetings Swift"]
                let snapshot = try JSONSerialization.data(withJSONObject: ["externalId": id.uuidString, "title": meeting.title, "recordedAt": ISO8601DateFormatter().string(from: meeting.createdAt), "metadata": metadata, "artifacts": artifacts], options: [.sortedKeys])
                var audio: [ArchiveAudio] = []
                for (index, file) in audioURLs(for: meeting).enumerated() {
                    statusMessage = "Preparing archive audio \(index + 1)…"
                    // Archive original supported bytes when they fit; transcription
                    // separately prefers compressed upload copies for network efficiency.
                    let prepared = try await prepareServerAudio(file, compressPCM: false)
                    var source = prepared.url
                    if prepared.temporary {
                        defer { try? FileManager.default.removeItem(at: prepared.url) }
                        source = folder.appendingPathComponent("archive-audio-\(index).m4a")
                        // Retain converted bytes so retries use the identical hash and server snapshot.
                        if !FileManager.default.fileExists(atPath: source.path) { try FileManager.default.copyItem(at: prepared.url, to: source) }
                        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: source.path)
                    }
                    let hashed = try await Self.archiveHash(source)
                    audio.append(ArchiveAudio(filename: source.lastPathComponent, path: source.lastPathComponent, sha256: hashed.hash, size: hashed.size))
                }
                var hash = SHA256(); hash.update(data: snapshot)
                for file in audio { hash.update(data: Data("\n\(file.filename):\(file.sha256):\(file.size)".utf8)) }
                checkpoint = ArchiveCheckpoint(origin: origin, externalID: id.uuidString, importKey: hash.finalize().map { String(format: "%02x", $0) }.joined(), snapshot: snapshot, audio: audio)
                try Self.saveArchive(checkpoint, to: checkpointURL)
            }
            for index in checkpoint.audio.indices where checkpoint.audio[index].url == nil {
                let item = checkpoint.audio[index]
                guard URL(fileURLWithPath: item.path).lastPathComponent == item.path else { throw ServiceError("The archive audio path is invalid.") }
                let source = folder.appendingPathComponent(item.path)
                let hash = try await Self.archiveHash(source)
                guard hash.hash == item.sha256, hash.size == item.size else { throw ServiceError("Audio changed after this archive was prepared. The original snapshot has been preserved locally.") }
                statusMessage = "Uploading archive audio \(index + 1) of \(checkpoint.audio.count)…"
                checkpoint.audio[index].url = try await server.upload(file: source)
                try Self.saveArchive(checkpoint, to: checkpointURL)
            }
            guard var body = try JSONSerialization.jsonObject(with: checkpoint.snapshot) as? [String: Any], let artifacts = body["artifacts"] as? [String: Any] else { throw ServiceError("The saved archive is invalid.") }
            body["importKey"] = checkpoint.importKey
            body["audio"] = checkpoint.audio.map { ["filename": $0.filename, "url": $0.url!.absoluteString, "sha256": $0.sha256, "size": $0.size] as [String: Any] }
            statusMessage = "Saving the archive to the server…"
            _ = try await server.importArchive(body)
            statusMessage = "Verifying the archived meeting and audio…"
            let verified = try await server.verifyArchive(externalID: checkpoint.externalID)
            guard verified["importKey"] as? String == checkpoint.importKey, verified["audioCount"] as? Int == checkpoint.audio.count, verified["artifactCount"] as? Int == artifacts.count else { throw ServiceError("The server archive could not be verified. Local files have been preserved; retry to verify.") }
            statusMessage = "Archive verified on the server. Your local meeting and recordings are preserved."
        } catch { errorMessage = error.localizedDescription; statusMessage = "Archive incomplete; local files are preserved. Retry to resume." }
    }
    private static func saveArchive(_ checkpoint: ArchiveCheckpoint, to url: URL) throws {
        let bytes = try JSONEncoder().encode(checkpoint)
        try bytes.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }
    private nonisolated static func archiveHash(_ url: URL) async throws -> (hash: String, size: UInt64) {
        let handle = try FileHandle(forReadingFrom: url); defer { try? handle.close() }
        var hash = SHA256(); var count: UInt64 = 0
        while let data = try handle.read(upToCount: 1024 * 1024), !data.isEmpty { try Task.checkCancellation(); hash.update(data: data); count += UInt64(data.count) }
        return (hash.finalize().map { String(format: "%02x", $0) }.joined(), count)
    }
}
