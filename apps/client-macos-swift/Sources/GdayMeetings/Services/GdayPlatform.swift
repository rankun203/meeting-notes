import Foundation

struct ServerTrackInput: Codable, Equatable {
    let url: URL
    let trackName: String
    let sourceType: String
    let channels: Int
}
struct ServerTranscriptSegment: Equatable {
    let start: Double
    let end: Double
    let text: String
    let speaker: String?
    let track: String
}
enum ServerTaskResult {
    case pending
    case complete([ServerTranscriptSegment])
    case failed(String)
}
struct ServerMeeting: Identifiable {
    let id: String
    let externalID: String
    let title: String
    let transcript: String
}
extension GdayServerService {
    func ensureTranscriptionAvailable() async throws {
        let result = try await ServiceHTTP.json(authorizedRequest("api/platform/capabilities"))
        guard result["durableTasks"] as? Bool == true, result["transcription"] as? Bool == true else {
            throw ServiceError(
                "This server does not have durable transcription configured. Ask its administrator to configure a worker."
            )
        }
    }
    func upload(file: URL) async throws -> URL {
        let size = try file.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
        guard size > 0, size <= 500_000_000 else {
            throw ServiceError(
                "Audio must be nonempty and no larger than 500 MB. Use M4A, Opus, or MP3 for long recordings.")
        }
        var r = try await authorizedRequest("upload")
        var components = URLComponents(url: r.url!, resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "filename", value: file.lastPathComponent)]
        r.url = components.url
        r.httpMethod = "POST"
        r.timeoutInterval = 900
        r.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        let (data, response) = try await ServiceHTTP.session.upload(for: r, fromFile: file)
        let json = try ServiceHTTP.decode(data, response)
        guard let value = json["url"] as? String, let url = URL(string: value, relativeTo: r.url)?.absoluteURL,
            ServiceHTTP.sameOrigin(url, r.url!)
        else { throw ServiceError("The uploaded audio URL is invalid or belongs to another server.") }
        return url
    }
    func submit(
        externalID: String, title: String, inputs: [ServerTrackInput], language: String, diarize: Bool,
        idempotencyKey: String
    ) async throws -> String {
        try TranscriptionLanguage.validate(language)
        guard !idempotencyKey.isEmpty else {
            throw ServiceError("A durable transcription attempt requires an idempotency key.")
        }
        var r = try await authorizedRequest("api/platform/tasks")
        r.httpMethod = "POST"
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        r.httpBody = try JSONSerialization.data(withJSONObject: [
            "externalId": externalID, "title": title,
            "inputs": inputs.map {
                [
                    "url": $0.url.absoluteString, "trackName": $0.trackName, "sourceType": $0.sourceType,
                    "channels": $0.channels,
                ] as [String: Any]
            }, "executionOptions": ["language": language, "diarize": diarize], "idempotencyKey": idempotencyKey,
        ])
        let result = try await ServiceHTTP.json(r)
        guard let id = result["id"] as? String else {
            throw ServiceError(
                "The server returned no task ID. Retry using the saved attempt to recover the same task.")
        }
        return id
    }
    func task(id: String) async throws -> ServerTaskResult {
        var r = try await authorizedRequest("api/platform/tasks")
        r.url = r.url!.appendingPathComponent(id)
        return try Self.parseTask(await ServiceHTTP.json(r))
    }
    static func parseTask(_ result: [String: Any]) throws -> ServerTaskResult {
        guard let outputs = result["outputs"] as? [[String: Any]] else {
            throw ServiceError("The server task has no outputs list.")
        }
        if let output = outputs.last(where: { $0["type"] as? String == "TRANSCRIPT_OUTPUT" }) {
            guard let body = output["body"] as? [String: Any], let tracks = body["tracks"] as? [String: [String: Any]]
            else { throw ServiceError("The server returned an invalid transcript.") }
            var segments: [ServerTranscriptSegment] = []
            for (name, track) in tracks {
                guard let entries = track["segments"] as? [[String: Any]] else {
                    throw ServiceError("The transcript track is missing its segments.")
                }
                for entry in entries {
                    guard let start = entry["start"] as? Double, let end = entry["end"] as? Double,
                        let text = entry["text"] as? String, start.isFinite, end.isFinite, start >= 0, end >= start
                    else { throw ServiceError("The server transcript contains an invalid segment.") }
                    segments.append(
                        .init(start: start, end: end, text: text, speaker: entry["speaker"] as? String, track: name))
                }
            }
            return .complete(segments.sorted { $0.start == $1.start ? $0.track < $1.track : $0.start < $1.start })
        }
        let status = result["status"] as? String ?? ""
        if ["FAILED", "CANCELLED", "TIMED_OUT"].contains(status) {
            return .failed("Transcription \(status.lowercased().replacingOccurrences(of: "_", with: " ")).")
        }
        if status == "COMPLETED" { throw ServiceError("The completed task contains no transcript output.") }
        return .pending
    }
    func search(query: String) async throws -> [ServerMeeting] {
        var r = try await authorizedRequest("api/platform/meetings/search")
        var components = URLComponents(url: r.url!, resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "query", value: query)]
        r.url = components.url
        let result = try await ServiceHTTP.json(r)
        guard let meetings = result["meetings"] as? [[String: Any]] else {
            throw ServiceError("Invalid server search results.")
        }
        return meetings.compactMap { value in
            guard let id = value["id"] as? String, let title = value["title"] as? String else { return nil }
            return ServerMeeting(
                id: id, externalID: value["externalId"] as? String ?? id, title: title,
                transcript: value["transcript"] as? String ?? "")
        }
    }
    func ensureArchiveAvailable() async throws {
        let capabilities = try await ServiceHTTP.json(authorizedRequest("api/platform/capabilities"))
        guard capabilities["meetingImports"] as? Bool == true else {
            throw ServiceError("Upgrade the server to support existing-meeting imports.")
        }
    }
    func importArchive(_ body: [String: Any]) async throws -> [String: Any] {
        try await ensureArchiveAvailable()
        var r = try await authorizedRequest("api/platform/meetings/import")
        r.httpMethod = "POST"
        r.timeoutInterval = 900
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        r.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, response) = try await ServiceHTTP.session.data(for: r)
        if (response as? HTTPURLResponse)?.statusCode == 409 {
            throw ServiceError(
                "The server already has this meeting with a different snapshot or transcription. Archives are immutable and cannot overwrite an existing meeting. Your local files are unchanged."
            )
        }
        return try ServiceHTTP.decode(data, response)
    }
    func verifyArchive(externalID: String) async throws -> [String: Any] {
        var r = try await authorizedRequest("api/platform/meetings/import")
        r.url = r.url!.appendingPathComponent(externalID)
        r.timeoutInterval = 900
        return try await ServiceHTTP.json(r)
    }
}
