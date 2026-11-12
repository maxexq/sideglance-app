//! Frames must match the Python version pixel for pixel (tests/golden/python.json).

use serde_json::Value;
use sideglance_core::{notifications, playback, render, usage::Account};

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/python.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn usage_faces_match_python() {
    let g = golden();
    let accounts: Vec<Account> = serde_json::from_value(g["usage_faces"]["accounts"].clone()).unwrap();
    let faces = render::usage_faces(&accounts);
    let want = g["usage_faces"]["faces"].as_array().unwrap();
    assert_eq!(faces.len(), want.len());
    for (i, (face, w)) in faces.iter().zip(want).enumerate() {
        assert_eq!(face.hex(), strings(w), "face {i}");
    }
}

#[test]
fn remaining_text_matches_python() {
    for (k, v) in golden()["remaining_text"].as_object().unwrap() {
        let rem = if k == "None" { None } else { Some(k.parse().unwrap()) };
        assert_eq!(render::remaining_text(rem), v.as_str().unwrap(), "{k}");
    }
}

#[test]
fn notifications_match_python() {
    for case in golden()["notifications"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let e = notifications::get(id).unwrap();
        assert_eq!(e.label, case["label"].as_str().unwrap());
        assert_eq!(e.behavior, case["behavior"].as_str().unwrap(), "{id}");
        assert_eq!(e.looping, case["loop"].as_bool().unwrap(), "{id}");
        assert_eq!(e.from_claude, case["from_claude"].as_bool().unwrap(), "{id}");
        let steps = notifications::steps(e);
        let ms: Vec<u64> = steps.iter().map(|(_, ms)| *ms as u64).collect();
        let want_ms: Vec<u64> = case["ms"].as_array().unwrap().iter().map(|m| m.as_u64().unwrap()).collect();
        assert_eq!(ms, want_ms, "{id}");
        for (i, ((frame, _), w)) in steps.iter().zip(case["frames"].as_array().unwrap()).enumerate() {
            assert_eq!(frame.hex(), strings(w), "{id} step {i}");
        }
        let (frames, durations) = playback::device_plan(&steps, e.looping, 65535);
        let want_plan: Vec<u64> = case["plan_ms"].as_array().unwrap().iter().map(|m| m.as_u64().unwrap()).collect();
        assert_eq!(durations.iter().map(|d| *d as u64).collect::<Vec<_>>(), want_plan, "{id}");
        assert_eq!(frames.len(), case["plan_frames"].as_array().unwrap().len());
    }
}

#[test]
fn old_names_still_work() {
    assert_eq!(notifications::get("waiting").unwrap().id, "claude-waiting");
    assert!(notifications::get("nope").unwrap_err().contains("claude-waiting"));
}

#[test]
fn icon_parser_reports_problems() {
    let good = format!("[palette]\n. = off\nW = #FFFFFF\n[x]\n{}\n", vec!["W".repeat(16); 16].join("\n"));
    assert!(notifications::parse_icons(&good, 16, 16).is_ok());
    let crlf = good.replace('\n', "\r\n");
    assert!(notifications::parse_icons(&crlf, 16, 16).is_ok());
    let extra = format!("{good}{}\n", "W".repeat(16));
    assert!(notifications::parse_icons(&extra, 16, 16).unwrap_err().contains("17 rows"));
    let unknown = good.replacen(&"W".repeat(16), &format!("Q{}", "W".repeat(15)), 1);
    assert!(notifications::parse_icons(&unknown, 16, 16).unwrap_err().contains("uses Q"));
}

#[test]
fn host_playback_stops_when_asked() {
    let e = notifications::get("claude-waiting").unwrap();
    let steps = notifications::steps(e);
    let mut shown = 0;
    let mut waits = 0;
    playback::play_on_host(&steps, true, |_| shown += 1, |_| { waits += 1; waits == 5 });
    assert_eq!((shown, waits), (5, 5));
    let once = notifications::steps(notifications::get("job-failed").unwrap());
    let mut shown = 0;
    playback::play_on_host(&once, false, |_| shown += 1, |_| false);
    assert_eq!(shown, 7);
}
