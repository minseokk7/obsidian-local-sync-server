use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Local;
use dav_server::{localfs::LocalFs, DavHandler};
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, Mutex};
use tower_http::cors::{Any, CorsLayer};

/// 접속 기기 정보
#[derive(Clone, Serialize)]
pub struct ClientInfo {
    /// 클라이언트 IP 주소
    pub ip: String,
    /// HTTP User-Agent 헤더 값 (기기/앱 식별)
    pub user_agent: String,
    /// 마지막 요청 시각 (로컬 시간)
    pub last_seen: String,
}

/// 서버 전역 상태
pub struct ServerState {
    /// 서버 종료 신호 채널
    pub shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub app_handle: AppHandle,
    /// 접속 기기 목록 (IP → ClientInfo)
    pub connected_clients: Mutex<HashMap<String, ClientInfo>>,
    /// Basic Auth 인증 정보 (username, password), None이면 인증 없음
    pub credentials: Mutex<Option<(String, String)>>,
}

impl ServerState {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            shutdown_tx: Mutex::new(None),
            app_handle,
            connected_clients: Mutex::new(HashMap::new()),
            credentials: Mutex::new(None),
        }
    }

    /// 프론트엔드로 로그 이벤트 전송
    pub fn emit_log(&self, msg: &str) {
        let _ = self.app_handle.emit("server-log", msg);
    }
}

/// WebDAV 서버 실행
pub async fn run_server(port: u16, custom_sync_dir: Option<String>, state: Arc<ServerState>) -> Result<(), String> {
    let sync_dir = if let Some(dir) = custom_sync_dir {
        std::path::PathBuf::from(dir)
    } else {
        let data_dir = state
            .app_handle
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| std::env::current_dir().unwrap());
        data_dir.join("sync_vault")
    };

    if !sync_dir.exists() {
        tokio::fs::create_dir_all(&sync_dir)
            .await
            .map_err(|e| format!("동기화 폴더 생성 실패: {}", e))?;
    }

    state.emit_log(&format!("WebDAV 저장 경로: {}", sync_dir.display()));

    let dav_handler = DavHandler::builder()
        .filesystem(LocalFs::new(&sync_dir, false, false, false))
        .locksystem(dav_server::memls::MemLs::new())
        .build_handler();

    let dav_handler = Arc::new(dav_handler);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    let app_state = (state.clone(), dav_handler);

    let app = Router::new()
        .route("/{*path}", any(webdav_handler))
        .route("/", any(webdav_handler))
        .layer(cors)
        .with_state(app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("포트 {}에 바인딩 실패: {}", port, e))?;

    let (tx, rx) = oneshot::channel();
    {
        let mut shutdown = state.shutdown_tx.lock().await;
        *shutdown = Some(tx);
    }

    state.emit_log(&format!("WebDAV 서버 시작됨 — 포트 {}", port));

    // ConnectInfo를 통해 클라이언트 IP를 추출하기 위해 with_connect_info 사용
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        rx.await.ok();
    })
    .await
    .map_err(|e| e.to_string())?;

    state.emit_log("서버가 중지되었습니다.");
    Ok(())
}

/// WebDAV 요청 처리기
async fn webdav_handler(
    State((server_state, dav_handler)): State<(Arc<ServerState>, Arc<DavHandler>)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let client_ip = addr.ip().to_string();

    // ── Basic Auth 검사 ──────────────────────────────────────────
    {
        let creds = server_state.credentials.lock().await;
        if let Some((username, password)) = creds.as_ref() {
            if !username.is_empty() {
                let auth_ok = req
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.strip_prefix("Basic "))
                    .and_then(|encoded| BASE64.decode(encoded).ok())
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .map(|cred_str| {
                        let mut parts = cred_str.splitn(2, ':');
                        let u = parts.next().unwrap_or("");
                        let p = parts.next().unwrap_or("");
                        u == username && p == password
                    })
                    .unwrap_or(false);

                if !auth_ok {
                    return (
                        StatusCode::UNAUTHORIZED,
                        [(header::WWW_AUTHENTICATE, "Basic realm=\"Obsidian Sync\"")],
                        "인증이 필요합니다.",
                    )
                        .into_response();
                }
            }
        }
    }

    // ── 접속 기기 추적 ───────────────────────────────────────────
    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Unknown")
        .to_string();

    {
        let mut clients = server_state.connected_clients.lock().await;
        let is_new = !clients.contains_key(&client_ip);

        clients.insert(
            client_ip.clone(),
            ClientInfo {
                ip: client_ip.clone(),
                user_agent: user_agent.clone(),
                last_seen: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            },
        );

        if is_new {
            // 새 기기가 접속했을 때 로그 + 이벤트 발송
            server_state.emit_log(&format!("🔌 새 기기 연결: {} — {}", client_ip, user_agent));
            let _ = server_state.app_handle.emit("clients-updated", ());
        }
    }

    // PROPFIND·OPTIONS는 로그 생략 (너무 자주 발생)
    if method != "OPTIONS" && method != "PROPFIND" {
        server_state.emit_log(&format!("[{}] {} ← {}", method, path, client_ip));
    }

    let dav_response = dav_handler.handle(req).await;
    // dav_server::body::Body → axum::body::Body 변환
    let (parts, body) = dav_response.into_parts();
    Response::from_parts(parts, axum::body::Body::new(body))
}
