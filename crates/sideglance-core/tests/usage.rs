//! Usage parsing, with a fake HTTP GET and a token entered by hand. No network.

use sideglance_core::config::Config;
use sideglance_core::usage;

fn config_with(tokens: &[(&str, &str)]) -> (tempdir::Dir, Config) {
    let dir = tempdir::Dir::new();
    let c = Config::at(dir.0.join("config.json"));
    c.update(&tokens.iter().map(|(k, v)| (*k, serde_json::Value::from(*v))).collect::<Vec<_>>()).unwrap();
    (dir, c)
}

mod tempdir {
    pub struct Dir(pub std::path::PathBuf);
    impl Dir {
        pub fn new() -> Dir {
            let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let p = std::env::temp_dir().join(format!("sideglance-test-{n}-{:?}", std::thread::current().id()));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

const NOW: i64 = 1_794_500_000; // 2026-11-12T16:13:20Z

#[test]
fn claude_and_codex() {
    let (_d, c) = config_with(&[("claude_token", "tok-c"), ("codex_token", "tok-x"), ("codex_account_id", "acc")]);
    let get = |url: &str, headers: &[(&str, String)]| -> Result<(u16, String), String> {
        if url == usage::CLAUDE_URL {
            assert!(headers.contains(&("Authorization", "Bearer tok-c".into())));
            assert!(headers.contains(&("anthropic-beta", "oauth-2025-04-20".into())));
            Ok((200, r#"{"five_hour":{"utilization":42.5,"resets_at":"2026-11-12T19:13:20.123+00:00"},
                         "seven_day":{"utilization":78,"resets_at":null}}"#.into()))
        } else {
            assert!(headers.contains(&("ChatGPT-Account-Id", "acc".into())));
            Ok((200, format!(r#"{{"rate_limit":{{"primary_window":{{"used_percent":93,"reset_at":{},"limit_window_seconds":18000}},
                     "secondary_window":{{"used_percent":10,"reset_after_seconds":3600}}}}}}"#, NOW + 1200)))
        }
    };
    let (accounts, notes) = usage::fetch_all(&c, &get, NOW).unwrap();
    assert!(notes.is_empty());
    assert_eq!((accounts[0].pct_d, accounts[0].rem_d, accounts[0].rem_w), (42.5, Some(3 * 3600), None));
    assert_eq!((accounts[1].pct_d, accounts[1].rem_d, accounts[1].rem_w, accounts[1].win_w), (93.0, Some(1200), Some(3600), usage::WINDOW_W));
}

#[test]
fn expired_token_is_explained_and_the_other_account_still_shows() {
    let (_d, c) = config_with(&[("claude_token", "old"), ("codex_token", "tok-x")]);
    let get = |url: &str, _: &[(&str, String)]| -> Result<(u16, String), String> {
        if url == usage::CLAUDE_URL { Ok((401, String::new())) } else { Ok((200, r#"{"rate_limit":{}}"#.into())) }
    };
    let (accounts, notes) = usage::fetch_all(&c, &get, NOW).unwrap();
    assert_eq!(accounts.len(), 1);
    assert!(notes[0].contains("rejected or has expired"));
}

#[test]
fn seconds_until() {
    assert_eq!(usage::seconds_until(Some("2026-11-12T16:13:30Z"), NOW), Some(10));
    assert_eq!(usage::seconds_until(Some("2026-11-12T16:13:00+00:00"), NOW), Some(0));
    assert_eq!(usage::seconds_until(Some("not a time"), NOW), None);
    assert_eq!(usage::seconds_until(None, NOW), None);
}

#[test]
fn config_removes_null_values() {
    let (_d, c) = config_with(&[("a", "1")]);
    c.update(&[("a", serde_json::Value::Null)]).unwrap();
    assert!(c.load().is_empty());
}
