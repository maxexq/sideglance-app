//! The local HTTP API on 127.0.0.1:8765, and the control page.
//!
//! Who may do what:
//!   GET  /api/scene      anyone who can reach the port (other devices too, with lan = true)
//!   everything else     only this computer, addressed as localhost (which also stops DNS rebinding),
//!                       from our own page, a program with no Origin (curl in Claude Code hooks),
//!                       or a hosted page listed in the config's allowed_origins.
//!                       A hosted page cannot change tokens or the connection.
//!
//! Claude Code hooks call POST /api/hook/<event-id> with curl: no JSON to quote, and no body back.

use std::io::Read;
use std::sync::Arc;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::frame::Frame;
use crate::link;
use crate::protocol;
use crate::state::{App, Base};

pub const PORT: u16 = 8765;
const LOCAL_HOSTS: &[&str] = &["localhost", "127.0.0.1", "::1"];

/// Files of the control page: path (without the leading slash) to (bytes, content type).
pub type Assets = Arc<dyn Fn(&str) -> Option<(Vec<u8>, String)> + Send + Sync>;

#[derive(PartialEq, Debug)]
pub enum OriginKind {
    None,
    Local,
    Hosted,
    Blocked,
}

pub fn origin_kind(origin: Option<&str>, allowed: &[String]) -> OriginKind {
    let Some(origin) = origin else { return OriginKind::None };
    let host = origin.split("//").last().unwrap_or("").split(':').next().unwrap_or("").trim_matches(['[', ']']);
    // the app's own window: tauri://localhost (macOS) or http://tauri.localhost (Windows)
    if LOCAL_HOSTS.contains(&host) || host == "tauri.localhost" {
        return OriginKind::Local;
    }
    if allowed.iter().any(|a| a.trim_end_matches('/') == origin.trim_end_matches('/')) {
        OriginKind::Hosted
    } else {
        OriginKind::Blocked
    }
}

fn header(req: &Request, name: &str) -> Option<String> {
    req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string())
}

fn is_local_client(req: &Request) -> bool {
    let from_here = req.remote_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
    let host = header(req, "Host").unwrap_or_default();
    let host = host.rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or(host);
    from_here && LOCAL_HOSTS.contains(&host.trim_matches(['[', ']']))
}

fn json_response(code: u16, value: &Value, origin: Option<&str>) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_data(serde_json::to_vec(value).unwrap()).with_status_code(code)
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap());
    if let Some(o) = origin {
        resp = resp
            .with_header(Header::from_bytes("Access-Control-Allow-Origin", o).unwrap())
            .with_header(Header::from_bytes("Vary", "Origin").unwrap())
            .with_header(Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap())
            .with_header(Header::from_bytes("Access-Control-Allow-Private-Network", "true").unwrap());
    }
    resp
}

/// Serve until the process ends. Each request runs on its own thread.
pub fn serve(app: Arc<App>, assets: Assets, lan: bool) -> Result<(), String> {
    let address = if lan { format!("0.0.0.0:{PORT}") } else { format!("127.0.0.1:{PORT}") };
    let server = Server::http(&address).map_err(|e| format!("Port {PORT} is in use or blocked: {e}"))?;
    for request in server.incoming_requests() {
        let app = Arc::clone(&app);
        let assets = Arc::clone(&assets);
        std::thread::spawn(move || handle(app, assets, request));
    }
    Ok(())
}

fn handle(app: Arc<App>, assets: Assets, mut req: Request) {
    let allowed: Vec<String> = app.config.load().get("allowed_origins").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let origin = header(&req, "Origin");
    let kind = origin_kind(origin.as_deref(), &allowed);
    let cors = matches!(kind, OriginKind::Local | OriginKind::Hosted).then_some(origin.as_deref()).flatten();
    let path = req.url().split('?').next().unwrap_or("/").to_string();

    if *req.method() == Method::Options {
        let _ = req.respond(json_response(204, &json!({}), cors));
        return;
    }
    if *req.method() == Method::Get && path == "/api/scene" {
        let _ = req.respond(json_response(200, &app.scene(), cors));
        return;
    }
    if !is_local_client(&req) {
        let _ = req.respond(json_response(403, &json!({"error": "Only /api/scene is available from other devices."}), None));
        return;
    }
    if kind == OriginKind::Blocked {
        let _ = req.respond(json_response(403, &json!({"error": "origin not allowed"}), None));
        return;
    }

    if *req.method() == Method::Get {
        let reply = match path.as_str() {
            "/api/status" => Some(app.status()),
            "/api/notifications" => Some(json!({"events": app.notification_previews()})),
            "/api/ports" => Some(json!({"ports": link::list_ports(), "setting": app.port_setting(), "help": link::how_to_pair()})),
            "/api/hooks" => Some(crate::hooks::status()),
            _ => None,
        };
        if let Some(value) = reply {
            let _ = req.respond(json_response(200, &value, cors));
            return;
        }
        let rel = path.trim_start_matches('/');
        let file = assets(if rel.is_empty() { "index.html" } else { rel }).or_else(|| assets("index.html"));
        let _ = match file {
            Some((bytes, kind)) => req.respond(Response::from_data(bytes)
                .with_header(Header::from_bytes("Content-Type", kind.as_bytes()).unwrap())),
            None => req.respond(json_response(404, &json!({"error": "not found"}), cors)),
        };
        return;
    }

    if kind == OriginKind::Hosted && ["/api/tokens", "/api/port", "/api/hooks"].contains(&path.as_str()) {
        let _ = req.respond(json_response(403, &json!({"error": "Open http://localhost:8765 to change tokens or the connection."}), cors));
        return;
    }
    if let Some(event) = path.strip_prefix("/api/hook/") {
        // from Claude Code hooks: no body back, so nothing is printed into Claude Code
        let _ = post(&app, &format!("/api/notify/{event}"), &json!({}));
        let _ = req.respond(Response::empty(204));
        return;
    }
    let mut body = String::new();
    let _ = req.as_reader().take(4 * 1024 * 1024).read_to_string(&mut body);
    let body: Value = if body.trim().is_empty() { json!({}) } else { serde_json::from_str(&body).unwrap_or(json!({})) };
    let result = post(&app, &path, &body);
    let _ = match result {
        Ok(true) => req.respond(json_response(200, &with_ok(app.status()), cors)),
        Ok(false) => req.respond(json_response(404, &json!({"error": "not found"}), cors)),
        Err(e) => {
            let mut status = app.status();
            status["error"] = json!(e);
            req.respond(json_response(400, &status, cors))
        }
    };
}

fn with_ok(mut status: Value) -> Value {
    status["ok"] = json!(true);
    status
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(|x| match x {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).collect()).unwrap_or_default()
}

/// One POST. Ok(false) means the route does not exist.
pub fn post(app: &Arc<App>, path: &str, body: &Value) -> Result<bool, String> {
    if path == "/api/usage" {
        app.dismiss(); // choosing what to show dismisses any notification
        let on = body["on"].as_bool().unwrap_or(false);
        app.set_usage(on);
        app.config.update(&[("start_with", if on { json!("usage") } else { Value::Null })]).map_err(|e| e.to_string())?;
    } else if path == "/api/weather" {
        app.dismiss();
        let on = body["on"].as_bool().unwrap_or(false);
        app.set_weather(on);
        app.config.update(&[("start_with", if on { json!("weather") } else { Value::Null })]).map_err(|e| e.to_string())?;
    } else if path == "/api/system" {
        app.dismiss();
        let on = body["on"].as_bool().unwrap_or(false);
        app.set_system(on);
        app.config.update(&[("start_with", if on { json!("system") } else { Value::Null })]).map_err(|e| e.to_string())?;
    } else if path == "/api/weather/settings" {
        app.weather_settings(body)?;
    } else if path == "/api/timer" {
        app.dismiss();
        let minutes = body["minutes"].as_i64().or_else(|| body["minutes"].as_str().and_then(|m| m.parse().ok())).unwrap_or(0);
        app.set_timer(minutes);
    } else if path == "/api/pomodoro" {
        app.pomodoro_action(body["action"].as_str().unwrap_or(""))?;
    } else if path == "/api/pomodoro/settings" {
        app.pomodoro_settings(body)?;
    } else if path == "/api/tokens" {
        app.set_tokens(body)?;
    } else if path == "/api/port" {
        app.use_port(body["port"].as_str().unwrap_or("auto"))?;
    } else if let Some(name) = path.strip_prefix("/api/notify") {
        // POST /api/notify {"event": "..."} from the page, or POST /api/notify/<event> from hooks
        let name = name.trim_start_matches('/');
        let name = if name.is_empty() { body["event"].as_str().or(body["state"].as_str()).unwrap_or("") } else { name };
        match name {
            "dismiss" | "clear" => { app.dismiss(); }
            "resume" => { app.resume_from_claude(); }
            _ => app.notify(name)?,
        }
    } else if path == "/api/hooks" {
        // connect or disconnect Claude Code
        if body["install"].as_bool().unwrap_or(true) { crate::hooks::install()? } else { crate::hooks::uninstall()? }
    } else if path == "/api/playback" {
        app.set_playback(body["mode"].as_str().unwrap_or(""))?;
    } else if path == "/api/command" {
        let cmd = body["cmd"].as_str().unwrap_or("");
        let args = strings(&body["args"]);
        if !protocol::COMMANDS.contains(&cmd) {
            return Err(format!("Unknown command: {cmd}"));
        }
        if ["brightness", "volume", "time"].contains(&cmd) {
            app.command_only(cmd, &args)?; // the picture stays, so the current mode stays
        } else {
            let cycling = cmd == "light" && args.get(2).map(|a| a != "0").unwrap_or(false);
            let faces = match cmd {
                "light" if !cycling => args.first().map(|c| vec![vec![format!("#{}", c.trim_start_matches('#').to_lowercase()); 256]]),
                "off" => Some(vec![vec!["#000000".to_string(); 256]]),
                _ => None,
            };
            app.manual(Base::Command { name: cmd.into(), args }, faces, cmd)?;
        }
    } else if path == "/api/pixels" {
        let frame = Frame::from_hex(&strings(&body["pixels"]))?;
        app.manual(Base::Frames { frames: vec![frame.clone()], durations: vec![4000] }, Some(vec![frame.hex()]), "pixels")?;
    } else {
        return Ok(false);
    }
    Ok(true)
}
