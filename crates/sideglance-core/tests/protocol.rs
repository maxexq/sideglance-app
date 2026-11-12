//! The encoder must produce the same bytes as the Python version, which was checked on a Ditoo Pro.
//! tests/golden/python.json was written by the Python version (see tests/golden/README.md).

use serde_json::Value;
use sideglance_core::frame::Frame;
use sideglance_core::protocol;

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/python.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn hex(packets: &[Vec<u8>]) -> Vec<String> {
    packets.iter().map(|p| p.iter().map(|b| format!("{b:02x}")).collect()).collect()
}

#[test]
fn images_encode_like_python() {
    for case in golden()["protocol"].as_array().unwrap() {
        let want = strings(&case["packets"]);
        let got = match case["kind"].as_str().unwrap() {
            "static" => protocol::static_packets(&Frame::from_hex(&strings(&case["pixels"])).unwrap()),
            _ => {
                let frames: Vec<Frame> = case["frames"].as_array().unwrap().iter()
                    .map(|f| Frame::from_hex(&strings(f)).unwrap()).collect();
                let delays: Vec<u32> = case["delays"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as u32).collect();
                protocol::animation_packets(&frames, &delays).unwrap()
            }
        };
        assert_eq!(hex(&got), want, "{}", case["kind"]);
    }
}

#[test]
fn commands_encode_like_python() {
    for case in golden()["commands"].as_array().unwrap() {
        let cmd = case["cmd"].as_str().unwrap();
        let got = protocol::command(cmd, &strings(&case["args"]), (2026, 11, 13, 1, 2, 3)).unwrap();
        assert_eq!(hex(&got), strings(&case["packets"]), "{cmd} {}", case["args"]);
    }
}

#[test]
fn time_command() {
    let got = protocol::command("time", &[], (2026, 11, 13, 1, 2, 3)).unwrap();
    assert_eq!(&got[0][3..12], &[0x18, 26, 20, 11, 13, 1, 2, 3, 0]);
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    assert!(protocol::command("light", &["red".into()], (2026, 1, 1, 0, 0, 0)).is_err());
    assert!(protocol::command("raw", &["45f".into()], (2026, 1, 1, 0, 0, 0)).is_err());
    assert!(protocol::command("brightness", &[], (2026, 1, 1, 0, 0, 0)).is_err());
    assert!(protocol::command("nope", &[], (2026, 1, 1, 0, 0, 0)).is_err());
}

#[test]
fn windows_bluetooth_instance_ids() {
    use sideglance_core::link::mac_from_instance;
    assert_eq!(mac_from_instance("8&2D8E2A9F&0&11223344AABB_C00000000").as_deref(), Some("11:22:33:44:AA:BB"));
    assert_eq!(mac_from_instance("8&2D8E2A9F&0&000000000000_00000003"), None); // incoming port
    assert_eq!(mac_from_instance("garbage"), None);
}

#[test]
fn off_then_a_picture_turns_the_screen_back_on() {
    // the Divoom driver needs a link, so this checks the protocol side: off is brightness 0
    let off = protocol::command("off", &[], (2026, 1, 1, 0, 0, 0)).unwrap();
    let zero = protocol::command("brightness", &["0".into()], (2026, 1, 1, 0, 0, 0)).unwrap();
    assert_eq!(off, zero);
}

#[test]
fn speaker_ports_are_not_the_display() {
    use sideglance_core::link::is_audio_port;
    assert!(is_audio_port("/dev/cu.DitooPro-Audio"));
    assert!(is_audio_port("/dev/cu.Divoom-Ditoo-Audio-1"));
    assert!(!is_audio_port("/dev/cu.DitooPro-Light"));
    assert!(!is_audio_port("/dev/cu.AirPods-Audio")); // not a Divoom at all
}
