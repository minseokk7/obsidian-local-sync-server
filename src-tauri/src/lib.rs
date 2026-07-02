mod server;

use server::{ClientInfo, ServerState};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

/// WebDAV 서버 시작
#[tauri::command]
async fn start_server(port: u16, sync_dir: Option<String>, state: State<'_, Arc<ServerState>>) -> Result<(), String> {
    let state_clone = (*state).clone();

    // 이미 실행 중인지 확인
    {
        let shutdown = state_clone.shutdown_tx.lock().await;
        if shutdown.is_some() {
            return Err("서버가 이미 실행 중입니다.".into());
        }
    }

    tokio::spawn(async move {
        if let Err(e) = server::run_server(port, sync_dir, state_clone.clone()).await {
            // 에러를 프론트엔드로 전달
            let _ = state_clone.app_handle.emit("server-error", e);
        }
    });

    Ok(())
}

/// WebDAV 서버 중지
#[tauri::command]
async fn stop_server(state: State<'_, Arc<ServerState>>) -> Result<(), String> {
    let mut shutdown = state.shutdown_tx.lock().await;
    if let Some(tx) = shutdown.take() {
        let _ = tx.send(());
        // 접속 기기 목록 초기화
        let mut clients = state.connected_clients.lock().await;
        clients.clear();
        Ok(())
    } else {
        Err("서버가 실행 중이지 않습니다.".into())
    }
}

/// 현재 접속 중인 기기 목록 반환
#[tauri::command]
async fn get_connected_clients(
    state: State<'_, Arc<ServerState>>,
) -> Result<Vec<ClientInfo>, String> {
    let clients = state.connected_clients.lock().await;
    Ok(clients.values().cloned().collect())
}

/// 로컬 네트워크 IP 주소 목록 반환 (실제 LAN IP만 — 가상 어댑터 제외)
#[tauri::command]
async fn get_local_ips() -> Result<Vec<String>, String> {
    use local_ip_address::list_afinet_netifas;
    let ifaces = list_afinet_netifas().map_err(|e| e.to_string())?;
    let ips: Vec<String> = ifaces
        .into_iter()
        .filter_map(|(name, ip)| {
            // 루프백 제외
            if ip.is_loopback() {
                return None;
            }
            let s = ip.to_string();
            // 실제 LAN 대역만 포함 (192.168.x.x 또는 10.x.x.x)
            // 172.16-31.x.x 는 WSL/Docker/Hyper-V 가상 어댑터가 많으므로 제외
            if s.starts_with("192.168.") || s.starts_with("10.") {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    Ok(ips)
}

/// Basic Auth 인증 정보 설정 (빈 문자열이면 인증 해제)
#[tauri::command]
async fn set_credentials(
    username: String,
    password: String,
    state: State<'_, Arc<ServerState>>,
) -> Result<(), String> {
    let mut creds = state.credentials.lock().await;
    if username.trim().is_empty() {
        *creds = None;
    } else {
        *creds = Some((username, password));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(ServerState::new(app.handle().clone())));

            // 시스템 트레이 설정
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let quit_i = MenuItem::with_id(app, "quit", "완전 종료", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "앱 열기", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // x 버튼 누를 때 트레이로 최소화 (숨김)
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    window_clone.hide().unwrap();
                    api.prevent_close();
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_connected_clients,
            get_local_ips,
            set_credentials,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 오류");
}
