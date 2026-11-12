//! Claude Code settings: ours are added once, others are kept, the Python version's are replaced.

use serde_json::{json, Map, Value};
use sideglance_core::hooks;

fn map(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn add_keeps_other_settings_and_hooks() {
    let mut s = map(json!({
        "model": "opus",
        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}],
                  "Notification": [{"matcher": "idle_prompt", "hooks": [{"type": "command",
                      "command": "\"C:/Python312/python.exe\" \"F:/Dev/sideglance/notify.py\" claude-waiting"}]}]}
    }));
    hooks::add(&mut s);
    hooks::add(&mut s); // twice: still one copy
    assert_eq!(s["model"], "opus");
    let stop = s["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert_eq!(stop[0]["hooks"][0]["command"], "say done");
    assert_eq!(stop[1]["hooks"][0]["command"], "curl -s -m 2 -X POST http://127.0.0.1:8765/api/hook/claude-finished");
    let notification = s["hooks"]["Notification"].as_array().unwrap();
    assert_eq!(notification.len(), 3); // the Python one is gone, three matchers of ours
    assert!(notification.iter().all(|g| g["hooks"][0]["command"].as_str().unwrap().contains("/api/hook/claude-waiting")));
    assert!(hooks::installed(&s));
}

#[test]
fn strip_removes_only_ours() {
    let mut s = map(json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}}));
    hooks::add(&mut s);
    hooks::strip(&mut s);
    assert_eq!(s, map(json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}})));
    let mut empty = Map::new();
    hooks::add(&mut empty);
    hooks::strip(&mut empty);
    assert!(empty.is_empty());
    assert!(!hooks::installed(&empty));
}
