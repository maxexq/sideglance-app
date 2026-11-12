//! Pomodoro timer state, with an injected clock. Nothing here draws or sleeps.
//!
//! Phases: focus, short break, long break. A set is `sessions` focus sessions; the long break comes
//! after the last one, then the count starts again.
//!
//! Status: idle (not started), running, paused, waiting (a phase ended and the next is ready).
//! Config keys (the same as the Python version): pomodoro_focus_minutes (25), pomodoro_short_minutes (5),
//! pomodoro_long_minutes (15), pomodoro_sessions (4), pomodoro_auto_start (false).

use serde_json::{json, Value};

use crate::config::Config;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Settings {
    pub focus: i64,
    pub short: i64,
    pub long: i64,
    pub sessions: i64,
    pub auto_start: bool,
}

const LIMITS: [(&str, i64, i64, i64); 4] = [("focus", 1, 99, 25), ("short", 1, 99, 5), ("long", 1, 99, 15), ("sessions", 1, 4, 4)];

fn key(name: &str) -> String {
    if name == "sessions" { "pomodoro_sessions".into() } else { format!("pomodoro_{name}_minutes") }
}

pub fn settings(config: &Config) -> Settings {
    let c = config.load();
    let get = |name: &str, low: i64, high: i64, default: i64| {
        c.get(&key(name)).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .map(|n| n.clamp(low, high)).unwrap_or(default)
    };
    let [f, s, l, n] = LIMITS.map(|(name, low, high, default)| get(name, low, high, default));
    Settings { focus: f, short: s, long: l, sessions: n,
               auto_start: c.get("pomodoro_auto_start").and_then(|v| v.as_bool()).unwrap_or(false) }
}

/// Keys: focus, short, long (minutes), sessions, auto_start.
pub fn save_settings(config: &Config, req: &Value) -> Result<Settings, String> {
    let mut changes: Vec<(String, Value)> = Vec::new();
    for (name, low, high, _) in LIMITS {
        let v = &req[name];
        if v.is_null() {
            continue;
        }
        let n = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
            .ok_or(format!("{name} must be a number"))?;
        if !(low..=high).contains(&n) {
            return Err(format!("{name} must be {low} to {high}"));
        }
        changes.push((key(name), json!(n)));
    }
    if let Some(on) = req["auto_start"].as_bool() {
        changes.push(("pomodoro_auto_start".into(), json!(on)));
    }
    let refs: Vec<(&str, Value)> = changes.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    config.update(&refs).map_err(|e| e.to_string())?;
    Ok(settings(config))
}

pub const PHASES: [&str; 3] = ["focus", "short", "long"];

pub fn phase_name(phase: &str) -> &'static str {
    match phase { "short" => "Short break", "long" => "Long break", _ => "Focus" }
}

pub struct Pomodoro {
    pub clock: Box<dyn Fn() -> f64 + Send + Sync>,
    pub phase: &'static str,
    pub status: &'static str,
    pub completed: usize,
    pub shown: bool,
    ends_at: f64,
    left: i64,
}

impl Pomodoro {
    pub fn new(clock: Box<dyn Fn() -> f64 + Send + Sync>, s: &Settings) -> Pomodoro {
        let mut p = Pomodoro { clock, phase: "focus", status: "idle", completed: 0, shown: false, ends_at: 0.0, left: 0 };
        p.reset(s);
        p
    }

    fn length(phase: &str, s: &Settings) -> i64 {
        60 * match phase { "short" => s.short, "long" => s.long, _ => s.focus }
    }

    /// Whole seconds left, rounded up, so a fresh 25-minute phase reads 1500.
    pub fn seconds_left(&self) -> i64 {
        if self.status == "running" {
            ((self.ends_at - (self.clock)()).ceil() as i64).max(0)
        } else {
            self.left
        }
    }

    /// True when the face should be on screen.
    pub fn active(&self) -> bool {
        self.shown || matches!(self.status, "running" | "paused" | "waiting")
    }

    pub fn show(&mut self) {
        self.shown = true;
    }

    /// Begin the ready phase. Running or paused: nothing (use resume).
    pub fn start(&mut self) {
        if matches!(self.status, "idle" | "waiting") {
            self.ends_at = (self.clock)() + self.left as f64;
            self.status = "running";
            self.shown = true;
        }
    }

    pub fn pause(&mut self) {
        if self.status == "running" {
            self.left = self.seconds_left();
            self.status = "paused";
        }
    }

    pub fn resume(&mut self) {
        if self.status == "paused" {
            self.ends_at = (self.clock)() + self.left as f64;
            self.status = "running";
        }
    }

    /// End the current phase now, without the notification, and get the next one ready.
    pub fn skip(&mut self, s: &Settings) {
        self.advance(s, false);
    }

    /// Back to the first focus session of a set, not started and not on screen.
    pub fn reset(&mut self, s: &Settings) {
        self.phase = "focus";
        self.completed = 0;
        self.status = "idle";
        self.shown = false;
        self.left = Self::length("focus", s);
    }

    /// New durations apply now to a phase that has not started yet.
    pub fn settings_changed(&mut self, s: &Settings) {
        if matches!(self.status, "idle" | "waiting") {
            self.left = Self::length(self.phase, s);
        }
    }

    /// Call often. True once when a running phase reaches zero.
    pub fn tick(&mut self, s: &Settings) -> bool {
        if self.status != "running" || self.seconds_left() > 0 {
            return false;
        }
        self.advance(s, s.auto_start);
        true
    }

    fn advance(&mut self, s: &Settings, auto: bool) {
        if self.phase == "focus" {
            self.completed += 1;
            self.phase = if self.completed as i64 >= s.sessions { "long" } else { "short" };
        } else {
            if self.phase == "long" {
                self.completed = 0;
            }
            self.phase = "focus";
        }
        self.left = Self::length(self.phase, s);
        self.status = "waiting";
        self.shown = true;
        if auto {
            self.start();
        }
    }

    pub fn view(&self, s: &Settings) -> Value {
        json!({"phase": self.phase, "status": self.status, "seconds_left": self.seconds_left(),
               "sessions_completed": self.completed, "sessions_per_set": s.sessions, "paused": self.status == "paused"})
    }

    pub fn phase_seconds(&self, s: &Settings) -> i64 {
        Self::length(self.phase, s)
    }
}
