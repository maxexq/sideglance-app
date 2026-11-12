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
