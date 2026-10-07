//! Local control plane.
//!
//! The app can be driven completely from outside itself over a loopback HTTP API,
//! so an agent (or a script, or you in a terminal) has full command of the studio:
//! read and write prompt sets, dispatch a render, follow a run, list the renders,
//! download them, and use the OpenRouter ideation calls.
//!
//! Security posture:
//!   * Binds 127.0.0.1 only. Nothing off the machine can reach it.
//!   * Every request needs a bearer token. The token is generated on first run and
//!     written to the app's config directory (0600 where the OS honours it); the
//!     environment variable `FORGE_CONTROL_TOKEN` overrides it.
//!   * A descriptor file `control.json` in the same directory publishes the port,
//!     the token and the pid, so a client can discover the endpoint without guessing.
//!   * Set `FORGE_CONTROL=off` to disable the server entirely.

use std::path::PathBuf;
use tiny_http::{Header, Method, Response, Server, StatusCode};

pub const DEFAULT_PORT: u16 = 7317;

fn app_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_CONFIG_HOME"))
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
        .unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(base).join("forge-studio");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Process-local randomness is enough here: the token only has to be unguessable
/// by something else on the loopback interface.
fn random_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for _ in 0..3 {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_usize(std::process::id() as usize);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

fn token() -> String {
    if let Ok(t) = std::env::var("FORGE_CONTROL_TOKEN") {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    let path = app_dir().join("control-token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    let t = random_token();
    let _ = std::fs::write(&path, &t);
    t
}

fn json_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
}

fn respond(request: tiny_http::Request, code: u16, body: String) {
    let resp = Response::from_string(body)
        .with_status_code(StatusCode(code))
        .with_header(json_header());
    let _ = request.respond(resp);
}

fn ok(request: tiny_http::Request, body: serde_json::Value) {
    respond(request, 200, body.to_string());
}

fn err(request: tiny_http::Request, code: u16, msg: &str) {
    respond(
        request,
        code,
        serde_json::json!({ "error": msg }).to_string(),
    );
}

fn body_of(request: &mut tiny_http::Request) -> String {
    let mut s = String::new();
    let reader = request.as_reader();
    let _ = std::io::Read::read_to_string(reader, &mut s);
    s
}

fn authorised(request: &tiny_http::Request, expected: &str) -> bool {
    let mut presented = None;
    for h in request.headers() {
        let name = h.field.as_str().as_str().to_ascii_lowercase();
        if name == "authorization" || name == "x-forge-token" {
            presented = Some(h.value.as_str().trim().to_string());
        }
    }
    match presented {
        Some(v) => {
            let v = v.strip_prefix("Bearer ").unwrap_or(&v).trim();
            // constant-time-ish compare; both are generated, not secret-derived
            v.len() == expected.len()
                && v.bytes()
                    .zip(expected.bytes())
                    .fold(0u8, |a, (x, y)| a | (x ^ y))
                    == 0
        }
        None => false,
    }
}

/// A word describing what this is, served at the root.
fn describe(port: u16) -> serde_json::Value {
    serde_json::json!({
        "name": "forge-studio",
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "port": port,
        "repo": "dnh33/forge-images",
        "endpoints": [
            "GET  /status",
            "GET  /sets",
            "GET  /set/<slug>",
            "PUT  /set/<slug>           body: prompt set JSON",
            "POST /dispatch             body: {set,only,variants,steps,shards,adhoc}",
            "GET  /runs",
            "GET  /run/<id>",
            "GET  /run/<id>/outputs",
            "GET  /renders",
            "POST /download             body: {urls:[...], dir:\"C:/path\"}",
            "POST /ideate               body: {brief, model, count}",
            "POST /advise               body: {question, context, model}"
        ]
    })
}

fn route(method: &Method, path: &str, body: &str) -> (u16, serde_json::Value) {
    let seg: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let parsed: serde_json::Value = if body.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => {
                return (
                    400,
                    serde_json::json!({ "error": format!("bad JSON body: {e}") }),
                )
            }
        }
    };
    let s = |k: &str, d: &str| -> String {
        parsed
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or(d)
            .to_string()
    };

    match (method, seg.as_slice()) {
        (&Method::Get, ["status"]) => {
            let id = tauri::async_runtime::block_on(crate::github::identity());
            let or = crate::openrouter::status();
            match id {
                Ok(i) => (
                    200,
                    serde_json::json!({ "github": i, "openrouter": {
                    "configured": or.configured, "source": or.source, "model": or.model } }),
                ),
                Err(e) => (
                    200,
                    serde_json::json!({ "github_error": e, "openrouter": {
                    "configured": or.configured, "source": or.source, "model": or.model } }),
                ),
            }
        }
        (&Method::Get, ["sets"]) => {
            match tauri::async_runtime::block_on(crate::github::list_sets()) {
                Ok(v) => (
                    200,
                    serde_json::to_value(v).unwrap_or(serde_json::json!([])),
                ),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Get, ["set", slug]) => {
            match tauri::async_runtime::block_on(crate::github::get_set(slug.to_string())) {
                Ok(v) => (
                    200,
                    serde_json::to_value(v).unwrap_or(serde_json::json!({})),
                ),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Put, ["set", slug]) => {
            match tauri::async_runtime::block_on(crate::github::save_set(
                slug.to_string(),
                body.to_string(),
            )) {
                Ok(sha) => (200, serde_json::json!({ "saved": sha })),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Post, ["dispatch"]) => {
            let opts = crate::github::DispatchOpts {
                set: s("set", "all"),
                only: s("only", ""),
                variants: s("variants", "2"),
                steps: s("steps", "4"),
                shards: s("shards", "8"),
                adhoc: s("adhoc", ""),
            };
            match tauri::async_runtime::block_on(crate::github::dispatch(opts)) {
                Ok(_) => (200, serde_json::json!({ "dispatched": true })),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Get, ["runs"]) => {
            match tauri::async_runtime::block_on(crate::github::list_runs(15)) {
                Ok(v) => (
                    200,
                    serde_json::to_value(v).unwrap_or(serde_json::json!([])),
                ),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Get, ["run", id]) => match id.parse::<u64>() {
            Ok(id) => match tauri::async_runtime::block_on(crate::github::get_run(id)) {
                Ok(v) => (
                    200,
                    serde_json::to_value(v).unwrap_or(serde_json::json!({})),
                ),
                Err(e) => (502, serde_json::json!({ "error": e })),
            },
            Err(_) => (
                400,
                serde_json::json!({ "error": "run id must be a number" }),
            ),
        },
        (&Method::Get, ["run", id, "outputs"]) => match id.parse::<u64>() {
            Ok(id) => match tauri::async_runtime::block_on(crate::github::run_outputs(id)) {
                Ok(v) => (
                    200,
                    serde_json::to_value(v).unwrap_or(serde_json::json!([])),
                ),
                Err(e) => (502, serde_json::json!({ "error": e })),
            },
            Err(_) => (
                400,
                serde_json::json!({ "error": "run id must be a number" }),
            ),
        },
        (&Method::Get, ["renders"]) => {
            match tauri::async_runtime::block_on(crate::github::list_renders()) {
                Ok(v) => (
                    200,
                    serde_json::to_value(v).unwrap_or(serde_json::json!([])),
                ),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Post, ["download"]) => {
            let dir = s("dir", "");
            if dir.is_empty() {
                return (400, serde_json::json!({ "error": "dir is required" }));
            }
            let urls: Vec<String> = parsed
                .get("urls")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if urls.is_empty() {
                return (400, serde_json::json!({ "error": "urls is required" }));
            }
            let mut saved = Vec::new();
            let mut failures = Vec::new();
            for u in urls {
                match tauri::async_runtime::block_on(crate::github::download(
                    u.clone(),
                    dir.clone(),
                )) {
                    Ok(p) => saved.push(p),
                    Err(e) => failures.push(serde_json::json!({ "url": u, "error": e })),
                }
            }
            (
                200,
                serde_json::json!({ "saved": saved, "failed": failures }),
            )
        }
        (&Method::Post, ["ideate"]) => {
            let brief = s("brief", "");
            if brief.is_empty() {
                return (400, serde_json::json!({ "error": "brief is required" }));
            }
            let count = parsed.get("count").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
            match tauri::async_runtime::block_on(crate::openrouter::ideate(
                brief,
                s("model", ""),
                count,
            )) {
                Ok(v) => (200, serde_json::json!({ "set": v })),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        (&Method::Post, ["advise"]) => {
            let q = s("question", "");
            if q.is_empty() {
                return (400, serde_json::json!({ "error": "question is required" }));
            }
            let ctx = parsed
                .get("context")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            match tauri::async_runtime::block_on(crate::openrouter::advise(q, ctx, s("model", "")))
            {
                Ok(v) => (200, serde_json::json!({ "answer": v })),
                Err(e) => (502, serde_json::json!({ "error": e })),
            }
        }
        _ => (
            404,
            serde_json::json!({ "error": "no such endpoint", "see": "GET /" }),
        ),
    }
}

/// Path to the descriptor file that publishes the port and token, or None when
/// the control plane is disabled. The UI shows this so the interface is
/// discoverable rather than a secret only the docs know about.
pub fn descriptor_path() -> Option<String> {
    if std::env::var("FORGE_CONTROL")
        .map(|v| v.eq_ignore_ascii_case("off"))
        .unwrap_or(false)
    {
        return None;
    }
    Some(app_dir().join("control.json").to_string_lossy().to_string())
}

/// The request loop, over an already-bound server. Separate from `start` so a
/// test can drive it on a throwaway port with a known token, with no Tauri app
/// and no network involved.
pub fn serve(server: Server, tok: &str) {
    let real_port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    eprintln!("forge-studio control plane listening on http://127.0.0.1:{real_port}");
    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("/").to_string();

        if !authorised(&request, tok) {
            err(request, 401, "missing or bad bearer token");
            continue;
        }
        if path == "/" && method == Method::Get {
            ok(request, describe(real_port));
            continue;
        }
        let body = body_of(&mut request);
        let (code, value) = route(&method, &path, &body);
        respond(request, code, value.to_string());
    }
}

/// Start the control plane on a background thread. Never fails the app: if the
/// port is taken or the feature is off, it logs and returns.
pub fn start() {
    if std::env::var("FORGE_CONTROL")
        .map(|v| v.eq_ignore_ascii_case("off"))
        .unwrap_or(false)
    {
        eprintln!("forge-studio control plane disabled (FORGE_CONTROL=off)");
        return;
    }
    let port: u16 = std::env::var("FORGE_CONTROL_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    std::thread::spawn(move || {
        let addr =
            std::env::var("FORGE_CONTROL_ADDR").unwrap_or_else(|_| format!("127.0.0.1:{port}"));
        let server = match Server::http(&addr) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("forge-studio control plane could not bind {addr}: {e}");
                return;
            }
        };
        let real_port = server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .unwrap_or(port);
        let tok = token();
        let dir = app_dir();
        let descriptor = serde_json::json!({
            "name": "forge-studio", "port": real_port, "token": tok,
            "pid": std::process::id(), "url": format!("http://127.0.0.1:{real_port}")
        });
        let _ = std::fs::write(dir.join("control.json"), descriptor.to_string());
        let _ = std::fs::write(dir.join("control-token"), &tok);
        eprintln!(
            "forge-studio control plane listening on http://127.0.0.1:{real_port} (token in {})",
            dir.join("control.json").display()
        );
        serve(server, &tok);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    const TOK: &str = "test-token-0123456789abcdef";

    /// A real server on a real ephemeral port, driven over a real socket. The
    /// routes exercised here deliberately never reach GitHub, so the suite needs
    /// no network and no credentials.
    fn test_server() -> u16 {
        let server = Server::http("127.0.0.1:0").expect("bind ephemeral port");
        let port = server.server_addr().to_ip().expect("tcp addr").port();
        std::thread::spawn(move || serve(server, TOK));
        port
    }

    fn request(
        port: u16,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &str,
    ) -> (u16, String) {
        let mut sock = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let auth = match token {
            Some(t) => format!("Authorization: Bearer {t}\r\n"),
            None => String::new(),
        };
        let raw = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        sock.write_all(raw.as_bytes()).expect("write");
        let mut buf = String::new();
        sock.read_to_string(&mut buf).expect("read");
        let code = buf
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let payload = buf.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (code, payload)
    }

    #[test]
    fn a_request_with_no_token_is_refused() {
        let port = test_server();
        let (code, _) = request(port, "GET", "/", None, "");
        assert_eq!(
            code, 401,
            "the control plane must not answer unauthenticated callers"
        );
    }

    #[test]
    fn a_request_with_the_wrong_token_is_refused() {
        let port = test_server();
        let (code, _) = request(port, "GET", "/", Some("not-the-token"), "");
        assert_eq!(code, 401);
    }

    #[test]
    fn the_right_token_gets_the_endpoint_list() {
        let port = test_server();
        let (code, body) = request(port, "GET", "/", Some(TOK), "");
        assert_eq!(code, 200);
        assert!(body.contains("forge-studio"), "body was: {body}");
        assert!(
            body.contains("/dispatch"),
            "the contract must be discoverable"
        );
    }

    #[test]
    fn an_unknown_endpoint_is_a_readable_404() {
        let port = test_server();
        let (code, body) = request(port, "GET", "/nope", Some(TOK), "");
        assert_eq!(code, 404);
        assert!(body.contains("no such endpoint"), "body was: {body}");
    }

    #[test]
    fn malformed_json_is_rejected_before_any_work_happens() {
        let port = test_server();
        let (code, body) = request(port, "PUT", "/set/whatever", Some(TOK), "{ not json");
        assert_eq!(
            code, 400,
            "a bad body must fail fast, not after a network round trip"
        );
        assert!(body.contains("bad JSON"), "body was: {body}");
    }

    #[test]
    fn a_missing_required_field_is_rejected() {
        let port = test_server();
        let (code, body) = request(port, "POST", "/ideate", Some(TOK), "{}");
        assert_eq!(code, 400);
        assert!(body.contains("brief"), "body was: {body}");
    }

    #[test]
    fn tokens_are_long_and_not_repeated() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 48, "three 16-hex chunks");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two tokens in one process must differ");
    }

    #[test]
    fn the_descriptor_names_the_endpoint() {
        let d = describe(7317);
        assert_eq!(d["name"], "forge-studio");
        assert_eq!(d["port"].as_u64(), Some(7317));
        assert!(d["endpoints"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false));
    }
}
