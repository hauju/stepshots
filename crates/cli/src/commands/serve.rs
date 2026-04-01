use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;

use crate::commands::record;
use crate::config::StepshotsConfig;
use crate::error::CliError;

struct ServeState {
    output_dir: PathBuf,
    token: Option<String>,
    record_semaphore: Semaphore,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordRequest {
    config: StepshotsConfig,
    tutorial_name: String,
}

#[derive(Serialize)]
struct RecordResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadRequest {
    dir: String,
    stepshots_url: String,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    demo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn run(port: u16, output: PathBuf) -> Result<(), CliError> {
    let token = std::env::var("STEPSHOTS_TOKEN").ok();

    let state = Arc::new(ServeState {
        output_dir: output,
        token,
        record_semaphore: Semaphore::new(1),
    });

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/record", post(handle_record))
        .route("/api/upload", post(handle_upload))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    println!("Stepshots CLI server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| CliError::Other(format!("Failed to bind to {addr}: {e}")))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| CliError::Other(format!("Server error: {e}")))?;

    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn handle_record(
    State(state): State<Arc<ServeState>>,
    Json(req): Json<RecordRequest>,
) -> impl IntoResponse {
    // Acquire semaphore to prevent concurrent Chromium launches
    let _permit = match state.record_semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                StatusCode::CONFLICT,
                Json(RecordResponse {
                    ok: false,
                    dir: None,
                    error: Some("A recording is already in progress".into()),
                }),
            );
        }
    };

    let tutorial = match req.config.tutorials.get(&req.tutorial_name) {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(RecordResponse {
                    ok: false,
                    dir: None,
                    error: Some(format!(
                        "Tutorial '{}' not found in config",
                        req.tutorial_name
                    )),
                }),
            );
        }
    };

    let output_dir = state.output_dir.clone();
    std::fs::create_dir_all(&output_dir).ok();
    let output_path = output_dir.join(format!("{}.stepshot", req.tutorial_name));

    match record::record_tutorial(&req.config, &tutorial, &req.config.viewport, &output_path).await
    {
        Ok(()) => {
            let dir = output_dir
                .canonicalize()
                .unwrap_or(output_dir.clone())
                .display()
                .to_string();
            (
                StatusCode::OK,
                Json(RecordResponse {
                    ok: true,
                    dir: Some(dir),
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RecordResponse {
                ok: false,
                dir: None,
                error: Some(e.to_string()),
            }),
        ),
    }
}

async fn handle_upload(
    State(state): State<Arc<ServeState>>,
    Json(req): Json<UploadRequest>,
) -> impl IntoResponse {
    let token = match &state.token {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(UploadResponse {
                    ok: false,
                    demo_id: None,
                    error: Some(
                        "No API token configured. Set STEPSHOTS_TOKEN environment variable.".into(),
                    ),
                }),
            );
        }
    };

    // Find the .stepshot file in the directory
    let dir = PathBuf::from(&req.dir);
    let bundle_path = if dir.is_file() && dir.extension().is_some_and(|e| e == "stepshot") {
        dir.clone()
    } else {
        // Look for .stepshot files in the directory
        match find_stepshot_file(&dir) {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(UploadResponse {
                        ok: false,
                        demo_id: None,
                        error: Some(format!("No .stepshot file found in {}", req.dir)),
                    }),
                );
            }
        }
    };

    let file_path = bundle_path.display().to_string();
    let files = vec![file_path];

    match crate::commands::upload::run(&files, Some(&req.title), None, &req.stepshots_url, &token)
        .await
    {
        Ok(results) => {
            let first = results.into_iter().next();
            (
                StatusCode::OK,
                Json(UploadResponse {
                    ok: true,
                    demo_id: first.map(|r| r.demo_id),
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UploadResponse {
                ok: false,
                demo_id: None,
                error: Some(e.to_string()),
            }),
        ),
    }
}

fn find_stepshot_file(dir: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "stepshot") {
            return Some(path);
        }
    }
    None
}
