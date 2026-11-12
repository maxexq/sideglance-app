//! Find and open the Bluetooth serial port of a Divoom Ditoo Pro. Windows and macOS.
//!
//! Pair the Ditoo Pro in the system Bluetooth settings first. The system then creates a serial
//! port for it (COM6 on Windows, /dev/cu.DitooPro-... on macOS). The port is detected
//! automatically, or set by hand on the page (config "port").
//! Only one program can hold the link, so close the Divoom phone app if the device does not answer.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

const DEVICE_NAMES: &[&str] = &["ditoo", "divoom", "timebox", "pixoo", "tivoo", "timoo"];
/// "Get light mode": read-only, changes nothing. The device answers with a frame starting 01.
pub const PROBE: &[u8] = &[0x01, 0x03, 0x00, 0x46, 0x49, 0x00, 0x02];

pub trait Link: Send {
    fn write(&mut self, data: &[u8]) -> std::io::Result<()>;
    /// Whatever arrives within `wait` (up to 256 bytes); empty if nothing does.
    fn read(&mut self, wait: Duration) -> Vec<u8>;
}

pub struct SerialLink(Box<dyn serialport::SerialPort>);

impl SerialLink {
    pub fn open(port: &str) -> Result<SerialLink, String> {
        serialport::new(port, 115_200)
            .timeout(Duration::from_secs(4))
            .open()
            .map(SerialLink)
            .map_err(|e| format!("{port}: {e}"))
    }
}

impl Link for SerialLink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        let _ = self.0.set_timeout(Duration::from_secs(4));
        self.0.write_all(data)?;
        self.0.flush()
    }

    fn read(&mut self, wait: Duration) -> Vec<u8> {
        let _ = self.0.set_timeout(wait.max(Duration::from_millis(1)));
        let mut buf = [0u8; 256];
        match self.0.read(&mut buf) {
            Ok(n) => buf[..n].to_vec(),
            Err(_) => Vec::new(),
        }
    }
}

/// Send the read-only probe. True if the device answered with a Divoom frame.
pub fn probe(link: &mut dyn Link) -> bool {
    link.read(Duration::from_millis(50)); // drop anything already waiting
    if link.write(PROBE).is_err() {
        return false;
    }
    let deadline = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < deadline {
        if link.read(Duration::from_millis(300)).contains(&0x01) {
            return true;
        }
    }
    false
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Candidate {
    pub id: String,
    pub label: String,
    pub likely: bool,
}

/// Windows keeps Bluetooth serial ports under Enum\BTHENUM. An outgoing port's instance id holds
/// the device address; an incoming port has 000000000000 and can never send to the device.
#[cfg(windows)]
fn windows_bluetooth_ports() -> Vec<(String, Option<String>)> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    let mut found = Vec::new();
    let Ok(bthenum) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Enum\BTHENUM", KEY_READ)
    else {
        return found;
    };
    for service in bthenum.enum_keys().flatten() {
        let Ok(service_key) = bthenum.open_subkey(&service) else { continue };
        for instance in service_key.enum_keys().flatten() {
            let Ok(params) = service_key.open_subkey(format!(r"{instance}\Device Parameters")) else { continue };
            let Ok(port) = params.get_value::<String, _>("PortName") else { continue };
            found.push((port, mac_from_instance(&instance)));
        }
    }
    found
}

/// "8&2D8E2A9F&0&11223344AABB_C00000000" to Some("11:22:33:44:AA:BB"); all zeros (incoming) to None.
pub fn mac_from_instance(instance: &str) -> Option<String> {
    let start = instance.rfind('&')? + 1;
    let hex: String = instance[start..].chars().take_while(|c| *c != '_').collect();
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) || hex.chars().all(|c| c == '0') {
        return None;
    }
    let pairs: Vec<String> = (0..12).step_by(2).map(|i| hex[i..i + 2].to_uppercase()).collect();
    Some(pairs.join(":"))
}

/// Every port that could be the Ditoo, most likely first.
pub fn list_ports() -> Vec<Candidate> {
    let ports: Vec<String> = serialport::available_ports().unwrap_or_default().into_iter().map(|p| p.port_name).collect();
    let mut found: Vec<Candidate> = Vec::new();
    #[cfg(windows)]
    {
        for (port, mac) in windows_bluetooth_ports() {
            if let Some(mac) = mac {
                if ports.iter().any(|p| p.eq_ignore_ascii_case(&port)) && !found.iter().any(|c| c.id == port) {
                    found.push(Candidate { label: format!("{port} (Bluetooth {mac})"), id: port, likely: true });
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        for port in ports {
            let lower = port.to_lowercase();
            if cfg!(target_os = "macos") {
                if !port.starts_with("/dev/cu.") || lower.contains("bluetooth-incoming") || lower.contains("debug-console") {
                    continue;
                }
                let likely = DEVICE_NAMES.iter().any(|n| lower.contains(n));
                found.push(Candidate { label: port.clone(), id: port, likely });
            } else if lower.contains("rfcomm") || DEVICE_NAMES.iter().any(|n| lower.contains(n)) {
                found.push(Candidate { label: port.clone(), id: port, likely: true });
            }
        }
    }
    let _ = DEVICE_NAMES;
    found.sort_by_key(|c| !c.likely);
    found
}

pub fn how_to_pair() -> &'static str {
    if cfg!(windows) {
        "Turn the Ditoo Pro on and pair it in Settings > Bluetooth & devices."
    } else if cfg!(target_os = "macos") {
        "Turn the Ditoo Pro on and pair it in System Settings > Bluetooth."
    } else {
        "Turn the Ditoo Pro on and pair it in your Bluetooth settings."
    }
}

/// Try each likely port. The one that answers the probe wins. With no answer anywhere, a single
/// port that opened is used; two or more are too ambiguous to guess.
pub fn autodetect() -> Result<(String, Box<dyn Link>), String> {
    let candidates: Vec<Candidate> = list_ports().into_iter().filter(|c| c.likely).collect();
    if candidates.is_empty() {
        return Err(format!("No Ditoo Pro found. {}", how_to_pair()));
    }
    let mut opened: Vec<(String, Box<dyn Link>)> = Vec::new();
    let mut errors = Vec::new();
    for c in candidates {
        match SerialLink::open(&c.id) {
            Ok(link) => {
                let mut link: Box<dyn Link> = Box::new(link);
                if probe(link.as_mut()) {
                    return Ok((c.id, link));
                }
                opened.push((c.id, link));
            }
            Err(e) => errors.push(e),
        }
    }
    if opened.len() == 1 {
        return Ok(opened.pop().unwrap());
    }
    if !opened.is_empty() {
        let names: Vec<String> = opened.iter().map(|(p, _)| p.clone()).collect();
        return Err(format!("Could not tell which port is the Ditoo Pro ({}). Pick one under Change connection.", names.join(", ")));
    }
    Err(format!("The Ditoo Pro did not answer. Check that it is on and close the Divoom phone app. {}", errors.join(" | ")))
}

/// Open the port set by hand, or detect one when the setting is "auto" or empty.
pub fn connect(setting: &str) -> Result<(String, Box<dyn Link>), String> {
    if setting.is_empty() || setting == "auto" {
        autodetect()
    } else {
        Ok((setting.to_string(), Box::new(SerialLink::open(setting)?)))
    }
}
