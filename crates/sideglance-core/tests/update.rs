//! Checking for and installing updates, with a fake updater. No network.

use std::sync::{Arc, Mutex};

use serde_json::json;
use sideglance_core::config::Config;
use sideglance_core::device::Fake;
use sideglance_core::http;
use sideglance_core::state::App;
use sideglance_core::update::{Available, Updater, Updates};

#[derive(Default)]
struct FakeUpdater {
    newer: Option<&'static str>,
    fail_check: bool,
    fail_install: bool,
    installs: Mutex<u32>,
}

impl Updater for FakeUpdater {
    fn check(&self) -> Result<Option<Available>, String> {
        if self.fail_check {
            return Err("offline".into());
        }
        Ok(self.newer.map(|v| Available { version: v.into(), notes: "notes".into() }))
    }
    fn install(&self) -> Result<(), String> {
        *self.installs.lock().unwrap() += 1;
        if self.fail_install { Err("signature mismatch".into()) } else { Ok(()) }
    }
}

#[test]
fn without_an_updater_the_page_hides_updates() {
    let updates = Updates::default();
    assert_eq!(updates.status()["supported"], json!(false));
    assert!(updates.check().unwrap_err().contains("only available in the Sideglance app"));
}

#[test]
fn a_newer_version_is_reported_with_its_download_page() {
    let updates = Updates::default();
    updates.set_updater(Arc::new(FakeUpdater { newer: Some("0.5.0"), ..Default::default() }));
    assert_eq!(updates.check().unwrap().unwrap().version, "0.5.0");
    let status = updates.status();
    assert_eq!(status["supported"], json!(true));
    assert_eq!(status["available"]["version"], json!("0.5.0"));
    assert_eq!(status["available"]["url"], json!("https://github.com/maxexq/sideglance-app/releases/tag/v0.5.0"));
    assert_eq!(status["busy"], json!(null));
    assert_eq!(status["checked_seconds_ago"], json!(0));
    assert_eq!(updates.available_version().as_deref(), Some("0.5.0"));
}

#[test]
fn up_to_date_clears_an_earlier_offer() {
    let updates = Updates::default();
    updates.set_updater(Arc::new(FakeUpdater { newer: Some("0.5.0"), ..Default::default() }));
    updates.check().unwrap();
    updates.set_updater(Arc::new(FakeUpdater::default()));
    assert_eq!(updates.check().unwrap(), None);
    assert_eq!(updates.status()["available"], json!(null));
}

#[test]
fn a_failed_check_keeps_what_was_known_and_says_why() {
    let updates = Updates::default();
    updates.set_updater(Arc::new(FakeUpdater { newer: Some("0.5.0"), ..Default::default() }));
    updates.check().unwrap();
    updates.set_updater(Arc::new(FakeUpdater { fail_check: true, ..Default::default() }));
    assert!(updates.check().is_err());
    let status = updates.status();
    assert_eq!(status["available"]["version"], json!("0.5.0"));
    assert_eq!(status["error"], json!("Could not check for updates: offline"));
}

#[test]
fn a_failed_install_is_shown_and_can_be_tried_again() {
    let updates = Updates::default();
    let fake = Arc::new(FakeUpdater { newer: Some("0.5.0"), fail_install: true, ..Default::default() });
    updates.set_updater(fake.clone());
    assert!(updates.install().is_err());
    assert_eq!(updates.status()["error"], json!("Could not install the update: signature mismatch"));
    assert_eq!(updates.status()["busy"], json!(null));
    assert!(updates.install().is_err());
    assert_eq!(*fake.installs.lock().unwrap(), 2);
}

#[test]
fn the_page_checks_through_the_api() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let config = Arc::new(Config::at(std::env::temp_dir().join(format!("sg-update-{n}")).join("config.json")));
    let app = App::new(Box::new(Fake::default()), config, Box::new(|_| Ok((vec![], vec![]))));
    assert_eq!(app.status()["update"]["supported"], json!(false));
    app.updates.set_updater(Arc::new(FakeUpdater { newer: Some("0.5.0"), ..Default::default() }));
    assert!(http::post(&app, "/api/update", &json!({"action": "check"})).unwrap());
    assert_eq!(app.status()["update"]["available"]["version"], json!("0.5.0"));
    assert!(http::post(&app, "/api/update", &json!({"action": "nope"})).is_err());
}
