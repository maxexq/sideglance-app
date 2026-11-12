//! Settings, kept as JSON in the per-user config folder:
//!   Windows  %APPDATA%\Sideglance\config.json
//!   macOS    ~/Library/Application Support/Sideglance/config.json
//!   Linux    ~/.config/Sideglance/config.json
//! Keys: port, claude_token, codex_token, codex_account_id, playback, allowed_origins, start_with.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value};

pub struct Config {
    path: PathBuf,
    lock: Mutex<()>,
}

pub fn default_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Sideglance")
}

impl Config {
    pub fn at(path: impl AsRef<Path>) -> Config {
        Config { path: path.as_ref().to_path_buf(), lock: Mutex::new(()) }
    }

    pub fn default_location() -> Config {
        Config::at(default_dir().join("config.json"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Everything saved, or an empty map if the file is missing or unreadable.
    pub fn load(&self) -> Map<String, Value> {
        let _guard = self.lock.lock().unwrap();
        std::fs::read_to_string(&self.path).ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    pub fn save(&self, map: &Map<String, Value>) -> std::io::Result<()> {
        let _guard = self.lock.lock().unwrap();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&Value::Object(map.clone())).unwrap() + "\n";
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)
    }

    /// Change some keys. A null value removes the key.
    pub fn update(&self, changes: &[(&str, Value)]) -> std::io::Result<()> {
        let mut map = self.load();
        for (k, v) in changes {
            if v.is_null() { map.remove(*k); } else { map.insert(k.to_string(), v.clone()); }
        }
        self.save(&map)
    }

    pub fn string(&self, key: &str) -> String {
        self.load().get(key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string()
    }
}
