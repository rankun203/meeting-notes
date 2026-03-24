pub mod routes;
pub mod web_ui;
pub mod ws;

use axum::Router;
use tower_http::cors::CorsLayer;

use crate::session::SessionManager;

pub fn create_router(session_manager: SessionManager, enable_web_ui: bool) -> Router {
    let mut app = Router::new()
        .merge(routes::session_routes())
        .merge(ws::ws_routes())
        .layer(CorsLayer::permissive())
        .with_state(session_manager);

    if enable_web_ui {
        app = app.merge(web_ui::web_ui_routes());
    }

    app
}
