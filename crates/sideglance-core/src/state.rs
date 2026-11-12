//! What is on the display, in three layers:
//!   base          AI usage, the weather, the countdown, or something sent from the page
//!   Pomodoro      over the base while a Pomodoro is on
//!   notification  on top of everything; when it ends or is dismissed, what was under it comes back

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::config::Config;
use crate::device::Device;
use crate::frame::Frame;
use crate::notifications::{self, Event};
use crate::playback;
use crate::pomodoro::{self, Pomodoro};
use crate::render;
use crate::usage::{self, Account};
use crate::weather::{self, Reading, Tracker};

/// The Pomodoro face is redrawn at most once a second, and only when a pixel changed.
/// One frame is a single packet of about 130 bytes.
pub const POMODORO_TICK: Duration = Duration::from_secs(1);

/// What the base layer shows, as data, so it can be put back after a notification or a reconnect.
#[derive(Clone)]
pub enum Base {
    Frames { frames: Vec<Frame>, durations: Vec<u32> },
    Command { name: String, args: Vec<String> },
}

struct Notification {
    event: &'static Event,
    steps: Vec<(Frame, u32)>,
    started: Instant,
    id: u64,
}

#[derive(Clone)]
struct Saved {
    mode: &'static str,
    base: Option<Base>,
    faces: Option<Vec<Vec<String>>>,
    showing: Option<String>,
    frame_ms: u32,
}

struct Inner {
    device: Box<dyn Device>,
    mode: &'static str, // manual, usage, weather or timer
    base: Option<Base>,
    base_faces: Option<Vec<Vec<String>>>,
    showing: Option<String>,
    frame_ms: u32,
    usage: Option<(Vec<Account>, Vec<String>)>,
    usage_at: Option<SystemTime>,
    error: Option<String>,
    notification: Option<Notification>,
    // countdown timer
    timer_end: Option<Instant>,
    timer_total: i64,
    before_timer: Option<Saved>,
    // the last weather reading, copied here so status never waits on the network
    weather_reading: Option<Reading>,
    weather_error: Option<String>,
    weather_place: (Option<f64>, Option<f64>, String),
    // Pomodoro
    pomodoro: Pomodoro,
    pomodoro_sent: Option<Frame>,
}

/// Fetches usage for every signed-in account: (accounts, notes for accounts that failed).
pub type Fetch = Box<dyn Fn(&Config) -> Result<(Vec<Account>, Vec<String>), String> + Send + Sync>;

/// Shared by the HTTP server, the loops and the tray.
pub struct App {
    inner: Mutex<Inner>,
    pub config: Arc<Config>,
    weather: Mutex<Tracker>,
    /// Bumped whenever a mode or a notification starts, so older threads know to stop.
    mode_generation: AtomicU64,
    notification_generation: AtomicU64,
    pomodoro_loop: AtomicU64,
    fetch: Fetch,
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn fetch_usage(config: &Config) -> Result<(Vec<Account>, Vec<String>), String> {
    usage::fetch_all(config, &usage::http_get, unix_now())
}

fn monotonic_seconds() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

impl App {
    pub fn new(device: Box<dyn Device>, config: Arc<Config>, fetch: Fetch) -> Arc<App> {
        App::with_parts(device, config, fetch, Tracker::live(), Box::new(monotonic_seconds))
    }

    /// With a weather tracker and a Pomodoro clock of your own, for tests.
    pub fn with_parts(device: Box<dyn Device>, config: Arc<Config>, fetch: Fetch, tracker: Tracker,
                      pomodoro_clock: Box<dyn Fn() -> f64 + Send + Sync>) -> Arc<App> {
        let pomodoro = Pomodoro::new(pomodoro_clock, &pomodoro::settings(&config));
        Arc::new(App {
            inner: Mutex::new(Inner {
                device,
                mode: "manual",
                base: None,
                base_faces: None,
                showing: None,
                frame_ms: render::FACE_MS,
                usage: None,
                usage_at: None,
                error: None,
                notification: None,
                timer_end: None,
                timer_total: 0,
                before_timer: None,
                weather_reading: None,
                weather_error: None,
                weather_place: (None, None, String::new()),
                pomodoro,
                pomodoro_sent: None,
            }),
            config,
            weather: Mutex::new(tracker),
            mode_generation: AtomicU64::new(0),
            notification_generation: AtomicU64::new(0),
            pomodoro_loop: AtomicU64::new(0),
            fetch,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn put(inner: &mut Inner, base: &Base) -> Result<(), String> {
        let result = match base {
            Base::Frames { frames, durations } => inner.device.show(frames, durations),
            Base::Command { name, args } => inner.device.command(name, args),
        };
        inner.error = result.as_ref().err().cloned();
        result
    }

    /// Replace the base layer. It is drawn now unless something is on top of it.
    fn set_base(&self, base: Base, faces: Option<Vec<Vec<String>>>, showing: &str, frame_ms: u32) -> Result<(), String> {
        let mut inner = self.lock();
        inner.base = Some(base.clone());
        inner.base_faces = faces;
        inner.showing = Some(showing.to_string());
        inner.frame_ms = frame_ms;
        if inner.notification.is_none() && !inner.pomodoro.active() {
            Self::put(&mut inner, &base)
        } else {
            Ok(())
        }
    }

    fn set_frames(&self, frames: Vec<Frame>, showing: &str, frame_ms: u32) -> Result<(), String> {
        let hex = frames.iter().map(|f| f.hex()).collect();
        let durations = vec![frame_ms; frames.len()];
        self.set_base(Base::Frames { frames, durations }, Some(hex), showing, frame_ms)
    }

    /// Put back what belongs under the notification: the Pomodoro face if one is on, else the base.
    fn restore_base(&self, inner: &mut Inner) {
        if inner.pomodoro.active() {
            inner.pomodoro_sent = None;
            self.draw_pomodoro(inner);
            return;
        }
        let base = inner.base.clone().unwrap_or(Base::Frames { frames: vec![Frame::default()], durations: vec![4000] });
        let _ = Self::put(inner, &base);
    }

    // ----- startup and connection -----

    pub fn try_connect(&self) {
        let mut inner = self.lock();
        let result = inner.device.connect();
        inner.error = result.err();
    }

    /// Change the port: save it and reconnect, then put the screen back.
    pub fn use_port(&self, port: &str) -> Result<(), String> {
        let port = if port.trim().is_empty() { "auto" } else { port.trim() };
        self.config.update(&[("port", json!(port))]).map_err(|e| e.to_string())?;
        let mut inner = self.lock();
        inner.device.close();
        let result = inner.device.connect();
        inner.error = result.clone().err();
        result?;
        if inner.notification.is_none() {
            self.restore_base(&mut inner); // the timers kept counting here; redraw from them
        }
        Ok(())
    }

    // ----- base layer modes -----

    fn new_mode(&self, mode: &'static str) -> u64 {
        let generation = self.mode_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut inner = self.lock();
        inner.mode = mode;
        if mode == "manual" && matches!(inner.showing.as_deref(), Some("usage" | "weather" | "timer")) {
            inner.showing = None;
        }
        if mode != "timer" {
            inner.timer_end = None;
        }
        generation
    }

    fn current_generation(&self, generation: u64) -> bool {
        self.mode_generation.load(Ordering::SeqCst) == generation
    }

    /// Sleep in short steps; false as soon as the mode has changed.
    fn wait_in_mode(&self, generation: u64, seconds: f64) -> bool {
        let until = Instant::now() + Duration::from_secs_f64(seconds);
        while Instant::now() < until {
            if !self.current_generation(generation) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.current_generation(generation)
    }

    /// Start or stop showing AI usage. Usage refreshes every 5 minutes on its own thread.
    pub fn set_usage(self: &Arc<Self>, on: bool) {
        if !on {
            self.new_mode("manual");
            return;
        }
        let generation = self.new_mode("usage");
        let app = Arc::clone(self);
        std::thread::spawn(move || app.usage_loop(generation));
    }

    fn usage_loop(&self, generation: u64) {
        while self.current_generation(generation) {
            let wait = match (self.fetch)(&self.config) {
                Ok((accounts, notes)) => {
                    if !self.current_generation(generation) {
                        return;
                    }
                    let mut faces = render::usage_faces(&accounts);
                    {
                        let mut inner = self.lock();
                        inner.usage = Some((accounts, notes));
                        inner.usage_at = Some(SystemTime::now());
                    }
                    if weather::settings(&self.config).in_rotation {
                        // optional extra face, after the usage faces
                        let r = self.weather_now();
                        faces.push(render::weather_face(&r.condition, r.temperature));
                    }
                    let _ = self.set_frames(faces, "usage", render::FACE_MS);
                    usage::REFRESH_SECONDS as f64
                }
                Err(e) => {
                    self.lock().error = Some(e);
                    20.0
                }
            };
            if !self.wait_in_mode(generation, wait) {
                return;
            }
        }
    }

    // ----- weather -----

    /// A reading, fetching if one is due. Network calls happen outside the screen lock.
    fn weather_now(&self) -> Reading {
        let s = weather::settings(&self.config);
        let (reading, error, place) = {
            let mut t = self.weather.lock().unwrap_or_else(|p| p.into_inner());
            let r = t.current(&s);
            (r, t.error.clone(), t.place(&s))
        };
        let mut inner = self.lock();
        inner.weather_reading = Some(reading.clone());
        inner.weather_error = error;
        inner.weather_place = place;
        reading
    }

    pub fn set_weather(self: &Arc<Self>, on: bool) {
        if !on {
            self.new_mode("manual");
            return;
        }
        let generation = self.new_mode("weather");
        let app = Arc::clone(self);
        std::thread::spawn(move || {
            while app.current_generation(generation) {
                let r = app.weather_now(); // keeps the last good reading if this fetch fails
                if !app.current_generation(generation) {
                    return;
                }
                let _ = app.set_frames(vec![render::weather_face(&r.condition, r.temperature)], "weather", 4000);
                let next = app.weather.lock().map(|t| t.seconds_to_next()).unwrap_or(60.0);
                if !app.wait_in_mode(generation, next.max(5.0)) {
                    return;
                }
            }
        });
    }

    /// Change weather settings. A city name is looked up with Open-Meteo.
    pub fn weather_settings(self: &Arc<Self>, req: &Value) -> Result<(), String> {
        let mut changes = req.clone();
        if let Some(city) = req["city"].as_str().filter(|c| !c.trim().is_empty()) {
            let get = weather::http_get_json;
            let place = weather::find_place(city, &get)?;
            changes = json!({"latitude": place.latitude, "longitude": place.longitude, "place": place.place});
        }
        let before = weather::settings(&self.config);
        let after = weather::save_settings(&self.config, &changes)?;
        let mode = self.lock().mode;
        if mode == "weather" {
            self.set_weather(true); // draw the new reading now
        } else if mode == "usage" && (before.in_rotation || after.in_rotation) {
            self.set_usage(true); // add or remove the weather face, or redraw it
        }
        Ok(())
    }

    // ----- countdown timer -----

    pub fn set_timer(self: &Arc<Self>, minutes: i64) {
        if minutes <= 0 {
            if self.lock().mode == "timer" {
                self.leave_timer();
            }
            return;
        }
        {
            let mut inner = self.lock();
            if inner.mode != "timer" {
                inner.before_timer = Some(Saved { mode: inner.mode, base: inner.base.clone(), faces: inner.base_faces.clone(),
                                                  showing: inner.showing.clone(), frame_ms: inner.frame_ms });
            }
        }
        let generation = self.new_mode("timer");
        {
            let mut inner = self.lock();
            inner.timer_total = minutes.min(99) * 60;
            inner.timer_end = Some(Instant::now() + Duration::from_secs(inner.timer_total as u64));
        }
        let app = Arc::clone(self);
        std::thread::spawn(move || app.timer_loop(generation));
    }

    fn timer_left(inner: &Inner) -> Option<i64> {
        inner.timer_end.map(|end| end.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as i64)
    }

    fn timer_loop(self: Arc<Self>, generation: u64) {
        let mut shown = -1;
        loop {
            let (left, total) = {
                let inner = self.lock();
                (Self::timer_left(&inner).unwrap_or(0), inner.timer_total)
            };
            if !self.current_generation(generation) {
                return;
            }
            if left <= 0 {
                break;
            }
            let minutes = (left + 59) / 60;
            if minutes != shown {
                shown = minutes;
                let _ = self.set_frames(vec![render::timer_frame(left, total)], "timer", 4000);
            }
            if !self.wait_in_mode(generation, 1.0) {
                return;
            }
        }
        let _ = self.notify("timer-finished"); // first, so the previous screen does not flash before it
        self.leave_timer();
    }

    /// Go back to what was on screen before the timer started.
    fn leave_timer(self: &Arc<Self>) {
        let saved = self.lock().before_timer.take();
        match saved.as_ref().map(|s| s.mode) {
            Some("usage") => return self.set_usage(true),
            Some("weather") => return self.set_weather(true),
            _ => {}
        }
        self.new_mode("manual");
        let mut inner = self.lock();
        if let Some(s) = saved {
            inner.base = s.base;
            inner.base_faces = s.faces;
            inner.showing = s.showing;
            inner.frame_ms = s.frame_ms;
        } else {
            inner.base = None;
            inner.base_faces = None;
            inner.showing = None;
        }
        if inner.notification.is_none() {
            self.restore_base(&mut inner);
        }
    }

    // ----- Pomodoro layer -----

    fn draw_pomodoro(&self, inner: &mut Inner) {
        if inner.notification.is_some() || !inner.pomodoro.active() {
            return;
        }
        let s = pomodoro::settings(&self.config);
        let v = inner.pomodoro.view(&s);
        let frame = render::pomodoro_face(inner.pomodoro.phase, v["seconds_left"].as_i64().unwrap_or(0),
                                          inner.pomodoro.completed, inner.pomodoro.status == "paused");
        if inner.pomodoro_sent.as_ref() == Some(&frame) {
            return; // nothing changed: send nothing
        }
        let result = inner.device.show(std::slice::from_ref(&frame), &[4000]);
        inner.error = result.as_ref().err().cloned();
        inner.pomodoro_sent = if result.is_ok() { Some(frame) } else { None }; // retried next tick
    }

    pub fn pomodoro_action(self: &Arc<Self>, action: &str) -> Result<(), String> {
        let s = pomodoro::settings(&self.config);
        {
            let mut inner = self.lock();
            match action {
                "show" => inner.pomodoro.show(),
                "start" => inner.pomodoro.start(),
                "pause" => inner.pomodoro.pause(),
                "resume" => inner.pomodoro.resume(),
                "skip" => inner.pomodoro.skip(&s),
                "reset" => inner.pomodoro.reset(&s),
                _ => return Err("Pomodoro actions: show, start, pause, resume, skip, reset".into()),
            }
        }
        // any action on the timer clears the timer-finished blink, and puts back what belongs on screen:
        // the Pomodoro face, or after a reset the base
        let mut inner = self.lock();
        inner.pomodoro_sent = None;
        if inner.notification.take().is_some() {
            self.notification_generation.fetch_add(1, Ordering::SeqCst);
        }
        self.restore_base(&mut inner);
        let active = inner.pomodoro.active();
        drop(inner);
        if active {
            self.ensure_pomodoro_loop();
        }
        Ok(())
    }

    pub fn pomodoro_settings(&self, req: &Value) -> Result<(), String> {
        let s = pomodoro::save_settings(&self.config, req)?;
        let mut inner = self.lock();
        inner.pomodoro.settings_changed(&s);
        self.draw_pomodoro(&mut inner);
        Ok(())
    }

    fn ensure_pomodoro_loop(self: &Arc<Self>) {
        let id = self.pomodoro_loop.fetch_add(1, Ordering::SeqCst) + 1;
        let app = Arc::clone(self);
        std::thread::spawn(move || {
            while app.pomodoro_loop.load(Ordering::SeqCst) == id {
                if !app.pomodoro_step() {
                    return;
                }
                std::thread::sleep(POMODORO_TICK / 4); // a quarter second, so the face changes on time
            }
        });
    }

    /// One tick: end the phase if its time is up (with the notification), then redraw if needed.
    /// False when the Pomodoro is off.
    pub fn pomodoro_step(self: &Arc<Self>) -> bool {
        let s = pomodoro::settings(&self.config);
        let finished = {
            let mut inner = self.lock();
            if !inner.pomodoro.active() {
                return false;
            }
            inner.pomodoro.tick(&s)
        };
        if finished {
            let _ = self.notify("timer-finished");
        }
        let mut inner = self.lock();
        self.draw_pomodoro(&mut inner);
        true
    }

    // ----- something sent by hand -----

    /// From the page: stops automatic modes and removes any notification.
    pub fn manual(&self, base: Base, faces: Option<Vec<Vec<String>>>, showing: &str) -> Result<(), String> {
        self.new_mode("manual");
        self.end_notification();
        self.set_base(base, faces, showing, 4000)
    }

    /// A command that does not change the picture, such as brightness or volume.
    pub fn command_only(&self, name: &str, args: &[String]) -> Result<(), String> {
        let mut inner = self.lock();
        let result = inner.device.command(name, args);
        inner.error = result.clone().err();
        result
    }

    // ----- notifications -----

    pub fn playback_mode(&self) -> String {
        let mode = self.config.string("playback");
        if playback::MODES.contains(&mode.as_str()) { mode } else { "device".into() }
    }

    /// Show a notification over whatever is on screen. Replaces any notification already showing.
    pub fn notify(self: &Arc<Self>, name: &str) -> Result<(), String> {
        let event = notifications::get(name)?;
        let steps = notifications::steps(event);
        let id = self.notification_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let host = self.playback_mode() == "host";
        {
            let mut inner = self.lock();
            inner.notification = Some(Notification { event, steps: steps.clone(), started: Instant::now(), id });
            inner.pomodoro_sent = None;
            if !host {
                let (frames, durations) = playback::device_plan(&steps, event.looping, inner.device.max_frame_ms());
                let result = inner.device.show(&frames, &durations);
                inner.error = result.err();
            }
        }
        if host {
            let app = Arc::clone(self);
            let looping = event.looping;
            std::thread::spawn(move || {
                let still = |id: u64| app.notification_generation.load(Ordering::SeqCst) == id;
                playback::play_on_host(
                    &steps,
                    looping,
                    |frame| {
                        let mut inner = app.lock();
                        if still(id) {
                            let _ = inner.device.show(std::slice::from_ref(frame), &[4000]);
                        }
                    },
                    |seconds| {
                        let until = Instant::now() + Duration::from_secs_f64(seconds);
                        while Instant::now() < until {
                            if !still(id) {
                                return true;
                            }
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        !still(id)
                    },
                );
            });
        }
        if !event.looping {
            let app = Arc::clone(self);
            let total = Duration::from_millis(event.total_ms());
            std::thread::spawn(move || {
                std::thread::sleep(total);
                app.dismiss_if(|n| n.id == id);
            });
        }
        Ok(())
    }

    fn end_notification(&self) {
        self.notification_generation.fetch_add(1, Ordering::SeqCst);
        self.lock().notification = None;
    }

    fn dismiss_if(&self, matches: impl Fn(&Notification) -> bool) -> bool {
        let mut inner = self.lock();
        match &inner.notification {
            Some(n) if matches(n) => {}
            _ => return false,
        }
        self.notification_generation.fetch_add(1, Ordering::SeqCst);
        inner.notification = None;
        self.restore_base(&mut inner);
        true
    }

    /// Remove any notification and put back what was under it. True if one was removed.
    pub fn dismiss(&self) -> bool {
        self.dismiss_if(|_| true)
    }

    /// From Claude Code hooks: you are back, so Claude Code notifications go away.
    pub fn resume_from_claude(&self) -> bool {
        self.dismiss_if(|n| n.event.from_claude)
    }

    // ----- settings from the page -----

    /// Tokens entered on the page. An empty value removes it. Tokens are never sent back.
    pub fn set_tokens(self: &Arc<Self>, req: &Value) -> Result<(), String> {
        let mut changes = Vec::new();
        for key in ["claude_token", "codex_token", "codex_account_id"] {
            if let Some(v) = req.get(key) {
                let text = v.as_str().unwrap_or("").trim().to_string();
                changes.push((key, if text.is_empty() { Value::Null } else { json!(text) }));
            }
        }
        self.config.update(&changes).map_err(|e| e.to_string())?;
        if self.lock().mode == "usage" {
            self.set_usage(true); // fetch again with the new tokens
        }
        Ok(())
    }

    pub fn set_playback(&self, mode: &str) -> Result<(), String> {
        if !playback::MODES.contains(&mode) {
            return Err(format!("Playback is one of: {}", playback::MODES.join(", ")));
        }
        self.config.update(&[("playback", json!(mode))]).map_err(|e| e.to_string())
    }

    // ----- what the outside sees -----

    fn weather_status(&self, inner: &Inner) -> Value {
        let s = weather::settings(&self.config);
        let reading = inner.weather_reading.clone().filter(|r| r.unit == s.unit)
            .unwrap_or_else(|| weather::empty_reading(&s.unit, &inner.weather_place.2));
        let (lat, lon, place) = if s.auto { inner.weather_place.clone() } else { (s.latitude, s.longitude, s.place.clone()) };
        json!({
            "auto": s.auto, "latitude": lat, "longitude": lon, "place": place, "unit": s.unit, "in_rotation": s.in_rotation,
            "system_location": weather::system_location(), "reading": reading, "error": inner.weather_error,
            "face": render::weather_face(&reading.condition, reading.temperature).hex(),
        })
    }

    fn pomodoro_status(&self, inner: &Inner) -> Value {
        let s = pomodoro::settings(&self.config);
        let mut v = inner.pomodoro.view(&s);
        let face = render::pomodoro_face(inner.pomodoro.phase, inner.pomodoro.seconds_left(), inner.pomodoro.completed,
                                         inner.pomodoro.status == "paused");
        v["active"] = json!(inner.pomodoro.active());
        v["settings"] = json!(s);
        v["face"] = json!(face.hex());
        v
    }

    /// Everything the control page needs, including pixel previews. Tokens are never included.
    pub fn status(&self) -> Value {
        let inner = self.lock();
        let usage = inner.usage.as_ref().map(|(accounts, notes)| {
            let providers: Vec<Value> = accounts.iter().map(|a| {
                let mut v = serde_json::to_value(a).unwrap();
                v["color"] = json!(render::account_color(&a.id));
                v
            }).collect();
            json!({"providers": providers, "notes": notes})
        });
        let pomodoro = self.pomodoro_status(&inner);
        let faces = if inner.pomodoro.active() { Some(vec![pomodoro["face"].clone()]) } else { None };
        let mut status = json!({
            "app": "sideglance-app",
            "version": env!("CARGO_PKG_VERSION"),
            "mode": inner.mode,
            "port": inner.device.port_id(),
            "port_setting": self.port_setting(),
            "usage": usage,
            "accounts": {"claude": usage::claude_source(&self.config), "codex": usage::codex_source(&self.config)},
            "faces": faces.map(Value::from).unwrap_or_else(|| json!(inner.base_faces)),
            "frame_ms": inner.frame_ms,
            "error": inner.error,
            "showing": inner.showing,
            "timer_left": Self::timer_left(&inner),
            "playback": self.playback_mode(),
            "weather": self.weather_status(&inner),
            "pomodoro": pomodoro,
            "notification": null,
        });
        if let Some(n) = &inner.notification {
            status["notification"] = json!(n.event.id);
            status["notification_label"] = json!(n.event.label);
            status["notification_loop"] = json!(n.event.looping);
            status["notification_frames"] = json!(n.steps.iter().map(|(f, _)| f.hex()).collect::<Vec<_>>());
            status["notification_ms"] = json!(n.steps.iter().map(|(_, ms)| ms).collect::<Vec<_>>());
            status["notification_elapsed_ms"] = json!(n.started.elapsed().as_millis() as u64);
        }
        status
    }

    pub fn port_setting(&self) -> String {
        let p = self.config.string("port");
        if p.is_empty() { "auto".into() } else { p }
    }

    /// What should be on a display, with no pixels. For displays that draw for themselves.
    pub fn scene(&self) -> Value {
        let inner = self.lock();
        let accounts: Vec<Value> = inner.usage.as_ref().map(|(accounts, _)| accounts.iter().map(|a| json!({
            "id": a.id, "name": a.name, "windows": [
                {"id": "5h", "label": "D", "used_percent": a.pct_d, "resets_in_seconds": a.rem_d, "window_seconds": a.win_d},
                {"id": "7d", "label": "W", "used_percent": a.pct_w, "resets_in_seconds": a.rem_w, "window_seconds": a.win_w},
            ]})).collect()).unwrap_or_default();
        let notification = inner.notification.as_ref().map(|n| {
            let elapsed = n.started.elapsed().as_millis() as u64;
            json!({
                "id": n.event.id, "label": n.event.label, "loop": n.event.looping,
                "steps": n.event.steps.iter().map(|s| json!({"ms": s.ms, "hide": match &s.hide {
                    notifications::Hide::Nothing => "", notifications::Hide::Chars(c) => c, notifications::Hide::All => "*"}})).collect::<Vec<_>>(),
                "elapsed_ms": elapsed,
                "seconds_left": if n.event.looping { Value::Null } else { json!(n.event.total_ms().saturating_sub(elapsed) / 1000) },
            })
        });
        let w = inner.weather_reading.clone();
        let s = pomodoro::settings(&self.config);
        let mut pomodoro = inner.pomodoro.view(&s);
        pomodoro["active"] = json!(inner.pomodoro.active());
        pomodoro["phase_seconds"] = json!(inner.pomodoro.phase_seconds(&s));
        json!({
            "version": 2,
            "showing": inner.showing,
            "notification": notification,
            "usage": {"accounts": accounts, "notes": inner.usage.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(),
                      "age_seconds": inner.usage_at.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs())},
            "timer": Self::timer_left(&inner).map(|left| json!({"seconds_left": left, "total_seconds": inner.timer_total})),
            "weather": w.map(|r| json!({"condition": r.condition, "temperature": r.temperature, "unit": r.unit, "code": r.code,
                "is_day": r.is_day, "place": r.place,
                "age_seconds": r.fetched_at.map(|t| (weather::wall_clock() - t).max(0.0) as i64)})),
            "pomodoro": pomodoro,
        })
    }

    /// Every notification with its frames, for the page's previews.
    pub fn notification_previews(&self) -> Value {
        json!(notifications::events().iter().map(|e| {
            let steps = notifications::steps(e);
            json!({"id": e.id, "label": e.label, "behavior": e.behavior, "loop": e.looping,
                   "ms": steps.iter().map(|(_, ms)| ms).collect::<Vec<_>>(),
                   "frames": steps.iter().map(|(f, _)| f.hex()).collect::<Vec<_>>()})
        }).collect::<Vec<_>>())
    }

    pub fn mode(&self) -> &'static str {
        self.lock().mode
    }

    pub fn close_device(&self) {
        self.lock().device.close();
    }
}
