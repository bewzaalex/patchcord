//! Patchcord: a PipeWire patchbay drawn as a rack of gear, patched with cords.

mod config;
mod dirs;
mod graph;
mod pw;
mod ui;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
use winit::event_loop::EventLoop;

use pw::Connection;
use ui::scene::Scene;

/// `patchcord --render out.png [rack] [hover | held] [save | presets]`: draws the live graph to a PNG
/// without opening a window, after letting the cords settle (PATCHCORD_SCALE sets the scale, 1.5
/// by default). "rack" puts the devices in the rack; "hover" rests the pointer on the jack the
/// first cord comes from; "held" pulls a cord out of the first output
/// and holds its plug in the air; "save" or "presets" opens that drop-down.
fn render_snapshot(path: &str, opts: &[String]) -> Result<(), String> {
    let menu = opts.iter().map(String::as_str).find(|o| matches!(*o, "save" | "presets"));
    let conn = Connection::start(Arc::new(|| {}));
    let mut scene = Scene::new();
    let started = Instant::now();
    while !scene.graph.synced {
        let ev = conn.events.recv_timeout(Duration::from_secs(5)).map_err(|_| "PipeWire did not answer in 5 s")?;
        scene.apply([ev]);
    }
    scene.apply(conn.events.try_iter().collect::<Vec<_>>());
    let synced = started.elapsed();
    scene.set_rack(opts.iter().any(|o| o == "rack"));
    // a plug in the hand, up and to the right of the jack it came from
    let held = opts.iter().any(|o| o == "held").then(|| scene.first_output()).flatten().and_then(|k| {
        let (x, y) = scene.spare(&k)?;
        Some((scene.pull_new(&k)?, ui::cord::Held { x: x + 150.0, y: y + 110.0, vx: 0.0, lift: 1.0, kind: None }))
    });
    // the pointer on the jack the first cord comes from: its cords light up, its tooltip shows
    let hover = opts.iter().any(|o| o == "hover").then(|| scene.cords.first().and_then(|c| c.a.jack.clone())).flatten();
    if let Some(k) = &hover {
        scene.hover = ui::scene::Hover::Jack(k.clone());
    }
    // let the cords settle
    for _ in 0..360 {
        scene.tick(1.0 / 60.0, None, held, false);
    }
    let s: f64 = std::env::var("PATCHCORD_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.5);
    let (x0, y0, x1, y1) = scene.bounds().ok_or("no devices")?;
    let m = 60.0;
    // the field between the top panel and the status bar, as in the window
    let top = ui::panel::H * s;
    let view = ui::scene::View { s, x: -(x0 - m) * s, y: top - (y0 - m) * s };
    let (w, h) = (((x1 - x0 + 2.0 * m) * s).max(1200.0 * s) as u32, ((y1 - y0 + 2.0 * m + 80.0 + ui::panel::H + ui::panel::STATUS_H) * s) as u32);
    let t = Instant::now();
    scene.prepare(s, 0);
    let mut out = ui::app::background(w, h);
    if let Some((_, l)) = &scene.rack_bg
        && let Some(pm) = &l.pm
    {
        out.draw_pixmap((l.x * s + view.x).round() as i32, (l.y * s + view.y).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    // a picture covering (x, y, w, h) in world units, stretched to it as the GPU does (shadows are
    // drawn small) and faded by `a`
    let draw = |out: &mut Pixmap, pm: &Pixmap, (x, y, lw, lh): (f64, f64, f64, f64), a: f32| {
        let (px, py) = ((x * s + view.x).round() as f32, (y * s + view.y).round() as f32);
        let (sx, sy) = ((lw * s) as f32 / pm.width() as f32, (lh * s) as f32 / pm.height() as f32);
        let paint = PixmapPaint { opacity: a, quality: FilterQuality::Bilinear, ..Default::default() };
        if (sx - 1.0).abs() < 0.01 && (sy - 1.0).abs() < 0.01 {
            out.draw_pixmap(px as i32, py as i32, pm.as_ref(), &PixmapPaint { opacity: a, ..Default::default() }, Transform::identity(), None);
        } else {
            out.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_row(sx, 0.0, 0.0, sy, px, py), None);
        }
    };
    let targets: Option<Vec<String>> = held.map(|(seq, _)| scene.targets(seq));
    for (di, d) in scene.devs.iter().enumerate() {
        for l in [d.shade.as_ref().map(|(_, l)| l), d.face.as_ref(), d.lcd.as_ref().map(|(_, l)| l)].into_iter().flatten() {
            if let Some(pm) = &l.pm {
                draw(&mut out, pm, (d.x + l.x, d.y + l.y, l.w, l.h), 1.0);
            }
        }
        for (pic, x, y, a) in scene.jack_sprites(di, targets.as_deref()) {
            if let Some(l) = &scene.jack_pics[pic]
                && let Some(pm) = &l.pm
            {
                draw(&mut out, pm, (x + l.x, y + l.y, l.w, l.h), a);
            }
        }
    }
    // with a plug in the hand, the jacks it may go into are marked, as in the window
    let rings: Vec<((f32, f32), bool)> = held.map_or(Vec::new(), |(seq, _)| scene.targets(seq).iter().filter_map(|t| scene.spare(t)).map(|p| (p, false)).collect());
    if let Some((pm, x, y)) = scene.cord_layer(view, (w, h), &rings, 1.0) {
        out.draw_pixmap(x, y, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    let width = w as f64 / s;
    let mut panel = ui::panel::Panel::new(ui::panel::State { rack: scene.rack, locked: false, cables: 0, sag: 45, sound: false, lcd: ["NO PRESET".into(), "48 kHz · 1024 · 21.3 ms".into()], pressed: None });
    let status = ui::panel::status(width, &format!("PipeWire {} · {} devices", scene.version.as_deref().unwrap_or("?"), scene.devs.len()), "", s);
    let presets = config::load_presets();
    panel.menu = match menu {
        Some("save") => Some(ui::panel::Menu::Save { text: "LATE NIGHT".into(), placeholder: String::new() }),
        Some("presets") => Some(ui::panel::Menu::Load { names: presets.iter().map(|p| p.name.clone()).collect(), current: (!presets.is_empty()).then_some(0), confirm: None }),
        _ => None,
    };
    panel.layer(width, s);
    panel.menu_layer(width, s);
    for l in panel.layers().chain([&status]) {
        if let Some(pm) = &l.pm {
            let y = if std::ptr::eq(l, &status) { h as f64 - ui::panel::STATUS_H * s } else { l.y * s };
            out.draw_pixmap((l.x * s).round() as i32, y.round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    if let Some(k) = &hover
        && let Some((x, y)) = scene.slot_pos(k, 0)
    {
        let tip = ui::panel::tooltip(&scene.tooltip(&scene.hover), s);
        if let Some(pm) = &tip.pm {
            let (px, py) = (x as f64 * s + view.x + 14.0 * s, y as f64 * s + view.y + 22.0 * s);
            out.draw_pixmap(px.round() as i32, py.round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    save(&out, path)?;
    eprintln!(
        "{path}: {} devices, {} cords; graph in {:.1} ms, drawn in {:.0} ms",
        scene.devs.len(),
        scene.cords.iter().filter(|c| c.live()).count(),
        synced.as_secs_f64() * 1000.0,
        t.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

fn save(pm: &Pixmap, path: &str) -> Result<(), String> {
    pm.save_png(path).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() >= 2 && args[0] == "--render" {
        if let Err(e) = render_snapshot(&args[1], &args[2..]) {
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
