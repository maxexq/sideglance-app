//! This computer: CPU and memory use, and its battery if it has one. Data only.
//!
//! The Ditoo Pro does not report its own battery (its firmware ignores the battery query), so the
//! low-battery notification is about this computer's battery, as in ih8d8/divoom-ditoo-controller.

use std::sync::Mutex;

use serde::Serialize;

/// How often the Computer face is redrawn while it shows.
pub const REFRESH_SECONDS: f64 = 3.0;
/// How often the battery is checked, whatever is on screen.
pub const BATTERY_CHECK_SECONDS: f64 = 60.0;
/// Below this, while not charging, the battery-low notification shows.
pub const BATTERY_LOW_PERCENT: f64 = 15.0;
/// Shown at most once in this long, so a battery hovering at the threshold does not keep blinking.
pub const BATTERY_ALERT_COOLDOWN_SECONDS: f64 = 30.0 * 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Battery {
    pub percent: f64,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Sample {
    pub cpu: f64,
    pub ram: f64,
    pub battery: Option<Battery>,
}

/// Reads the live values. CPU use is measured between two reads, so the first read is 0.
pub struct Meter {
    sys: Mutex<sysinfo::System>,
}

impl Default for Meter {
    fn default() -> Self {
        Meter { sys: Mutex::new(sysinfo::System::new()) }
    }
}

impl Meter {
    pub fn sample(&self) -> Sample {
        let mut sys = self.sys.lock().unwrap_or_else(|p| p.into_inner());
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let total = sys.total_memory().max(1) as f64;
        Sample {
            cpu: sys.global_cpu_usage() as f64,
            ram: 100.0 * sys.used_memory() as f64 / total,
            battery: battery(),
        }
    }
}

/// The first battery, or None on a computer without one.
pub fn battery() -> Option<Battery> {
    let manager = starship_battery::Manager::new().ok()?;
    let b = manager.batteries().ok()?.flatten().next()?;
    let percent = (b.state_of_charge().value as f64 * 100.0).clamp(0.0, 100.0);
    let charging = matches!(b.state(), starship_battery::State::Charging | starship_battery::State::Full);
    Some(Battery { percent, charging })
}

/// Decides when to show the battery-low notification: when the battery drops below the threshold
/// while not charging, then again only after the cooldown. Times are in seconds.
#[derive(Default)]
pub struct BatteryWatch {
    was_low: bool,
    last_alert: Option<f64>,
}

impl BatteryWatch {
    /// True when the notification should show now.
    pub fn check(&mut self, battery: Option<Battery>, now: f64) -> bool {
        let low = battery.map(|b| b.percent < BATTERY_LOW_PERCENT && !b.charging).unwrap_or(false);
        let fire = low && (!self.was_low || self.last_alert.map(|t| now - t >= BATTERY_ALERT_COOLDOWN_SECONDS).unwrap_or(true));
        self.was_low = low;
        if fire {
            self.last_alert = Some(now);
        }
        fire
    }
}
