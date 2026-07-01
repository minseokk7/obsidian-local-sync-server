use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    routing::{get, post},
    Router, response::IntoResponse, Json,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, path::Path};
use tokio::{sync::{oneshot, Mutex}, fs};
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

#[derive(Deserialize)]
struct SyncMessage {
    #[serde(rename = "type")]
    msg_type: String,
    path: String,
    content: Option<String>,
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
        .layer(cors)
        .with_state(state.clone());

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

async fn ws_handler(State(state): State<Arc<ServerState>>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<ServerState>) {
    let sync_dir = Path::new("sync_vault");
    if !sync_dir.exists() {
        let _ = fs::create_dir_all(sync_dir).await;
    }

    state.emit_log("새로운 기기가 실시간 웹소켓에 연결되었습니다!");

    while let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if let Message::Text(text) = msg {
                if let Ok(sync_msg) = serde_json::from_str::<SyncMessage>(&text) {
                    let file_path = sync_dir.join(&sync_msg.path);
                    
                    if let Some(parent) = file_path.parent() {
                        let _ = fs::create_dir_all(parent).await;
                    }

                    match sync_msg.msg_type.as_str() {
                        "create" | "modify" => {
                            if let Some(content) = sync_msg.content {
                                if let Ok(_) = fs::write(&file_path, content).await {
                                    state.emit_log(&format!("[{}] {}", sync_msg.msg_type.to_uppercase(), sync_msg.path));
                                    
                                    // DB 기록
                                    if let Some(pool) = state.db.lock().await.as_ref() {
                                        let _ = sqlx::query("INSERT OR REPLACE INTO files (path, hash) VALUES (?, ?)")
                                            .bind(&sync_msg.path)
                                            .bind("hash_placeholder") // TODO: 실제 해시
                                            .execute(pool).await;
                                    }
                                } else {
                                    state.emit_log(&format!("[오류] 파일 저장 실패: {}", sync_msg.path));
                                }
                            }
                        },
                        "delete" => {
                            if file_path.exists() {
                                let _ = fs::remove_file(&file_path).await;
                                state.emit_log(&format!("[DELETE] {}", sync_msg.path));
                                
                                if let Some(pool) = state.db.lock().await.as_ref() {
                                    let _ = sqlx::query("DELETE FROM files WHERE path = ?")
                                        .bind(&sync_msg.path)
                                        .execute(pool).await;
                                }
                            }
                        },
                        _ => {}
                    }
                } else {
                    // JSON이 아닌 일반 문자열인 경우 (Ping 등)
                    if text.contains("Ping") {
                        state.emit_log("클라이언트 수동 동기화(Ping)를 감지했습니다.");
                    }
                }
            }
        } else {
            state.emit_log("기기 연결이 해제되었습니다.");
            break;
        }
    }
}
