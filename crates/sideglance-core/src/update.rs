//! Newer versions of the app. The desktop app supplies an Updater that checks GitHub Releases and
//! installs; the headless server has none, so the page shows no update controls there.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

/// Where people can download a version by hand when installing from the app fails.
pub const RELEASES: &str = "https://github.com/maxexq/sideglance-app/releases";

/// The app checks when it starts and then this often.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, PartialEq)]
pub struct Available {
    pub version: String,
    pub notes: String,
}

pub trait Updater: Send + Sync {
    /// The newer version, if there is one.
    fn check(&self) -> Result<Option<Available>, String>;
    /// Download and install the newer version, then restart the app.
    /// Returns only when that failed, or when there was nothing to install.
    fn install(&self) -> Result<(), String>;
}

#[derive(Default)]
struct State {
    available: Option<Available>,
    busy: Option<&'static str>, // "checking" or "installing"
    error: Option<String>,
    checked_at: Option<SystemTime>,
}

#[derive(Default)]
pub struct Updates {
    updater: Mutex<Option<Arc<dyn Updater>>>,
    state: Mutex<State>,
}

impl Updates {
    pub fn set_updater(&self, updater: Arc<dyn Updater>) {
        *self.updater.lock().unwrap_or_else(|p| p.into_inner()) = Some(updater);
    }

    fn updater(&self) -> Result<Arc<dyn Updater>, String> {
        self.updater.lock().unwrap_or_else(|p| p.into_inner()).clone()
            .ok_or_else(|| "Updates are only available in the Sideglance app.".to_string())
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Start a step unless another one is running.
    fn begin(&self, step: &'static str) -> Result<(), String> {
        let mut s = self.state();
        if let Some(running) = s.busy {
            return Err(format!("Already {running} for updates."));
        }
        s.busy = Some(step);
        s.error = None;
        Ok(())
    }

    /// Ask for a newer version. Blocks while it talks to the network.
    pub fn check(&self) -> Result<Option<Available>, String> {
        let updater = self.updater()?;
        self.begin("checking")?;
        let result = updater.check();
        let mut s = self.state();
        s.busy = None;
        s.checked_at = Some(SystemTime::now());
        match &result {
            Ok(found) => s.available = found.clone(),
            Err(e) => s.error = Some(format!("Could not check for updates: {e}")),
        }
        result
    }

    /// Install the newer version. Blocks; on success the app restarts and this never returns.
    pub fn install(&self) -> Result<(), String> {
        let updater = self.updater()?;
        self.begin("installing")?;
        let result = updater.install();
        let mut s = self.state();
        s.busy = None;
        if let Err(e) = &result {
            s.error = Some(format!("Could not install the update: {e}"));
        }
        result
    }

    pub fn available_version(&self) -> Option<String> {
        self.state().available.as_ref().map(|a| a.version.clone())
    }

    /// For the control page.
    pub fn status(&self) -> Value {
        let supported = self.updater.lock().unwrap_or_else(|p| p.into_inner()).is_some();
        let s = self.state();
        json!({
            "supported": supported,
            "available": s.available.as_ref().map(|a| json!({"version": a.version, "notes": a.notes,
                "url": format!("{RELEASES}/tag/v{}", a.version)})),
            "busy": s.busy,
            "error": s.error,
            "checked_seconds_ago": s.checked_at.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()),
            "releases": RELEASES,
        })
    }
}
