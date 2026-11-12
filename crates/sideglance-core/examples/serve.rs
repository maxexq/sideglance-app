//! Run the local server without the desktop app, to try the page or develop the core.
//!
//!   cargo run -p sideglance-core --example serve -- ui/dist          with a Ditoo Pro
//!   SIDEGLANCE_FAKE=1 cargo run -p sideglance-core --example serve -- ui/dist   with no device

use std::path::PathBuf;
use std::sync::Arc;

use sideglance_core::config::Config;
use sideglance_core::device::{Device, Divoom, Fake};
use sideglance_core::http;
use sideglance_core::state::{self, App};

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

fn main() {
    let dist = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "ui/dist".into()));
    let config = Arc::new(Config::default_location());
    let device: Box<dyn Device> = if std::env::var("SIDEGLANCE_FAKE").as_deref() == Ok("1") {
        Box::new(Fake::default())
    } else {
        let c = Arc::clone(&config);
        Box::new(Divoom::new(Box::new(move || c.string("port"))))
    };
    let app = App::new(device, Arc::clone(&config), Box::new(state::fetch_usage));
    let assets: http::Assets = Arc::new(move |path: &str| {
        let full = dist.join(path);
        if !full.starts_with(&dist) || path.contains("..") {
            return None;
        }
        std::fs::read(&full).ok().map(|bytes| (bytes, content_type(path).to_string()))
    });
    if config.string("start_with") == "usage" {
        app.set_usage(true);
    }
    println!("Sideglance is running at http://localhost:{}", http::PORT);
    if let Err(e) = http::serve(app, assets, false) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
