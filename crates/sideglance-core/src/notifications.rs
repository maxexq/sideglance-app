//! Notification events: what each one means, its icon, and how it animates. No device knowledge.
//!
//! An icon is a grid of palette characters from notification-icons.txt. An animation is a timeline
//! of steps; each step hides some palette characters (or all of them) for a number of milliseconds.
//! A looping timeline repeats until dismissed. One that does not loop plays once, then the display
//! goes back to what it showed before.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::frame::{parse_color, Frame, Rgb, SIZE};

pub const OFF: char = '.';

// ----- every duration, in milliseconds -----
pub const CURSOR_BLINK_MS: u32 = 500; // claude-waiting: amber cursor on, then off
pub const TIMER_BLINK_MS: u32 = 400; // timer-finished: whole icon on, then off
pub const FAILED_BLINK_ON_MS: u32 = 300; // job-failed: whole icon on ...
pub const FAILED_BLINK_OFF_MS: u32 = 200; // ... then off
pub const FAILED_BLINKS: usize = 3;
pub const SETTLE_MS: u32 = 10_000; // how long a non-looping icon stays lit before the display goes back

#[derive(Clone, Debug, PartialEq)]
pub enum Hide {
    Nothing,
    Chars(&'static str),
    All,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub ms: u32,
    pub hide: Hide,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub id: &'static str,
    pub label: &'static str,
    pub behavior: String,
    pub steps: Vec<Step>,
    pub looping: bool,
    /// Cleared when Claude Code reports you are back.
    pub from_claude: bool,
}

impl Event {
    pub fn total_ms(&self) -> u64 {
        self.steps.iter().map(|s| s.ms as u64).sum()
    }
}

fn blinks(on: u32, off: u32, times: usize) -> Vec<Step> {
    (0..times).flat_map(|_| [Step { ms: on, hide: Hide::Nothing }, Step { ms: off, hide: Hide::All }]).collect()
}

fn secs(ms: u32) -> String {
    let s = ms as f64 / 1000.0;
    if s.fract() == 0.0 { format!("{}", s as u64) } else { format!("{s}") }
}

pub fn events() -> &'static Vec<Event> {
    static EVENTS: OnceLock<Vec<Event>> = OnceLock::new();
    EVENTS.get_or_init(|| {
        let stays = format!("Stays for {}\u{a0}s", SETTLE_MS / 1000);
        let mut failed = blinks(FAILED_BLINK_ON_MS, FAILED_BLINK_OFF_MS, FAILED_BLINKS);
        failed.push(Step { ms: SETTLE_MS, hide: Hide::Nothing });
        vec![
            Event {
                id: "claude-waiting",
                label: "Claude Code is waiting for you",
                behavior: format!("Cursor blinks every {}\u{a0}s until dismissed", secs(CURSOR_BLINK_MS)),
                steps: vec![Step { ms: CURSOR_BLINK_MS, hide: Hide::Nothing }, Step { ms: CURSOR_BLINK_MS, hide: Hide::Chars("A") }],
                looping: true,
                from_claude: true,
            },
            Event {
                id: "claude-finished",
                label: "Claude Code finished",
                behavior: stays.clone(),
                steps: vec![Step { ms: SETTLE_MS, hide: Hide::Nothing }],
                looping: false,
                from_claude: true,
            },
            Event {
                id: "job-passed",
                label: "Job passed",
                behavior: stays.clone(),
                steps: vec![Step { ms: SETTLE_MS, hide: Hide::Nothing }],
                looping: false,
                from_claude: false,
            },
            Event {
                id: "job-failed",
                label: "Job failed",
                behavior: format!("Blinks {FAILED_BLINKS} times, then stays for {}\u{a0}s", SETTLE_MS / 1000),
                steps: failed,
                looping: false,
                from_claude: false,
            },
            Event {
                id: "timer-finished",
                label: "Timer finished",
                behavior: format!("Blinks every {}\u{a0}s until dismissed", secs(TIMER_BLINK_MS)),
                steps: blinks(TIMER_BLINK_MS, TIMER_BLINK_MS, 1),
                looping: true,
                from_claude: false,
            },
        ]
    })
}

/// Older names, still sent by hooks that older versions installed.
const ALIASES: &[(&str, &str)] = &[("waiting", "claude-waiting"), ("done", "claude-finished"), ("ok", "job-passed"),
    ("fail", "job-failed"), ("timeup", "timer-finished")];

pub fn get(name: &str) -> Result<&'static Event, String> {
    let id = ALIASES.iter().find(|(old, _)| *old == name).map(|(_, new)| *new).unwrap_or(name);
    events().iter().find(|e| e.id == id).ok_or_else(|| {
        let ids: Vec<&str> = events().iter().map(|e| e.id).collect();
        format!("Unknown notification '{name}'. Use one of: {}", ids.join(", "))
    })
}

// ----- icon file -----

#[derive(Debug)]
pub struct Icons {
    pub palette: HashMap<char, Rgb>,
    pub icons: HashMap<String, Vec<Vec<char>>>,
}

/// Parse an icons file: a [palette] section mapping one character to #RRGGBB ("." is always off),
/// then one [name] section per icon with `rows` lines of `columns` characters.
pub fn parse_icons(text: &str, rows: usize, columns: usize) -> Result<Icons, String> {
    let mut palette = HashMap::new();
    let mut icons: HashMap<String, Vec<Vec<char>>> = HashMap::new();
    let mut order = Vec::new();
    let mut section: Option<String> = None;
    for (n, raw) in text.lines().enumerate() {
        let number = n + 1;
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_string();
            if name != "palette" {
                if icons.contains_key(&name) {
                    return Err(format!("Icon '{name}' is defined twice (line {number})"));
                }
                icons.insert(name.clone(), Vec::new());
                order.push(name.clone());
            }
            section = Some(name);
            continue;
        }
        match section.as_deref() {
            None => return Err(format!("Line {number} is outside any [section]")),
            Some("palette") => {
                let (key, value) = line.split_once('=').ok_or(format!("Palette line {number} should look like 'C = #D97757'"))?;
                let key = key.trim();
                let value = value.trim();
                let ch = key.chars().next().filter(|_| key.chars().count() == 1)
                    .ok_or(format!("Palette line {number} should look like 'C = #D97757'"))?;
                if ch == OFF {
                    if !value.eq_ignore_ascii_case("off") {
                        return Err(format!("'{OFF}' always means off (line {number})"));
                    }
                    continue;
                }
                if !value.starts_with('#') {
                    return Err(format!("Palette color for '{ch}' should be #RRGGBB (line {number})"));
                }
                let color = parse_color(value).map_err(|_| format!("Palette color for '{ch}' should be #RRGGBB (line {number})"))?;
                palette.insert(ch, color);
            }
            Some(name) => icons.get_mut(name).unwrap().push(line.chars().collect()),
        }
    }
    for name in &order {
        let lines = &icons[name];
        if lines.len() != rows {
            return Err(format!("Icon '{name}' has {} rows, expected {rows}", lines.len()));
        }
        for (y, row) in lines.iter().enumerate() {
            if row.len() != columns {
                return Err(format!("Icon '{name}' row {y} has {} characters, expected {columns}", row.len()));
            }
            let mut unknown: Vec<char> = row.iter().copied().filter(|c| *c != OFF && !palette.contains_key(c)).collect();
            unknown.sort();
            unknown.dedup();
            if !unknown.is_empty() {
                let list: Vec<String> = unknown.iter().map(|c| c.to_string()).collect();
                return Err(format!("Icon '{name}' row {y} uses {}, which is not in the palette", list.join(", ")));
            }
        }
    }
    Ok(Icons { palette, icons })
}

pub fn icons() -> &'static Icons {
    static ICONS: OnceLock<Icons> = OnceLock::new();
    ICONS.get_or_init(|| {
        let parsed = parse_icons(include_str!("notification-icons.txt"), SIZE, SIZE).expect("notification-icons.txt");
        for e in events() {
            assert!(parsed.icons.contains_key(e.id), "no icon for {}", e.id);
        }
        parsed
    })
}

/// One step of an event as a frame.
pub fn step_frame(event: &Event, step: &Step) -> Frame {
    let icons = icons();
    let mut frame = Frame::default();
    for (y, row) in icons.icons[event.id].iter().enumerate() {
        for (x, ch) in row.iter().enumerate() {
            let hidden = match &step.hide {
                Hide::All => true,
                Hide::Chars(chars) => chars.contains(*ch),
                Hide::Nothing => false,
            };
            if *ch != OFF && !hidden {
                frame.set(x, y, icons.palette[ch]);
            }
        }
    }
    frame
}

/// The event's timeline as frames: [(frame, milliseconds)].
pub fn steps(event: &Event) -> Vec<(Frame, u32)> {
    event.steps.iter().map(|s| (step_frame(event, s), s.ms)).collect()
}
