use axum::{
    Router,
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "apps/webui/"]
struct WebAssets;

pub fn web_ui_routes() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/{*path}", get(static_file))
}

async fn index() -> impl IntoResponse {
    match WebAssets::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(content.data.as_ref()).to_string()).into_response(),
        None => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
    }
}

async fn static_file(Path(path): Path<String>) -> impl IntoResponse {
    match WebAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .body(axum::body::Body::from(content.data.to_vec()))
                .unwrap()
                .into_response()
        }
        None => {
            // SPA fallback: serve index.html for unmatched routes
            match WebAssets::get("index.html") {
                Some(content) => Html(String::from_utf8_lossy(content.data.as_ref()).to_string()).into_response(),
                None => (StatusCode::NOT_FOUND, "not found").into_response(),
            }
        }
    }
}
