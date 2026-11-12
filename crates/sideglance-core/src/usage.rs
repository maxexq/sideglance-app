//! AI usage for Claude and Codex. Data only: nothing here knows about any display.
//!
//! Tokens come from the logins that Claude Code and the Codex CLI keep on this computer,
//! or from values entered by hand (config: claude_token, codex_token, codex_account_id):
//! a token, or the path to a credentials file. Both usage endpoints are unofficial.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;

pub const CLAUDE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub const CODEX_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const REFRESH_SECONDS: u64 = 300;
pub const WINDOW_D: i64 = 5 * 3600;
pub const WINDOW_W: i64 = 7 * 86400;

/// One account's two windows: percent used, seconds until reset (None if unknown), window length.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub pct_d: f64,
    pub rem_d: Option<i64>,
    pub win_d: i64,
    pub pct_w: f64,
    pub rem_w: Option<i64>,
    pub win_w: i64,
}

/// An HTTP GET: (url, headers) to (status, body). A parameter so tests need no network.
pub type Get<'a> = &'a dyn Fn(&str, &[(&str, String)]) -> Result<(u16, String), String>;

pub fn http_get(url: &str, headers: &[(&str, String)]) -> Result<(u16, String), String> {
    let mut req = ureq::get(url).timeout(std::time::Duration::from_secs(10));
    for (k, v) in headers {
        req = req.set(k, v);
    }
    match req.call() {
        Ok(resp) => Ok((resp.status(), resp.into_string().map_err(|e| e.to_string())?)),
        Err(ureq::Error::Status(code, resp)) => Ok((code, resp.into_string().unwrap_or_default())),
        Err(e) => Err(format!("could not connect ({e})")),
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

pub fn claude_credentials_path() -> PathBuf {
    home().join(".claude").join(".credentials.json")
}

pub fn codex_auth_path() -> PathBuf {
    home().join(".codex").join("auth.json")
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    }
}

fn read_json(path: &std::path::Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Where the Claude token comes from: "manual", "file", "keychain", or None when there is no login.
pub fn claude_source(config: &Config) -> Option<&'static str> {
    if !config.string("claude_token").is_empty() {
        Some("manual")
    } else if claude_credentials_path().exists() {
        Some("file")
    } else if cfg!(target_os = "macos") {
        Some("keychain")
    } else {
        None
    }
}

pub fn codex_source(config: &Config) -> Option<&'static str> {
    if !config.string("codex_token").is_empty() {
        Some("manual")
    } else if codex_auth_path().exists() {
        Some("file")
    } else {
        None
    }
}

/// Why a fetch did not happen or failed.
#[derive(Debug)]
pub enum Problem {
    NotSignedIn,
    Failed(String),
}

fn claude_token(config: &Config) -> Result<String, Problem> {
    let entered = config.string("claude_token");
    if !entered.is_empty() && !expand(&entered).is_file() {
        return Ok(entered);
    }
    let data = if !entered.is_empty() {
        read_json(&expand(&entered)).map_err(Problem::Failed)?
    } else if claude_credentials_path().exists() {
        read_json(&claude_credentials_path()).map_err(Problem::Failed)?
    } else if cfg!(target_os = "macos") {
        let out = std::process::Command::new("security")
            .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
            .output().map_err(|_| Problem::NotSignedIn)?;
        if !out.status.success() {
            return Err(Problem::NotSignedIn);
        }
        serde_json::from_slice(&out.stdout).map_err(|e| Problem::Failed(format!("Keychain entry: {e}")))?
    } else {
        return Err(Problem::NotSignedIn);
    };
    let inner = data.get("claudeAiOauth").unwrap_or(&data);
    inner["accessToken"].as_str().map(str::to_string)
        .ok_or_else(|| Problem::Failed("the credentials file has no accessToken".into()))
}

fn codex_tokens(config: &Config) -> Result<(String, String), Problem> {
    let entered = config.string("codex_token");
    let account = config.string("codex_account_id");
    if !entered.is_empty() && !expand(&entered).is_file() {
        return Ok((entered, account));
    }
    let path = if entered.is_empty() { codex_auth_path() } else { expand(&entered) };
    if !path.exists() {
        return Err(Problem::NotSignedIn);
    }
    let data = read_json(&path).map_err(Problem::Failed)?;
    let tokens = &data["tokens"];
    let token = tokens["access_token"].as_str().ok_or_else(|| Problem::Failed("auth.json has no access_token".into()))?;
    let id = if account.is_empty() { tokens["account_id"].as_str().unwrap_or("").to_string() } else { account };
    Ok((token.to_string(), id))
}

/// Seconds from `now` (Unix time) until an ISO 8601 time, or None.
pub fn seconds_until(iso: Option<&str>, now: i64) -> Option<i64> {
    let t = chrono::DateTime::parse_from_rfc3339(&iso?.replace(' ', "T")).ok()?;
    Some((t.timestamp() - now).max(0))
}

fn checked(name: &str, tool: &str, result: Result<(u16, String), String>) -> Result<Value, Problem> {
    let (status, body) = result.map_err(|e| Problem::Failed(format!("{name}: {e}")))?;
    match status {
        200..=299 => serde_json::from_str(&body).map_err(|e| Problem::Failed(format!("{name}: unexpected answer ({e})"))),
        401 | 403 => Err(Problem::Failed(format!(
            "{name}: the token was rejected or has expired. Open {tool} once to refresh it, or enter a new token."))),
        429 => Err(Problem::Failed(format!("{name}: too many requests. It will try again in a few minutes."))),
        code => Err(Problem::Failed(format!("{name}: HTTP {code}"))),
    }
}

pub fn fetch_claude(config: &Config, get: Get, now: i64) -> Result<Account, Problem> {
    let token = claude_token(config)?;
    let data = checked("Claude", "Claude Code", get(CLAUDE_URL, &[
        ("Authorization", format!("Bearer {token}")), ("anthropic-beta", "oauth-2025-04-20".into())]))?;
    let window = |key: &str| {
        let w = &data[key];
        (w["utilization"].as_f64().unwrap_or(0.0), seconds_until(w["resets_at"].as_str(), now))
    };
    let (pct_d, rem_d) = window("five_hour");
    let (pct_w, rem_w) = window("seven_day");
    Ok(Account { id: "claude".into(), name: "Claude".into(), pct_d, rem_d, win_d: WINDOW_D, pct_w, rem_w, win_w: WINDOW_W })
}

fn codex_window(w: &Value, default_seconds: i64, now: i64) -> (f64, Option<i64>, i64) {
    let rem = if let Some(at) = w["reset_at"].as_f64() {
        Some(((at as i64) - now).max(0))
    } else {
        w["reset_after_seconds"].as_f64().map(|s| (s as i64).max(0))
    };
    (w["used_percent"].as_f64().unwrap_or(0.0), rem, w["limit_window_seconds"].as_i64().unwrap_or(default_seconds))
}

pub fn fetch_codex(config: &Config, get: Get, now: i64) -> Result<Account, Problem> {
    let (token, account) = codex_tokens(config)?;
    let mut headers = vec![("Authorization", format!("Bearer {token}")), ("Accept", "application/json".into())];
    if !account.is_empty() {
        headers.push(("ChatGPT-Account-Id", account));
    }
    let data = checked("Codex", "the Codex CLI", get(CODEX_URL, &headers))?;
    let limit = &data["rate_limit"];
    let (pct_d, rem_d, win_d) = codex_window(&limit["primary_window"], WINDOW_D, now);
    let (pct_w, rem_w, win_w) = codex_window(&limit["secondary_window"], WINDOW_W, now);
    Ok(Account { id: "codex".into(), name: "Codex".into(), pct_d, rem_d, win_d, pct_w, rem_w, win_w })
}

/// Every signed-in account. Ok((accounts, notes for accounts that failed)), or Err when none worked.
pub fn fetch_all(config: &Config, get: Get, now: i64) -> Result<(Vec<Account>, Vec<String>), String> {
    let mut found = Vec::new();
    let mut notes = Vec::new();
    for result in [fetch_claude(config, get, now), fetch_codex(config, get, now)] {
        match result {
            Ok(a) => found.push(a),
            Err(Problem::NotSignedIn) => {}
            Err(Problem::Failed(note)) => notes.push(note),
        }
    }
    if found.is_empty() && notes.is_empty() {
        return Err("No Claude or Codex login found on this computer. Enter a token on the AI usage tab.".into());
    }
    if found.is_empty() {
        return Err(notes.join("; "));
    }
    Ok((found, notes))
}
