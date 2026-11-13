//! What the screen logic needs from a display, and the Divoom implementation of it.

use std::time::Duration;

use crate::frame::Frame;
use crate::link::{self, Link};
use crate::protocol;

pub trait Device: Send {
    fn connect(&mut self) -> Result<(), String>;
    fn close(&mut self);
    fn port_id(&self) -> Option<String>;
    fn max_frame_ms(&self) -> u32;
    /// One frame is a still. Several loop on the device, each with its own duration.
    fn show(&mut self, frames: &[Frame], durations_ms: &[u32]) -> Result<(), String>;
    /// A native command, such as brightness, light or off.
    fn command(&mut self, name: &str, args: &[String]) -> Result<(), String>;
    /// The channel the device shows now, if it can say. Changes when someone presses its button.
    /// Asks only over a link that is already open.
    fn channel(&mut self) -> Option<u8> { None }
    /// Go to a channel, so that the next picture is seen.
    fn switch_to(&mut self, _channel: u8) -> Result<(), String> { Ok(()) }
}

/// Pause between packets.
const CHUNK_GAP: Duration = Duration::from_millis(50);

/// Keeps the link open. If the link drops, it reconnects and sends once more.
pub struct Divoom {
    link: Option<Box<dyn Link>>,
    port_id: Option<String>,
    /// The port asked for: "auto" or a port name. Read again on each connect.
    pub setting: Box<dyn Fn() -> String + Send>,
    /// The last brightness set, restored after "off".
    brightness: u8,
    dark: bool,
}

impl Divoom {
    pub fn new(setting: Box<dyn Fn() -> String + Send>) -> Divoom {
        Divoom { link: None, port_id: None, setting, brightness: 100, dark: false }
    }

    fn send_once(&mut self, packets: &[Vec<u8>]) -> Result<(), String> {
        self.connect()?;
        let link = self.link.as_mut().unwrap();
        for p in packets {
            link.write(p).map_err(|e| format!("{}: {e}", self.port_id.clone().unwrap_or_default()))?;
            std::thread::sleep(CHUNK_GAP);
        }
        Ok(())
    }

    fn send(&mut self, packets: &[Vec<u8>]) -> Result<(), String> {
        let was_open = self.link.is_some();
        match self.send_once(packets) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.close();
                if !was_open {
                    return Err(e);
                }
                let again = self.send_once(packets); // the old link may have dropped
                if again.is_err() {
                    self.close();
                }
                again
            }
        }
    }

    /// Something to look at. After "off", turn the brightness back up first.
    fn display(&mut self, packets: Vec<Vec<u8>>) -> Result<(), String> {
        let mut all = Vec::new();
        if self.dark {
            all.extend(protocol::command("brightness", &[self.brightness.to_string()], (2000, 1, 1, 0, 0, 0))?);
        }
        all.extend(packets);
        self.send(&all)?;
        self.dark = false;
        Ok(())
    }
}

fn now_local() -> (u16, u8, u8, u8, u8, u8) {
    use chrono::{Datelike, Timelike};
    let n = chrono::Local::now();
    (n.year() as u16, n.month() as u8, n.day() as u8, n.hour() as u8, n.minute() as u8, n.second() as u8)
}

/// How long to wait for the answer to a channel query.
const ANSWER_WAIT: Duration = Duration::from_millis(400);

impl Device for Divoom {
    fn channel(&mut self) -> Option<u8> {
        let link = self.link.as_mut()?;
        link.read(Duration::from_millis(5)); // drop answers to earlier packets
        if link.write(&protocol::channel_query()).is_err() {
            self.link = None;
            return None;
        }
        let until = std::time::Instant::now() + ANSWER_WAIT;
        let mut got = Vec::new();
        while std::time::Instant::now() < until {
            got.extend(link.read(Duration::from_millis(60)));
            if let Some(c) = protocol::parse_channel(&got) {
                return Some(c);
            }
        }
        None
    }

    fn switch_to(&mut self, channel: u8) -> Result<(), String> {
        self.send(&[protocol::switch_channel(channel)])
    }

    fn connect(&mut self) -> Result<(), String> {
        if self.link.is_none() {
            let (port, mut link) = link::connect(&(self.setting)())?;
            std::thread::sleep(Duration::from_millis(500)); // let the Bluetooth link settle
            link.read(Duration::from_millis(50));
            self.port_id = Some(port);
            self.link = Some(link);
        }
        Ok(())
    }

    fn close(&mut self) {
        self.link = None;
    }

    fn port_id(&self) -> Option<String> {
        self.port_id.clone()
    }

    fn max_frame_ms(&self) -> u32 {
        protocol::MAX_FRAME_MS
    }

    fn show(&mut self, frames: &[Frame], durations_ms: &[u32]) -> Result<(), String> {
        let packets = if frames.len() == 1 {
            protocol::static_packets(&frames[0])
        } else {
            protocol::animation_packets(frames, durations_ms)?
        };
        self.display(packets)
    }

    fn command(&mut self, name: &str, args: &[String]) -> Result<(), String> {
        let packets = protocol::command(name, args, now_local())?;
        match name {
            "off" => {
                self.send(&packets)?;
                self.dark = true;
            }
            "brightness" => {
                self.send(&packets)?;
                self.brightness = args[0].trim().parse::<i64>().unwrap_or(100).clamp(0, 100) as u8;
                self.dark = false;
            }
            "volume" | "time" | "clock-weather" | "raw" => self.send(&packets)?, // the picture stays
            _ => self.display(packets)?,
        }
        Ok(())
    }
}

/// A device that only records what it was asked to do. For tests and SIDEGLANCE_FAKE=1.
#[derive(Default)]
pub struct Fake {
    pub shown: Vec<(usize, Vec<u32>)>,
    pub commands: Vec<(String, Vec<String>)>,
    pub last: Option<Vec<Frame>>,
}

impl Device for Fake {
    fn connect(&mut self) -> Result<(), String> { Ok(()) }
    fn close(&mut self) {}
    fn port_id(&self) -> Option<String> { Some("fake".into()) }
    fn max_frame_ms(&self) -> u32 { protocol::MAX_FRAME_MS }
    fn show(&mut self, frames: &[Frame], durations_ms: &[u32]) -> Result<(), String> {
        self.shown.push((frames.len(), durations_ms.to_vec()));
        self.last = Some(frames.to_vec());
        Ok(())
    }
    fn command(&mut self, name: &str, args: &[String]) -> Result<(), String> {
        self.commands.push((name.into(), args.to_vec()));
        Ok(())
    }
}
