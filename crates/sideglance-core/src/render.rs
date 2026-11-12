//! 16x16 renderer: data to frames. Knows nothing about any device.

use crate::frame::{Frame, Rgb};
use crate::usage::Account;

/// Each usage face stays this long.
pub const FACE_MS: u32 = 4000;

pub const CLAUDE_LABEL: Rgb = [240, 130, 90];
pub const CODEX_LABEL: Rgb = [40, 200, 150];
const WHITE: Rgb = [255, 255, 255];
const YELLOW: Rgb = [255, 190, 0];
const RED: Rgb = [255, 45, 45];
const BLUE: Rgb = [60, 140, 255];
const EMPTY: Rgb = [38, 38, 38];

pub fn account_color(id: &str) -> Rgb {
    if id == "codex" { CODEX_LABEL } else { CLAUDE_LABEL }
}

fn glyph(ch: char) -> &'static [&'static str] {
    match ch {
        'D' => &["### ", "#  #", "#  #", "#  #", "### "],
        'W' => &["#   #", "#   #", "# # #", "## ##", "#   #"],
        '0' => &["###", "# #", "# #", "# #", "###"],
        '1' => &[" #", "##", " #", " #", " #"],
        '2' => &["###", "  #", "###", "#  ", "###"],
        '3' => &["###", "  #", "###", "  #", "###"],
        '4' => &["# #", "# #", "###", "  #", "  #"],
        '5' => &["###", "#  ", "###", "  #", "###"],
        '6' => &["###", "#  ", "###", "# #", "###"],
        '7' => &["###", "  #", "  #", "  #", "  #"],
        '8' => &["###", "# #", "###", "# #", "###"],
        '9' => &["###", "# #", "###", "  #", "###"],
        'h' => &["#  ", "#  ", "###", "# #", "# #"],
        'd' => &["  #", "  #", "###", "# #", "###"],
        '<' => &["  #", " # ", "#  ", " # ", "  #"],
        '°' => &["###", "# #", "###", "   ", "   "],
        _ => &["   ", "   ", "###", "   ", "   "], // '-'
    }
}

fn level_color(pct: f64) -> Rgb {
    if pct >= 90.0 { RED } else if pct >= 70.0 { YELLOW } else { WHITE }
}

/// Draw text right-aligned at column `right`, with a 1-pixel gap between glyphs.
fn draw_text(f: &mut Frame, text: &str, right: i32, top: usize, color: Rgb) {
    let mut x = right + 1;
    for ch in text.chars().rev() {
        let rows = glyph(ch);
        x -= rows[0].len() as i32;
        for (dy, row) in rows.iter().enumerate() {
            for (dx, c) in row.chars().enumerate() {
                let px = x + dx as i32;
                if c == '#' && (0..16).contains(&px) {
                    f.set(px as usize, top + dy, color);
                }
            }
        }
        x -= 1;
    }
}

/// "3h", "2d", "<1h", or "--" when the reset time is unknown.
pub fn remaining_text(rem: Option<i64>) -> String {
    match rem {
        None => "--".into(),
        Some(r) if r >= 86400 => format!("{}d", (r / 86400).min(9)),
        Some(r) if r >= 3600 => format!("{}h", r / 3600),
        Some(_) => "<1h".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_block(f: &mut Frame, top: usize, label: char, pct: f64, rem: Option<i64>, window: i64, face: u8, tint: Rgb) {
    let color = level_color(pct);
    draw_text(f, &label.to_string(), glyph(label)[0].len() as i32 - 1, top, tint);
    if face == 1 {
        draw_text(f, &(pct.round_ties_even() as i64).min(100).to_string(), 15, top, color);
    } else {
        draw_text(f, &remaining_text(rem), 15, top, BLUE);
    }
    let bar_y = top + 6;
    let mut filled = (16.0 * pct.clamp(0.0, 100.0) / 100.0).round_ties_even() as usize;
    if pct > 0.0 {
        filled = filled.max(1);
    }
    for x in 0..16 {
        f.set(x, bar_y, if x < filled { color } else { EMPTY });
    }
    if let Some(r) = rem {
        // blue dot: how far through the window we are
        let elapsed = 1.0 - (r.min(window) as f64) / window as f64;
        f.set(((elapsed * 16.0) as usize).min(15), bar_y, BLUE);
    }
}

/// Face 1 shows the percent used, face 2 the time left, for both windows of one account.
pub fn usage_face(face: u8, a: &Account) -> Frame {
    let mut f = Frame::default();
    let tint = account_color(&a.id);
    draw_block(&mut f, 1, 'D', a.pct_d, a.rem_d, a.win_d, face, tint);
    draw_block(&mut f, 9, 'W', a.pct_w, a.rem_w, a.win_w, face, tint);
    f
}

/// Two faces per account: percent used, then time left.
pub fn usage_faces(accounts: &[Account]) -> Vec<Frame> {
    accounts.iter().flat_map(|a| [usage_face(1, a), usage_face(2, a)]).collect()
}

// ---------- countdown timer ----------

/// Minutes left in large digits, and a bar for the time left.
pub fn timer_frame(remaining: i64, total: i64) -> Frame {
    let mut f = Frame::default();
    let minutes = ((remaining + 59).div_euclid(60)).min(99);
    let text = minutes.to_string();
    let n = text.len() as i32;
    let mut x = (16 - (n * 6 + (n - 1))).div_euclid(2);
    for ch in text.chars() {
        let rows: Vec<String> = if ch == '1' {
            vec![" # ".into(), "## ".into(), " # ".into(), " # ".into(), "###".into()]
        } else {
            glyph(ch).iter().map(|r| format!("{r:<3}")).collect()
        };
        for (gy, row) in rows.iter().enumerate() {
            for (gx, c) in row.chars().enumerate() {
                if c == '#' {
                    for dx in 0..2 {
                        for dy in 0..2 {
                            let px = x + 2 * gx as i32 + dx;
                            if (0..16).contains(&px) {
                                f.set(px as usize, 1 + 2 * gy + dy as usize, WHITE);
                            }
                        }
                    }
                }
            }
        }
        x += 7;
    }
    let filled = if remaining > 0 {
        ((16.0 * remaining as f64 / total as f64).round_ties_even() as usize).max(1)
    } else {
        0
    };
    for bx in 0..16 {
        for by in [13, 14] {
            f.set(bx, by, if bx < filled { BLUE } else { EMPTY });
        }
    }
    f
}

// ---------- weather ----------
// Rows 0-8 icon, row 9 empty, rows 10-14 temperature, row 15 empty.

const WEATHER_TEXT_TOP: usize = 10;
const WEATHER_TEXT: Rgb = [0xF2, 0xEF, 0xE6]; // the icons' white

/// "32°", "-5°", or "--°" when there is no reading. Rounded half away from zero, clamped to -99..99.
pub fn temperature_text(temperature: Option<f64>) -> String {
    match temperature {
        None => "--°".into(),
        Some(t) => format!("{}°", (t.round() as i64).clamp(-99, 99)),
    }
}

/// (first column, width) for text in 3-wide glyphs with 1-pixel gaps, centred on 16 columns.
pub fn temperature_layout(text: &str) -> (i32, i32) {
    let width = text.chars().count() as i32 * 4 - 1;
    ((16 - width + 1).div_euclid(2), width)
}

/// Condition icon on top, temperature below.
pub fn weather_face(condition: &str, temperature: Option<f64>) -> Frame {
    let mut f = Frame::default();
    for (y, row) in crate::weather::icon_grid(condition).iter().enumerate() {
        for (x, color) in row.iter().enumerate() {
            if let Some(c) = color {
                f.set(x, y, *c);
            }
        }
    }
    let text = temperature_text(temperature);
    let (mut x, _) = temperature_layout(&text);
    for ch in text.chars() {
        for (dy, row) in glyph(ch).iter().enumerate() {
            for (dx, c) in format!("{row:<3}").chars().enumerate() {   // every glyph is 3 wide here, "1" too
                let px = x + dx as i32;
                if c == '#' && (0..16).contains(&px) {
                    f.set(px as usize, WEATHER_TEXT_TOP + dy, WEATHER_TEXT);
                }
            }
        }
        x += 4;
    }
    f
}

// ---------- Pomodoro ----------

pub fn pomodoro_color(letter: char) -> Option<Rgb> {
    Some(match letter {
        't' => [0x34, 0x34, 0x2F], // dim track
        'R' => [0xFF, 0x4B, 0x3A], // focus ring, completed dots
        'G' => [0x3D, 0xDC, 0x84], // short-break ring
        'B' => [0x4C, 0x9B, 0xFF], // long-break ring
        'W' => [0xF2, 0xEF, 0xE6], // digits, current dot
        'A' => [0xFF, 0xB2, 0x24], // digits in the last minute
        'S' => [0x8F, 0x8D, 0x85], // paused
        _ => return None,
    })
}

const DIGITS_5X7: [[&str; 7]; 10] = [
    [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    ["####.", "....#", "....#", ".###.", "....#", "....#", "####."],
    ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
    ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
    ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
];
const DIGIT_COLUMNS: [usize; 2] = [2, 9];
const DIGIT_TOP: usize = 3;
const DOT_COLUMNS: [usize; 4] = [2, 5, 9, 12];
const DOT_TOP: usize = 11;

/// The 60 border pixels, clockwise from the top centre.
pub fn ring() -> Vec<(usize, usize)> {
    let mut r: Vec<(usize, usize)> = (8..16).map(|x| (x, 0)).collect();
    r.extend((1..16).map(|y| (15, y)));
    r.extend((0..15).rev().map(|x| (x, 15)));
    r.extend((0..15).rev().map(|y| (0, y)));
    r.extend((1..8).map(|x| (x, 0)));
    r
}

/// The face as 16 strings of palette letters ("." = off).
pub fn pomodoro_grid(phase: &str, seconds_left: i64, sessions_completed: usize, paused: bool) -> Vec<String> {
    let mut g = vec![vec!['.'; 16]; 16];
    let left = seconds_left.max(0);
    let lit = if paused { 'S' } else {
        match phase { "short" => 'G', "long" => 'B', _ => 'R' }
    };
    let s = (left % 60) as usize; // seconds left in the current minute
    for (i, (x, y)) in ring().into_iter().enumerate() {
        g[y][x] = if s > 0 && i >= 60 - s { lit } else { 't' };
    }
    let last_minute = left < 60;
    let number = if last_minute { left } else { (left / 60).min(99) };
    let digit_color = if paused { 'S' } else if last_minute { 'A' } else { 'W' };
    let text = format!("{number:02}");
    for (ch, x0) in text.chars().zip(DIGIT_COLUMNS) {
        for (dy, row) in DIGITS_5X7[ch.to_digit(10).unwrap() as usize].iter().enumerate() {
            for (dx, c) in row.chars().enumerate() {
                if c == '#' {
                    g[DIGIT_TOP + dy][x0 + dx] = digit_color;
                }
            }
        }
    }
    for (i, x0) in DOT_COLUMNS.into_iter().enumerate() {
        let color = if i < sessions_completed {
            if paused { 'S' } else { 'R' }
        } else if i == sessions_completed && phase == "focus" && !paused {
            'W'
        } else {
            't'
        };
        for dx in 0..2 {
            for dy in 0..2 {
                g[DOT_TOP + dy][x0 + dx] = color;
            }
        }
    }
    g.into_iter().map(|r| r.into_iter().collect()).collect()
}

pub fn pomodoro_face(phase: &str, seconds_left: i64, sessions_completed: usize, paused: bool) -> Frame {
    let mut f = Frame::default();
    for (y, row) in pomodoro_grid(phase, seconds_left, sessions_completed, paused).iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            if let Some(color) = pomodoro_color(c) {
                f.set(x, y, color);
            }
        }
    }
    f
}
