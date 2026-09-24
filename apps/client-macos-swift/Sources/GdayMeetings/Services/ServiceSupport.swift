import Foundation
import Security

struct ServiceError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

enum KeychainStore {
    static let service = "app.gday.meetings.swift"
    static func get(_ account: String) throws -> String? {
        var query = base(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw ServiceError("Unable to read credentials from Keychain (\(status)).") }
        return String(data: data, encoding: .utf8)
    }
    static func set(_ value: String, for account: String) throws {
        let attributes = [kSecValueData as String: Data(value.utf8)]
        let status = SecItemUpdate(base(account) as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var query = base(account)
            query.merge(attributes) { _, new in new }
            query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let added = SecItemAdd(query as CFDictionary, nil)
            guard added == errSecSuccess else { throw ServiceError("Unable to save credentials to Keychain (\(added)).") }
        } else if status != errSecSuccess { throw ServiceError("Unable to update Keychain (\(status)).") }
    }
    static func delete(_ account: String) throws {
        let status = SecItemDelete(base(account) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw ServiceError("Unable to remove Keychain credentials (\(status)).") }
    }
    private static func base(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account]
    }
}

// Credentials must never follow redirects to another origin.
final class NoRedirectDelegate: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}
enum ServiceHTTP {
    static let session = URLSession(configuration: .ephemeral, delegate: NoRedirectDelegate(), delegateQueue: nil)
    static func origin(_ text: String) throws -> URL {
        guard let u = URL(string: text), let host = u.host, u.user == nil, u.password == nil,
              u.query == nil, u.fragment == nil, u.path.isEmpty || u.path == "/",
              u.scheme == "https" || (u.scheme == "http" && ["localhost", "127.0.0.1", "[::1]"].contains(host))
        else { throw ServiceError("Enter an HTTPS server origin. HTTP is supported only on localhost.") }
        return URL(string: "\(u.scheme!)://\(host)\(u.port.map { ":\($0)" } ?? "")")!
    }
    static func sameOrigin(_ a: URL, _ b: URL) -> Bool { a.scheme == b.scheme && a.host == b.host && a.port == b.port }
    static func json(_ request: URLRequest) async throws -> [String: Any] {
        let (data, response) = try await session.data(for: request)
        return try decode(data, response)
    }
    static func decode(_ data: Data, _ response: URLResponse) throws -> [String: Any] {
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw ServiceError("The server rejected the request (HTTP \((response as? HTTPURLResponse)?.statusCode ?? 0)). Check the server address and sign-in.")
        }
        guard let value = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw ServiceError("The server returned an invalid response.") }
        return value
    }
    static func request(_ url: URL, json: [String: Any]) throws -> URLRequest {
        var r = URLRequest(url: url); r.httpMethod = "POST"; r.timeoutInterval = 120
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        r.httpBody = try JSONSerialization.data(withJSONObject: json)
        return r
    }
    static func form(_ url: URL, _ values: [String: String]) -> URLRequest {
        var r = URLRequest(url: url); r.httpMethod = "POST"
        r.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._~"))
        r.httpBody = Data(values.sorted { $0.key < $1.key }.map { "\($0.key.addingPercentEncoding(withAllowedCharacters: allowed)!)=\($0.value.addingPercentEncoding(withAllowedCharacters: allowed)!)" }.joined(separator: "&").utf8)
        return r
    }
}

struct LLMMessage: Codable { let role: String; let content: String }
enum LLMService {
    static func complete(baseURL: String, apiKey: String, model: String, messages: [LLMMessage]) async throws -> String {
        guard let base = URL(string: baseURL), base.scheme == "https" || (base.scheme == "http" && ["localhost", "127.0.0.1", "[::1]"].contains(base.host ?? "")), base.user == nil, base.password == nil else { throw ServiceError("Use an HTTPS AI endpoint, or HTTP on localhost.") }
        let endpoint = base.path.hasSuffix("/chat/completions") ? base : base.appendingPathComponent("chat/completions")
        var r = try ServiceHTTP.request(endpoint, json: ["model": model, "messages": messages.map { ["role": $0.role, "content": $0.content] }, "stream": false])
        if !apiKey.isEmpty { r.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization") }
        let result = try await ServiceHTTP.json(r)
        guard let choices = result["choices"] as? [[String: Any]], let message = choices.first?["message"] as? [String: Any], let text = message["content"] as? String else { throw ServiceError("The AI provider returned no message.") }
        return text
    }
}
