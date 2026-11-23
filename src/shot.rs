//! Pictures of the app without a window, from the live PipeWire graph: a snapshot (`--render`)
//! for the README and the AppStream screenshots, and a short scripted session (`--record`) for
//! the README's animation. Neither changes anything in PipeWire: cords plugged in here are in the
//! picture only.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Rect, Transform};
use resvg::usvg;

use crate::config;
use crate::graph::Dir;
use crate::pw::{Cmd, Connection, Event};
use crate::ui::cord::Held;
use crate::ui::panel::{self, Key, Panel};
use crate::ui::scene::{Hover, Layer, Scene, TARGET_PICS, View, rasterize};
use crate::ui::engine::mask_macs;
use crate::ui::{self, ease};

/// What PipeWire said, with no MAC addresses: the pictures are for others to see, and Bluetooth
/// devices are named by theirs.
fn masked(ev: Event) -> Event {
    let props = |p: crate::pw::Props| p.into_iter().map(|(k, v)| (k, mask_macs(&v))).collect();
    match ev {
        Event::Added { id, kind, props: p } => Event::Added { id, kind, props: props(p) },
        Event::Info { id, props: p, state } => Event::Info { id, props: props(p), state },
        Event::Setting { key, value } => Event::Setting { key, value: mask_macs(&value) },
        Event::Meta { id, subject, key, ty, value } => Event::Meta { id, subject, key, ty, value: value.map(|v| mask_macs(&v)) },
        Event::Param { id, kind, index, text } => Event::Param { id, kind, index, text: mask_macs(&text) },
        ev => ev,
    }
}

/// The next event within `t`, masked.
fn next(conn: &Connection, t: Duration) -> Option<Event> {
    conn.events.recv_timeout(t).ok().map(masked)
}

/// The events waiting, masked.
fn waiting(conn: &Connection) -> Vec<Event> {
    conn.events.try_iter().map(masked).collect()
}

/// A scene on the live graph, with all devices shown, and the connection to PipeWire it came from.
fn live_scene() -> Result<(Scene, Connection, Duration), String> {
    let conn = Connection::start(Arc::new(|| {}));
    let mut scene = Scene::new();
    let started = Instant::now();
    while !scene.graph.synced {
        let ev = next(&conn, Duration::from_secs(5)).ok_or("PipeWire did not answer in 5 s")?;
        scene.apply([ev]);
    }
    scene.apply(waiting(&conn));
    Ok((scene, conn, started.elapsed()))
}

/// Starts the meters the scene asked for. Only those: links the scene wants (cords plugged in for
/// the picture) never reach PipeWire.
fn start_meters(scene: &mut Scene, conn: &Connection) {
    for cmd in scene.outbox.drain(..) {
        if matches!(cmd, Cmd::Meter { .. } | Cmd::Unmeter(_)) {
            conn.send(cmd);
        }
    }
}

/// The signal levels measured since the last look, into the scene. Nothing else: the graph stays
/// as it was, so the cords in the picture stay too.
fn levels(scene: &mut Scene, conn: &Connection) {
    let levels: Vec<Event> = conn.events.try_iter().filter(|e| matches!(e, Event::Levels(_))).collect();
    scene.apply(levels);
}

/// Lets the cords settle for `secs` (as many frames at 60 a second), the meters measuring in
/// real time meanwhile.
fn settle(scene: &mut Scene, conn: &Connection, secs: f64, held: Option<(u64, Held)>) {
    let start = Instant::now();
    for i in 0..(secs * 60.0) as usize {
        levels(scene, conn);
        scene.tick(1.0 / 60.0, None, held, false);
        // the meters need the time to pass
        let due = Duration::from_secs_f64(i as f64 / 60.0);
        if let Some(wait) = due.checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
    }
}

/// The key of a jack: on the first device whose name starts with `dev`, in its group `group`.
fn jack(scene: &Scene, dev: &str, group: &str, dir: Dir) -> Option<String> {
    scene.devs.iter().filter(|d| d.dev.name.starts_with(dev)).flat_map(|d| d.defs.iter().map(move |j| (d, j))).find(|(d, j)| j.dir == dir && d.dev.groups.get(j.group).is_some_and(|g| g.label == group)).map(|(_, j)| j.key.clone())
}

/// Everything in the window at `view`, `size` pixels: the field, the top panel (`dpi` pixels to
/// its units) and the status bar. `held`: the cord whose end is in the hand, and the jack it is
/// pulled to; the jacks it may go into are marked, as in the window.
fn compose(scene: &Scene, view: View, size: (u32, u32), dpi: f64, held: Option<(u64, Option<&str>)>, panel: &mut Panel, status: &Layer) -> Pixmap {
    let (w, h) = size;
    let mut out = ui::app::background(w, h);
    // a picture covering (x, y, w, h) in world units, stretched to it as the GPU does (shadows are
    // drawn small) and faded by `a`
    let draw = |out: &mut Pixmap, pm: &Pixmap, (x, y, lw, lh): (f64, f64, f64, f64), a: f32| {
        let (px, py) = ((x * view.s + view.x).round() as f32, (y * view.s + view.y).round() as f32);
        let (sx, sy) = ((lw * view.s) as f32 / pm.width() as f32, (lh * view.s) as f32 / pm.height() as f32);
        if (sx - 1.0).abs() < 0.01 && (sy - 1.0).abs() < 0.01 {
            out.draw_pixmap(px as i32, py as i32, pm.as_ref(), &PixmapPaint { opacity: a, ..Default::default() }, Transform::identity(), None);
        } else {
            let paint = PixmapPaint { opacity: a, quality: FilterQuality::Bilinear, ..Default::default() };
            out.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_row(sx, 0.0, 0.0, sy, px, py), None);
        }
    };
    // the rack's rails, the whole height
    if scene.rack {
        for (l, [x, y, lw, lh]) in scene.rails(-view.y / view.s, (h as f64 - view.y) / view.s) {
            if let Some(pm) = &l.pm {
                draw(&mut out, pm, (x, y, lw, lh), 1.0);
            }
        }
    }
    let targets: Option<Vec<String>> = held.map(|(seq, _)| scene.targets(seq));
    for (di, d) in scene.devs.iter().enumerate() {
        for l in d.layers() {
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
    // alignment guides, as in the window
    let lw = (1.5 * dpi) as f32;
    let mut amber = resvg::tiny_skia::Paint::default();
    amber.set_color_rgba8(255, 200, 90, 217);
    for &[x0, y0, x1, y1] in &scene.guides {
        let (x, y) = ((x0 * view.s + view.x) as f32, (y0 * view.s + view.y) as f32);
        let (gw, gh) = (((x1 - x0) * view.s) as f32, ((y1 - y0) * view.s) as f32);
        let r = if x0 == x1 { Rect::from_xywh(x - lw / 2.0, y, lw, gh) } else { Rect::from_xywh(x, y - lw / 2.0, gw, lw) };
        if let Some(r) = r {
            out.fill_rect(r, &amber, Transform::identity(), None);
        }
    }
    // the rings round the jacks a cord's end in the hand may go into, under the cords (as in
    // the window, at the top of their pulse)
    if let (Some((_, magnet)), Some(t)) = (held, &targets) {
        for k in t {
            if let Some((x, y)) = scene.spare(k)
                && let Some(l) = &scene.jack_pics[TARGET_PICS + (Some(k.as_str()) == magnet) as usize]
                && let Some(pm) = &l.pm
            {
                draw(&mut out, pm, (x as f64 + l.x, y as f64 + l.y, l.w, l.h), 1.0);
            }
        }
    }
    if let Some((pm, x, y)) = scene.cord_layer(view, (w, h)) {
        out.draw_pixmap(x, y, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    panel.layer(w as f64 / dpi, dpi);
    panel.menu_layer(w as f64 / dpi, dpi);
    for l in panel.layers() {
        if let Some(pm) = &l.pm {
            out.draw_pixmap((l.x * dpi).round() as i32, (l.y * dpi).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    if let Some(pm) = &status.pm {
        out.draw_pixmap(0, (h as f64 - panel::STATUS_H * dpi).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    out
}

fn new_panel(scene: &Scene) -> Panel {
    Panel::new(panel::State { rack: scene.rack, engine: false, mixer: false, locked: false, align: scene.align, cables: 0, defaults: [scene.default_shown(true), scene.default_shown(false)], cue_led: false, cue_level: 70, lcd: ["NO PRESET".into(), "48 kHz · 1024 · 21.3 ms".into()], hidden: scene.hidden_devs.len(), flow: scene.flow, pressed: None, more: false })
}

fn status(scene: &Scene, width: f64, dpi: f64) -> Layer {
    panel::status(width, &format!("PipeWire {} · {} devices", scene.version.as_deref().unwrap_or("?"), scene.devs.len()), "", dpi)
}

/// `patchcord --render out.png [rack] [demo] [split] [hover | held] [save | presets]
/// [profile[=NAME]] [only=NAME,NAME…] [size=WxH]`: draws the live graph to a PNG without opening a window, after
/// letting the cords settle (PATCHCORD_SCALE sets the scale, 1.5 by default). "rack" puts the
/// devices in the rack; "split" shows every stereo jack as L and R; "demo" adds a few cords to the
/// picture (not to PipeWire); "hover" rests the pointer on the jack the first cord comes from;
/// "held" pulls a cord out of the first output and holds its end in the air; "save", "presets",
/// "defaults" or "hidden" opens that drop-down; "profile" opens a card's profile list (the named one's, else a
/// Bluetooth card's). "only" shows just the devices whose names start with those, arranged by
/// signal flow; "size" makes the picture W×H pixels, with the devices fitted into it.
pub fn render(path: &str, opts: &[String]) -> Result<(), String> {
    if opts.iter().any(|o| o == "engine") && opts.iter().any(|o| o == "specimen") {
        return render_specimen(path);
    }
    if opts.iter().any(|o| o == "engine") {
        return render_engine(path, opts);
    }
    if opts.iter().any(|o| o == "mixer") {
        return render_mixer(path, opts);
    }
    let menu = opts.iter().map(String::as_str).find(|o| matches!(*o, "save" | "presets" | "defaults" | "hidden"));
    let opt = |name: &str| opts.iter().find_map(|o| o.strip_prefix(name)?.strip_prefix('='));
    let (mut scene, conn, synced) = live_scene()?;
    start_meters(&mut scene, &conn);
    if let Some(names) = opt("only") {
        scene.keep_only(&names.split(',').collect::<Vec<_>>());
    }
    scene.set_rack(opts.iter().any(|o| o == "rack"));
    // every stereo jack as L above R
    if opts.iter().any(|o| o == "split") {
        for di in 0..scene.devs.len() {
            for g in 0..scene.devs[di].split.len() {
                if scene.devs[di].dev.groups[g].stereo && !scene.devs[di].split[g] {
                    scene.toggle_split(di, g);
                }
            }
        }
    }
    // a cord's end in the hand, up and to the right of the jack it came from
    let held = opts.iter().any(|o| o == "held").then(|| scene.first_output()).flatten().and_then(|k| {
        let (x, y) = scene.spare(&k)?;
        Some((scene.pull_new(&k)?, Held { x: x + 150.0, y: y + 110.0, kind: None }))
    });
    // a few more cords, in the picture only, for screenshots: (device, group) to (device, group)
    if opts.iter().any(|o| o == "demo") {
        let patch = [
            (("Vivaldi", "OUTPUT"), ("Ryzen", "PLAYBACK")),
            (("JLab GO Work 2", "CAPTURE"), ("Bluetooth internal", "INPUT")),
            (("Midi-Bridge", "MIDI"), ("BLE MIDI", "MIDI")),
            (("Ryzen", "CAPTURE"), ("JLab GO Work 2", "PLAYBACK")),
        ];
        for ((od, og), (id, ig)) in patch {
            if let (Some(a), Some(b)) = (jack(&scene, od, og, Dir::Out), jack(&scene, id, ig, Dir::In)) {
                scene.show_cord(&a, &b);
            }
        }
        if scene.rack {
            // the rack in the order of the signal flow
            scene.arrange();
        } else if opt("only").is_some() {
            // a few devices in the order of the signal flow, zigzag, so the cords hang between them
            scene.arrange();
            let mut order: Vec<(f64, f64, String, f64)> = scene.devs.iter().map(|d| (d.dx, d.dy, d.dev.key.clone(), d.geo.w)).collect();
            order.sort_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap());
            let step = order.iter().map(|o| o.3).sum::<f64>() / order.len().max(1) as f64 * 0.62;
            for (i, (_, _, k, _)) in order.iter().enumerate() {
                scene.move_device(k, i as f64 * step, (i % 2) as f64 * 218.0);
            }
        } else {
            // on the desk: what plays on the left, what it plays into on the right, three rows
            let place = [["Vivaldi", "Ryzen", "Midi-Bridge"], ["JLab GO Work 2", "Bluetooth internal", "BLE MIDI"]];
            let key = |scene: &Scene, name: &str| scene.devs.iter().find(|d| d.dev.name.starts_with(name)).map(|d| (d.dev.key.clone(), d.geo.w));
            let left_w = place[0].iter().filter_map(|n| key(&scene, n)).map(|(_, w)| w).fold(0.0, f64::max);
            for (col, names) in place.iter().enumerate() {
                for (row, name) in names.iter().enumerate() {
                    if let Some((k, _)) = key(&scene, name) {
                        scene.move_device(&k, col as f64 * (left_w + 170.0), row as f64 * 200.0);
                    }
                }
            }
        }
    }
    // the pointer on the jack the first cord comes from: its cords light up, its tooltip shows
    let hover = opts.iter().any(|o| o == "hover").then(|| scene.cords.first().and_then(|c| c.a.jack.clone())).flatten();
    if let Some(k) = &hover {
        scene.hover = Hover::Jack(k.clone());
    }
    // let the cords settle, and the meters show what plays
    settle(&mut scene, &conn, 6.0, held);
    let s: f64 = std::env::var("PATCHCORD_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.5);
    let (x0, y0, x1, y1) = scene.bounds().ok_or("no devices")?;
    // room for the cords that hang below the devices
    let y1 = scene.cords.iter().flat_map(|c| c.pts.iter()).map(|p| p.y as f64 + 10.0).fold(y1, f64::max);
    let m = 60.0;
    // the field between the top panel and the status bar, as in the window
    let top = panel::H * s;
    let size = opt("size").and_then(|v| v.split_once('x')).and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
    let ((w, h), view) = match size {
        // the devices fitted between the panel and the status bar
        Some((w, h)) => ((w, h), scene.fit(w as f64, h as f64 - top - panel::STATUS_H * s, top, 2.0 * s, crate::ui::scene::EDGE * s)),
        None => (
            (((x1 - x0 + 2.0 * m) * s).max(1520.0 * s) as u32, ((y1 - y0 + 2.0 * m + 80.0 + panel::H + panel::STATUS_H) * s) as u32),
            // in the middle when the picture is wider than they are (wide enough for the whole
            // top panel)
            View { s, x: -(x0 - m) * s + ((1520.0 - (x1 - x0 + 2.0 * m)) * s / 2.0).max(0.0), y: top - (y0 - m) * s },
        ),
    };
    let t = Instant::now();
    scene.prepare(view.s, 0);
    scene.rail_numbers((h as f64 - view.y) / view.s);
    scene.finish();
    let mut panel = new_panel(&scene);
    let presets = config::load_presets();
    panel.menu = match menu {
        Some("save") => Some(panel::Menu::Save { text: "LATE NIGHT".into(), placeholder: String::new() }),
        Some("presets") => Some(panel::Menu::Load { names: presets.iter().map(|p| p.name.clone()).collect(), current: (!presets.is_empty()).then_some(0), confirm: None }),
        Some("defaults") => Some(crate::ui::app::defaults_menu(&scene)),
        Some("hidden") => Some(crate::ui::app::hidden_menu(&scene)),
        _ => None,
    };
    let status = status(&scene, w as f64 / s, s);
    let mut out = compose(&scene, view, (w, h), s, held.map(|(seq, _)| (seq, None)), &mut panel, &status);
    // a card's profile list, open under its PROFILE key (a Bluetooth one first: it has codecs)
    if let Some(name) = opt("profile").or(opts.iter().any(|o| o == "profile").then_some(""))
        && let Some(d) = scene.devs.iter().filter(|d| d.dev.card.is_some() && d.dev.name.starts_with(name)).max_by_key(|d| d.dev.model.contains("BLUETOOTH"))
        && let (Some(card), Some([kx, ky, _, kh])) = (&d.dev.card, d.geo.profile)
    {
        let (l, _) = panel::profile_menu(card, s);
        if let Some(pm) = &l.pm {
            let (px, py) = ((d.x + kx) * view.s + view.x, (d.y + ky + kh) * view.s + view.y + 4.0 * s);
            out.draw_pixmap(px.round() as i32, py.round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    // a card's port list, open under its PORT key
    if let Some(name) = opt("port").or(opts.iter().any(|o| o == "port").then_some(""))
        && let Some(di) = scene.devs.iter().position(|d| d.geo.port.is_some() && d.dev.name.starts_with(name))
        && let Some([kx, ky, _, kh]) = scene.devs[di].geo.port
    {
        let d = &scene.devs[di];
        let (l, _) = panel::port_menu(&scene.port_sections(di), s);
        if let Some(pm) = &l.pm {
            let (px, py) = ((d.x + kx) * view.s + view.x, (d.y + ky + kh) * view.s + view.y + 4.0 * s);
            out.draw_pixmap(px.round() as i32, py.round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    if let Some(k) = &hover
        && let Some((x, y)) = scene.slot_pos(k, 0)
    {
        let tip = panel::tooltip(&scene.tooltip(&scene.hover), s);
        if let Some(pm) = &tip.pm {
            let (px, py) = (x as f64 * s + view.x + 14.0 * s, y as f64 * s + view.y + 22.0 * s);
            out.draw_pixmap(px.round() as i32, py.round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    out.save_png(path).map_err(|e| e.to_string())?;
    eprintln!(
        "{path}: {} devices, {} cords; graph in {:.1} ms, drawn in {:.0} ms",
        scene.devs.len(),
        scene.cords.iter().filter(|c| c.live()).count(),
        synced.as_secs_f64() * 1000.0,
        t.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

/// Frames a second of the recording.
const FPS: f64 = 25.0;
/// The window recorded: its size in pixels, and pixels to the panel's units.
const SIZE: (u32, u32) = (1980, 1485);
const DPI: f64 = 1.5;
/// How near a jack the end in the hand is pulled into it, as in the window.
const MAGNET: f64 = 34.0;

/// A session played by a script, frame by frame, with a pointer drawn on the frames.
struct Rec {
    scene: Scene,
    /// for the meters' levels only
    conn: Connection,
    view: View,
    view_to: View,
    /// the scale the faces are drawn at (they are stretched to the view, as in the window)
    raster: f64,
    panel: Panel,
    status: Layer,
    cursor: Pixmap,
    /// the pointer, in window pixels, and whether its button is down
    ptr: (f64, f64),
    down: bool,
    /// the cord whose end is in the hand: where the end is, and the jack it is pulled to
    held: Option<(u64, (f64, f64), Option<String>)>,
    /// the device dragged, and where on it the hand holds it
    dragged: Option<(String, (f64, f64))>,
    dir: PathBuf,
    frame: usize,
    /// ENGINE, fed everything PipeWire says; shown while on
    eng: ui::engine::Engine,
    version: String,
}

impl Rec {
    fn world(&self, (x, y): (f64, f64)) -> (f64, f64) {
        ((x - self.view.x) / self.view.s, (y - self.view.y) / self.view.s)
    }

    fn screen(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x * self.view.s + self.view.x, y * self.view.s + self.view.y)
    }

    /// The view that shows every device, between the panel and the status bar.
    fn fit(&self) -> View {
        let top = panel::H * DPI;
        self.scene.fit(SIZE.0 as f64, SIZE.1 as f64 - top - panel::STATUS_H * DPI, top, 1.2 * DPI, 60.0 * DPI)
    }

    /// One frame: everything moves on by 1/FPS second, and the picture is saved.
    fn shoot(&mut self) -> Result<(), String> {
        let dt = (1.0 / FPS) as f32;
        let k = ease(0.15, dt) as f64;
        let (v, t) = (&mut self.view, self.view_to);
        (v.x, v.y, v.s) = (v.x + (t.x - v.x) * k, v.y + (t.y - v.y) * k, v.s + (t.s - v.s) * k);
        let hand = self.world(self.ptr);
        // the end in the hand follows the pointer, and is pulled into a jack it fits nearby
        let mut held = None;
        if let Some((seq, _, _)) = self.held {
            let near = self.scene.targets(seq).into_iter().filter_map(|t| self.scene.spare(&t).map(|p| (t, p))).map(|(t, (x, y))| (t, (x as f64, y as f64))).map(|(t, p)| (t, p, (p.0 - hand.0).hypot(p.1 - hand.1))).filter(|(_, _, d)| *d < MAGNET).min_by(|a, b| a.2.total_cmp(&b.2));
            let to = near.as_ref().map_or(hand, |(_, p, _)| *p);
            let kind = near.as_ref().and_then(|(t, _, _)| self.scene.jack_kind(t));
            if let Some((_, end, magnet)) = &mut self.held {
                let k = ease(0.45, dt) as f64;
                (end.0, end.1) = (end.0 + (to.0 - end.0) * k, end.1 + (to.1 - end.1) * k);
                *magnet = near.map(|(t, _, _)| t);
                held = Some((seq, Held { x: end.0 as f32, y: end.1 as f32, kind }));
            }
        }
        if let Some((key, off)) = self.dragged.clone() {
            self.scene.drag_device(&key, hand.0 - off.0, hand.1 - off.1, DPI / self.view.s);
        }
        self.pump();
        if self.eng.on {
            let devices: Vec<&crate::graph::Device> = self.scene.devs.iter().map(|d| &d.dev).chain(self.scene.hidden_devs.iter()).collect();
            self.eng.build(&self.scene.graph, &devices, &Default::default(), &self.scene.settings);
            let mut out = compose_engine(&mut self.eng, SIZE, DPI, &self.version, &mut self.panel);
            return self.save(&mut out);
        }
        self.scene.tick(dt, None, held, self.dragged.is_some());
        self.scene.prepare(self.raster, (self.frame as f64 * 1000.0 / FPS) as u128);
        self.scene.rail_numbers((SIZE.1 as f64 - self.view.y) / self.view.s);
        self.scene.finish();
        self.panel.state.rack = self.scene.rack;
        let magnet = self.held.as_ref().map(|(seq, _, m)| (*seq, m.as_deref()));
        let mut out = compose(&self.scene, self.view, SIZE, DPI, magnet, &mut self.panel, &self.status);
        self.save(&mut out)
    }

    /// What PipeWire said since the last look: all of it to ENGINE, the levels to the scene too
    /// (its graph stays as it was, so the cords in the picture stay).
    fn pump(&mut self) {
        let mut lv = Vec::new();
        for ev in waiting(&self.conn) {
            self.eng.apply(&ev);
            if matches!(ev, Event::Levels(_)) {
                lv.push(ev);
            }
        }
        self.scene.apply(lv);
        for c in self.eng.outbox.drain(..) {
            self.conn.send(c);
        }
    }

    /// The frame with the pointer on it, saved.
    fn save(&mut self, out: &mut Pixmap) -> Result<(), String> {
        // the pointer: an arrow, with a ring while its button is down
        if self.down {
            let mut ring = resvg::tiny_skia::Paint::default();
            ring.set_color_rgba8(255, 255, 255, 70);
            if let Some(c) = resvg::tiny_skia::PathBuilder::from_circle(self.ptr.0 as f32, self.ptr.1 as f32, 14.0 * DPI as f32) {
                out.fill_path(&c, &ring, resvg::tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        }
        out.draw_pixmap((self.ptr.0 - 2.0 * DPI) as i32, (self.ptr.1 - 2.0 * DPI) as i32, self.cursor.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        out.save_png(self.dir.join(format!("f{:04}.png", self.frame))).map_err(|e| e.to_string())?;
        self.frame += 1;
        Ok(())
    }

    fn wait(&mut self, secs: f64) -> Result<(), String> {
        for _ in 0..(secs * FPS).round() as usize {
            self.shoot()?;
        }
        Ok(())
    }

    /// Moves the pointer to `to` (window pixels) in `secs`, in a gentle arc, slow at both ends.
    fn glide(&mut self, to: (f64, f64), secs: f64) -> Result<(), String> {
        let from = self.ptr;
        let n = (secs * FPS).round().max(1.0) as usize;
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let e = t * t * (3.0 - 2.0 * t);
            let bow = (t * std::f64::consts::PI).sin() * 0.08;
            self.ptr = (from.0 + dx * e - dy * bow, from.1 + dy * e + dx * bow);
            self.shoot()?;
        }
        Ok(())
    }

    /// Moves the pointer to a point in the world.
    fn glide_world(&mut self, to: (f32, f32), secs: f64) -> Result<(), String> {
        let to = self.screen((to.0 as f64, to.1 as f64));
        self.glide(to, secs)
    }

    /// A cord pulled out of the free jack of `out` and plugged into `into`.
    fn patch(&mut self, out: &str, into: &str) -> Result<(), String> {
        let (Some(from), Some(to)) = (self.scene.spare(out), self.scene.spare(into)) else { return Ok(()) };
        self.glide_world(from, 0.9)?;
        self.down = true;
        let Some(seq) = self.scene.pull_new(out) else { return Ok(()) };
        self.held = Some((seq, (from.0 as f64, from.1 as f64), None));
        self.wait(0.15)?;
        self.glide_world(to, 1.4)?;
        self.wait(0.3)?;
        let magnet = self.held.take().and_then(|(_, _, m)| m);
        self.down = false;
        self.scene.drop_held(seq, magnet);
        // in the picture only: PipeWire never hears of it, so it must not wait for its links
        if let Some(c) = self.scene.cords.iter_mut().find(|c| c.seq == seq) {
            c.pending = f32::MAX;
        }
        self.wait(0.7)
    }

    /// Takes device `key` by its name plate and drags it by (dx, dy) world units.
    fn drag(&mut self, key: &str, (dx, dy): (f64, f64), secs: f64) -> Result<(), String> {
        let Some(d) = self.scene.devs.iter().find(|d| d.dev.key == key) else { return Ok(()) };
        let grip = (d.x + 70.0, d.y + 30.0);
        let off = (grip.0 - d.x, grip.1 - d.y);
        self.glide(self.screen(grip), 0.8)?;
        self.down = true;
        self.dragged = Some((key.to_owned(), off));
        self.wait(0.15)?;
        let to = self.screen((grip.0 + dx, grip.1 + dy));
        self.glide(to, secs)?;
        self.wait(0.3)?;
        self.dragged = None;
        self.down = false;
        self.scene.drop_device();
        self.wait(0.4)
    }

    /// Clicks a key of the top panel.
    fn click(&mut self, key: Key) -> Result<(), String> {
        let Some([x, y, w, h]) = self.panel.rect(key) else { return Ok(()) };
        self.glide(((x + w / 2.0) * DPI, (y + h / 2.0) * DPI), 0.9)?;
        self.down = true;
        self.panel.state.pressed = Some(key);
        self.wait(0.12)?;
        self.panel.state.pressed = None;
        self.down = false;
        match key {
            Key::Rack => self.scene.set_rack(true),
            Key::Desk => self.scene.set_rack(false),
            _ => {}
        }
        // ENGINE on its key, off on the desk's and the rack's
        let on = key == Key::Engine;
        if matches!(key, Key::Engine | Key::Desk | Key::Rack) && self.eng.on != on {
            self.eng.set_on(on);
            self.panel.state.engine = on;
            self.pump();
        }
        // the view follows, as Fit does in the window
        self.view_to = self.fit();
        Ok(())
    }

    /// In ENGINE: a click on the node named `name`, which opens its details.
    fn choose(&mut self, name: &str) -> Result<(), String> {
        let Some(to) = self.eng.node_point(name) else { return Ok(()) };
        self.glide(to, 1.0)?;
        self.down = true;
        self.eng.press(self.ptr, SIZE, DPI);
        self.wait(0.12)?;
        self.eng.release();
        self.down = false;
        Ok(())
    }
}

/// The output of an app and a playback input of a device it has no cord to yet.
fn playing_into_new(scene: &Scene) -> Option<(String, String)> {
    let app = |d: &&ui::scene::DevView| d.dev.model.starts_with("APP");
    let out = scene.devs.iter().filter(app).flat_map(|d| d.defs.iter()).find(|j| j.dir == Dir::Out)?.key.clone();
    let inp = scene
        .devs
        .iter()
        .filter(|d| !app(d))
        .flat_map(|d| d.defs.iter().filter(move |j| j.dir == Dir::In && d.dev.groups.get(j.group).is_some_and(|g| g.role == "PLAYBACK")))
        .map(|j| j.key.clone())
        .find(|k| !scene.cords.iter().any(|c| c.a.jack.as_ref() == Some(&out) && c.b.jack.as_ref() == Some(k)))?;
    Some((out, inp))
}

/// `patchcord --record DIR`: plays a short session on the live graph — cords patched, a device
/// dragged into line, the rack and back — and saves it as numbered frames (FPS a second) in DIR,
/// for the README's animation. Nothing is linked in PipeWire.
pub fn record(dir: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // the scene and ENGINE both hear PipeWire from the start
    let conn = Connection::start(Arc::new(|| {}));
    let mut scene = Scene::new();
    let mut eng = ui::engine::Engine::new();
    eng.private = true;
    while !scene.graph.synced {
        let ev = next(&conn, Duration::from_secs(5)).ok_or("PipeWire did not answer in 5 s")?;
        eng.apply(&ev);
        scene.apply([ev]);
    }
    let rest = waiting(&conn);
    for ev in &rest {
        eng.apply(ev);
    }
    scene.apply(rest);
    start_meters(&mut scene, &conn);
    let version = scene.version.clone().unwrap_or_default();
    scene.set_rack(false);
    settle(&mut scene, &conn, 6.0, None);
    let cursor = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><path d="M3 2 L3 19.5 L7.6 15.2 L10.7 22 L13.6 20.7 L10.6 14 L16.8 14 Z" fill="#fff" stroke="#111" stroke-width="1.4" stroke-linejoin="round"/></svg>"##;
    let cursor = rasterize(&usvg::Options::default(), cursor, DPI).ok_or("cursor")?;
    let status = status(&scene, SIZE.0 as f64 / DPI, DPI);
    let panel = new_panel(&scene);
    let mut rec = Rec { scene, conn, view: View { x: 0.0, y: 0.0, s: 1.0 }, view_to: View { x: 0.0, y: 0.0, s: 1.0 }, raster: 0.0, panel, status, cursor, ptr: (SIZE.0 as f64 * 0.55, SIZE.1 as f64 * 0.7), down: false, held: None, dragged: None, dir: Path::new(dir).to_owned(), frame: 0, eng, version };
    rec.view = rec.fit();
    rec.view_to = rec.view;
    // faces drawn once, sharp enough for the desk and the rack
    rec.raster = rec.view.s.max(1.2 * DPI);
    rec.panel.layer(SIZE.0 as f64 / DPI, DPI);

    rec.wait(0.8)?;
    // an app that plays into speakers or headphones it isn't patched to yet, and MIDI from one
    // device to another
    let audio = playing_into_new(&rec.scene);
    let midi = jack(&rec.scene, "Midi-Bridge", "MIDI", Dir::Out).zip(jack(&rec.scene, "BLE MIDI", "MIDI", Dir::In));
    for (a, b) in [audio, midi].into_iter().flatten() {
        rec.patch(&a, &b)?;
    }
    // a device dragged across, lining up with the others
    if let Some(key) = rec.scene.devs.iter().find(|d| d.dev.name.starts_with("BLE MIDI")).or(rec.scene.devs.last()).map(|d| d.dev.key.clone()) {
        rec.drag(&key, (260.0, -60.0), 1.6)?;
    }
    // into the rack and back
    rec.click(Key::Rack)?;
    rec.wait(2.6)?;
    rec.click(Key::Desk)?;
    rec.wait(1.6)?;
    // ENGINE: the real graph; a device chosen shows its details
    rec.click(Key::Engine)?;
    rec.wait(1.8)?;
    let chosen = rec.scene.devs.iter().find(|d| d.dev.model.starts_with("APP")).or(rec.scene.devs.first()).map(|d| d.dev.name.clone());
    if let Some(name) = chosen {
        rec.choose(&name)?;
        rec.wait(2.8)?;
    }
    rec.click(Key::Desk)?;
    rec.wait(1.2)?;
    rec.conn.send(Cmd::Profile(false));
    eprintln!("{dir}: {} frames, {FPS} a second", rec.frame);
    Ok(())
}

/// ENGINE in a window `size` pixels at `dpi`, all of it drawn, with the top panel and the status
/// bar.
fn compose_engine(eng: &mut ui::engine::Engine, (w, h): (u32, u32), dpi: f64, version: &str, panel: &mut Panel) -> Pixmap {
    let mut out = ui::app::background(w, h);
    eng.finish((w, h), dpi, version);
    for (l, x, y, lw, lh, clip, alpha) in eng.sprites((w, h), dpi, version) {
        let Some(pm) = &l.pm else { continue };
        let (sx, sy) = (lw as f32 / pm.width() as f32, lh as f32 / pm.height() as f32);
        let paint = PixmapPaint { quality: FilterQuality::Bilinear, opacity: alpha, ..Default::default() };
        // a body cut to its view
        let mask = clip.and_then(|[cx, cy, cw, ch]| {
            let mut m = resvg::tiny_skia::Mask::new(w, h)?;
            let r = resvg::tiny_skia::Rect::from_xywh(cx as f32, cy as f32, cw as f32, ch as f32)?;
            m.fill_path(&resvg::tiny_skia::PathBuilder::from_rect(r), resvg::tiny_skia::FillRule::Winding, false, Transform::identity());
            Some(m)
        });
        out.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_row(sx, 0.0, 0.0, sy, x.round() as f32, y.round() as f32), mask.as_ref());
    }
    panel.layer(w as f64 / dpi, dpi);
    for l in panel.layers() {
        if let Some(pm) = &l.pm {
            out.draw_pixmap((l.x * dpi).round() as i32, (l.y * dpi).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    let st = panel::status(w as f64 / dpi, &eng.summary(), "click: inspect · drag: pan · wheel: zoom · F: fit · E: back to the desk", dpi);
    if let Some(pm) = &st.pm {
        out.draw_pixmap(0, (h as f64 - panel::STATUS_H * dpi).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    out
}

/// `patchcord --render out.png engine [select=NODE] [tab=top|issues|objects|metadata|events]
/// [itab=info|props|params|timing] [shut=bottom] [legend] [info] [size=WxH]`: the ENGINE screen of the live graph, after the
/// profiler has run for two seconds (it only reads: nothing in PipeWire changes).
fn render_engine(path: &str, opts: &[String]) -> Result<(), String> {
    use crate::ui::engine::Engine;
    let opt = |name: &str| opts.iter().find_map(|o| o.strip_prefix(name)?.strip_prefix('='));
    let conn = Connection::start(Arc::new(|| {}));
    let mut scene = Scene::new();
    let mut eng = Engine::new();
    eng.private = true;
    let start = Instant::now();
    let mut asked = false;
    while start.elapsed() < Duration::from_millis(2600) {
        let Some(ev) = next(&conn, Duration::from_millis(100)) else { continue };
        eng.apply(&ev);
        scene.apply([ev]);
        if scene.graph.synced && !asked {
            asked = true;
            eng.set_on(true);
            for c in eng.outbox.drain(..) {
                conn.send(c);
            }
        }
    }
    let s: f64 = std::env::var("PATCHCORD_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let (w, h) = opt("size").and_then(|v| v.split_once('x')).and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))).unwrap_or((1700, 1000));
    let devices: Vec<&crate::graph::Device> = scene.devs.iter().map(|d| &d.dev).chain(scene.hidden_devs.iter()).collect();
    eng.build(&scene.graph, &devices, &Default::default(), &scene.settings);
    eng.demo(opt("select"), opt("tab"), opt("itab"), opt("shut"));
    if opts.iter().any(|o| o == "legend") {
        eng.toggle_legend();
    }
    if opts.iter().any(|o| o == "info") {
        eng.open_info();
    }
    // the params, if asked for, come back from PipeWire
    for c in eng.outbox.drain(..) {
        conn.send(c);
    }
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(400) {
        if let Some(ev) = next(&conn, Duration::from_millis(50)) {
            eng.apply(&ev);
        }
    }
    eng.dirty = true;
    eng.build(&scene.graph, &devices, &Default::default(), &scene.settings);
    let version = scene.version.clone().unwrap_or_default();
    let mut panel = new_panel(&scene);
    panel.state.engine = true;
    let out = compose_engine(&mut eng, (w, h), s, &version, &mut panel);
    conn.send(Cmd::Profile(false));
    if let Some(r) = opt("report") {
        std::fs::write(r, eng.report(&version)).map_err(|e| e.to_string())?;
    }
    out.save_png(path).map_err(|e| e.to_string())
}

/// MIXER in a window `size` pixels at `dpi`: the console, the top panel and the status bar.
pub fn compose_mixer(mixer: &mut ui::mixer::Mixer, scene: &Scene, (w, h): (u32, u32), dpi: f64, panel: &mut Panel) -> Pixmap {
    let mut out = ui::app::background(w, h);
    mixer.finish(scene, (w, h), dpi);
    for (l, x, y, lw, lh, clip, alpha) in mixer.sprites(scene, &[], (w, h), dpi) {
        let Some(pm) = &l.pm else { continue };
        let (sx, sy) = (lw as f32 / pm.width() as f32, lh as f32 / pm.height() as f32);
        let paint = PixmapPaint { quality: FilterQuality::Bilinear, opacity: alpha, ..Default::default() };
        let mask = clip.and_then(|[cx, cy, cw, ch]| {
            let mut m = resvg::tiny_skia::Mask::new(w, h)?;
            let r = resvg::tiny_skia::Rect::from_xywh(cx as f32, cy as f32, cw as f32, ch as f32)?;
            m.fill_path(&resvg::tiny_skia::PathBuilder::from_rect(r), resvg::tiny_skia::FillRule::Winding, false, Transform::identity());
            Some(m)
        });
        out.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_row(sx, 0.0, 0.0, sy, x.round() as f32, y.round() as f32), mask.as_ref());
    }
    panel.state.mixer = true;
    panel.layer(w as f64 / dpi, dpi);
    for l in panel.layers() {
        if let Some(pm) = &l.pm {
            out.draw_pixmap((l.x * dpi).round() as i32, (l.y * dpi).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    let st = panel::status(w as f64 / dpi, &format!("PipeWire {} · {} devices", scene.version.as_deref().unwrap_or("?"), scene.devs.len()), "drag: scroll · wheel over a fader: volume · M: back · E: ENGINE", dpi);
    if let Some(pm) = &st.pm {
        out.draw_pixmap(0, (h as f64 - panel::STATUS_H * dpi).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    out
}

/// `patchcord --render out.png mixer [size=WxH]`: the MIXER screen of the live graph, its meters
/// measuring for a second (they are Patchcord's own streams; nothing else in PipeWire changes).
fn render_mixer(path: &str, opts: &[String]) -> Result<(), String> {
    let opt = |name: &str| opts.iter().find_map(|o| o.strip_prefix(name)?.strip_prefix('='));
    let (mut scene, conn, _) = live_scene()?;
    start_meters(&mut scene, &conn);
    settle(&mut scene, &conn, 1.0, None);
    let s: f64 = std::env::var("PATCHCORD_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let (w, h) = opt("size").and_then(|v| v.split_once('x')).and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))).unwrap_or((1700, 640));
    let mut mixer = ui::mixer::Mixer::new(true);
    let mut panel = new_panel(&scene);
    let out = compose_mixer(&mut mixer, &scene, (w, h), s, &mut panel);
    out.save_png(path).map_err(|e| e.to_string())
}

/// `--render out.png engine specimen`: every kind of block, link and band ENGINE draws, from a made
/// up graph (not PipeWire's), with nothing, a node, a port and a link selected: the design's
/// specimen sheet.
fn render_specimen(path: &str) -> Result<(), String> {
    use crate::graph::Graph;
    use crate::pw::{DriverTiming, Kind, NodeTiming, Props};
    use crate::ui::engine::Engine;
    let mut g = Graph::default();
    let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
    let node = |g: &mut Graph, id: u32, name: &str, desc: &str, class: &str, state: &'static str| {
        let p = props(&[("node.name", name), ("node.description", desc), ("media.class", class)]);
        g.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
        g.apply(&Event::Info { id, props: p, state: Some(state) });
    };
    // (port, node, direction, name, channel, monitor, midi)
    let ports: &[(u32, u32, &str, &str, &str, bool, bool)] = &[
        (51, 50, "in", "playback_FL", "FL", false, false),
        (52, 50, "in", "playback_FR", "FR", false, false),
        (53, 50, "out", "monitor_FL", "FL", true, false),
        (54, 50, "out", "monitor_FR", "FR", true, false),
        (61, 60, "out", "output_FL", "FL", false, false),
        (62, 60, "out", "output_FR", "FR", false, false),
        (66, 65, "out", "output_FL", "FL", false, false),
        (67, 65, "out", "output_FR", "FR", false, false),
        (71, 70, "in", "input_FL", "FL", false, false),
        (72, 70, "in", "input_FR", "FR", false, false),
        (73, 70, "out", "monitor_FL", "FL", true, false),
        (74, 70, "out", "monitor_FR", "FR", true, false),
        (76, 75, "in", "input_FL", "FL", false, false),
        (77, 75, "in", "input_FR", "FR", false, false),
        (78, 75, "out", "monitor_FL", "FL", true, false),
        (79, 75, "out", "monitor_FR", "FR", true, false),
        (81, 80, "out", "output_MONO", "MONO", false, false),
        (91, 90, "out", "capture_MONO", "MONO", false, false),
        (96, 95, "in", "input_MONO", "MONO", false, false),
        (97, 95, "out", "monitor_MONO", "MONO", true, false),
        (101, 100, "in", "input_MONO", "MONO", false, false),
        (106, 105, "in", "input_MONO", "MONO", false, false),
        (111, 110, "in", "playback_FL", "FL", false, false),
        (112, 110, "in", "playback_FR", "FR", false, false),
        (116, 115, "out", "output_FL", "FL", false, false),
        (117, 115, "out", "output_FR", "FR", false, false),
        (121, 120, "in", "Midi Through Port-0", "", false, true),
        (122, 120, "out", "Midi Through Port-0", "", false, true),
        (126, 125, "in", "in", "", false, true),
        (131, 130, "out", "capture_1", "", false, false),
        (151, 150, "in", "input_FL", "FL", false, false),
        (152, 150, "in", "input_FR", "FR", false, false),
        (156, 155, "out", "output_FL", "FL", false, false),
        (157, 155, "out", "output_FR", "FR", false, false),
    ];
    node(&mut g, 50, "alsa_output.usb-Focusrite_Scarlett_2i2.analog-stereo", "Scarlett 2i2", "Audio/Sink", "running");
    node(&mut g, 60, "Firefox", "Firefox", "Stream/Output/Audio", "running");
    node(&mut g, 65, "fooyin", "fooyin", "Stream/Output/Audio", "running");
    node(&mut g, 70, "patchcord-meter-1", "Patchcord signal meter", "Stream/Input/Audio", "running");
    // a meter on an app's output, as Patchcord puts on each app that plays
    node(&mut g, 75, "patchcord-meter-2", "Patchcord signal meter", "Stream/Input/Audio", "running");
    node(&mut g, 80, "Broken app", "Broken app", "Stream/Output/Audio", "running");
    node(&mut g, 90, "alsa_input.usb-Rode_NT-USB.mono", "RØDE NT-USB", "Audio/Source", "running");
    node(&mut g, 95, "OBS", "OBS", "Stream/Input/Audio", "running");
    node(&mut g, 100, "bluez_capture_internal", "Bluetooth internal capture", "Stream/Input/Audio/Internal", "running");
    node(&mut g, 105, "Recorder", "Recorder", "Stream/Input/Audio", "running");
    node(&mut g, 110, "alsa_output.pci-0000_01_00.1.hdmi-stereo", "HDMI Output", "Audio/Sink", "suspended");
    node(&mut g, 115, "spotify", "Spotify", "Stream/Output/Audio", "idle");
    node(&mut g, 120, "Midi-Bridge", "Midi-Bridge", "Midi/Bridge", "suspended");
    node(&mut g, 125, "synth", "Synth", "Midi/Bridge", "suspended");
    node(&mut g, 130, "v4l2_input.webcam", "Webcam", "Video/Source", "suspended");
    // the cue: Firefox's left channel kept on, played into the Scarlett
    node(&mut g, 150, "patchcord-cue", "Patchcord cue in", "Stream/Input/Audio", "running");
    node(&mut g, 155, "patchcord-cue-out", "Patchcord cue out", "Stream/Output/Audio", "running");
    node(&mut g, 140, "Dummy-Driver", "Dummy-Driver", "", "suspended");
    node(&mut g, 141, "Freewheel-Driver", "Freewheel-Driver", "", "suspended");
    for &(id, n, dir, name, ch, mon, midi) in ports {
        let mut kv = vec![("node.id", n.to_string()), ("port.direction", dir.to_owned()), ("port.name", name.to_owned()), ("format.dsp", if midi { "8 bit raw midi".to_owned() } else { "32 bit float mono audio".to_owned() })];
        if !ch.is_empty() {
            kv.push(("audio.channel", ch.to_owned()));
        }
        if mon {
            kv.push(("port.monitor", "true".to_owned()));
        }
        g.apply(&Event::Added { id, kind: Kind::Port, props: kv.into_iter().map(|(k, v)| (k.to_owned(), v)).collect() });
    }
    // (link, out node, out port, in node, in port, passive, state)
    let links: &[(u32, u32, u32, u32, u32, bool, &'static str)] = &[
        (200, 60, 61, 50, 51, false, "active"),
        (201, 60, 62, 50, 52, false, "active"),
        (202, 65, 66, 50, 51, false, "active"),
        (203, 65, 67, 50, 52, false, "active"),
        (204, 50, 53, 70, 71, true, "active"),
        (205, 50, 54, 70, 72, true, "active"),
        (206, 80, 81, 50, 51, false, "error"),
        (207, 90, 91, 95, 96, false, "active"),
        (208, 90, 91, 100, 101, true, "active"),
        (209, 90, 91, 105, 106, false, "paused"),
        (210, 120, 122, 125, 126, false, "active"),
        (211, 60, 61, 75, 76, true, "active"),
        (212, 60, 62, 75, 77, true, "active"),
        (213, 60, 61, 150, 151, true, "active"),
        (214, 155, 156, 50, 51, false, "active"),
        (215, 155, 157, 50, 52, false, "active"),
        // across bands: Spotify (idle) recorded by the Recorder (on the RØDE's clock)
        (216, 115, 116, 105, 106, true, "paused"),
    ];
    for &(id, on, op, inn, ip, passive, state) in links {
        let p = props(&[("link.output.node", &on.to_string()), ("link.output.port", &op.to_string()), ("link.input.node", &inn.to_string()), ("link.input.port", &ip.to_string()), ("link.passive", &passive.to_string())]);
        g.apply(&Event::Added { id, kind: Kind::Link, props: p.clone() });
        g.apply(&Event::Info { id, props: p, state: Some(state) });
    }
    g.apply(&Event::Synced);
    // the profiler: two drivers and the nodes they clock; fooyin resampled, with xruns
    let t = |id: u32, name: &str, busy_us: i64, rate: u32, xruns: i32| {
        let mut n = NodeTiming::default();
        (n.id, n.name, n.busy_ns, n.busy_max_ns, n.wait_ns, n.wait_max_ns, n.latency, n.xruns, n.cycles) = (id, name.to_owned(), busy_us * 1000, busy_us * 2600, 40_000, 90_000, (1024, rate), xruns, 23);
        n
    };
    let drivers = vec![
        DriverTiming { node: t(50, "Scarlett 2i2", 52, 48000, 0), quantum: 1024, rate: 48000, load: 0.06, xruns: 0, end_ns: 420_000, end_max_ns: 1_300_000, followers: vec![t(60, "Firefox", 31, 48000, 0), t(65, "fooyin", 44, 44100, 3), t(70, "patchcord-meter-1", 12, 48000, 0), t(75, "patchcord-meter-2", 11, 48000, 0), t(80, "Broken app", 9, 48000, 0), t(150, "patchcord-cue", 7, 48000, 0), t(155, "patchcord-cue-out", 8, 48000, 0)] },
        DriverTiming { node: t(90, "RØDE NT-USB", 18, 48000, 0), quantum: 512, rate: 48000, load: 0.02, xruns: 0, end_ns: 150_000, end_max_ns: 400_000, followers: vec![t(95, "OBS", 61, 48000, 0), t(100, "bluez_capture_internal", 16, 48000, 0), t(105, "Recorder", 22, 48000, 0)] },
    ];
    let mut eng = Engine::new();
    // fooyin kept on too, though it plays in the output listened on already: filled all the same
    eng.cue_kept = vec![66, 67];
    eng.apply(&Event::Profile(drivers.clone()));
    eng.apply(&Event::Profile(drivers));
    let devs = g.devices();
    let devices: Vec<&crate::graph::Device> = devs.iter().collect();
    // the HDMI output hidden on the desk
    let hidden = devs.iter().filter(|d| d.name.contains("HDMI")).map(|d| d.key.clone()).collect();
    eng.dirty = true;
    eng.build(&g, &devices, &hidden, &Default::default());
    let svg = eng.specimen(50, 91, 207);
    // an .svg: the drawing itself, to work on as a mockup
    if path.ends_with(".svg") {
        return std::fs::write(path, svg).map_err(|e| e.to_string());
    }
    let s: f64 = std::env::var("PATCHCORD_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.5);
    let pm = Engine::picture(&svg, s).ok_or("cannot draw the specimen")?;
    pm.save_png(path).map_err(|e| e.to_string())
}
