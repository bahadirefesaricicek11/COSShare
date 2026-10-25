mod server;

use local_ip_address::local_ip;
use server::{start_file_server, ServerState, SharedFileInfo};
use std::path::PathBuf;
use std::process::Stdio;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

struct AppState {
    server_state: ServerState,
}

#[tauri::command]
async fn get_local_ip() -> Result<String, String> {
    let my_ip = local_ip().map_err(|e| format!("Yerel IP alınamadı: {}", e))?;
    Ok(my_ip.to_string())
}

#[tauri::command]
async fn register_file(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err("Dosya bulunamadı".into());
    }

    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let file_size = metadata.len();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    let size_formatted = format_bytes(file_size);
    let code = rand_code();

    let info = SharedFileInfo {
        path,
        name,
        size_formatted,
    };

    let mut files = state.server_state.shared_files.lock().unwrap();
    files.insert(code.clone(), info);

    Ok(code)
}

#[tauri::command]
async fn start_tunnel(_app: AppHandle, port: u16) -> Result<String, String> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", &format!("npx localtunnel --port {}", port)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn().map_err(|e| format!("Tünel başlatılamadı: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        if line.contains("https://") {
            for word in line.split_whitespace() {
                if word.starts_with("https://") {
                    return Ok(word.to_string());
                }
            }
        }
    }

    Err("Tünel bağlantısı zaman aşımına uğradı".into())
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn rand_code() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{:x}", nanos)[..6].to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let server_state = ServerState::default();
    let state_clone = server_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            
            // Background Axum Server
            tauri::async_runtime::spawn(async move {
                start_file_server(8080, state_clone, handle).await.unwrap();
            });

            // System Tray Setup
            let quit_i = MenuItem::with_id(app, "quit", "Quit COSShare", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Open COSShare", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("COSShare - P2P File Sharing")
                .icon(app.default_window_icon().unwrap().clone())
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

            // Intercept window close event to hide to tray instead of killing app
            let window = app.get_webview_window("main").unwrap();
            let win_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = win_clone.hide();
                }
            });

            Ok(())
        })
        .manage(AppState { server_state })
        .invoke_handler(tauri::generate_handler![register_file, start_tunnel, get_local_ip])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}