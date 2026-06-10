use axum::body::Body;
use axum::extract::{Request as AxumRequest, State as AxumState};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WEB_ADDR_ENV: &str = "COCKPIT_TOOLS_WEB_ADDR";
const WEB_ROOT_ENV: &str = "COCKPIT_TOOLS_WEB_ROOT";
const WEB_PASSWORD_ENV: &str = "COCKPIT_TOOLS_WEB_PASSWORD";
const DEFAULT_WEB_ADDR: &str = "127.0.0.1:18082";
const SESSION_COOKIE: &str = "cockpit_tools_web_session";
const SSE_FLUSH_PADDING_BYTES: usize = 8193;

#[derive(Clone)]
struct WebState {
    service_addr: String,
    web_root: PathBuf,
    client: reqwest::Client,
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginForm {
    password: String,
}

fn read_env_trim(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_web_addr() -> String {
    read_env_trim(WEB_ADDR_ENV).unwrap_or_else(|| DEFAULT_WEB_ADDR.to_string())
}

pub fn resolve_web_root() -> PathBuf {
    read_env_trim(WEB_ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist"))
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn sse_frame(event: &str, data: serde_json::Value) -> Vec<u8> {
    let payload = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let mut frame = format!("event: {event}\ndata: {payload}\n\n").into_bytes();
    if frame.len() < SSE_FLUSH_PADDING_BYTES {
        frame.extend_from_slice(b":");
        frame.extend(std::iter::repeat(b' ').take(SSE_FLUSH_PADDING_BYTES - frame.len()));
        frame.extend_from_slice(b"\n\n");
    }
    frame
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn session_cookie_value(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"cockpit-tools-web-session-v1");
    hasher.update(password.as_bytes());
    hasher.update(cockpit_service::rpc_auth_token().as_bytes());
    hex(&hasher.finalize())
}

fn is_loopback(addr: SocketAddr) -> bool {
    addr.ip().is_loopback()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get("cookie")?.to_str().ok()?.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

fn request_authenticated(headers: &HeaderMap, remote_addr: SocketAddr, state: &WebState) -> bool {
    // No password configured = no auth required
    if state.password.is_none() {
        return true;
    }
    // Password configured = check cookie
    let Some(password) = state.password.as_ref() else {
        return true;
    };
    let expected = session_cookie_value(password);
    cookie_value(headers, SESSION_COOKIE).as_deref() == Some(expected.as_str())
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn sanitize_path(url: &str) -> Option<PathBuf> {
    let path = url.split('?').next().unwrap_or(url);
    let path = path.trim_start_matches('/');
    if path.split('/').any(|part| part == "..") {
        return None;
    }
    Some(PathBuf::from(path))
}

// --- Route handlers ---

async fn login_page() -> Html<&'static str> {
    Html(r#"<!doctype html><html><head><meta charset="utf-8"><title>Cockpit Tools Login</title></head><body><form method="post" action="/__login"><input type="password" name="password" autofocus><button type="submit">Login</button></form></body></html>"#)
}

async fn login_submit(
    AxumState(state): AxumState<Arc<WebState>>,
    axum::Form(form): axum::Form<LoginForm>,
) -> impl IntoResponse {
    if state.password.as_deref() != Some(form.password.as_str()) {
        return (StatusCode::FORBIDDEN, Html("Forbidden")).into_response();
    }
    let cookie_val = session_cookie_value(&form.password);
    let cookie = format!("{SESSION_COOKIE}={cookie_val}; Path=/; HttpOnly; SameSite=Lax");
    let mut response = Redirect::temporary("/").into_response();
    response.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&cookie).unwrap(),
    );
    response
}

async fn runtime_endpoint(AxumState(state): AxumState<Arc<WebState>>) -> Json<serde_json::Value> {
    Json(json!({
        "mode": "web-gateway",
        "rpcBaseUrl": "/api/rpc",
        "eventBaseUrl": "/api/events",
        "serviceHost": state.service_addr,
        "features": {
            "jsonRpc": true,
            "events": true,
            "serviceHostExecution": true,
            "appVersion": cockpit_service::application_version()
        }
    }))
}

async fn proxy_rpc(
    AxumState(state): AxumState<Arc<WebState>>,
    body: String,
) -> Response {
    let target = format!("http://{}/rpc", state.service_addr);
    match state
        .client
        .post(target)
        .header(
            cockpit_service::RPC_TOKEN_HEADER,
            cockpit_service::rpc_auth_token(),
        )
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
    {
        Ok(response) => {
            let status = StatusCode::from_u16(response.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            match response.text().await {
                Ok(text) => (
                    status,
                    [("content-type", "application/json")],
                    text,
                )
                    .into_response(),
                Err(err) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("read service response failed: {err}") })),
                )
                    .into_response(),
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "service_unreachable",
                "message": err.to_string(),
                "serviceHost": state.service_addr
            })),
        )
            .into_response(),
    }
}

async fn proxy_events(AxumState(state): AxumState<Arc<WebState>>) -> Response {
    let target = format!("http://{}/events", state.service_addr);
    match state
        .client
        .get(target)
        .header(
            cockpit_service::RPC_TOKEN_HEADER,
            cockpit_service::rpc_auth_token(),
        )
        .send()
        .await
    {
        Ok(response) => {
            if !response.status().is_success() {
                let status = StatusCode::from_u16(response.status().as_u16())
                    .unwrap_or(StatusCode::BAD_GATEWAY);
                let text = response.text().await.unwrap_or_default();
                return (status, [("content-type", "application/json")], text).into_response();
            }
            let ready_frame = sse_frame(
                "gateway.ready",
                json!({ "emittedAt": unix_timestamp_millis(), "serviceHost": state.service_addr }),
            );
            match response.bytes().await {
                Ok(body) => {
                    let mut full = ready_frame;
                    full.extend_from_slice(&body);
                    (
                        StatusCode::OK,
                        [
                            ("content-type", "text/event-stream; charset=utf-8"),
                            ("cache-control", "no-cache"),
                            ("x-accel-buffering", "no"),
                        ],
                        full,
                    )
                        .into_response()
                }
                Err(err) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("read events failed: {err}") })),
                )
                    .into_response(),
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "service_unreachable",
                "message": err.to_string(),
                "serviceHost": state.service_addr
            })),
        )
            .into_response(),
    }
}

async fn external_import(
    AxumState(state): AxumState<Arc<WebState>>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> impl IntoResponse {
    let url = uri.to_string();
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .filter(|query| !query.trim().is_empty());
    let Some(query) = query else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing_query", "message": "External import requires query parameters" })),
        ).into_response();
    };
    let raw_url = format!("cockpit-tools://import?{query}");
    let body = json!({
        "jsonrpc": "2.0",
        "id": "external-import",
        "method": "external-import/pending/submit-url",
        "params": {
            "rawUrl": raw_url,
            "source": "web-gateway"
        }
    })
    .to_string();
    let target = format!("http://{}/rpc", state.service_addr);
    match state
        .client
        .post(target)
        .header(
            cockpit_service::RPC_TOKEN_HEADER,
            cockpit_service::rpc_auth_token(),
        )
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Redirect::temporary("/?externalImport=1").into_response(),
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let text = resp.text().await.unwrap_or_default();
            (status, [("content-type", "application/json")], text).into_response()
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "service_unreachable", "message": err.to_string(), "serviceHost": state.service_addr })),
        ).into_response(),
    }
}

async fn static_files(
    AxumState(state): AxumState<Arc<WebState>>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> impl IntoResponse {
    let url = uri.to_string();
    let Some(relative) = sanitize_path(&url) else {
        return (StatusCode::BAD_REQUEST, "Bad request").into_response();
    };
    let mut path = state.web_root.join(relative);
    if path.is_dir() {
        path = path.join("index.html");
    }
    if !path.exists() {
        path = state.web_root.join("index.html");
    }
    match fs::read(&path) {
        Ok(bytes) => (
            StatusCode::OK,
            [("content-type", content_type(&path))],
            bytes,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Html(format!(
                "Cockpit Tools web UI not found. Build the frontend or set {WEB_ROOT_ENV}. root={}",
                state.web_root.display()
            )),
        )
            .into_response(),
    }
}

// --- Auth middleware ---

async fn auth_middleware(
    AxumState(state): AxumState<Arc<WebState>>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    req: AxumRequest,
    next: axum::middleware::Next,
) -> Response {
    let path = req.uri().path();
    let public = path == "/__login";
    if !public && !request_authenticated(&headers, addr, &state) {
        return Html(r#"<a href="/__login">Login required</a>"#).into_response();
    }
    next.run(req).await
}

// --- Server ---

pub async fn start_server_async(
    web_addr: &str,
    service_addr: &str,
    web_root: PathBuf,
) -> Result<(), String> {
    let state = Arc::new(WebState {
        service_addr: service_addr.to_string(),
        web_root,
        client: reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| format!("create http client failed: {err}"))?,
        password: read_env_trim(WEB_PASSWORD_ENV),
    });

    let app = Router::new()
        .route("/__login", get(login_page).post(login_submit))
        .route("/api/runtime", get(runtime_endpoint))
        .route("/api/rpc", post(proxy_rpc))
        .route("/api/events", get(proxy_events))
        .route("/external-import", get(external_import))
        .route("/provider-import", get(external_import))
        .route("/account-import", get(external_import))
        .fallback(static_files)
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state);

    let addr: SocketAddr = web_addr
        .parse()
        .map_err(|err| format!("invalid web addr {web_addr}: {err}"))?;
    println!("cockpit-web listening on {web_addr} (service={service_addr})");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("bind web {web_addr} failed: {err}"))?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|err| format!("serve failed: {err}"))
}

pub fn start_server(web_addr: &str, service_addr: &str, web_root: PathBuf) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("create tokio runtime failed: {err}"))?;
    rt.block_on(start_server_async(web_addr, service_addr, web_root))
}

pub fn run_from_env() -> Result<(), String> {
    let web_addr = resolve_web_addr();
    let service_addr = cockpit_service::resolve_addr_from_env();
    start_server(&web_addr, &service_addr, resolve_web_root())
}
