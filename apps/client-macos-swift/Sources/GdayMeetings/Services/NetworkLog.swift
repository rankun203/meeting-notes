import Foundation
import OSLog

/// Names what an outbound request carries, for the network log and Data Privacy
/// traces. `data` is a category such as "recorded audio", never a value.
struct NetworkTrace: Sendable {
    let provider: String
    let data: String
}

/// One `network` log entry per outbound request. Entries record the provider,
/// host and path, data category, bytes sent, and outcome. They never record
/// bodies, headers, query strings, credentials, or local file paths.
enum NetworkLog {
    static let logger = Logger(subsystem: CaptureLog.subsystem, category: "network")

    /// Host, port, and path only. Query strings and fragments can carry tokens or
    /// filenames, and user info can carry credentials, so all three are dropped.
    static func destination(_ url: URL?) -> String {
        guard let url, let host = url.host, !host.isEmpty else { return "unknown host" }
        let port = url.port.map { ":\($0)" } ?? ""
        return host + port + url.path
    }

    static func message(
        _ trace: NetworkTrace, method: String?, url: URL?, bytesSent: Int, outcome: String
    ) -> String {
        "\(trace.provider) · \(trace.data) · \(method ?? "GET") \(destination(url)) · \(bytesSent) bytes sent · \(outcome)"
    }

    static func outcome(_ response: URLResponse) -> String {
        (response as? HTTPURLResponse).map { "HTTP \($0.statusCode)" } ?? "no HTTP status"
    }

    /// Error descriptions can embed the failing URL, so log only the error code.
    static func outcome(_ error: Error) -> String {
        if error is CancellationError { return "cancelled" }
        if let error = error as? URLError {
            return error.code == .cancelled ? "cancelled" : "failed (URLError \(error.code.rawValue))"
        }
        return "failed (\(String(describing: type(of: error))))"
    }

    static func record(_ trace: NetworkTrace, request: URLRequest, bytesSent: Int, outcome: String, failed: Bool) {
        let text = message(
            trace, method: request.httpMethod, url: request.url, bytesSent: bytesSent, outcome: outcome)
        // Values are already reduced to non-secret categories and hosts; mark them
        // public so the in-app export shows them instead of "<private>".
        if failed {
            logger.error("\(text, privacy: .public)")
        }
        else {
            logger.notice("\(text, privacy: .public)")
        }
    }
}
