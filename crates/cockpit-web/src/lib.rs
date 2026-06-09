use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, ResponseBox, Server, StatusCode};

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
    client: Client,
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginForm {
    password: String,
}

struct PrefixThenRead<R> {
    prefix: Cursor<Vec<u8>>,
    inner: R,
}

impl<R: Read> PrefixThenRead<R> {
    fn new(prefix: Vec<u8>, inner: R) -> Self {
        Self {
            prefix: Cursor::new(prefix),
            inner,
        }
    }
}

impl<R: Read> Read for PrefixThenRead<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if (self.prefix.position() as usize) < self.prefix.get_ref().len() {
            return self.prefix.read(out);
        }
        self.inner.read(out)
    }
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

fn response_with_content_type(status: u16, body: Vec<u8>, content_type: &str) -> ResponseBox {
    let mut response = Response::from_data(body).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("content-type", content_type) {
        response.add_header(header);
    }
    response.boxed()
}

fn json_response(status: u16, value: serde_json::Value) -> ResponseBox {
    response_with_content_type(status, value.to_string().into_bytes(), "application/json")
}

fn html_response(status: u16, body: impl Into<String>) -> ResponseBox {
    response_with_content_type(status, body.into().into_bytes(), "text/html; charset=utf-8")
}

fn text_response(status: u16, body: impl Into<String>) -> ResponseBox {
    response_with_content_type(
        status,
        body.into().into_bytes(),
        "text/plain; charset=utf-8",
    )
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

fn is_loopback(addr: Option<SocketAddr>) -> bool {
    matches!(
        addr.map(|addr| addr.ip()),
        Some(IpAddr::V4(ip)) if ip.is_loopback()
    ) || matches!(
        addr.map(|addr| addr.ip()),
        Some(IpAddr::V6(ip)) if ip.is_loopback()
    )
}

fn cookie_value(request: &Request, name: &str) -> Option<String> {
    request.headers().iter().find_map(|header| {
        if !header
            .field
            .as_str()
            .as_str()
            .eq_ignore_ascii_case("cookie")
        {
            return None;
        }
        header.value.as_str().split(';').find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then(|| value.to_string())
        })
    })
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

fn request_authenticated(request: &Request, state: &WebState) -> bool {
    if state.password.is_none() && is_loopback(request.remote_addr().copied()) {
        return true;
    }
    let Some(password) = state.password.as_ref() else {
        return false;
    };
    let expected = session_cookie_value(password);
    cookie_value(request, SESSION_COOKIE).as_deref() == Some(expected.as_str())
}

fn login_page() -> ResponseBox {
    html_response(
        200,
        r#"<!doctype html><html><head><meta charset="utf-8"><title>Cockpit Tools Login</title></head><body><form method="post" action="/__login"><input type="password" name="password" autofocus><button type="submit">Login</button></form></body></html>"#,
    )
}

fn read_body(request: &mut Request) -> Result<String, String> {
    let mut body = String::new();
    request
        .as_reader()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut body)
        .map_err(|err| format!("read request body failed: {err}"))?;
    Ok(body)
}

fn login_submit(request: &mut Request, state: &WebState) -> ResponseBox {
    let body = read_body(request).unwrap_or_default();
    let password = serde_urlencoded::from_str::<LoginForm>(&body)
        .map(|form| form.password)
        .unwrap_or_default();
    if state.password.as_deref() != Some(password.as_str()) {
        return html_response(403, "Forbidden");
    }
    let mut response = html_response(302, "");
    let cookie_value = session_cookie_value(&password);
    if let Ok(header) = Header::from_bytes(
        "set-cookie",
        format!("{SESSION_COOKIE}={cookie_value}; Path=/; HttpOnly; SameSite=Lax"),
    ) {
        response.add_header(header);
    }
    if let Ok(header) = Header::from_bytes("location", "/") {
        response.add_header(header);
    }
    response
}

fn redirect_response(location: &str) -> ResponseBox {
    let mut response = html_response(302, "");
    if let Ok(header) = Header::from_bytes("location", location) {
        response.add_header(header);
    }
    response
}

fn runtime_response(state: &WebState) -> ResponseBox {
    json_response(
        200,
        json!({
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
        }),
    )
}

fn post_service_rpc(state: &WebState, body: String) -> Result<(u16, String), String> {
    let target = format!("http://{}/rpc", state.service_addr);
    state
        .client
        .post(target)
        .header(
            cockpit_service::RPC_TOKEN_HEADER,
            cockpit_service::rpc_auth_token(),
        )
        .header("content-type", "application/json")
        .body(body)
        .send()
        .map_err(|err| err.to_string())
        .map(|response| {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_else(|err| {
                json!({ "error": format!("read service response failed: {err}") }).to_string()
            });
            (status, text)
        })
}

fn proxy_rpc(request: &mut Request, state: &WebState) -> ResponseBox {
    let body = match read_body(request) {
        Ok(body) => body,
        Err(err) => return json_response(400, json!({ "error": err })),
    };
    match post_service_rpc(state, body) {
        Ok((status, text)) => {
            response_with_content_type(status, text.into_bytes(), "application/json")
        }
        Err(err) => json_response(
            502,
            json!({
                "error": "service_unreachable",
                "message": err.to_string(),
                "serviceHost": state.service_addr
            }),
        ),
    }
}

fn external_import_response(url: &str, state: &WebState) -> ResponseBox {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .filter(|query| !query.trim().is_empty());
    let Some(query) = query else {
        return json_response(
            400,
            json!({ "error": "missing_query", "message": "External import requires query parameters" }),
        );
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
    match post_service_rpc(state, body) {
        Ok((status, text)) if status == 200 => {
            let parsed = serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default();
            if parsed.get("error").is_some() {
                return response_with_content_type(400, text.into_bytes(), "application/json");
            }
            redirect_response("/?externalImport=1")
        }
        Ok((status, text)) => {
            response_with_content_type(status, text.into_bytes(), "application/json")
        }
        Err(err) => json_response(
            502,
            json!({
                "error": "service_unreachable",
                "message": err,
                "serviceHost": state.service_addr
            }),
        ),
    }
}

fn make_header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn proxy_events(state: &WebState) -> ResponseBox {
    let target = format!("http://{}/events", state.service_addr);
    match state
        .client
        .get(target)
        .header(
            cockpit_service::RPC_TOKEN_HEADER,
            cockpit_service::rpc_auth_token(),
        )
        .send()
    {
        Ok(response) => {
            let status = response.status().as_u16();
            if !response.status().is_success() {
                let text = response.text().unwrap_or_else(|err| {
                    json!({ "error": format!("read service response failed: {err}") }).to_string()
                });
                return response_with_content_type(status, text.into_bytes(), "application/json");
            }
            let mut headers = Vec::new();
            for (name, value) in [
                ("content-type", "text/event-stream; charset=utf-8"),
                ("cache-control", "no-cache"),
                ("x-accel-buffering", "no"),
            ] {
                if let Some(header) = make_header(name, value) {
                    headers.push(header);
                }
            }
            Response::new(
                StatusCode(status),
                headers,
                Box::new(PrefixThenRead::new(
                    sse_frame(
                        "gateway.ready",
                        json!({ "emittedAt": unix_timestamp_millis(), "serviceHost": state.service_addr }),
                    ),
                    response,
                )) as Box<dyn Read + Send>,
                None,
                None,
            )
            .with_chunked_threshold(0)
            .boxed()
        }
        Err(err) => json_response(
            502,
            json!({
                "error": "service_unreachable",
                "message": err.to_string(),
                "serviceHost": state.service_addr
            }),
        ),
    }
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

fn static_response(url: &str, state: &WebState) -> ResponseBox {
    let Some(relative) = sanitize_path(url) else {
        return text_response(400, "Bad request");
    };
    let mut path = state.web_root.join(relative);
    if path.is_dir() {
        path = path.join("index.html");
    }
    if !path.exists() {
        path = state.web_root.join("index.html");
    }
    match fs::read(&path) {
        Ok(bytes) => response_with_content_type(200, bytes, content_type(&path)),
        Err(_) => html_response(
            404,
            format!(
                "Cockpit Tools web UI not found. Build the frontend or set {WEB_ROOT_ENV}. root={}",
                state.web_root.display()
            ),
        ),
    }
}

fn handle_request(mut request: Request, state: &WebState) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or(url.as_str()).to_string();
    let public = matches!(
        (method.clone(), path.as_str()),
        (Method::Get, "/__login") | (Method::Post, "/__login")
    );
    let response = if !public && !request_authenticated(&request, state) {
        html_response(401, r#"<a href="/__login">Login required</a>"#)
    } else {
        match (method, path.as_str()) {
            (Method::Get, "/__login") => login_page(),
            (Method::Post, "/__login") => login_submit(&mut request, state),
            (Method::Get, "/api/runtime") => runtime_response(state),
            (Method::Post, "/api/rpc") => proxy_rpc(&mut request, state),
            (Method::Get, "/api/events") => proxy_events(state),
            (Method::Get, "/external-import")
            | (Method::Get, "/provider-import")
            | (Method::Get, "/account-import") => external_import_response(&url, state),
            _ => static_response(&url, state),
        }
    };
    let _ = request.respond(response);
}

pub fn start_server(web_addr: &str, service_addr: &str, web_root: PathBuf) -> Result<(), String> {
    let server =
        Server::http(web_addr).map_err(|err| format!("bind web {web_addr} failed: {err}"))?;
    let state = WebState {
        service_addr: service_addr.to_string(),
        web_root,
        client: Client::new(),
        password: read_env_trim(WEB_PASSWORD_ENV),
    };
    println!("cockpit-web listening on {web_addr} (service={service_addr})");
    for request in server.incoming_requests() {
        let state = state.clone();
        thread::spawn(move || handle_request(request, &state));
    }
    Ok(())
}

pub fn run_from_env() -> Result<(), String> {
    let web_addr = resolve_web_addr();
    let service_addr = cockpit_service::resolve_addr_from_env();
    start_server(&web_addr, &service_addr, resolve_web_root())
}
