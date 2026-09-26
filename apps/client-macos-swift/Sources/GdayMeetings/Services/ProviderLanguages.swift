import CryptoKit
import Foundation

struct ProviderLanguage: Codable, Equatable, Identifiable {
    let code: String
    let name: String
    var id: String { code }
}
struct ProviderLanguageCatalog: Equatable {
    let languages: [ProviderLanguage]
    let source: String
}
enum ProviderLanguageState: Equatable {
    case idle
    case loading
    case loaded(ProviderLanguageCatalog)
    case failed(String)
}
struct ProviderLanguageIdentity: Hashable {
    let providerID: UUID
    let fingerprint: String
    init(provider: ServiceProvider, account: String = "") {
        providerID = provider.id
        let fields = [
            provider.kind.rawValue, provider.endpoint, provider.model, provider.apiKey, account,
            String(provider.isEnabled), String(provider.enabledCapabilities.contains(.transcription)),
        ]
        fingerprint = SHA256.hash(data: Data(fields.joined(separator: "\n").utf8))
            .map { String(format: "%02x", $0) }.joined()
    }
}
protocol ProviderLanguageListing {
    func supportedLanguages() async throws -> ProviderLanguageCatalog
}

extension RunPodProvider: ProviderLanguageListing {
    func supportedLanguages() async throws -> ProviderLanguageCatalog {
        guard !provider.apiKey.isEmpty else { throw ServiceError("Enter the RunPod API key.") }
        var request = try ServiceHTTP.request(
            ProviderEndpoint.runpod(provider.endpoint).appendingPathComponent("runsync"),
            json: ["input": ["operation": "capabilities"]])
        request.setValue("Bearer \(provider.apiKey)", forHTTPHeaderField: "Authorization")
        let trace = NetworkTrace(provider: provider.name, data: "language list request")
        var response = try await ServiceHTTP.json(request, trace: trace)
        for poll in 0...15 {
            if response["status"] as? String == "COMPLETED" {
                guard let output = response["output"] as? [String: Any] else {
                    throw ServiceError(
                        "This worker did not return language capabilities. Update the Gday audio worker.")
                }
                return try ProviderLanguageService.workerCatalog(output, source: provider.name)
            }
            guard let status = response["status"] as? String, ["IN_QUEUE", "IN_PROGRESS"].contains(status),
                let id = response["id"] as? String, !id.isEmpty, !id.contains("/")
            else {
                throw ServiceError(
                    "This worker could not report its supported languages. Update the Gday audio worker or check its logs."
                )
            }
            guard poll < 15 else { break }
            try await Task.sleep(for: .seconds(2))
            let url = try ProviderEndpoint.runpod(provider.endpoint).appendingPathComponent("status")
                .appendingPathComponent(id)
            response = try await ServiceHTTP.json(ProviderEndpoint.authorized(url, key: provider.apiKey), trace: trace)
        }
        throw ServiceError("The worker has not returned its supported languages. Check again after it starts.")
    }
}

@MainActor enum ProviderLanguageService {
    static func catalog(for provider: ServiceProvider) async throws -> ProviderLanguageCatalog {
        guard provider.supports(.transcription) else {
            throw ServiceError("Enable Transcription for this provider to load its languages.")
        }
        switch provider.kind {
        case .runpod:
            return try await RunPodProvider(provider: provider).supportedLanguages()
        case .gdayWebsite:
            let server = GdayServerService.shared
            let origin = try ServiceHTTP.origin(provider.endpoint)
            guard server.connected,
                server.origin.flatMap(URL.init(string:)).map({ ServiceHTTP.sameOrigin($0, origin) }) == true
            else {
                throw ServiceError("Sign in to this Gday Meetings website to load its languages.")
            }
            let response = try await ServiceHTTP.json(
                server.authorizedRequest("api/platform/capabilities"),
                trace: .init(provider: provider.name, data: "language list request"))
            guard response["protocolVersion"] as? Int == 1 else {
                throw ServiceError(
                    "This website does not support language discovery. Update the Gday Meetings website.")
            }
            return try parseLanguages(response["transcriptionLanguages"], source: provider.name)
        case .openAICompatible, .filedrop:
            throw ServiceError("This provider does not support transcription.")
        }
    }

    nonisolated static func workerCatalog(_ response: [String: Any], source: String) throws -> ProviderLanguageCatalog {
        guard response["protocolVersion"] as? Int == 1,
            let transcription = response["transcription"] as? [String: Any]
        else {
            throw ServiceError("This worker does not support language discovery. Update the Gday audio worker.")
        }
        return try parseLanguages(transcription["languages"], source: source)
    }

    nonisolated static func parseLanguages(_ value: Any?, source: String) throws -> ProviderLanguageCatalog {
        guard let entries = value as? [[String: Any]], !entries.isEmpty, entries.count <= 256 else {
            throw ServiceError("The provider did not report any transcription languages.")
        }
        var codes = Set<String>()
        var languages: [ProviderLanguage] = []
        for entry in entries {
            guard let code = entry["code"] as? String, let name = entry["name"] as? String,
                TranscriptionLanguage.isExplicit(code), code.count <= 40,
                code.range(of: "^[a-z]{2,3}(?:-[a-z0-9]{2,8})*$", options: .regularExpression) != nil,
                !name.isEmpty, name.count <= 120, name == name.trimmingCharacters(in: .whitespacesAndNewlines),
                name.rangeOfCharacter(from: .controlCharacters) == nil,
                codes.insert(code.lowercased()).inserted
            else {
                throw ServiceError("The provider returned an invalid language list.")
            }
            languages.append(ProviderLanguage(code: code, name: name))
        }
        return ProviderLanguageCatalog(languages: languages, source: source)
    }
}
