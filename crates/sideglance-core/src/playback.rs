//! How a timeline of frames reaches a device. Pure functions.
//!
//! device  Send every frame once, each with its own duration, and let the device loop them.
//!         For a timeline that plays once, the last frame is held for as long as the device
//!         allows, so it never wraps to the start before the host puts the previous screen back.
//! host    Send one still frame per step and wait between them here. For devices that ignore
//!         multi-frame animations. Timing then depends on the Bluetooth link.

use crate::frame::Frame;

pub const MODES: &[&str] = &["device", "host"];

/// Frames and per-frame durations to send once.
pub fn device_plan(steps: &[(Frame, u32)], looping: bool, max_frame_ms: u32) -> (Vec<Frame>, Vec<u32>) {
    let frames: Vec<Frame> = steps.iter().map(|(f, _)| f.clone()).collect();
    let mut durations: Vec<u32> = steps.iter().map(|(_, ms)| (*ms).min(max_frame_ms)).collect();
    if !looping && frames.len() > 1 {
        *durations.last_mut().unwrap() = max_frame_ms;
    }
    (frames, durations)
}

/// Show each frame for its duration. `show` puts one still on the device; `wait(seconds)` returns
/// true when playback should stop. A timeline that plays once ends on its last frame without waiting.
pub fn play_on_host(steps: &[(Frame, u32)], looping: bool, mut show: impl FnMut(&Frame), mut wait: impl FnMut(f64) -> bool) {
    loop {
        for (i, (frame, ms)) in steps.iter().enumerate() {
            show(frame);
            if !looping && i == steps.len() - 1 {
                return;
            }
            if wait(*ms as f64 / 1000.0) {
                return;
            }
        }
        if !looping {
            return;
        }
    }
}
