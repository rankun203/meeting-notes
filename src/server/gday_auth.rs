//! Native public OIDC client. Credentials stay in a private daemon-owned file.
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata, CoreTokenResponse},
    ClientId, CsrfToken, IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const SCOPES: &str = "openid profile email offline_access meetings:read meetings:write";

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    origin: String,
    issuer: String,
    client_id: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: i64,
    subject: String,
    email: Option<String>,
}
struct Pending {
    origin: String,
    client_id: String,
    redirect: String,
    verifier: String,
    nonce: String,
    cookie: String,
    metadata: CoreProviderMetadata,
    discovery: Value,
    created: Instant,
}
pub struct GdayAuth {
    path: PathBuf,
    session: Mutex<Option<Session>>,
    pending: Mutex<HashMap<String, Pending>>,
    http: reqwest::Client,
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("valid OIDC HTTP configuration")
}
fn origin(input: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(input).map_err(|_| "Enter a valid Gday URL")?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Use a HTTPS Gday origin (HTTP is allowed for localhost)".into());
    }
    Ok(url.origin().ascii_serialization())
}
fn endpoint(discovery: &Value, key: &str, issuer_origin: &str) -> Result<String, String> {
    let endpoint = discovery[key]
        .as_str()
        .ok_or_else(|| format!("Provider missing {key}"))?;
    let parsed = reqwest::Url::parse(endpoint).map_err(|_| "Invalid provider endpoint")?;
    // The hosted Gday issuer owns these credentials; never send them to another host.
    if parsed.origin().ascii_serialization() != issuer_origin {
        return Err("Provider endpoint does not belong to the Gday origin".into());
    }
    Ok(endpoint.to_string())
}
impl GdayAuth {
    pub fn load(data_dir: &Path) -> Arc<Self> {
        let path = data_dir.join("gday-auth.json");
        let session = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Arc::new(Self {
            path,
            session: Mutex::new(session),
            pending: Mutex::new(HashMap::new()),
            http: http_client(),
        })
    }
    fn save(&self, session: &Option<Session>) -> Result<(), String> {
        use std::io::Write;
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temporary)?;
            file.write_all(&serde_json::to_vec(session).map_err(std::io::Error::other)?)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|_| "Unable to save Gday credentials securely".into())
    }
    pub async fn connected_origin(&self) -> Option<String> {
        self.session
            .lock()
            .await
            .as_ref()
            .map(|session| session.origin.clone())
    }
    pub async fn status(&self) -> Value {
        match self.session.lock().await.as_ref() {
            Some(session) => {
                json!({"connected":true,"url":session.origin,"email":session.email,"subject":session.subject})
            }
            None => json!({"connected":false}),
        }
    }
    pub async fn begin(&self, base: &str, redirect: &str) -> Result<(String, String), String> {
        let base = origin(base)?;
        let discovery: Value = self
            .http
            .get(format!("{base}/.well-known/openid-configuration"))
            .send()
            .await
            .map_err(|_| "Gday discovery unavailable")?
            .error_for_status()
            .map_err(|_| "Gday discovery unavailable")?
            .json()
            .await
            .map_err(|_| "Invalid OIDC discovery")?;
        let issuer = endpoint(&discovery, "issuer", &base)?;
        for key in ["authorization_endpoint", "token_endpoint", "jwks_uri"] {
            endpoint(&discovery, key, &base)?;
        }
        if discovery.get("userinfo_endpoint").is_some() {
            endpoint(&discovery, "userinfo_endpoint", &base)?;
        }
        let registration = endpoint(&discovery, "registration_endpoint", &base)?;
        let metadata = CoreProviderMetadata::discover_async(
            IssuerUrl::new(issuer).map_err(|_| "Invalid issuer")?,
            &self.http,
        )
        .await
        .map_err(|_| "Unable to verify Gday OIDC discovery")?;
        let registered: Value = self
            .http
            .post(registration)
            .json(&json!({
                "client_name":"Meeting Notes Desktop", "application_type":"native",
                "redirect_uris":[redirect], "grant_types":["authorization_code","refresh_token"],
                "response_types":["code"], "token_endpoint_auth_method":"none", "scope":SCOPES
            }))
            .send()
            .await
            .map_err(|_| "Client registration unavailable")?
            .error_for_status()
            .map_err(|_| "Gday rejected public client registration")?
            .json()
            .await
            .map_err(|_| "Invalid client registration")?;
        let client_id = registered["client_id"]
            .as_str()
            .ok_or("Client registration has no client ID")?
            .to_string();
        let client = CoreClient::from_provider_metadata(
            metadata.clone(),
            ClientId::new(client_id.clone()),
            None,
        )
        .set_redirect_uri(RedirectUrl::new(redirect.to_owned()).map_err(|_| "Invalid callback")?);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for scope in SCOPES.split_whitespace().filter(|scope| *scope != "openid") {
            request = request.add_scope(Scope::new(scope.into()));
        }
        let (url, state, nonce) = request
            .set_pkce_challenge(challenge)
            .add_extra_param("resource", format!("{base}/api/platform"))
            .url();
        let cookie = CsrfToken::new_random().secret().to_string();
        let mut pending = self.pending.lock().await;
        pending.retain(|_, p| p.created.elapsed() < Duration::from_secs(600));
        if pending.len() >= 8 {
            return Err("Too many sign-in attempts; wait and try again".into());
        }
        pending.insert(
            state.secret().clone(),
            Pending {
                origin: base,
                client_id,
                redirect: redirect.into(),
                verifier: verifier.secret().clone(),
                nonce: nonce.secret().clone(),
                cookie: cookie.clone(),
                metadata,
                discovery,
                created: Instant::now(),
            },
        );
        Ok((url.into(), cookie))
    }
    pub async fn complete(
        &self,
        state: &str,
        code: &str,
        cookie: &str,
        response_issuer: Option<&str>,
    ) -> Result<(), String> {
        let mut guard = self.pending.lock().await;
        let candidate = guard
            .get(state)
            .ok_or("Sign-in expired or state does not match")?;
        if candidate.cookie != cookie || candidate.created.elapsed() >= Duration::from_secs(600) {
            return Err("Sign-in browser session does not match".into());
        }
        let pending = guard.remove(state).expect("validated pending login");
        drop(guard);
        let issuer = pending.metadata.issuer().as_str().to_string();
        if response_issuer.is_some_and(|value| value != issuer) {
            return Err("Authorization issuer does not match".into());
        }
        let token_endpoint = endpoint(&pending.discovery, "token_endpoint", &pending.origin)?;
        let token: CoreTokenResponse = self
            .http
            .post(&token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", pending.client_id.as_str()),
                ("redirect_uri", pending.redirect.as_str()),
                ("code_verifier", pending.verifier.as_str()),
                ("resource", &format!("{}/api/platform", pending.origin)),
            ])
            .send()
            .await
            .map_err(|_| "Token exchange unavailable")?
            .error_for_status()
            .map_err(|_| "Gday rejected sign-in code")?
            .json()
            .await
            .map_err(|_| "Invalid token response")?;
        let client = CoreClient::from_provider_metadata(
            pending.metadata,
            ClientId::new(pending.client_id.clone()),
            None,
        );
        let id_token = token.id_token().ok_or("Provider returned no ID token")?;
        let verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &Nonce::new(pending.nonce))
            .map_err(|_| "Invalid ID token signature, issuer, audience, expiry or nonce")?;
        if let Some(expected) = claims.access_token_hash() {
            let actual = openidconnect::AccessTokenHash::from_token(
                token.access_token(),
                id_token
                    .signing_alg()
                    .map_err(|_| "Invalid signing algorithm")?,
                id_token
                    .signing_key(&verifier)
                    .map_err(|_| "Invalid signing key")?,
            )
            .map_err(|_| "Invalid access token hash")?;
            if actual != *expected {
                return Err("Access token does not match ID token".into());
            }
        }
        let mut email = claims.email().map(|email| email.as_str().to_string());
        if email.is_none() {
            // UserInfo is optional, but any profile must belong to the verified ID subject.
            if let Ok(request) =
                client.user_info(token.access_token().clone(), Some(claims.subject().clone()))
            {
                let profile: Result<openidconnect::core::CoreUserInfoClaims, _> =
                    request.request_async(&self.http).await;
                if let Ok(profile) = profile {
                    email = profile.email().map(|email| email.as_str().to_string());
                }
            }
        }
        let session = Session {
            origin: pending.origin.clone(),
            issuer,
            client_id: pending.client_id,
            token_endpoint,
            revocation_endpoint: pending
                .discovery
                .get("revocation_endpoint")
                .map(|_| endpoint(&pending.discovery, "revocation_endpoint", &pending.origin))
                .transpose()?,
            access_token: token.access_token().secret().clone(),
            refresh_token: token.refresh_token().map(|value| value.secret().clone()),
            expires_at: chrono::Utc::now().timestamp()
                + token
                    .expires_in()
                    .unwrap_or(Duration::from_secs(300))
                    .as_secs() as i64,
            subject: claims.subject().as_str().into(),
            email,
        };
        let mut saved = self.session.lock().await;
        self.save(&Some(session.clone()))?;
        *saved = Some(session);
        Ok(())
    }
    pub async fn access_token(&self, expected_origin: &str) -> Result<String, String> {
        let mut saved = self.session.lock().await;
        let session = saved.as_mut().ok_or("Sign in to Gday Meetings")?;
        if session.origin != expected_origin {
            return Err("Sign in to the Gday server that owns this task".into());
        }
        if session.expires_at <= chrono::Utc::now().timestamp() + 30 {
            let refresh = session
                .refresh_token
                .as_deref()
                .ok_or("Gday sign-in expired; sign in again")?;
            let token: CoreTokenResponse = self
                .http
                .post(&session.token_endpoint)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("client_id", session.client_id.as_str()),
                    ("resource", &format!("{}/api/platform", session.origin)),
                ])
                .send()
                .await
                .map_err(|_| "Gday token refresh unavailable")?
                .error_for_status()
                .map_err(|_| "Gday sign-in expired or revoked; sign in again")?
                .json()
                .await
                .map_err(|_| "Invalid refreshed token")?;
            session.access_token = token.access_token().secret().clone();
            if let Some(refresh) = token.refresh_token() {
                session.refresh_token = Some(refresh.secret().clone());
            }
            session.expires_at = chrono::Utc::now().timestamp()
                + token
                    .expires_in()
                    .unwrap_or(Duration::from_secs(300))
                    .as_secs() as i64;
            self.save(&saved)?;
        }
        Ok(saved.as_ref().unwrap().access_token.clone())
    }
    pub async fn logout(&self) -> Result<(), String> {
        let mut saved = self.session.lock().await;
        if let Some(session) = saved.as_ref() {
            if let Some(endpoint) = &session.revocation_endpoint {
                // Local logout still succeeds when the remote issuer is offline.
                for token in [Some(&session.access_token), session.refresh_token.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    let _ = self
                        .http
                        .post(endpoint)
                        .form(&[("token", token.as_str()), ("client_id", &session.client_id)])
                        .send()
                        .await;
                }
            }
        }
        self.save(&None)?;
        *saved = None;
        self.pending.lock().await.clear();
        Ok(())
    }
}

fn request_origin(headers: &HeaderMap) -> Result<String, String> {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .ok_or("Missing local host")?;
    let local = origin(&format!("http://{host}"))?;
    if headers.get("origin").and_then(|v| v.to_str().ok()) != Some(local.as_str()) {
        return Err("Gday sign-in must start from this local app".into());
    }
    Ok(local)
}
fn failure(message: String) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error":message}))).into_response()
}
async fn status(State(auth): State<Arc<GdayAuth>>) -> Json<Value> {
    Json(auth.status().await)
}
async fn login(
    State(auth): State<Arc<GdayAuth>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let local = match request_origin(&headers) {
        Ok(value) => value,
        Err(error) => return failure(error),
    };
    let base = body["url"].as_str().unwrap_or_default();
    match auth.begin(base, &format!("{local}/api/gday/auth/callback")).await {
        Ok((url, cookie)) => ([ ("set-cookie", format!("gday-login={cookie}; HttpOnly; SameSite=Lax; Path=/api/gday/auth; Max-Age=600")), ("cache-control", "no-store".into()) ], Json(json!({"authorization_url":url}))).into_response(),
        Err(error) => failure(error),
    }
}
async fn callback(
    State(auth): State<Arc<GdayAuth>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let cookie = headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| *key == "gday-login")
        .map(|(_, value)| value)
        .unwrap_or_default();
    let result = auth
        .complete(
            query.get("state").map(String::as_str).unwrap_or_default(),
            query.get("code").map(String::as_str).unwrap_or_default(),
            cookie,
            query.get("iss").map(String::as_str),
        )
        .await;
    match result {
        Ok(()) => (
            [(
                "set-cookie",
                "gday-login=; HttpOnly; SameSite=Lax; Path=/api/gday/auth; Max-Age=0",
            )],
            Redirect::to("/?gday=connected"),
        )
            .into_response(),
        Err(error) => failure(error),
    }
}
async fn logout(State(auth): State<Arc<GdayAuth>>, headers: HeaderMap) -> Response {
    if let Err(error) = request_origin(&headers) {
        return failure(error);
    }
    match auth.logout().await {
        Ok(()) => Json(json!({"connected":false})).into_response(),
        Err(error) => failure(error),
    }
}
pub fn routes<S: Clone + Send + Sync + 'static>(auth: Arc<GdayAuth>) -> Router<S> {
    Router::new()
        .route("/gday/auth/status", get(status))
        .route("/gday/auth/login", post(login))
        .route("/gday/auth/callback", get(callback))
        .route("/gday/auth/logout", post(logout))
        .with_state(auth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Bytes, extract::Form};
    use openidconnect::{
        core::{
            CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
            CoreRsaPrivateSigningKey,
        },
        Audience, EmptyAdditionalClaims, JsonWebKeyId, PkceCodeVerifier, PrivateSigningKey,
        StandardClaims, SubjectIdentifier,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockProvider {
        origin: String,
        key: CoreRsaPrivateSigningKey,
        grant: Mutex<Option<HashMap<String, String>>>,
        submissions: Mutex<Vec<Value>>,
        refreshes: AtomicUsize,
        uploads: AtomicUsize,
    }
    async fn metadata(State(state): State<Arc<MockProvider>>) -> Json<Value> {
        let issuer = format!("{}/api/auth", state.origin);
        Json(
            json!({"issuer":issuer,"authorization_endpoint":format!("{}/authorize",state.origin),
            "token_endpoint":format!("{}/token",state.origin),"registration_endpoint":format!("{}/register",state.origin),
            "revocation_endpoint":format!("{}/revoke",state.origin),"jwks_uri":format!("{}/jwks",state.origin),
            "response_types_supported":["code"],"subject_types_supported":["public"],"userinfo_endpoint":format!("{}/userinfo",state.origin),
            "id_token_signing_alg_values_supported":["RS256"],"scopes_supported":SCOPES.split_whitespace().collect::<Vec<_>>() }),
        )
    }
    async fn register(Json(body): Json<Value>) -> Json<Value> {
        assert_eq!(body["application_type"], "native");
        assert_eq!(body["token_endpoint_auth_method"], "none");
        Json(json!({"client_id":"desktop-client"}))
    }
    async fn authorize(
        State(state): State<Arc<MockProvider>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Redirect {
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["resource"], format!("{}/api/platform", state.origin));
        assert!(query["scope"].contains("meetings:write"));
        let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("state", &query["state"])
            .append_pair("code", "test-code");
        *state.grant.lock().await = Some(query);
        Redirect::to(callback.as_str())
    }
    async fn token(
        State(state): State<Arc<MockProvider>>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Json<Value> {
        assert_eq!(form["client_id"], "desktop-client");
        assert_eq!(form["resource"], format!("{}/api/platform", state.origin));
        if form["grant_type"] == "refresh_token" {
            assert_eq!(form["refresh_token"], "refresh-one");
            state.refreshes.fetch_add(1, Ordering::SeqCst);
            return Json(
                json!({"access_token":"access-two","refresh_token":"refresh-two","token_type":"Bearer","expires_in":3600}),
            );
        }
        let grant = state.grant.lock().await.take().unwrap();
        assert_eq!(form["code"], "test-code");
        assert_eq!(form["redirect_uri"], grant["redirect_uri"]);
        let challenge = PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(
            form["code_verifier"].clone(),
        ));
        assert_eq!(challenge.as_str(), grant["code_challenge"]);
        let claims = CoreIdTokenClaims::new(
            IssuerUrl::new(format!("{}/api/auth", state.origin)).unwrap(),
            vec![Audience::new("desktop-client".into())],
            chrono::Utc::now() + chrono::Duration::minutes(5),
            chrono::Utc::now(),
            StandardClaims::new(SubjectIdentifier::new("test-user".into())),
            EmptyAdditionalClaims {},
        )
        .set_nonce(Some(Nonce::new(grant["nonce"].clone())));
        let access = openidconnect::AccessToken::new("access-one".into());
        let id_token = CoreIdToken::new(
            claims,
            &state.key,
            CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
            Some(&access),
            None,
        )
        .unwrap();
        Json(
            json!({"id_token":id_token.to_string(),"access_token":"access-one","refresh_token":"refresh-one","token_type":"Bearer","expires_in":3600}),
        )
    }
    async fn upload(
        State(state): State<Arc<MockProvider>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Json<Value> {
        assert_eq!(headers["authorization"], "Bearer access-two");
        assert_eq!(body.as_ref(), b"audio fixture");
        state.uploads.fetch_add(1, Ordering::SeqCst);
        Json(json!({"url":"/d/audio.opus"}))
    }
    async fn task(
        State(state): State<Arc<MockProvider>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(headers["authorization"], "Bearer access-two");
        assert_eq!(body["execute"], true);
        assert_eq!(body["executionOptions"]["language"], "en");
        assert_eq!(
            body["inputs"][0]["url"],
            format!("{}/d/audio.opus", state.origin)
        );
        let mut requests = state.submissions.lock().await;
        requests.push(body);
        if requests.len() == 1 {
            // Simulate a successful server submission whose acknowledgement was lost/malformed.
            Json(json!({}))
        } else {
            assert_eq!(
                requests[0], requests[1],
                "resume must reuse exact URLs/options/idempotency key"
            );
            Json(json!({"id":"durable-task","status":"RUNNING"}))
        }
    }
    async fn task_output(headers: HeaderMap) -> Json<Value> {
        assert_eq!(headers["authorization"], "Bearer access-two");
        Json(
            json!({"status":"COMPLETED","outputs":[{"type":"TRANSCRIPT_OUTPUT","body":{
                "language":"en","model":"gday-server-model","tracks":{"mic":{"source_type":"mic","duration_secs":1,
                    "segments":[{"start":0,"end":1,"text":"Durable Gday transcript","speaker":null,"words":[]}],"speaker_embeddings":{}}}
            }}]}),
        )
    }

    #[tokio::test]
    async fn oidc_login_refresh_restart_upload_and_durable_transcription() {
        let directory =
            std::env::temp_dir().join(format!("gday-oidc-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(MockProvider {
            origin: base.clone(),
            key: CoreRsaPrivateSigningKey::from_pem(
                include_str!("../../tests/fixtures/gday-test-only-rsa.pem"),
                Some(JsonWebKeyId::new("test-key".into())),
            )
            .unwrap(),
            grant: Mutex::new(None),
            submissions: Mutex::new(Vec::new()),
            refreshes: AtomicUsize::new(0),
            uploads: AtomicUsize::new(0),
        });
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(metadata))
            .route("/api/auth/.well-known/openid-configuration", get(metadata))
            .route("/api/.well-known/openid-configuration", get(metadata))
            .route("/register", post(register))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .route(
                "/jwks",
                get(|State(state): State<Arc<MockProvider>>| async move {
                    Json(CoreJsonWebKeySet::new(vec![state
                        .key
                        .as_verification_key()]))
                }),
            )
            .route(
                "/userinfo",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers["authorization"], "Bearer access-one");
                    Json(json!({"sub":"test-user","email":"reader@example.test"}))
                }),
            )
            .route("/revoke", post(|| async { StatusCode::OK }))
            .route("/upload", post(upload))
            .route("/api/platform/tasks", post(task))
            .route("/api/platform/tasks/durable-task", get(task_output))
            .route(
                "/api/platform/capabilities",
                get(|| async { Json(json!({"durableTasks":true,"transcription":true})) }),
            )
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let auth = GdayAuth::load(&directory);
        let local_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_origin = format!("http://{}", local_listener.local_addr().unwrap());
        let local_app = Router::new().nest("/api", routes(auth.clone()));
        let local_server = tokio::spawn(async move {
            axum::serve(local_listener, local_app).await.unwrap();
        });
        let http = http_client();
        let login = http
            .post(format!("{local_origin}/api/gday/auth/login"))
            .header("origin", &local_origin)
            .json(&json!({"url":base}))
            .send()
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let browser_cookie = login.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let cookie = browser_cookie.strip_prefix("gday-login=").unwrap();
        let login: Value = login.json().await.unwrap();
        let authorization = http
            .get(login["authorization_url"].as_str().unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(authorization.status(), StatusCode::SEE_OTHER);
        let callback =
            reqwest::Url::parse(authorization.headers()["location"].to_str().unwrap()).unwrap();
        let params: HashMap<String, String> = callback
            .query_pairs()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        assert!(auth
            .complete(&params["state"], &params["code"], "wrong-browser", None)
            .await
            .is_err());
        let completed = http
            .get(callback.clone())
            .header("cookie", &browser_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(completed.status(), StatusCode::SEE_OTHER);
        assert!(
            auth.complete(&params["state"], &params["code"], &cookie, None)
                .await
                .is_err(),
            "authorization code callback cannot replay"
        );
        assert_eq!(auth.status().await["email"], "reader@example.test");
        assert!(!auth.status().await.to_string().contains("access-one"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&auth.path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        {
            let mut saved = auth.session.lock().await;
            saved.as_mut().unwrap().expires_at = 0;
            auth.save(&saved).unwrap();
        }
        let auth = GdayAuth::load(&directory);
        // Concurrent requests serialize rotation: only one refresh consumes the refresh token.
        let (first, second) = tokio::join!(auth.access_token(&base), auth.access_token(&base));
        assert_eq!(first.unwrap(), "access-two");
        assert_eq!(second.unwrap(), "access-two");
        assert_eq!(state.refreshes.load(Ordering::SeqCst), 1);
        assert!(auth.access_token("https://another.example").await.is_err());

        let recordings = directory.join("recordings");
        let manager = crate::session::SessionManager::new(recordings.clone());
        let info = manager
            .create_session(crate::session::config::SessionConfig::default())
            .await;
        let session_dir = manager.session_dir(&info.id);
        std::fs::write(session_dir.join("mic.opus"), b"audio fixture").unwrap();
        let sources = vec![crate::session::session::SourceMetadata {
            filename: "mic.opus".into(),
            source_type: crate::audio::source::SourceType::Mic,
            source_label: "mic".into(),
            channels: 1,
            raw_sample_rate: 48000,
        }];
        let people = crate::people::PeopleManager::new(&directory);
        let files = crate::filesdb::FilesDb::new(recordings.clone());
        let first = super::super::routes::run_transcription_pipeline(
            &info.id,
            &session_dir,
            "en",
            &sources,
            "",
            "",
            "",
            "",
            false,
            false,
            0.75,
            &manager,
            &people,
            &files,
            Some(auth.clone()),
        )
        .await;
        assert!(first.is_err());
        let saved = std::fs::read_to_string(session_dir.join("metadata.json")).unwrap();
        assert!(saved.contains("idempotency_key"));
        assert!(!saved.contains("access-two"));
        let manager = crate::session::SessionManager::new(recordings);
        manager.load_from_disk().await;
        super::super::routes::run_transcription_pipeline(
            &info.id,
            &session_dir,
            "en",
            &sources,
            "",
            "",
            "",
            "",
            false,
            false,
            0.75,
            &manager,
            &people,
            &files,
            Some(auth.clone()),
        )
        .await
        .unwrap();
        assert_eq!(state.uploads.load(Ordering::SeqCst), 1);
        assert_eq!(state.submissions.lock().await.len(), 2);
        assert!(std::fs::read_to_string(session_dir.join("transcript.json"))
            .unwrap()
            .contains("Durable Gday transcript"));
        assert!(manager.get_pending_extractions().await.is_empty());
        auth.logout().await.unwrap();
        assert!(auth.access_token(&base).await.is_err());
        assert_eq!(
            GdayAuth::load(&directory).status().await["connected"],
            false
        );
        let (authorize_url, cookie) = auth
            .begin(&base, "http://127.0.0.1:8765/api/gday/auth/callback")
            .await
            .unwrap();
        let authorization = http.get(authorize_url).send().await.unwrap();
        let callback =
            reqwest::Url::parse(authorization.headers()["location"].to_str().unwrap()).unwrap();
        let query: HashMap<String, String> = callback
            .query_pairs()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        auth.pending
            .lock()
            .await
            .get_mut(&query["state"])
            .unwrap()
            .nonce = "nonce-from-another-login".into();
        assert!(auth
            .complete(&query["state"], &query["code"], &cookie, None)
            .await
            .unwrap_err()
            .contains("nonce"));
        assert_eq!(auth.status().await["connected"], false);
        local_server.abort();
        server.abort();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn local_login_rejects_cross_origin_browser_requests_and_insecure_remote_servers() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "127.0.0.1:8765".parse().unwrap());
        headers.insert("origin", "https://evil.example".parse().unwrap());
        assert!(request_origin(&headers).is_err());
        headers.insert("origin", "http://127.0.0.1:8765".parse().unwrap());
        assert!(request_origin(&headers).is_ok());
        assert!(origin("http://remote.example").is_err());
        assert!(origin("https://user:secret@example.com").is_err());
    }
    /// Opt-in cross-repository contract check. The fixture must use only a disposable
    /// database and the synthetic account below; never point this at a deployment.
    #[tokio::test]
    #[ignore = "requires a disposable Gday Better Auth fixture at GDAY_TEST_ORIGIN"]
    async fn maintained_gday_provider_contract() {
        let base = std::env::var("GDAY_TEST_ORIGIN").expect("set local fixture origin");
        assert_eq!(
            reqwest::Url::parse(&base).unwrap().host_str(),
            Some("127.0.0.1")
        );
        let directory =
            std::env::temp_dir().join(format!("gday-real-provider-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let auth = GdayAuth::load(&directory);
        let (authorize_url, browser) = auth
            .begin(&base, "http://127.0.0.1:8765/api/gday/auth/callback")
            .await
            .unwrap();
        let http = http_client();
        let login = http.post(format!("{base}/api/auth/sign-in/gday")).header("origin", &base)
            .json(&json!({"email":"rust-integration@example.test","password":"synthetic-test-password-not-production"}))
            .send().await.unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookies = login
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|value| value.to_str().unwrap().split(';').next().unwrap())
            .collect::<Vec<_>>()
            .join("; ");
        let authorized = http
            .get(authorize_url)
            .header("cookie", &cookies)
            .send()
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::FOUND);
        let consent_url = reqwest::Url::parse(&base)
            .unwrap()
            .join(authorized.headers()["location"].to_str().unwrap())
            .unwrap();
        assert_eq!(consent_url.path(), "/consent", "{}", consent_url);
        let consent = http
            .post(format!("{base}/api/auth/oauth2/consent"))
            .header("origin", &base)
            .header("cookie", &cookies)
            .json(&json!({"accept":true,"oauth_query":consent_url.query().unwrap()}))
            .send()
            .await
            .unwrap();
        assert_eq!(consent.status(), StatusCode::OK);
        let consent: Value = consent.json().await.unwrap();
        let callback = reqwest::Url::parse(
            consent["redirect_uri"]
                .as_str()
                .or_else(|| consent["url"].as_str())
                .unwrap(),
        )
        .unwrap();
        let query: HashMap<String, String> = callback
            .query_pairs()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        auth.complete(
            &query["state"],
            &query["code"],
            &browser,
            query.get("iss").map(String::as_str),
        )
        .await
        .unwrap();
        let status = auth.status().await;
        assert_eq!(status["connected"], true);
        assert!(!status["subject"].as_str().unwrap().is_empty());
        if !status["email"].is_null() {
            assert_eq!(status["email"], "rust-integration@example.test");
        }
        for refresh in [false, true] {
            if refresh {
                auth.session.lock().await.as_mut().unwrap().expires_at = 0;
            }
            let token = auth.access_token(&base).await.unwrap();
            let capabilities = http
                .get(format!("{base}/api/platform/capabilities"))
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            assert_eq!(
                capabilities.status(),
                StatusCode::OK,
                "real provider token must authorize platform (refresh={refresh})"
            );
            assert_eq!(
                capabilities.json::<Value>().await.unwrap()["durableTasks"],
                true
            );
        }
        auth.logout().await.unwrap();
        assert!(auth.access_token(&base).await.is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
