use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::{get, post},
    Router, response::IntoResponse, Json,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{oneshot, Mutex};
use tower_http::cors::{Any, CorsLayer};
use tauri::{AppHandle, Emitter};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub struct ServerState {
    pub shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub app_handle: AppHandle,
    pub db: Mutex<Option<SqlitePool>>,
}

impl ServerState {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            shutdown_tx: Mutex::new(None),
            app_handle,
            db: Mutex::new(None),
        }
    }

    pub fn emit_log(&self, msg: &str) {
        let _ = self.app_handle.emit("server-log", msg);
    }
}

#[derive(Serialize, Deserialize)]
pub struct SyncStatus {
    version: u32,
    files: usize,
}

pub async fn run_server(port: u16, state: Arc<ServerState>) -> Result<(), String> {
    state.emit_log(&format!("Initializing SQLite database..."));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:obsidian_sync.db?mode=rwc")
        .await
        .map_err(|e| format!("Failed to connect to DB: {}", e))?;
    
    // Create tables if not exist
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            hash TEXT NOT NULL,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await.map_err(|e| format!("Failed to create tables: {}", e))?;

    {
        let mut db_guard = state.db.lock().await;
        *db_guard = Some(pool);
    }
    state.emit_log("Database initialized successfully.");

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/sync/pull", get(handle_pull))
        .route("/api/sync/push", post(handle_push))
        .route("/ws", get(ws_handler))
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| e.to_string())?;

    let (tx, rx) = oneshot::channel();
    {
        let mut shutdown = state.shutdown_tx.lock().await;
        *shutdown = Some(tx);
    }

    state.emit_log(&format!("Server listening on {}", addr));

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            rx.await.ok();
        })
        .await
        .map_err(|e| e.to_string())?;

    state.emit_log("Server stopped.");
    Ok(())
}

async fn handle_pull() -> impl IntoResponse {
    let status = SyncStatus { version: 1, files: 0 };
    Json(status)
}

async fn handle_push() -> impl IntoResponse {
    "OK"
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if let Message::Text(text) = msg {
                println!("Client sent str: {:?}", text);
                let _ = socket.send(Message::Text(format!("Echo: {}", text).into())).await;
            }
        } else {
            break;
        }
    }
}
