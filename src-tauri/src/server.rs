use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;

#[derive(Clone, Debug)]
pub struct SharedFileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size_formatted: String,
}

#[derive(Clone)]
pub struct ServerState {
    pub shared_files: Arc<Mutex<HashMap<String, SharedFileInfo>>>,
    pub app_handle: Option<AppHandle>,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            shared_files: Arc::new(Mutex::new(HashMap::new())),
            app_handle: None,
        }
    }
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct FileMetaResponse {
    name: String,
    sizeFormatted: String,
    isFolder: bool,
}

const DOWNLOAD_TEMPLATE: &str = include_str!("../../server/template.html");

pub async fn start_file_server(
    port: u16,
    mut state: ServerState,
    app_handle: AppHandle,
) -> Result<(), String> {
    state.app_handle = Some(app_handle);

    let app = Router::new()
        .route("/s/{code}", get(serve_download_page))
        .route("/api/meta/{code}", get(get_file_meta))
        .route("/download/{code}", get(download_file))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", addr, e))?;

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Ok(())
}

async fn serve_download_page(Path(_code): Path<String>) -> Response {
    let mut res = Html(DOWNLOAD_TEMPLATE).into_response();
    // Bypasses the localtunnel warning screen programmatically
    res.headers_mut().insert(
        "bypass-tunnel-reminder",
        HeaderValue::from_static("true"),
    );
    res
}

async fn get_file_meta(
    Path(code): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let files = state.shared_files.lock().unwrap();
    if let Some(info) = files.get(&code) {
        let mut res = Json(FileMetaResponse {
            name: info.name.clone(),
            sizeFormatted: info.size_formatted.clone(),
            isFolder: false,
        }).into_response();
        res.headers_mut().insert(
            "bypass-tunnel-reminder",
            HeaderValue::from_static("true"),
        );
        res
    } else {
        (StatusCode::NOT_FOUND, "File expired or not found").into_response()
    }
}

async fn download_file(
    Path(code): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let info = {
        let files = state.shared_files.lock().unwrap();
        files.get(&code).cloned()
    };

    let info = match info {
        Some(i) => i,
        None => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    if let Some(ref handle) = state.app_handle {
        let _ = handle.emit("download-started", info.name.clone());
    }

    let file = match File::open(&info.path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Cannot open file").into_response(),
    };

    let stream = ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);
    let content_disposition = format!("attachment; filename=\"{}\"", info.name);

    let mut res = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .body(body)
        .unwrap()
        .into_response();

    res.headers_mut().insert(
        "bypass-tunnel-reminder",
        HeaderValue::from_static("true"),
    );
    res
}