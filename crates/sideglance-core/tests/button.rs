//! The device's button: moving to another channel pauses Sideglance, coming back redraws it,
//! and choosing something in Sideglance takes the screen back. No device, no sleeping.

use std::sync::{Arc, Mutex};

use serde_json::json;
use sideglance_core::config::Config;
use sideglance_core::device::Device;
use sideglance_core::frame::Frame;
use sideglance_core::http;
use sideglance_core::protocol;
use sideglance_core::state::App;

#[derive(Default)]
struct Screen {
    channel: Option<u8>, // None: the device does not answer
    shown: usize,
    switched: Vec<u8>,
    asked: usize,
}

struct Ditoo(Arc<Mutex<Screen>>);

impl Device for Ditoo {
    fn connect(&mut self) -> Result<(), String> { Ok(()) }
    fn close(&mut self) {}
    fn port_id(&self) -> Option<String> { Some("test".into()) }
    fn max_frame_ms(&self) -> u32 { 65535 }
    fn show(&mut self, _: &[Frame], _: &[u32]) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        s.shown += 1;
        if s.channel.is_some() {
            s.channel = Some(5); // pictures show on the animation channel
        }
        Ok(())
    }
    fn command(&mut self, name: &str, _: &[String]) -> Result<(), String> {
        if name == "clock" {
            let mut s = self.0.lock().unwrap();
            if s.channel.is_some() { s.channel = Some(0); }
        }
        Ok(())
    }
    fn channel(&mut self) -> Option<u8> {
        let mut s = self.0.lock().unwrap();
        s.asked += 1;
        s.channel
    }
    fn switch_to(&mut self, channel: u8) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        s.switched.push(channel);
        s.channel = Some(channel);
        Ok(())
    }
}

fn setup(answers: bool) -> (Arc<App>, Arc<Mutex<Screen>>) {
    let screen = Arc::new(Mutex::new(Screen { channel: answers.then_some(5), ..Default::default() }));
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let config = Arc::new(Config::at(std::env::temp_dir().join(format!("sg-button-{n}-{:?}", std::thread::current().id())).join("config.json")));
    let app = App::new(Box::new(Ditoo(Arc::clone(&screen))), config, Box::new(|_| Ok((vec![], vec![]))));
    (app, screen)
}

fn draw(app: &Arc<App>) {
    http::post(app, "/api/pixels", &json!({"pixels": vec!["#ff0000"; 256]})).unwrap();
}

fn press_button(screen: &Arc<Mutex<Screen>>, channel: u8) {
    screen.lock().unwrap().channel = Some(channel);
}

#[test]
fn the_channel_answer_is_read_from_the_reply() {
    let reply = protocol::packet(&[0x04, 0x46, 0x55, 0x05, 0x00, 0x00, 0x4a]);
    assert_eq!(protocol::parse_channel(&reply), Some(5));
    assert_eq!(protocol::parse_channel(&protocol::packet(&[0x04, 0x44, 0x55, 0x05])), None);
    assert_eq!(protocol::switch_channel(5), protocol::packet(&[0x45, 0x05]));
}

#[test]
fn another_channel_pauses_sideglance_and_coming_back_redraws_it() {
    let (app, screen) = setup(true);
    draw(&app);
    assert_eq!(screen.lock().unwrap().shown, 1);

    press_button(&screen, 0); // the clock
    app.check_screen();
    assert!(app.away());
    let before = screen.lock().unwrap().shown;
    app.notify("done").unwrap(); // not from the page: waits until the button comes back
    app.dismiss();
    assert_eq!(screen.lock().unwrap().shown, before);

    press_button(&screen, 5); // round to the animation channel again
    app.check_screen();
    assert!(!app.away());
    assert_eq!(screen.lock().unwrap().shown, before + 1);
}

#[test]
fn choosing_something_in_sideglance_switches_back() {
    let (app, screen) = setup(true);
    draw(&app);
    press_button(&screen, 1); // the light
    app.check_screen();
    assert!(app.away());

    http::post(&app, "/api/usage", &json!({"on": true})).unwrap();
    assert!(!app.away());
    assert_eq!(screen.lock().unwrap().switched, vec![5]);
}

#[test]
fn choosing_before_the_next_check_still_switches_back() {
    let (app, screen) = setup(true);
    draw(&app);
    press_button(&screen, 3);
    draw(&app); // no check_screen in between
    assert_eq!(screen.lock().unwrap().switched, vec![5]);
}

#[test]
fn brightness_does_not_take_the_screen() {
    let (app, screen) = setup(true);
    draw(&app);
    press_button(&screen, 0);
    app.check_screen();
    http::post(&app, "/api/command", &json!({"cmd": "brightness", "args": ["40"]})).unwrap();
    assert!(app.away());
    assert!(screen.lock().unwrap().switched.is_empty());
}

#[test]
fn a_device_channel_chosen_on_the_page_is_left_alone() {
    let (app, screen) = setup(true);
    draw(&app);
    http::post(&app, "/api/command", &json!({"cmd": "clock", "args": ["0"]})).unwrap();
    app.check_screen();
    assert!(!app.away());
    assert!(screen.lock().unwrap().switched.is_empty());
}

#[test]
fn a_device_that_never_answers_is_asked_only_a_few_times() {
    let (app, screen) = setup(false);
    for _ in 0..6 {
        draw(&app);
        app.check_screen();
    }
    let s = screen.lock().unwrap();
    assert_eq!(s.shown, 6);
    assert_eq!(s.asked, 3);
    assert!(s.switched.is_empty());
    assert!(!app.away());
}

#[test]
fn the_page_can_bring_sideglance_back() {
    let (app, screen) = setup(true);
    draw(&app);
    press_button(&screen, 2);
    app.check_screen();
    let before = screen.lock().unwrap().shown;
    http::post(&app, "/api/screen", &json!({})).unwrap();
    let s = screen.lock().unwrap();
    assert_eq!(s.switched, vec![5]);
    assert_eq!(s.shown, before + 1);
    drop(s);
    assert!(!app.away());
}
