//! The Divoom Timebox Evo protocol, as the Ditoo Pro speaks it over Bluetooth serial.
//! Pure encoding: nothing here opens a port.
//!
//! A packet is `01 | length (LE) | payload | checksum (LE) | 02`, where length counts the
//! payload plus the two checksum bytes, and the checksum is the sum of length and payload bytes.

use crate::frame::{parse_color, Frame, Rgb};

/// A frame duration is 2 bytes in the protocol.
pub const MAX_FRAME_MS: u32 = 65535;
/// Animations are sent in chunks of this many bytes.
const CHUNK: usize = 200;

pub fn packet(payload: &[u8]) -> Vec<u8> {
    let len = (payload.len() + 2) as u16;
    let mut body = len.to_le_bytes().to_vec();
    body.extend_from_slice(payload);
    let sum: u32 = body.iter().map(|&b| b as u32).sum();
    let mut out = vec![0x01];
    out.extend_from_slice(&body);
    out.extend_from_slice(&((sum & 0xFFFF) as u16).to_le_bytes());
    out.push(0x02);
    out
}

/// Palette (count, then RGB per color, in order of first use) and pixel indexes packed
/// least-significant bit first, with as few bits per pixel as the palette needs.
pub fn palette_and_pixels(frame: &Frame) -> (Vec<u8>, Vec<u8>) {
    let mut colors: Vec<Rgb> = Vec::new();
    let mut indexes = Vec::with_capacity(frame.0.len());
    for c in &frame.0 {
        let i = match colors.iter().position(|k| k == c) {
            Some(i) => i,
            None => {
                colors.push(*c);
                colors.len() - 1
            }
        };
        indexes.push(i);
    }
    let bits = std::cmp::max(1, usize::BITS - (colors.len() - 1).leading_zeros()) as usize;
    let mut pixels = vec![0u8; (indexes.len() * bits).div_ceil(8)];
    let mut pos = 0;
    for i in indexes {
        for b in 0..bits {
            if (i >> b) & 1 == 1 {
                pixels[pos / 8] |= 1 << (pos % 8);
            }
            pos += 1;
        }
    }
    let mut palette = vec![(colors.len() % 256) as u8];
    for c in colors {
        palette.extend_from_slice(&c);
    }
    (palette, pixels)
}

pub fn static_packets(frame: &Frame) -> Vec<Vec<u8>> {
    let (palette, pixels) = palette_and_pixels(frame);
    let body = [palette, pixels].concat();
    let mut data = vec![0x44, 0x00, 0x0a, 0x0a, 0x04, 0xaa];
    data.extend_from_slice(&((body.len() + 6) as u16).to_le_bytes());
    data.extend_from_slice(&[0, 0, 0]);
    data.extend_from_slice(&body);
    vec![packet(&data)]
}

/// Frames that the device loops, each with its own duration in milliseconds.
pub fn animation_packets(frames: &[Frame], delays_ms: &[u32]) -> Result<Vec<Vec<u8>>, String> {
    if frames.len() != delays_ms.len() {
        return Err(format!("{} frames but {} durations", frames.len(), delays_ms.len()));
    }
    let mut blob = Vec::new();
    for (frame, &delay) in frames.iter().zip(delays_ms) {
        let (palette, pixels) = palette_and_pixels(frame);
        let mut body = (delay.min(MAX_FRAME_MS) as u16).to_le_bytes().to_vec();
        body.push(0);
        body.extend_from_slice(&palette);
        body.extend_from_slice(&pixels);
        blob.push(0xaa);
        blob.extend_from_slice(&((body.len() + 3) as u16).to_le_bytes());
        blob.extend_from_slice(&body);
    }
    let total = (blob.len() as u16).to_le_bytes();
    let chunks: Vec<&[u8]> = blob.chunks(CHUNK).collect();
    if chunks.len() > 255 {
        return Err(format!("Animation too large ({} packets, the limit is 255)", chunks.len()));
    }
    Ok(chunks
        .iter()
        .enumerate()
        .map(|(n, chunk)| {
            let mut data = vec![0x49, total[0], total[1], n as u8];
            data.extend_from_slice(chunk);
            packet(&data)
        })
        .collect())
}

fn number(args: &[String], i: usize, low: i64, high: i64, name: &str) -> Result<i64, String> {
    let text = args.get(i).ok_or(format!("{name} needs a value"))?;
    let n: i64 = text.trim().parse().map_err(|_| format!("{name} must be a number (got '{text}')"))?;
    Ok(n.clamp(low, high))
}

fn optional(args: &[String], i: usize, low: i64, high: i64, default: i64, name: &str) -> Result<i64, String> {
    if args.len() > i { number(args, i, low, high, name) } else { Ok(default) }
}

/// The device's own commands that this app sends.
pub const COMMANDS: &[&str] = &["brightness", "clock", "light", "off", "cloud", "score", "clock-weather", "time",
    "volume", "raw"];

/// A native command as packets. `time` uses the local time passed in: (year, month, day, hour, minute, second).
pub fn command(cmd: &str, args: &[String], now: (u16, u8, u8, u8, u8, u8)) -> Result<Vec<Vec<u8>>, String> {
    let p = |bytes: Vec<u8>| Ok(vec![packet(&bytes)]);
    match cmd {
        "brightness" => p(vec![0x74, number(args, 0, 0, 100, "brightness")? as u8]),
        "clock" => {
            let style = optional(args, 0, 0, 5, 0, "style")? as u8;
            let color = match args.get(1) { Some(c) => parse_color(c)?, None => [255, 255, 255] };
            p([vec![0x45, 0x00, 0x01, style, 1, 0, 0, 0], color.to_vec()].concat())
        }
        "light" => {
            let color = parse_color(args.first().ok_or("light needs a color")?)?;
            let bright = optional(args, 1, 0, 100, 100, "brightness")? as u8;
            let effect = optional(args, 2, 0, 4, 0, "effect")? as u8;   // 0 = one color, 1 = cycle colors
            p([vec![0x45, 0x01], color.to_vec(), vec![bright, effect, 1, 0, 0, 0]].concat())
        }
        // Brightness 0. Light mode at brightness 0 would leave the device dark for the next picture.
        "off" => p(vec![0x74, 0]),
        "cloud" => p(vec![0x45, 0x02]),
        // The Ditoo scoreboard tool (72 01), as hass-divoom sends it.
        "score" => {
            let red = number(args, 0, 0, 999, "red")? as u16;
            let blue = number(args, 1, 0, 999, "blue")? as u16;
            p([vec![0x72, 0x01, 0x01], red.to_le_bytes().to_vec(), blue.to_le_bytes().to_vec()].concat())
        }
        "clock-weather" => {
            let temp = number(args, 0, -127, 127, "temperature")?;
            let kind = number(args, 1, 0, 255, "weather")? as u8;
            p(vec![0x5F, temp.rem_euclid(256) as u8, kind])
        }
        "time" => {
            let (y, mo, d, h, mi, s) = now;
            p(vec![0x18, (y % 100) as u8, (y / 100) as u8, mo, d, h, mi, s, 0])
        }
        "volume" => p(vec![0x08, number(args, 0, 0, 15, "volume")? as u8]),
        "raw" => {
            let text: String = args.concat().chars().filter(|c| !c.is_whitespace()).collect();
            if !text.len().is_multiple_of(2) || !text.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("raw takes hex bytes, for example 4505".into());
            }
            p((0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect())
        }
        _ => Err(format!("Unknown command '{cmd}'")),
    }
}
