use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct DebugLogResponse {
    pub content: String,
    pub line_count: usize,
}

/// Retrieve the query debug log file content.
/// The log file is written to ~/.dbx-web/query-debug.log
pub async fn get_debug_log() -> impl IntoResponse {
    let data_dir = std::env::var("DBX_DATA_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home).join(".dbx-web")
    });
    let log_path = data_dir.join("query-debug.log");

    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let line_count = content.lines().count();
            (StatusCode::OK, Json(DebugLogResponse { content, line_count }))
        }
        Err(e) => {
            let (status, content) = if e.kind() == std::io::ErrorKind::NotFound {
                (StatusCode::NOT_FOUND, "Log file not found. Start the server with debug logging enabled.".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read log file: {e}"))
            };
            (status, Json(DebugLogResponse { content, line_count: 0 }))
        }
    }
}
