//! Patchcord: a PipeWire patchbay drawn as a rack of gear, patched with cords.

mod config;
mod dirs;
mod graph;
mod pw;
mod shot;
mod ui;

use std::sync::{Arc, Mutex};

use winit::event_loop::EventLoop;

use pw::Connection;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // pictures without a window (see `shot`)
    let shot = match args.first().map(String::as_str) {
        Some("--render") if args.len() >= 2 => Some(shot::render(&args[1], &args[2..])),
        Some("--record") if args.len() >= 2 => Some(shot::record(&args[1])),
        _ => None,
    };
    if let Some(r) = shot {
        if let Err(e) = r {
            eprintln!("patchcord: {e}");
            std::process::exit(1);
        }
        return;
    }
    let event_loop = EventLoop::<()>::with_user_event().build().expect("event loop");
    // PipeWire wakes the event loop whenever it has news
    let proxy = Mutex::new(event_loop.create_proxy());
    let conn = Connection::start(Arc::new(move || {
        let _ = proxy.lock().map(|p| p.send_event(()));
    }));
    let mut app = ui::app::App::new(conn);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("patchcord: {e}");
    }
}
