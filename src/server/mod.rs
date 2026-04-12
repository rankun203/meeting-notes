pub mod routes;
pub mod web_ui;
pub mod ws;

use axum::Router;
use tower_http::cors::CorsLayer;

use self::routes::AppState;

pub fn create_router(state: AppState, enable_web_ui: bool) -> Router {
    // All API routes (REST + WebSocket) under /api
    let api_routes = Router::new()
        .merge(routes::session_routes())
        .merge(routes::conversation_routes())
        .merge(routes::claude_routes())
        .merge(ws::ws_routes());

    let mut app = Router::new()
        .nest("/api", api_routes)
        .layer(CorsLayer::permissive())
        .with_state(state);

    if enable_web_ui {
        app = app.merge(web_ui::web_ui_routes());
    }

    app
}
