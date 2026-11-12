//! Weather, countdown and Pomodoro faces must match the Python version pixel for pixel
//! (tests/golden/python-faces.json), and the timers and weather logic must behave the same.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use sideglance_core::config::Config;
use sideglance_core::pomodoro::{self, Pomodoro};
use sideglance_core::render;
use sideglance_core::weather::{self, Failure, Tracker};

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/python-faces.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn weather_faces_match_python() {
    for case in golden()["weather_faces"].as_array().unwrap() {
        let t = case["temperature"].as_f64();
        let c = case["condition"].as_str().unwrap();
        assert_eq!(render::temperature_text(t), case["text"].as_str().unwrap(), "{t:?}");
        assert_eq!(render::weather_face(c, t).hex(), strings(&case["pixels"]), "{c} {t:?}");
    }
}

#[test]
fn condition_codes_match_python() {
    for case in golden()["conditions"].as_array().unwrap() {
        let code = case["code"].as_i64();
        let is_day = case["is_day"].as_bool();
        assert_eq!(weather::condition_for(code, is_day), case["condition"].as_str().unwrap(), "{code:?} {is_day:?}");
    }
}

#[test]
fn countdown_faces_match_python() {
    for case in golden()["timer_frames"].as_array().unwrap() {
        let (r, t) = (case["remaining"].as_i64().unwrap(), case["total"].as_i64().unwrap());
        assert_eq!(render::timer_frame(r, t).hex(), strings(&case["pixels"]), "{r}/{t}");
    }
}

#[test]
fn pomodoro_faces_match_python() {
    let g = golden();
    let ring: Vec<(usize, usize)> = g["ring"].as_array().unwrap().iter()
        .map(|p| (p[0].as_u64().unwrap() as usize, p[1].as_u64().unwrap() as usize)).collect();
    assert_eq!(render::ring(), ring);
    for (letter, hex) in g["pomodoro_colors"].as_object().unwrap() {
        let c = render::pomodoro_color(letter.chars().next().unwrap()).unwrap();
        assert_eq!(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]), hex.as_str().unwrap());
    }
    for v in g["pomodoro"].as_array().unwrap() {
        let grid = render::pomodoro_grid(v["phase"].as_str().unwrap(), v["seconds_left"].as_i64().unwrap(),
                                         v["sessions_completed"].as_u64().unwrap() as usize, v["paused"].as_bool().unwrap());
        assert_eq!(grid, strings(&v["grid"]), "{v}");
    }
}

#[test]
fn pomodoro_reference_frame() {
    let want = [
        "RRRRRRRRtttttttt", "R..............t", "R..............t", "R...W.....WWW..t", "R..WW....W...W.t",
        "R...W....W...W.t", "R...W.....WWW..t", "R...W....W...W.t", "R...W....W...W.t", "R..WWW....WWW..t",
        "R..............t", "R.RR.WW..tt.tt.R", "R.RR.WW..tt.tt.R", "R..............R", "R..............R",
        "RRRRRRRRRRRRRRRR"];
    assert_eq!(render::pomodoro_grid("focus", 18 * 60 + 42, 1, false), want);
}

fn clock() -> (Arc<Mutex<f64>>, Box<dyn Fn() -> f64 + Send + Sync>) {
    let now = Arc::new(Mutex::new(500.0));
    let c = Arc::clone(&now);
    (now, Box::new(move || *c.lock().unwrap()))
}

const DEFAULTS: pomodoro::Settings = pomodoro::Settings { focus: 25, short: 5, long: 15, sessions: 4, auto_start: false };

#[test]
fn pomodoro_phases_and_the_long_break() {
    let (now, c) = clock();
    let mut p = Pomodoro::new(c, &DEFAULTS);
    assert_eq!((p.status, p.seconds_left(), p.active()), ("idle", 1500, false));
    let mut phases = Vec::new();
    for _ in 0..9 {
        p.start();
        *now.lock().unwrap() += p.seconds_left() as f64;
        assert!(p.tick(&DEFAULTS));
        assert!(!p.tick(&DEFAULTS));
        phases.push((p.phase, p.completed));
    }
    assert_eq!(phases, [("short", 1), ("focus", 1), ("short", 2), ("focus", 2), ("short", 3), ("focus", 3),
                        ("long", 4), ("focus", 0), ("short", 1)]);
}

#[test]
fn pomodoro_pause_resume_skip_reset_auto() {
    let (now, c) = clock();
    let mut p = Pomodoro::new(c, &DEFAULTS);
    p.start();
    *now.lock().unwrap() += 100.0;
    p.pause();
    *now.lock().unwrap() += 1000.0;
    assert_eq!((p.status, p.seconds_left()), ("paused", 1400));
    p.resume();
    *now.lock().unwrap() += 10.0;
    assert_eq!(p.seconds_left(), 1390);
    p.skip(&DEFAULTS);
    assert_eq!((p.status, p.phase, p.completed), ("waiting", "short", 1));
    let auto = pomodoro::Settings { auto_start: true, ..DEFAULTS };
    p.start();
    *now.lock().unwrap() += 300.0;
    assert!(p.tick(&auto));
    assert_eq!((p.status, p.phase, p.seconds_left()), ("running", "focus", 1500));
    p.reset(&DEFAULTS);
    assert_eq!((p.status, p.completed, p.active()), ("idle", 0, false));
}

fn temp_config() -> Config {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    Config::at(std::env::temp_dir().join(format!("sg-faces-{n}-{:?}/config.json", std::thread::current().id())))
}

#[test]
fn pomodoro_settings_limits() {
    let c = temp_config();
    assert_eq!(pomodoro::settings(&c), DEFAULTS);
    let s = pomodoro::save_settings(&c, &json!({"focus": "50", "sessions": 2, "auto_start": true})).unwrap();
    assert_eq!((s.focus, s.sessions, s.auto_start), (50, 2, true));
    assert!(pomodoro::save_settings(&c, &json!({"sessions": 5})).is_err());
}

type Calls = Arc<Mutex<Vec<String>>>;

fn fake_get(answers: Vec<Result<Value, Failure>>) -> (Calls, Box<weather::GetJson>) {
    let calls: Calls = Arc::default();
    let log = Arc::clone(&calls);
    let queue = Mutex::new(answers.into_iter());
    (calls, Box::new(move |url: &str, _: &[(&str, String)]| {
        log.lock().unwrap().push(url.to_string());
        queue.lock().unwrap().next().expect("no more answers")
    }))
}

fn manual() -> weather::Settings {
    weather::Settings { auto: false, latitude: Some(18.79), longitude: Some(98.98), place: "Chiang Mai".into(),
                        unit: "c".into(), in_rotation: false }
}

fn answer(t: f64, code: i64, day: i64) -> Result<Value, Failure> {
    Ok(json!({"current": {"temperature_2m": t, "weather_code": code, "is_day": day}}))
}

#[test]
fn weather_keeps_the_last_good_reading_and_retries_sooner() {
    let (now, c) = clock();
    let (calls, get) = fake_get(vec![answer(30.2, 61, 1), Err(Failure::Connect("offline".into())), answer(29.0, 0, 0)]);
    let mut t = Tracker::new(get, c);
    let s = manual();
    let first = t.current(&s);
    assert_eq!((first.condition.as_str(), first.temperature), ("rain", Some(30.2)));
    *now.lock().unwrap() += weather::REFRESH_SECONDS;
    assert_eq!(t.current(&s).temperature, Some(30.2));
    assert!(t.error.as_deref().unwrap().contains("Showing the last reading"));
    *now.lock().unwrap() += weather::RETRY_SECONDS;
    let third = t.current(&s);
    assert_eq!((third.condition.as_str(), t.error.is_none()), ("night", true));
    assert_eq!(calls.lock().unwrap().len(), 3);
}

#[test]
fn weather_never_read_shows_cloudy_and_dashes() {
    let (_now, c) = clock();
    let (_calls, get) = fake_get(vec![Err(Failure::Status(500))]);
    let mut t = Tracker::new(get, c);
    let r = t.current(&manual());
    assert_eq!((r.condition.as_str(), r.temperature), ("cloudy", None));
}

#[test]
fn automatic_location_falls_back_to_the_next_service() {
    let (_now, c) = clock();
    let (calls, get) = fake_get(vec![
        Err(Failure::Status(429)),
        Ok(json!({"success": true, "city": "Bangkok", "region": "Bangkok", "country": "Thailand", "latitude": 13.75, "longitude": 100.5})),
        answer(33.0, 3, 1),
    ]);
    let mut t = Tracker::new(get, c);
    let s = weather::Settings { auto: true, latitude: None, longitude: None, place: String::new(), unit: "c".into(), in_rotation: false };
    let r = t.current(&s);
    assert_eq!((r.place.as_str(), r.temperature), ("Bangkok, Thailand", Some(33.0)));
    assert_eq!(calls.lock().unwrap()[1], "https://ipwho.is/");
}

#[test]
fn weather_settings_switch_between_automatic_and_set() {
    let c = temp_config();
    assert!(weather::settings(&c).auto);
    let s = weather::save_settings(&c, &json!({"latitude": 13.75, "longitude": 100.5, "place": "Bangkok"})).unwrap();
    assert_eq!((s.auto, s.place.as_str()), (false, "Bangkok"));
    assert!(weather::save_settings(&c, &json!({"auto": true})).unwrap().auto);
    assert!(weather::save_settings(&c, &json!({"latitude": 95, "longitude": 0})).is_err());
}
