//! Claude Code hooks that send notifications to this app, in ~/.claude/settings.json.
//!
//! Each hook runs curl (built into Windows 10 and later, and macOS) against the local API, which
//! answers hook calls with no body, so nothing is printed into Claude Code.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

/// Every hook this app adds contains this, so they can be found and removed again.
pub const MARK: &str = "127.0.0.1:8765/api/hook/";
/// Hooks added by the Python version, removed when these are installed.
const OLD_MARK: &str = "notify.py";

/// (Claude Code hook event, matcher or None, what to send). Notification matches on the notification type.
pub const HOOKS: &[(&str, Option<&str>, &str)] = &[
    ("Notification", Some("permission_prompt"), "claude-waiting"), // waiting for you to approve a tool
    ("Notification", Some("idle_prompt"), "claude-waiting"),       // finished 60 s ago and you have not typed
    ("Notification", Some("elicitation_dialog"), "claude-waiting"), // an MCP server is asking you something
    ("Stop", None, "claude-finished"),                            // finished responding
    ("UserPromptSubmit", None, "resume"),                         // you typed again
    ("PostToolUse", None, "resume"),                              // a tool ran, so permission was given
    ("SessionEnd", None, "resume"),
];

pub fn settings_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude").join("settings.json")
}

pub fn command(event: &str) -> String {
    format!("curl -s -m 2 -X POST http://{MARK}{event}")
}

fn ours(hook: &Value) -> bool {
    let c = hook["command"].as_str().unwrap_or("");
    c.contains(MARK) || c.contains(OLD_MARK)
}

/// Remove every hook this app (or the Python version) added, and empty groups and events.
pub fn strip(settings: &mut Map<String, Value>) {
    let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) else { return };
    for groups in hooks.values_mut() {
        if let Some(list) = groups.as_array_mut() {
            for group in list.iter_mut() {
                if let Some(inner) = group.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                    inner.retain(|h| !ours(h));
                }
            }
            list.retain(|g| g["hooks"].as_array().map(|a| !a.is_empty()).unwrap_or(true));
        }
    }
    hooks.retain(|_, groups| groups.as_array().map(|a| !a.is_empty()).unwrap_or(true));
    if hooks.is_empty() {
        settings.remove("hooks");
    }
}

/// Add ours, after removing any earlier copies. Everything else in the settings is kept.
pub fn add(settings: &mut Map<String, Value>) {
    strip(settings);
    let hooks = settings.entry("hooks").or_insert_with(|| json!({})).as_object_mut().unwrap();
    for (event, matcher, send) in HOOKS {
        let mut group = json!({"hooks": [{"type": "command", "command": command(send), "timeout": 5}]});
        if let Some(m) = matcher {
            group["matcher"] = json!(m);
        }
        hooks.entry(*event).or_insert_with(|| json!([])).as_array_mut().unwrap().push(group);
    }
}

pub fn installed(settings: &Map<String, Value>) -> bool {
    settings.get("hooks").and_then(|h| h.as_object()).map(|hooks| hooks.values().any(|groups| {
        groups.as_array().into_iter().flatten().any(|g| g["hooks"].as_array().into_iter().flatten()
            .any(|h| h["command"].as_str().unwrap_or("").contains(MARK)))
    })).unwrap_or(false)
}

fn load() -> Result<Map<String, Value>, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(format!("{} is not valid JSON, so it was left alone. Fix it, then try again.", path.display())),
    }
}

fn save(settings: &Map<String, Value>) -> Result<(), String> {
    let path = settings_path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    if path.exists() {
        std::fs::copy(&path, path.with_extension("json.sideglance-backup")).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(settings.clone())).unwrap() + "\n";
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn install() -> Result<(), String> {
    let mut settings = load()?;
    add(&mut settings);
    save(&settings)
}

pub fn uninstall() -> Result<(), String> {
    let mut settings = load()?;
    strip(&mut settings);
    save(&settings)
}

pub fn status() -> Value {
    match load() {
        Ok(s) => json!({"installed": installed(&s), "path": settings_path().display().to_string()}),
        Err(e) => json!({"installed": false, "path": settings_path().display().to_string(), "error": e}),
    }
}
