import AppKit
import Combine
import CryptoKit
import Foundation
import Network
import Security

private struct OAuthSession: Codable {
    let origin: URL
    let issuer: String
    let clientID: String
    let tokenEndpoint: URL
    let revocationEndpoint: URL?
    var accessToken: String
    var refreshToken: String?
    var expiry: Date
    let email: String?
}

@MainActor final class GdayServerService: ObservableObject {
    static let shared = GdayServerService()
    @Published private(set) var connected = false
    @Published private(set) var email: String?
    @Published private(set) var origin: String?
    private var credentials: OAuthSession?
    private var refreshTask: Task<OAuthSession, Error>?
    private var signingIn = false
    private let account = "gday-oauth"
    init() {
        if let saved = try? KeychainStore.get(account), let data = saved.data(using: .utf8),
            let value = try? JSONDecoder().decode(OAuthSession.self, from: data)
        {
            credentials = value
            connected = true
            email = value.email
            origin = value.origin.absoluteString
        }
    }
    func signIn(origin text: String) async throws {
        guard !signingIn else { throw ServiceError("A sign-in is already in progress.") }
        signingIn = true
        defer { signingIn = false }
        let base = try ServiceHTTP.origin(text)
        let discovery = try await ServiceHTTP.json(
            URLRequest(url: base.appendingPathComponent(".well-known/openid-configuration")),
            trace: Self.trace("sign-in discovery"))
        func endpoint(_ key: String) throws -> URL {
            guard let text = discovery[key] as? String, let url = URL(string: text), ServiceHTTP.sameOrigin(url, base),
                url.user == nil, url.password == nil
            else { throw ServiceError("Invalid or cross-origin OAuth \(key).") }
            return url
        }
        let issuer = try endpoint("issuer").absoluteString
        let authorization = try endpoint("authorization_endpoint")
        let tokenEndpoint = try endpoint("token_endpoint")
        let jwksURL = try endpoint("jwks_uri")
        let registration = try endpoint("registration_endpoint")
        let revocation = discovery["revocation_endpoint"] == nil ? nil : try endpoint("revocation_endpoint")
        let callback = try LoopbackCallback()
        defer { callback.cancel() }
        let port = try await callback.start()
        let redirect = "http://127.0.0.1:\(port)/callback"
        let scopes = "openid profile email offline_access meetings:read meetings:write"
        let registered = try await ServiceHTTP.json(
            ServiceHTTP.request(
                registration,
                json: [
                    "client_name": "Gday Meetings for Mac", "application_type": "native", "redirect_uris": [redirect],
                    "grant_types": ["authorization_code", "refresh_token"], "response_types": ["code"],
                    "token_endpoint_auth_method": "none", "scope": scopes,
                ]), trace: Self.trace("app registration"))
        guard let client = registered["client_id"] as? String else {
            throw ServiceError("The server did not register this app.")
        }
        let verifier = try Self.random()
        let state = try Self.random()
        let nonce = try Self.random()
        var url = URLComponents(url: authorization, resolvingAgainstBaseURL: false)!
        url.queryItems = [
            "client_id": client, "redirect_uri": redirect, "response_type": "code", "scope": scopes, "state": state,
            "nonce": nonce, "code_challenge": Self.base64(Data(SHA256.hash(data: Data(verifier.utf8)))),
            "code_challenge_method": "S256", "resource": base.absoluteString + "/api/platform",
        ].map { URLQueryItem(name: $0.key, value: $0.value) }
        // HIG Privacy: authentication occurs in the user's browser; the app never asks for the account password.
        // https://developer.apple.com/design/human-interface-guidelines/privacy
        guard NSWorkspace.shared.open(url.url!) else { throw ServiceError("Unable to open the sign-in browser.") }
        let returned = try await callback.receive(expectedState: state)
        if let responseIssuer = returned["iss"], responseIssuer != issuer {
            throw ServiceError("The sign-in issuer does not match.")
        }
        guard let code = returned["code"], !code.isEmpty else {
            throw ServiceError("Sign-in was declined or cancelled.")
        }
        let token = try await ServiceHTTP.json(
            ServiceHTTP.form(
                tokenEndpoint,
                [
                    "grant_type": "authorization_code", "code": code, "client_id": client, "redirect_uri": redirect,
                    "code_verifier": verifier, "resource": base.absoluteString + "/api/platform",
                ]), trace: Self.trace("sign-in token request"))
        guard let access = token["access_token"] as? String, let id = token["id_token"] as? String,
            (token["token_type"] as? String)?.lowercased() == "bearer"
        else { throw ServiceError("Invalid OAuth token response.") }
        let jwks = try await ServiceHTTP.json(URLRequest(url: jwksURL), trace: Self.trace("sign-in key request"))
        let claims = try Self.verifyIDToken(
            id, jwks: jwks, issuer: issuer, clientID: client, nonce: nonce, accessToken: access)
        let session = OAuthSession(
            origin: base, issuer: issuer, clientID: client, tokenEndpoint: tokenEndpoint,
            revocationEndpoint: revocation, accessToken: access, refreshToken: token["refresh_token"] as? String,
            expiry: Date().addingTimeInterval((token["expires_in"] as? Double) ?? 300),
            email: claims["email"] as? String)
        try Task.checkCancellation()
        try save(session)
    }
    func signOut() async throws {
        let old = credentials
        refreshTask?.cancel()
        refreshTask = nil
        try KeychainStore.delete(account)
        credentials = nil
        connected = false
        email = nil
        origin = nil
        if let old, let revoke = old.revocationEndpoint {
            for token in [old.accessToken, old.refreshToken].compactMap({ $0 }) {
                _ = try? await ServiceHTTP.data(
                    for: ServiceHTTP.form(revoke, ["token": token, "client_id": old.clientID]),
                    trace: Self.trace("sign-out token revocation"))
            }
        }
    }
    func authorizedRequest(_ path: String) async throws -> URLRequest {
        guard let session = credentials else { throw ServiceError("Sign in to Gday Meetings Server in Settings.") }
        let token = try await accessToken()
        var r = URLRequest(url: session.origin.appendingPathComponent(path))
        r.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        return r
    }
    private func accessToken() async throws -> String {
        guard let current = credentials else { throw ServiceError("Sign in to Gday Meetings Server.") }
        if current.expiry.timeIntervalSinceNow > 30 { return current.accessToken }
        if let running = refreshTask { return try await running.value.accessToken }
        let task = Task<OAuthSession, Error> {
            guard let refresh = current.refreshToken else {
                throw ServiceError("Your server sign-in expired. Sign in again.")
            }
            let token = try await ServiceHTTP.json(
                ServiceHTTP.form(
                    current.tokenEndpoint,
                    [
                        "grant_type": "refresh_token", "refresh_token": refresh, "client_id": current.clientID,
                        "resource": current.origin.absoluteString + "/api/platform",
                    ]), trace: Self.trace("sign-in token refresh"))
            guard let access = token["access_token"] as? String,
                (token["token_type"] as? String)?.lowercased() == "bearer"
            else { throw ServiceError("Invalid refreshed token.") }
            var updated = current
            updated.accessToken = access
            updated.refreshToken = token["refresh_token"] as? String ?? current.refreshToken
            updated.expiry = Date().addingTimeInterval((token["expires_in"] as? Double) ?? 300)
            try Task.checkCancellation()
            try self.save(updated)
            return updated
        }
        refreshTask = task
        defer { refreshTask = nil }
        return try await task.value.accessToken
    }
    private func save(_ value: OAuthSession) throws {
        try KeychainStore.set(String(decoding: JSONEncoder().encode(value), as: UTF8.self), for: account)
        credentials = value
        connected = true
        email = value.email
        origin = value.origin.absoluteString
    }
    static func base64(_ data: Data) -> String {
        data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
    static func decode64(_ text: String) throws -> Data {
        let padded =
            text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
            + String(repeating: "=", count: (4 - text.count % 4) % 4)
        guard let data = Data(base64Encoded: padded) else { throw ServiceError("Invalid signed token encoding.") }
        return data
    }
    private static func random() throws -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw ServiceError("Unable to generate secure sign-in credentials.")
        }
        return base64(Data(bytes))
    }
    static func verifyIDToken(
        _ token: String, jwks: [String: Any], issuer: String, clientID: String, nonce: String, accessToken: String
    ) throws -> [String: Any] {
        let parts = token.split(separator: ".").map(String.init)
        guard parts.count == 3,
            let header = try JSONSerialization.jsonObject(with: decode64(parts[0])) as? [String: Any],
            let claims = try JSONSerialization.jsonObject(with: decode64(parts[1])) as? [String: Any],
            let keys = jwks["keys"] as? [[String: Any]],
            let key = keys.first(where: { ($0["kid"] as? String) == (header["kid"] as? String) }),
            let alg = header["alg"] as? String
        else { throw ServiceError("Invalid identity token.") }
        let signed = Data("\(parts[0]).\(parts[1])".utf8)
        let signature = try decode64(parts[2])
        let valid: Bool
        if alg == "EdDSA", key["kty"] as? String == "OKP", key["crv"] as? String == "Ed25519",
            let x = key["x"] as? String
        {
            valid = try Curve25519.Signing.PublicKey(rawRepresentation: decode64(x)).isValidSignature(
                signature, for: signed)
        }
        else if alg == "RS256", key["kty"] as? String == "RSA", let n = key["n"] as? String,
            let e = key["e"] as? String
        {
            func der(_ tag: UInt8, _ bytes: Data) -> Data {
                let count = bytes.count
                var size: [UInt8] = []
                var v = count
                repeat {
                    size.insert(UInt8(v & 255), at: 0)
                    v >>= 8
                } while v > 0
                return Data([tag] + (count < 128 ? [UInt8(count)] : [0x80 | UInt8(size.count)] + size)) + bytes
            }
            func integer(_ bytes: Data) -> Data {
                der(2, (bytes.first.map { $0 >= 128 } == true ? Data([0]) : Data()) + bytes)
            }
            let data = try der(0x30, integer(decode64(n)) + integer(decode64(e)))
            guard
                let rsa = SecKeyCreateWithData(
                    data as CFData,
                    [kSecAttrKeyType: kSecAttrKeyTypeRSA, kSecAttrKeyClass: kSecAttrKeyClassPublic] as CFDictionary, nil
                )
            else { throw ServiceError("Invalid server signing key.") }
            valid = SecKeyVerifySignature(
                rsa, .rsaSignatureMessagePKCS1v15SHA256, signed as CFData, signature as CFData, nil)
        }
        else {
            throw ServiceError("The server uses an unsupported identity signing algorithm.")
        }
        let audiences = (claims["aud"] as? [String]) ?? (claims["aud"] as? String).map { [$0] } ?? []
        guard valid, claims["iss"] as? String == issuer, audiences.contains(clientID),
            claims["nonce"] as? String == nonce,
            let expiry = claims["exp"] as? Double, expiry > Date().timeIntervalSince1970,
            let subject = claims["sub"] as? String, !subject.isEmpty,
            audiences.count <= 1 || claims["azp"] as? String == clientID,
            claims["azp"] == nil || claims["azp"] as? String == clientID,
            ((claims["nbf"] as? Double) ?? 0) <= Date().timeIntervalSince1970 + 30
        else { throw ServiceError("The identity signature, issuer, audience, expiry, or nonce is invalid.") }
        if let hash = claims["at_hash"] as? String {
            let digest: Data =
                alg == "EdDSA"
                ? Data(SHA512.hash(data: Data(accessToken.utf8))) : Data(SHA256.hash(data: Data(accessToken.utf8)))
            guard base64(digest.prefix(digest.count / 2)) == hash else {
                throw ServiceError("The access token does not match the signed identity.")
            }
        }
        return claims
    }
}

private final class LoopbackCallback: @unchecked Sendable {
    private let listener: NWListener
    private let queue = DispatchQueue(label: "com.gdaymeetings.macos.oauth-callback")
    private var completion: CheckedContinuation<[String: String], Error>?
    private var expectedState = ""
    private var didStart = false
    init() throws {
        let parameters = NWParameters.tcp
        parameters.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: .any)
        listener = try NWListener(using: parameters)
    }
    func start() async throws -> UInt16 {
        try await withCheckedThrowingContinuation { continuation in
            listener.stateUpdateHandler = { state in
                guard !self.didStart else { return }
                switch state {
                case .ready:
                    self.didStart = true
                    continuation.resume(returning: self.listener.port!.rawValue)
                case .failed(let error):
                    self.didStart = true
                    continuation.resume(throwing: error)
                default: break
                }
            }
            listener.newConnectionHandler = { connection in self.handle(connection) }
            listener.start(queue: queue)
        }
    }
    func receive(expectedState: String) async throws -> [String: String] {
        try await withTaskCancellationHandler(
            operation: {
                try await withCheckedThrowingContinuation { continuation in
                    queue.async {
                        self.expectedState = expectedState
                        self.completion = continuation
                        self.queue.asyncAfter(deadline: .now() + 300) {
                            self.finish(.failure(ServiceError("Sign-in timed out. Try again.")))
                        }
                    }
                }
            }, onCancel: { self.cancel() })
    }
    func cancel() {
        queue.async {
            self.listener.cancel()
            self.finish(.failure(CancellationError()))
        }
    }
    private func finish(_ result: Result<[String: String], Error>) {
        let pending = completion
        completion = nil
        pending?.resume(with: result)
    }
    private func handle(_ connection: NWConnection) {
        connection.start(queue: queue)
        queue.asyncAfter(deadline: .now() + 10) { connection.cancel() }
        read(connection, accumulated: Data())
    }
    private func read(_ connection: NWConnection, accumulated: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 8192) { data, _, complete, error in
            guard let data, error == nil else {
                connection.cancel()
                return
            }
            let bytes = accumulated + data
            guard bytes.count <= 8192 else {
                connection.cancel()
                return
            }
            guard bytes.range(of: Data("\r\n\r\n".utf8)) != nil else {
                if complete {
                    connection.cancel()
                }
                else {
                    self.read(connection, accumulated: bytes)
                }
                return
            }
            guard let request = String(data: bytes, encoding: .utf8),
                let first = request.components(separatedBy: "\r\n").first
            else {
                connection.cancel()
                return
            }
            let pieces = first.split(separator: " ")
            guard pieces.count == 3, pieces[0] == "GET", let components = URLComponents(string: String(pieces[1])),
                components.path == "/callback"
            else {
                connection.cancel()
                return
            }
            var query: [String: String] = [:]
            for item in components.queryItems ?? [] {
                if query[item.name] != nil {
                    connection.cancel()
                    return
                }
                query[item.name] = item.value
            }
            guard query["state"] == self.expectedState, self.completion != nil else {
                connection.cancel()
                return
            }
            let body = "Sign-in received. You can return to Gday Meetings."
            let response =
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: \(body.utf8.count)\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n\(body)"
            connection.send(content: Data(response.utf8), completion: .contentProcessed { _ in connection.cancel() })
            self.finish(.success(query))
        }
    }
}
