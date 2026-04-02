pub mod routes;
pub mod web_ui;
pub mod ws;

use axum::Router;
use tower_http::cors::CorsLayer;

use crate::chat::manager::ConversationManager;
use crate::filesdb::FilesDb;
use crate::llm::secrets::SharedSecrets;
use crate::people::PeopleManager;
use crate::session::SessionManager;
use crate::settings::SharedSettings;
use crate::tags::TagsManager;

use self::routes::AppState;

pub fn create_router(
    session_manager: SessionManager,
    people_manager: PeopleManager,
    settings: SharedSettings,
    files_db: FilesDb,
    tags_manager: TagsManager,
    conversation_manager: ConversationManager,
    llm_secrets: SharedSecrets,
    enable_web_ui: bool,
) -> Router {
    let state = AppState {
        session_manager,
        people_manager,
        settings,
        files_db,
        tags_manager,
        conversation_manager,
        llm_secrets,
    };

    // All API routes (REST + WebSocket) under /api
    let api_routes = Router::new()
        .merge(routes::session_routes())
        .merge(routes::conversation_routes())
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
