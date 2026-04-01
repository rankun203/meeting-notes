use axum::{
    Router,
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tracing::{info, warn};

use super::routes::AppState;

pub fn ws_routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.session_manager, state.tags_manager))
}

async fn handle_socket(
    socket: WebSocket,
    manager: crate::session::SessionManager,
    tags_manager: crate::tags::TagsManager,
) {
    info!("WebSocket client connected");

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Send initial state
    let hidden_tags = tags_manager.hidden_tag_names().await;
    let (sessions, total) = manager.list_sessions(1000, 0, &hidden_tags).await;
    let init_msg = serde_json::to_string(&json!({
        "type": "init",
        "data": { "sessions": sessions, "total": total }
    }))
    .unwrap();
    if ws_tx.send(Message::Text(init_msg.into())).await.is_err() {
        return;
    }

    // Subscribe to broadcast events
    let mut event_rx = manager.subscribe();

    // Forward broadcast events to WebSocket
    let mut send_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap();
                    if ws_tx.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("WebSocket client lagged, skipped {} events", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Receive messages from client (currently unused, ready for future commands)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(text) => {
                    info!("WebSocket received: {}", text);
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // If either task finishes, abort the other
    tokio::select! {
        _ = &mut send_task => { recv_task.abort(); }
        _ = &mut recv_task => { send_task.abort(); }
    }

    info!("WebSocket client disconnected");
}
