//! Screen layers and the HTTP routes, with a recording device and a fake usage fetch. No network.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use sideglance_core::config::Config;
use sideglance_core::device::Device;
use sideglance_core::frame::Frame;
use sideglance_core::http::{self, origin_kind, OriginKind};
use sideglance_core::state::App;
use sideglance_core::usage::{Account, WINDOW_D, WINDOW_W};

#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);

struct Recorder(Log);

impl Device for Recorder {
    fn connect(&mut self) -> Result<(), String> { Ok(()) }
    fn close(&mut self) {}
    fn port_id(&self) -> Option<String> { Some("test".into()) }
    fn max_frame_ms(&self) -> u32 { 65535 }
    fn show(&mut self, frames: &[Frame], durations: &[u32]) -> Result<(), String> {
        self.0 .0.lock().unwrap().push(format!("show {} {:?}", frames.len(), durations));
        Ok(())
    }
    fn command(&mut self, name: &str, args: &[String]) -> Result<(), String> {
        self.0 .0.lock().unwrap().push(format!("{name} {}", args.join(" ")));
        Ok(())
    }
}

fn account() -> Account {
    Account { id: "claude".into(), name: "Claude".into(), pct_d: 42.0, rem_d: Some(3600), win_d: WINDOW_D,
              pct_w: 78.0, rem_w: None, win_w: WINDOW_W }
}

fn setup() -> (Arc<App>, Log, std::path::PathBuf) {
    let log = Log::default();
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("sg-state-{n}-{:?}", std::thread::current().id()));
    let config = Arc::new(Config::at(dir.join("config.json")));
    let app = App::new(Box::new(Recorder(log.clone())), config, Box::new(|_| Ok((vec![account()], vec![]))));
    (app, log, dir)
}

fn last(log: &Log) -> String {
    log.0.lock().unwrap().last().cloned().unwrap_or_default()
}

fn wait_for(f: impl Fn() -> bool) {
    for _ in 0..1000 {
        if f() { return; }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out");
}

#[test]
fn usage_shows_two_faces_per_account() {
    let (app, log, _d) = setup();
    app.set_usage(true);
    wait_for(|| last(&log).starts_with("show 2"));
    assert_eq!(last(&log), "show 2 [4000, 4000]");
    let status = app.status();
    assert_eq!(status["mode"], "usage");
    assert_eq!(status["usage"]["providers"][0]["color"], json!([240, 130, 90]));
    app.set_usage(false);
}

#[test]
fn notification_goes_over_the_base_and_dismiss_puts_it_back() {
    let (app, log, _d) = setup();
    app.manual(sideglance_core::state::Base::Command { name: "clock".into(), args: vec![] }, None, "clock").unwrap();
    app.notify("claude-waiting").unwrap();
    assert_eq!(last(&log), "show 2 [500, 500]");
    app.manual(sideglance_core::state::Base::Command { name: "light".into(), args: vec!["ff0000".into()] }, None, "light").unwrap();
    assert_eq!(last(&log), "light ff0000");     // something chosen by hand removes the notification
    app.notify("job-failed").unwrap();
    assert_eq!(last(&log), "show 7 [300, 200, 300, 200, 300, 200, 65535]");
    assert!(app.dismiss());
    assert_eq!(last(&log), "light ff0000");
    assert!(app.status()["notification"].is_null());
}

#[test]
fn resume_from_claude_only_removes_claude_events() {
    let (app, _log, _d) = setup();
    app.notify("job-failed").unwrap();
    assert!(!app.resume_from_claude());
    app.notify("claude-finished").unwrap();
    assert!(app.resume_from_claude());
}

#[test]
fn routes() {
    let (app, log, _d) = setup();
    assert!(http::post(&app, "/api/notify/claude-waiting", &json!({})).unwrap());   // what the hooks send
    assert_eq!(app.status()["notification"], "claude-waiting");
    assert!(http::post(&app, "/api/notify", &json!({"event": "dismiss"})).unwrap());
    assert!(http::post(&app, "/api/notify", &json!({"state": "fail"})).unwrap());   // older field and name
    assert_eq!(app.status()["notification"], "job-failed");
    assert!(http::post(&app, "/api/notify/nope", &json!({})).unwrap_err().contains("Unknown notification"));
    assert!(http::post(&app, "/api/command", &json!({"cmd": "brightness", "args": [30]})).unwrap());
    assert_eq!(last(&log), "brightness 30");
    assert!(http::post(&app, "/api/command", &json!({"cmd": "vj", "args": [1]})).is_err());
    assert!(http::post(&app, "/api/playback", &json!({"mode": "host"})).unwrap());
    assert_eq!(app.status()["playback"], "host");
    assert!(http::post(&app, "/api/playback", &json!({"mode": "nope"})).is_err());
    assert!(!http::post(&app, "/api/nothing", &json!({})).unwrap());
}

#[test]
fn tokens_are_saved_and_never_returned() {
    let (app, _log, _d) = setup();
    http::post(&app, "/api/tokens", &json!({"claude_token": "secret"})).unwrap();
    assert_eq!(app.config.string("claude_token"), "secret");
    let text = app.status().to_string();
    assert!(!text.contains("secret"));
    assert_eq!(app.status()["accounts"]["claude"], "manual");
    http::post(&app, "/api/tokens", &json!({"claude_token": ""})).unwrap();
    assert_eq!(app.config.string("claude_token"), "");
}

#[test]
fn scene_has_usage_and_the_notification() {
    let (app, log, _d) = setup();
    app.set_usage(true);
    wait_for(|| last(&log).starts_with("show 2"));
    app.notify("claude-finished").unwrap();
    let scene = app.scene();
    assert_eq!(scene["version"], 2);
    assert_eq!(scene["usage"]["accounts"][0]["windows"][0]["used_percent"], 42.0);
    assert_eq!(scene["notification"]["id"], "claude-finished");
    app.set_usage(false);
}

#[test]
fn origins() {
    let allowed = vec!["https://sideglance.pages.dev".to_string()];
    assert_eq!(origin_kind(None, &allowed), OriginKind::None);
    assert_eq!(origin_kind(Some("http://localhost:8765"), &allowed), OriginKind::Local);
    assert_eq!(origin_kind(Some("tauri://localhost"), &allowed), OriginKind::Local);
    assert_eq!(origin_kind(Some("http://tauri.localhost"), &allowed), OriginKind::Local);
    assert_eq!(origin_kind(Some("https://sideglance.pages.dev/"), &allowed), OriginKind::Hosted);
    assert_eq!(origin_kind(Some("https://evil.example"), &allowed), OriginKind::Blocked);
}

// ----- weather, countdown and Pomodoro layers -----

use sideglance_core::weather::{Failure, Tracker};

fn setup_with_clock() -> (Arc<App>, Log, Arc<Mutex<f64>>) {
    let log = Log::default();
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("sg-layers-{n}-{:?}", std::thread::current().id()));
    let config = Arc::new(Config::at(dir.join("config.json")));
    config.update(&[("weather_latitude", json!(18.79)), ("weather_longitude", json!(98.98)),
                    ("weather_place", json!("Chiang Mai")), ("weather_auto", json!(false))]).unwrap();
    let get: Box<sideglance_core::weather::GetJson> = Box::new(|url: &str, _: &[(&str, String)]| {
        if url.contains("forecast") {
            Ok(json!({"current": {"temperature_2m": 29.6, "weather_code": 3, "is_day": 1}}))
        } else {
            Err(Failure::Status(404))
        }
    });
    let now = Arc::new(Mutex::new(100.0));
    let c = Arc::clone(&now);
    let tracker = Tracker::new(get, Box::new(|| 1_000.0));
    let app = App::with_parts(Box::new(Recorder(log.clone())), config, Box::new(|_| Ok((vec![account()], vec![]))),
                              tracker, Box::new(move || *c.lock().unwrap()));
    (app, log, now)
}

#[test]
fn weather_mode_draws_the_reading() {
    let (app, log, _now) = setup_with_clock();
    http::post(&app, "/api/weather", &json!({"on": true})).unwrap();
    wait_for(|| app.status()["weather"]["reading"]["temperature"] == json!(29.6));
    wait_for(|| last(&log) == "show 1 [4000]");
    let status = app.status();
    assert_eq!(status["mode"], "weather");
    assert_eq!(status["weather"]["reading"]["condition"], "cloudy");
    assert_eq!(status["weather"]["place"], "Chiang Mai");
    assert_eq!(app.scene()["weather"]["temperature"], json!(29.6));
    app.set_weather(false);
}

#[test]
fn weather_joins_the_usage_rotation_when_asked() {
    let (app, log, _now) = setup_with_clock();
    http::post(&app, "/api/weather/settings", &json!({"in_rotation": true})).unwrap();
    app.set_usage(true);
    wait_for(|| last(&log).starts_with("show 3"));      // two usage faces and the weather face
    app.set_usage(false);
}

#[test]
fn pomodoro_covers_the_base_until_reset() {
    let (app, log, now) = setup_with_clock();
    app.manual(sideglance_core::state::Base::Command { name: "clock".into(), args: vec![] }, None, "clock").unwrap();
    http::post(&app, "/api/pomodoro", &json!({"action": "start"})).unwrap();
    assert_eq!(app.status()["pomodoro"]["status"], "running");
    let shown = log.0.lock().unwrap().len();
    app.manual(sideglance_core::state::Base::Command { name: "light".into(), args: vec!["ff0000".into()] }, None, "light").unwrap();
    assert_eq!(log.0.lock().unwrap().len(), shown);       // hidden under the Pomodoro
    *now.lock().unwrap() += 25.0 * 60.0;
    app.pomodoro_step();
    assert_eq!(app.status()["notification"], "timer-finished");
    assert_eq!(app.status()["pomodoro"]["phase"], "short");
    http::post(&app, "/api/pomodoro", &json!({"action": "reset"})).unwrap();
    assert_eq!(last(&log), "light ff0000");               // the base comes back
    assert!(http::post(&app, "/api/pomodoro", &json!({"action": "nope"})).is_err());
}

#[test]
fn countdown_timer_shows_minutes_and_stops() {
    let (app, log, _now) = setup_with_clock();
    http::post(&app, "/api/timer", &json!({"minutes": 25})).unwrap();
    wait_for(|| last(&log) == "show 1 [4000]");
    let status = app.status();
    assert_eq!((status["mode"].as_str(), status["showing"].as_str()), (Some("timer"), Some("timer")));
    assert!(status["timer_left"].as_i64().unwrap() > 24 * 60);
    http::post(&app, "/api/timer", &json!({"minutes": 0})).unwrap();
    assert_eq!(app.status()["mode"], "manual");
}

#[test]
fn status_says_it_is_the_app() {
    let (app, _log, _now) = setup_with_clock();
    assert_eq!(app.status()["app"], "sideglance-app");
}
