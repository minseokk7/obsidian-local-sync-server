mod server;

use std::sync::Arc;
use tauri::{State, Manager};
use server::ServerState;

#[tauri::command]
async fn start_server(port: u16, state: State<'_, Arc<ServerState>>) -> Result<(), String> {
    let state_clone = (*state).clone();
    
    // Check if server is already running
    {
        let shutdown = state_clone.shutdown_tx.lock().await;
        if shutdown.is_some() {
            return Err("Server is already running".into());
        }
    }
    
    tokio::spawn(async move {
        if let Err(e) = server::run_server(port, state_clone).await {
            println!("Server error: {}", e);
        }
    });
    
    Ok(())
}

#[tauri::command]
async fn stop_server(state: State<'_, Arc<ServerState>>) -> Result<(), String> {
    let mut shutdown = state.shutdown_tx.lock().await;
    if let Some(tx) = shutdown.take() {
        let _ = tx.send(());
        Ok(())
    } else {
        Err("Server is not running".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(Arc::new(ServerState::new(app.handle().clone())));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_server, stop_server])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
