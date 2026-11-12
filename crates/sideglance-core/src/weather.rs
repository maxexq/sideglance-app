//! Current weather from Open-Meteo (https://open-meteo.com). Data only: nothing here draws.
//!
//! Open-Meteo needs no account or key for non-commercial use:
//!   forecast   https://api.open-meteo.com/v1/forecast         current temperature, weather code, day or night
//!   geocoding  https://geocoding-api.open-meteo.com/v1/search  city name to latitude and longitude
//! The location is found from this computer's internet address unless one is set. Three free services
//! need no key; the next is tried when one fails or is rate limited: ipapi.co, ipwho.is, ipinfo.io.
//!
//! Config keys (the same as the Python version): weather_auto, weather_latitude, weather_longitude,
//! weather_place, weather_unit ("c" or "f"), weather_in_rotation.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::Config;
use crate::frame::Rgb;
use crate::notifications::parse_icons;

pub const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
pub const GEOCODING_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
pub const REFRESH_SECONDS: f64 = 15.0 * 60.0;
/// After a failed fetch, try again sooner than the normal refresh.
pub const RETRY_SECONDS: f64 = 60.0;
/// Look up the internet address's location again after this long.
pub const LOCATE_SECONDS: f64 = 6.0 * 3600.0;
/// After every location service failed, wait this long before asking them again.
pub const LOCATE_RETRY_SECONDS: f64 = 10.0 * 60.0;
const USER_AGENT: &str = "Sideglance (https://github.com/maxexq/sideglance-app)";

pub const CONDITIONS: [&str; 8] = ["sunny", "partly-cloudy", "cloudy", "rain", "storm", "night", "fog", "snow"];
/// Shown when a code has no good match, and before the first reading.
pub const FALLBACK: &str = "cloudy";

// ---------- icons ----------

pub struct WeatherIcons {
    pub palette: HashMap<char, Rgb>,
    pub icons: HashMap<String, Vec<Vec<char>>>,
}

pub fn icons() -> &'static WeatherIcons {
    static ICONS: OnceLock<WeatherIcons> = OnceLock::new();
    ICONS.get_or_init(|| {
        let parsed = parse_icons(include_str!("weather-icons.txt"), 9, 16).expect("weather-icons.txt");
        for c in CONDITIONS {
            assert!(parsed.icons.contains_key(c), "no weather icon for {c}");
        }
        WeatherIcons { palette: parsed.palette, icons: parsed.icons }
    })
}

/// The icon's 9 rows of 16 colors, None where the pixel is off.
pub fn icon_grid(condition: &str) -> Vec<Vec<Option<Rgb>>> {
    let i = icons();
    let rows = i.icons.get(condition).unwrap_or(&i.icons[FALLBACK]);
    rows.iter().map(|r| r.iter().map(|c| i.palette.get(c).copied()).collect()).collect()
}

// ---------- WMO weather code to icon: the one table ----------

pub fn code_condition(code: i64) -> Option<&'static str> {
    Some(match code {
        0 | 1 => "sunny",                                       // clear sky, mainly clear
        2 => "partly-cloudy",
        3 => "cloudy",                                          // overcast
        45 | 48 => "fog",                                       // fog, depositing rime fog
        51 | 53 | 55 | 56 | 57 => "rain",                       // drizzle, freezing drizzle
        61 | 63 | 65 | 66 | 67 => "rain",                       // rain, freezing rain
        80..=82 => "rain",                                      // rain showers
        71 | 73 | 75 | 77 | 85 | 86 => "snow",                  // snowfall, snow grains, snow showers
        95 | 96 | 97 | 99 => "storm",                           // thunderstorm, with hail, heavy
        _ => return None,
    })
}

/// The icon for a WMO code. is_day false means after sunset at the location: clear sky shows the
/// moon, and partly cloudy (which has no night icon) shows plain cloud rather than a sun.
pub fn condition_for(code: Option<i64>, is_day: Option<bool>) -> &'static str {
    let c = code.and_then(code_condition).unwrap_or(FALLBACK);
    if is_day == Some(false) {
        match c {
            "sunny" => return "night",
            "partly-cloudy" => return "cloudy",
            _ => {}
        }
    }
    c
}

// ---------- settings ----------

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Settings {
    pub auto: bool,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub place: String,
    pub unit: String,
    pub in_rotation: bool,
}

pub fn settings(config: &Config) -> Settings {
    let c = config.load();
    let unit = c.get("weather_unit").and_then(|v| v.as_str()).unwrap_or("c").to_lowercase();
    let latitude = c.get("weather_latitude").and_then(|v| v.as_f64());
    Settings {
        auto: c.get("weather_auto").and_then(|v| v.as_bool()).unwrap_or(latitude.is_none()),
        latitude,
        longitude: c.get("weather_longitude").and_then(|v| v.as_f64()),
        place: c.get("weather_place").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        unit: if unit == "f" { "f".into() } else { "c".into() },
        in_rotation: c.get("weather_in_rotation").and_then(|v| v.as_bool()).unwrap_or(false),
    }
}

/// Change settings from a JSON object with any of: auto, latitude, longitude, place, unit, in_rotation.
/// Setting a location turns automatic location off; auto: true turns it back on.
pub fn save_settings(config: &Config, req: &Value) -> Result<Settings, String> {
    let mut changes: Vec<(&str, Value)> = Vec::new();
    if req["auto"].as_bool() == Some(true) {
        changes.push(("weather_auto", json!(true)));
    }
    let number = |v: &Value| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    if !req["latitude"].is_null() || !req["longitude"].is_null() {
        let (lat, lon) = (number(&req["latitude"]), number(&req["longitude"]));
        let (Some(lat), Some(lon)) = (lat, lon) else { return Err("Enter both latitude and longitude.".into()) };
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err("Latitude is -90 to 90 and longitude is -180 to 180.".into());
        }
        let place = req["place"].as_str().filter(|p| !p.is_empty()).map(str::to_string)
            .unwrap_or_else(|| format!("{lat:.2}, {lon:.2}"));
        let round = |x: f64| (x * 10000.0).round() / 10000.0;
        changes.extend([("weather_latitude", json!(round(lat))), ("weather_longitude", json!(round(lon))),
                        ("weather_place", json!(place)), ("weather_auto", json!(false))]);
    }
    if let Some(unit) = req["unit"].as_str() {
        let u = unit.to_lowercase().chars().next().unwrap_or(' ');
        if u != 'c' && u != 'f' {
            return Err("The unit is c (Celsius) or f (Fahrenheit).".into());
        }
        changes.push(("weather_unit", json!(u.to_string())));
    }
    if let Some(on) = req["in_rotation"].as_bool() {
        changes.push(("weather_in_rotation", json!(on)));
    }
    config.update(&changes).map_err(|e| e.to_string())?;
    Ok(settings(config))
}

// ---------- network ----------

/// An HTTP GET returning JSON, as a parameter so tests need no network.
pub type GetJson = dyn Fn(&str, &[(&str, String)]) -> Result<Value, Failure> + Send + Sync;

#[derive(Debug, Clone)]
pub enum Failure {
    Status(u16),
    Connect(String),
    Other(String),
}

pub fn describe(f: &Failure) -> String {
    match f {
        Failure::Status(429) => "too many requests, try again later".into(),
        Failure::Status(code) => format!("HTTP {code}"),
        Failure::Connect(e) => format!("could not connect ({e})"),
        Failure::Other(e) => e.clone(),
    }
}

pub fn http_get_json(url: &str, params: &[(&str, String)]) -> Result<Value, Failure> {
    let mut req = ureq::get(url).timeout(Duration::from_secs(10)).set("User-Agent", USER_AGENT);
    for (k, v) in params {
        req = req.query(k, v);
    }
    match req.call() {
        Ok(resp) => resp.into_json().map_err(|e| Failure::Other(format!("unexpected answer ({e})"))),
        Err(ureq::Error::Status(code, _)) => Err(Failure::Status(code)),
        Err(e) => Err(Failure::Connect(e.kind().to_string())),
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Place {
    pub place: String,
    pub latitude: f64,
    pub longitude: f64,
}

fn join_place(parts: &[Option<&str>]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for p in parts.iter().flatten() {
        if !p.is_empty() && !seen.contains(p) {
            seen.push(p);
        }
    }
    seen.join(", ")
}

/// The best match for a city name. Err explains what to do.
pub fn find_place(name: &str, get: &GetJson) -> Result<Place, String> {
    let name = name.trim();
    if name.chars().count() < 2 {
        return Err("Type at least two letters of the city name.".into());
    }
    let data = get(GEOCODING_URL, &[("name", name.into()), ("count", "1".into()), ("language", "en".into()),
                                    ("format", "json".into())])
        .map_err(|e| format!("Could not look up '{name}': Open-Meteo: {}.", describe(&e)))?;
    let r = data["results"].get(0).ok_or_else(|| format!(
        "No place called '{name}' was found. Try adding the country, for example 'Paris, France'."))?;
    Ok(Place {
        place: join_place(&[r["name"].as_str(), r["admin1"].as_str(), r["country"].as_str()]),
        latitude: r["latitude"].as_f64().unwrap_or(0.0),
        longitude: r["longitude"].as_f64().unwrap_or(0.0),
    })
}

type Reader = fn(&Value) -> Result<(Option<f64>, Option<f64>, String), String>;

fn read_ipapi(d: &Value) -> Result<(Option<f64>, Option<f64>, String), String> {
    if d["error"].as_bool() == Some(true) {
        return Err(d["reason"].as_str().unwrap_or("error").into());
    }
    Ok((d["latitude"].as_f64(), d["longitude"].as_f64(),
        join_place(&[d["city"].as_str(), d["region"].as_str(), d["country_name"].as_str()])))
}

fn read_ipwhois(d: &Value) -> Result<(Option<f64>, Option<f64>, String), String> {
    if d["success"].as_bool() == Some(false) {
        return Err(d["message"].as_str().unwrap_or("error").into());
    }
    Ok((d["latitude"].as_f64(), d["longitude"].as_f64(),
        join_place(&[d["city"].as_str(), d["region"].as_str(), d["country"].as_str()])))
}

fn read_ipinfo(d: &Value) -> Result<(Option<f64>, Option<f64>, String), String> {
    let loc = d["loc"].as_str().unwrap_or("");
    let (lat, lon) = loc.split_once(',').unwrap_or(("", ""));
    Ok((lat.trim().parse().ok(), lon.trim().parse().ok(),
        join_place(&[d["city"].as_str(), d["region"].as_str(), d["country"].as_str()])))
}

/// (name, URL, reader), tried in this order.
pub const LOCATORS: [(&str, &str, Reader); 3] = [
    ("ipapi.co", "https://ipapi.co/json/", read_ipapi),
    ("ipwho.is", "https://ipwho.is/", read_ipwhois),
    ("ipinfo.io", "https://ipinfo.io/json", read_ipinfo),
];

/// Where this computer's internet address is. City-level at best.
pub fn detect_location(get: &GetJson) -> Result<Place, String> {
    let mut problems = Vec::new();
    for (name, url, read) in LOCATORS {
        let result = get(url, &[]).map_err(|e| describe(&e)).and_then(|d| read(&d));
        match result {
            Ok((Some(lat), Some(lon), place)) => {
                return Ok(Place { place: if place.is_empty() { "your area".into() } else { place }, latitude: lat, longitude: lon });
            }
            Ok(_) => problems.push(format!("{name}: no location for this internet address")),
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    Err(format!("Could not find your location automatically ({}). Set a city instead.", problems.join("; ")))
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Reading {
    pub temperature: Option<f64>,
    pub unit: String,
    pub code: Option<i64>,
    pub is_day: Option<bool>,
    pub condition: String,
    pub place: String,
    /// Unix time the reading arrived, or None before the first good reading.
    pub fetched_at: Option<f64>,
}

pub fn empty_reading(unit: &str, place: &str) -> Reading {
    Reading { temperature: None, unit: unit.into(), code: None, is_day: None, condition: FALLBACK.into(),
              place: place.into(), fetched_at: None }
}

pub fn fetch(lat: f64, lon: f64, place: &str, unit: &str, get: &GetJson, now: f64) -> Result<Reading, String> {
    let data = get(FORECAST_URL, &[
        ("latitude", lat.to_string()), ("longitude", lon.to_string()),
        ("current", "temperature_2m,weather_code,is_day".into()),
        ("temperature_unit", if unit == "f" { "fahrenheit".into() } else { "celsius".into() }),
        ("timezone", "auto".into()),
    ]).map_err(|e| describe(&e))?;
    let current = &data["current"];
    let temperature = current["temperature_2m"].as_f64().ok_or("unexpected answer")?;
    let code = current["weather_code"].as_i64();
    let is_day = current["is_day"].as_i64().map(|d| d != 0);
    Ok(Reading { temperature: Some(temperature), unit: unit.into(), code, is_day,
                 condition: condition_for(code, is_day).into(), place: place.into(), fetched_at: Some(now) })
}

/// Keeps the last good reading. `current()` fetches again when it is older than REFRESH_SECONDS.
/// A failed fetch keeps the last good reading and records the error. The clock is in seconds.
pub struct Tracker {
    pub get: Box<GetJson>,
    pub clock: Box<dyn Fn() -> f64 + Send + Sync>,
    pub reading: Option<Reading>,
    key: Option<String>,
    attempted_at: Option<f64>,
    pub error: Option<String>,
    pub located: Option<Place>,
    located_at: f64,
    locate_error: Option<String>,
    locate_failed_at: f64,
}

pub fn wall_clock() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl Tracker {
    pub fn new(get: Box<GetJson>, clock: Box<dyn Fn() -> f64 + Send + Sync>) -> Tracker {
        Tracker { get, clock, reading: None, key: None, attempted_at: None, error: None, located: None,
                  located_at: 0.0, locate_error: None, locate_failed_at: 0.0 }
    }

    pub fn live() -> Tracker {
        Tracker::new(Box::new(http_get_json), Box::new(wall_clock))
    }

    fn wait(&self) -> f64 {
        if self.error.is_none() { REFRESH_SECONDS } else { RETRY_SECONDS }
    }

    pub fn seconds_to_next(&self) -> f64 {
        match self.attempted_at {
            None => 0.0,
            Some(at) => (self.wait() - ((self.clock)() - at)).max(0.0),
        }
    }

    /// The location in use: the one set, or the automatic one once found.
    pub fn place(&self, s: &Settings) -> (Option<f64>, Option<f64>, String) {
        if !s.auto {
            return (s.latitude, s.longitude, s.place.clone());
        }
        match &self.located {
            Some(p) => (Some(p.latitude), Some(p.longitude), p.place.clone()),
            None => (None, None, String::new()),
        }
    }

    fn locate(&mut self) -> Result<(), String> {
        let now = (self.clock)();
        if self.located.is_some() && now - self.located_at < LOCATE_SECONDS {
            return Ok(());
        }
        if let Some(e) = &self.locate_error {
            if now - self.locate_failed_at < LOCATE_RETRY_SECONDS {
                return Err(e.clone()); // do not ask the services again so soon
            }
        }
        match detect_location(self.get.as_ref()) {
            Ok(p) => {
                self.located = Some(p);
                self.located_at = now;
                self.locate_error = None;
                Ok(())
            }
            Err(e) => {
                self.locate_error = Some(e.clone());
                self.locate_failed_at = now;
                Err(e)
            }
        }
    }

    pub fn current(&mut self, s: &Settings) -> Reading {
        let key = if s.auto { format!("auto {}", s.unit) } else { format!("{:?} {:?} {}", s.latitude, s.longitude, s.unit) };
        if self.key.as_deref() != Some(key.as_str()) {
            self.reading = None;
            self.attempted_at = None;
            self.error = None;
            self.key = Some(key);
        }
        let due = match self.attempted_at {
            None => true,
            Some(at) => (self.clock)() - at >= self.wait(),
        };
        if due {
            self.attempted_at = Some((self.clock)());
            let result = (|| {
                if s.auto {
                    self.locate()?;
                }
                let (lat, lon, place) = self.place(s);
                let (Some(lat), Some(lon)) = (lat, lon) else {
                    return Err("No location set. Set a city or coordinates first.".to_string());
                };
                fetch(lat, lon, &place, &s.unit, self.get.as_ref(), (self.clock)()).map_err(|e| {
                    let then = if self.reading.is_some() { " Showing the last reading." } else { "" };
                    format!("Open-Meteo: {e}.{then}")
                })
            })();
            match result {
                Ok(r) => {
                    self.reading = Some(r);
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            }
        }
        self.peek(s)
    }

    /// The reading without fetching, for status pages.
    pub fn peek(&self, s: &Settings) -> Reading {
        self.reading.clone().unwrap_or_else(|| empty_reading(&s.unit, &self.place(s).2))
    }
}

// ---------- the computer's own location setting ----------

/// Whether this computer lets apps have a location. On Windows, the Privacy & security > Location
/// switches that are off, by their names in Settings. Elsewhere, not checked.
pub fn system_location() -> Value {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location";
        let switches = [
            (HKEY_LOCAL_MACHINE, "", "Location services"),
            (HKEY_CURRENT_USER, "", "Let apps access your location"),
            (HKEY_CURRENT_USER, r"\NonPackaged", "Let desktop apps access your location"),
        ];
        let mut read_any = false;
        let mut off = Vec::new();
        for (hive, sub, name) in switches {
            let value: Option<String> = RegKey::predef(hive).open_subkey(format!("{KEY}{sub}")).ok()
                .and_then(|k| k.get_value("Value").ok());
            if let Some(v) = value {
                read_any = true;
                if v.eq_ignore_ascii_case("deny") {
                    off.push(name);
                }
            }
        }
        if read_any {
            return json!({"checked": true, "off": off});
        }
    }
    json!({"checked": false})
}
