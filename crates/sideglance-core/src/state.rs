//! What is on the display, in two layers: the base (AI usage, or something sent from the page)
//! and a notification on top. When a notification ends or is dismissed, the base comes back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::config::Config;
use crate::device::Device;
use crate::frame::Frame;
use crate::notifications::{self, Event};
use crate::playback;
use crate::render;
use crate::usage::{self, Account};

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

struct Inner {
    device: Box<dyn Device>,
    mode: &'static str, // "manual" or "usage"
    base: Option<Base>,
    base_faces: Option<Vec<Vec<String>>>,
    showing: Option<String>,
    frame_ms: u32,
    usage: Option<(Vec<Account>, Vec<String>)>,
    usage_at: Option<SystemTime>,
    error: Option<String>,
    notification: Option<Notification>,
}

/// Fetches usage for every signed-in account: (accounts, notes for accounts that failed).
pub type Fetch = Box<dyn Fn(&Config) -> Result<(Vec<Account>, Vec<String>), String> + Send + Sync>;

/// Shared by the HTTP server, the usage loop and the tray.
pub struct App {
    inner: Mutex<Inner>,
    pub config: Arc<Config>,
    /// Bumped whenever a mode or a notification starts, so older threads know to stop.
    mode_generation: AtomicU64,
    notification_generation: AtomicU64,
    /// The usage fetch, a parameter so tests need no network.
    fetch: Fetch,
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn fetch_usage(config: &Config) -> Result<(Vec<Account>, Vec<String>), String> {
    usage::fetch_all(config, &usage::http_get, unix_now())
}

impl App {
    pub fn new(
        device: Box<dyn Device>,
        config: Arc<Config>,
        fetch: Fetch,
    ) -> Arc<App> {
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
            }),
            config,
            mode_generation: AtomicU64::new(0),
            notification_generation: AtomicU64::new(0),
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

    /// Replace the base layer. It is drawn now unless a notification is on top.
    fn set_base(&self, base: Base, faces: Option<Vec<Vec<String>>>, showing: &str, frame_ms: u32) -> Result<(), String> {
        let mut inner = self.lock();
        inner.base = Some(base.clone());
        inner.base_faces = faces;
        inner.showing = Some(showing.to_string());
        inner.frame_ms = frame_ms;
        if inner.notification.is_none() {
            Self::put(&mut inner, &base)
        } else {
            Ok(())
        }
    }

    fn restore_base(inner: &mut Inner) {
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
        if let Some(base) = inner.base.clone() {
            Self::put(&mut inner, &base)?;
        }
        Ok(())
    }

    // ----- base layer modes -----

    fn new_mode(&self, mode: &'static str) -> u64 {
        let generation = self.mode_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut inner = self.lock();
        inner.mode = mode;
        if mode == "manual" && inner.showing.as_deref() == Some("usage") {
            inner.showing = None;
        }
        generation
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
        while self.mode_generation.load(Ordering::SeqCst) == generation {
            let wait = match (self.fetch)(&self.config) {
                Ok((accounts, notes)) => {
                    if self.mode_generation.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let faces = render::usage_faces(&accounts);
                    {
                        let mut inner = self.lock();
                        inner.usage = Some((accounts, notes));
                        inner.usage_at = Some(SystemTime::now());
                    }
                    let hex = faces.iter().map(|f| f.hex()).collect();
                    let durations = vec![render::FACE_MS; faces.len()];
                    let _ = self.set_base(Base::Frames { frames: faces, durations }, Some(hex), "usage", render::FACE_MS);
                    usage::REFRESH_SECONDS
                }
                Err(e) => {
                    self.lock().error = Some(e);
                    20
                }
            };
            // wait in short steps so a stopped mode ends quickly
            let until = Instant::now() + Duration::from_secs(wait);
            while Instant::now() < until {
                if self.mode_generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }

    /// Something sent by hand from the page: stops automatic modes and removes any notification.
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
            // plays once, then the previous screen comes back
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
        Self::restore_base(&mut inner);
        true
    }

    /// Remove any notification and put the base back. Returns true if one was removed.
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
        let mut status = json!({
            "mode": inner.mode,
            "port": inner.device.port_id(),
            "port_setting": self.port_setting(),
            "usage": usage,
            "accounts": {"claude": usage::claude_source(&self.config), "codex": usage::codex_source(&self.config)},
            "faces": inner.base_faces,
            "frame_ms": inner.frame_ms,
            "error": inner.error,
            "showing": inner.showing,
            "timer_left": null,
            "playback": self.playback_mode(),
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
        json!({
            "version": 2,
            "showing": inner.showing,
            "notification": notification,
            "usage": {"accounts": accounts, "notes": inner.usage.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(),
                      "age_seconds": inner.usage_at.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs())},
            "timer": null,
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

    /// For tests: run something with the device.
    pub fn with_device<R>(&self, f: impl FnOnce(&mut dyn Device) -> R) -> R {
        f(self.lock().device.as_mut())
    }
}
