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
    var serverTranscription: ServerTranscriptionAttempt?
    enum CodingKeys: String, CodingKey { case id, title, createdAt, duration, notes, summary, transcript, personIDs, tagIDs, audioFiles, chat, todos, recordingProfile, serverTranscription }

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
    var llmBaseURL = "https://api.openai.com/v1"
    var llmModel = "gpt-4o-mini"
    var llmAPIKey = ""
    var transcriptionBaseURL = "https://api.openai.com/v1"
    var transcriptionModel = "whisper-1"
    var transcriptionAPIKey = ""
    var autoTranscribe = false
    var captureSystemAudio = true
    var captureMicrophone = true
    var microphoneVoiceProcessing = false
    var recordingFormat: RecordingFormat = .opus
    var summarizationPrompt = "Summarize this meeting with decisions, key points, and action items. Do not invent information."
    enum CodingKeys: String, CodingKey { case llmBaseURL, llmModel, transcriptionBaseURL, transcriptionModel, autoTranscribe, captureSystemAudio, captureMicrophone, microphoneVoiceProcessing, recordingFormat, summarizationPrompt }

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
    var errorDescription: String? { switch self { case .message(let text): return text } }
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
        serverTranscription = try values.decodeIfPresent(ServerTranscriptionAttempt.self, forKey: .serverTranscription)
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
        llmBaseURL = try values.decodeIfPresent(String.self, forKey: .llmBaseURL) ?? "https://api.openai.com/v1"
        llmModel = try values.decodeIfPresent(String.self, forKey: .llmModel) ?? "gpt-4o-mini"
        transcriptionBaseURL = try values.decodeIfPresent(String.self, forKey: .transcriptionBaseURL) ?? "https://api.openai.com/v1"
        transcriptionModel = try values.decodeIfPresent(String.self, forKey: .transcriptionModel) ?? "whisper-1"
        autoTranscribe = try values.decodeIfPresent(Bool.self, forKey: .autoTranscribe) ?? false
        captureSystemAudio = try values.decodeIfPresent(Bool.self, forKey: .captureSystemAudio) ?? true
        captureMicrophone = try values.decodeIfPresent(Bool.self, forKey: .captureMicrophone) ?? true
        microphoneVoiceProcessing = try values.decodeIfPresent(Bool.self, forKey: .microphoneVoiceProcessing) ?? false
        recordingFormat = try values.decodeIfPresent(RecordingFormat.self, forKey: .recordingFormat) ?? .opus
        summarizationPrompt = try values.decodeIfPresent(String.self, forKey: .summarizationPrompt) ?? "Summarize this meeting with decisions, key points, and action items. Do not invent information."
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
