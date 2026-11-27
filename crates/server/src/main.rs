//! `fetchly-server` — hosts the Fetchly web build and exposes `POST /api/proxy`,
//! which executes requests server-side so the browser app isn't limited by CORS.
//!
//! Usage: fetchly-server [--host 127.0.0.1] [--port 8080] [--dist ./dist]
//! Env:   FETCHLY_PROXY_TOKEN=secret  → require `X-Fetchly-Token: secret` on proxy calls.

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use fetchly_core::http::{direct, PreparedRequest};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    services::{ServeDir, ServeFile},
};

struct Config {
    token: Option<String>,
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

#[tokio::main]
async fn main() {
    let host = arg("--host").unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = arg("--port").or_else(|| std::env::var("PORT").ok()).and_then(|p| p.parse().ok()).unwrap_or(8080);
    let dist = PathBuf::from(arg("--dist").unwrap_or_else(|| "dist".into()));
    let token = std::env::var("FETCHLY_PROXY_TOKEN").ok().filter(|t| !t.is_empty());

    if !dist.join("index.html").exists() {
        eprintln!("warning: {} has no index.html — build the web app first with `dx build --platform web --release -p fetchly` and pass --dist target/dx/fetchly/release/web/public (see README). The proxy still works.", dist.display());
    }
    if host != "127.0.0.1" && host != "localhost" && token.is_none() {
        eprintln!("warning: listening on {host} without FETCHLY_PROXY_TOKEN — anyone who can reach this server can use it to make requests.");
    }

    let state = Arc::new(Config { token });
    let static_files = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));
    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/proxy", post(proxy))
        .with_state(state)
        .fallback_service(static_files)
        .layer(RequestBodyLimitLayer::new(64 * 1024 * 1024))
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = format!("{host}:{port}").parse().expect("invalid --host/--port");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("could not bind address");
    println!("Fetchly web is running at http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}

async fn proxy(State(cfg): State<Arc<Config>>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(expected) = &cfg.token {
        let given = headers.get("x-fetchly-token").and_then(|v| v.to_str().ok());
        if given != Some(expected.as_str()) {
            return (StatusCode::UNAUTHORIZED, "Missing or invalid X-Fetchly-Token").into_response();
        }
    }
    let req: PreparedRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid proxy payload: {e}")).into_response(),
    };
    match direct::send_without_cookies(req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
    }
}
