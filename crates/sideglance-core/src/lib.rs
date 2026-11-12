//! Sideglance without a window. The Tauri app wraps this; tests run it without a device.
//!
//! Layers, as in the Python version:
//!   usage          data (AI usage). No display knowledge.
//!   render         data to 16x16 frames.
//!   notifications  events, icons and timelines. No display knowledge.
//!   playback       how a timeline reaches the device.
//!   protocol, link, driver   frames and commands to a Divoom device.
//!   state, http    what is on screen, and the local API.

pub mod frame;
pub mod protocol;
pub mod notifications;
pub mod playback;
pub mod render;
pub mod config;
pub mod usage;
pub mod weather;
pub mod pomodoro;
pub mod system;
pub mod device;
pub mod link;
pub mod hooks;
pub mod update;
pub mod http;
pub mod state;
