pub mod routes;
pub mod web_ui;
pub mod ws;

use axum::Router;
use tower_http::cors::CorsLayer;

use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::settings::SharedSettings;
use crate::tunnel::TunnelManager;

use self::routes::AppState;

pub fn create_router(
    session_manager: SessionManager,
    people_manager: PeopleManager,
    tunnel_manager: TunnelManager,
    settings: SharedSettings,
    daemon_port: u16,
    enable_web_ui: bool,
) -> Router {
    let state = AppState {
        session_manager,
        people_manager,
        tunnel_manager,
        settings,
        daemon_port,
    };

    let mut app = Router::new()
        .merge(routes::session_routes())
        .merge(ws::ws_routes())
        .layer(CorsLayer::permissive())
        .with_state(state);

    if enable_web_ui {
        app = app.merge(web_ui::web_ui_routes());
    }

    app
}
