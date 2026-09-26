import Foundation

struct TranscriptSegment: Codable, Identifiable, Equatable {
    var id = UUID()
    var start: Double = 0
    var end: Double = 0
    var speaker = "Speaker"
    var text = ""
    enum CodingKeys: String, CodingKey { case id, start, end, speaker, text }

}
struct MeetingTodo: Codable, Identifiable, Equatable {
    var id = UUID()
    var title = ""
    var isCompleted = false
    enum CodingKeys: String, CodingKey { case id, title, isCompleted }

}
struct ChatMessage: Codable, Identifiable, Equatable {
    var id = UUID()
    var role = "user"
    var content = ""
    var createdAt = Date()
    enum CodingKeys: String, CodingKey { case id, role, content, createdAt }

}
struct Meeting: Codable, Identifiable, Equatable {
    var id = UUID()
    var title = "Untitled Meeting"
    var language = "en"
    var createdAt = Date()
    var duration: TimeInterval = 0
    var notes = ""
    var summary = ""
    var transcript: [TranscriptSegment] = []
    var personIDs: [UUID] = []
    var tagIDs: [UUID] = []
    var audioFiles: [String] = []
    var chat: [ChatMessage] = []
    var todos: [MeetingTodo] = []
    var recordingProfile: RecordingProfile?
    var transcriptionAttempt: ProviderTranscriptionAttempt?
    enum CodingKeys: String, CodingKey {
        case id, title, language, createdAt, duration, notes, summary, transcript, personIDs, tagIDs, audioFiles, chat,
            todos,
            recordingProfile, transcriptionAttempt
    }

}
struct Person: Codable, Identifiable, Equatable {
    var id = UUID()
    var name = ""
    var email = ""
    var notes = ""
    enum CodingKeys: String, CodingKey { case id, name, email, notes }

}
struct MeetingTag: Codable, Identifiable, Equatable {
    var id = UUID()
    var name = ""
    var color = "blue"
    enum CodingKeys: String, CodingKey { case id, name, color }

}
enum RecordingFormat: String, Codable, CaseIterable {
    case opus, m4a, wav
}

struct AppSettings: Codable, Equatable {
    var serviceProviders: [ServiceProvider] = []
    var transcriptionProviderID: UUID?
    var summaryProviderID: UUID?
    var defaultLanguage = "en"
    var autoTranscribe = false
    var captureSystemAudio = true
    var captureMicrophone = true
    var recordingFormat: RecordingFormat = .opus
    var summarizationPrompt =
        "Summarize this meeting with decisions, key points, and action items. Do not invent information."
    enum CodingKeys: String, CodingKey {
        case serviceProviders, transcriptionProviderID, summaryProviderID, defaultLanguage, autoTranscribe,
            captureSystemAudio,
            captureMicrophone, recordingFormat, summarizationPrompt
    }

}
struct MeetingLibrary: Codable {
    var contextualChats: [String: [ChatMessage]] = [:]
    var version = 1
    var meetings: [Meeting] = []
    var people: [Person] = []
    var tags: [MeetingTag] = []
    enum CodingKeys: String, CodingKey { case version, meetings, people, tags, contextualChats }

}
enum MeetingError: LocalizedError {
    case message(String)
    var errorDescription: String? {
        switch self {
        case .message(let text): return text
        }
    }
}

extension TranscriptSegment {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        start = try values.decodeIfPresent(Double.self, forKey: .start) ?? 0
        end = try values.decodeIfPresent(Double.self, forKey: .end) ?? 0
        speaker = try values.decodeIfPresent(String.self, forKey: .speaker) ?? "Speaker"
        text = try values.decodeIfPresent(String.self, forKey: .text) ?? ""
    }
}

extension MeetingTodo {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        title = try values.decodeIfPresent(String.self, forKey: .title) ?? ""
        isCompleted = try values.decodeIfPresent(Bool.self, forKey: .isCompleted) ?? false
    }
}

extension ChatMessage {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        role = try values.decodeIfPresent(String.self, forKey: .role) ?? "user"
        content = try values.decodeIfPresent(String.self, forKey: .content) ?? ""
        createdAt = try values.decodeIfPresent(Date.self, forKey: .createdAt) ?? Date()
    }
}

extension Meeting {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        title = try values.decodeIfPresent(String.self, forKey: .title) ?? "Untitled Meeting"
        language = try values.decodeIfPresent(String.self, forKey: .language) ?? "en"
        createdAt = try values.decodeIfPresent(Date.self, forKey: .createdAt) ?? Date()
        duration = try values.decodeIfPresent(TimeInterval.self, forKey: .duration) ?? 0
        notes = try values.decodeIfPresent(String.self, forKey: .notes) ?? ""
        summary = try values.decodeIfPresent(String.self, forKey: .summary) ?? ""
        transcript = try values.decodeIfPresent([TranscriptSegment].self, forKey: .transcript) ?? []
        personIDs = try values.decodeIfPresent([UUID].self, forKey: .personIDs) ?? []
        tagIDs = try values.decodeIfPresent([UUID].self, forKey: .tagIDs) ?? []
        audioFiles = try values.decodeIfPresent([String].self, forKey: .audioFiles) ?? []
        chat = try values.decodeIfPresent([ChatMessage].self, forKey: .chat) ?? []
        todos = try values.decodeIfPresent([MeetingTodo].self, forKey: .todos) ?? []
        recordingProfile = try values.decodeIfPresent(RecordingProfile.self, forKey: .recordingProfile)
        transcriptionAttempt = try values.decodeIfPresent(
            ProviderTranscriptionAttempt.self, forKey: .transcriptionAttempt)
    }
}

extension Person {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        name = try values.decodeIfPresent(String.self, forKey: .name) ?? ""
        email = try values.decodeIfPresent(String.self, forKey: .email) ?? ""
        notes = try values.decodeIfPresent(String.self, forKey: .notes) ?? ""
    }
}

extension MeetingTag {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        name = try values.decodeIfPresent(String.self, forKey: .name) ?? ""
        color = try values.decodeIfPresent(String.self, forKey: .color) ?? "blue"
    }
}

extension AppSettings {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        serviceProviders = try values.decodeIfPresent([ServiceProvider].self, forKey: .serviceProviders) ?? []
        transcriptionProviderID = try values.decodeIfPresent(UUID.self, forKey: .transcriptionProviderID)
        summaryProviderID = try values.decodeIfPresent(UUID.self, forKey: .summaryProviderID)
        defaultLanguage = try values.decodeIfPresent(String.self, forKey: .defaultLanguage) ?? "en"
        autoTranscribe = try values.decodeIfPresent(Bool.self, forKey: .autoTranscribe) ?? false
        captureSystemAudio = try values.decodeIfPresent(Bool.self, forKey: .captureSystemAudio) ?? true
        captureMicrophone = try values.decodeIfPresent(Bool.self, forKey: .captureMicrophone) ?? true
        recordingFormat = try values.decodeIfPresent(RecordingFormat.self, forKey: .recordingFormat) ?? .opus
        summarizationPrompt =
            try values.decodeIfPresent(String.self, forKey: .summarizationPrompt)
            ?? "Summarize this meeting with decisions, key points, and action items. Do not invent information."
    }
}

extension MeetingLibrary {
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        contextualChats = try values.decodeIfPresent([String: [ChatMessage]].self, forKey: .contextualChats) ?? [:]
        version = try values.decodeIfPresent(Int.self, forKey: .version) ?? 1
        meetings = try values.decodeIfPresent([Meeting].self, forKey: .meetings) ?? []
        people = try values.decodeIfPresent([Person].self, forKey: .people) ?? []
        tags = try values.decodeIfPresent([MeetingTag].self, forKey: .tags) ?? []
    }
}
