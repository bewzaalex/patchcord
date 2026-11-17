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
use crate::ui::scene::{Hover, Layer, Scene, View, rasterize};
use crate::ui::{self, ease};

/// A scene on the live graph, with all devices shown, and the connection to PipeWire it came from.
fn live_scene() -> Result<(Scene, Connection, Duration), String> {
    let conn = Connection::start(Arc::new(|| {}));
    let mut scene = Scene::new();
    let started = Instant::now();
    while !scene.graph.synced {
        let ev = conn.events.recv_timeout(Duration::from_secs(5)).map_err(|_| "PipeWire did not answer in 5 s")?;
        scene.apply([ev]);
    }
    scene.apply(conn.events.try_iter().collect::<Vec<_>>());
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
    if let Some((_, l)) = &scene.rack_bg
        && let Some(pm) = &l.pm
    {
        draw(&mut out, pm, (l.x, l.y, l.w, l.h), 1.0);
    }
    let targets: Option<Vec<String>> = held.map(|(seq, _)| scene.targets(seq));
    for (di, d) in scene.devs.iter().enumerate() {
        for l in [d.shade.as_ref().map(|(_, l)| l), d.face.as_ref(), d.lcd.as_ref().map(|(_, l)| l), d.meter.as_ref().map(|(_, l)| l)].into_iter().flatten() {
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
    let rings: Vec<((f32, f32), bool)> = match (held, &targets) {
        (Some((_, magnet)), Some(t)) => t.iter().filter_map(|k| scene.spare(k).map(|p| (p, Some(k.as_str()) == magnet))).collect(),
        _ => Vec::new(),
    };
    if let Some((pm, x, y)) = scene.cord_layer(view, (w, h), &rings, 1.0) {
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
    Panel::new(panel::State { rack: scene.rack, locked: false, align: scene.align, cables: 0, sound: false, lcd: ["NO PRESET".into(), "48 kHz · 1024 · 21.3 ms".into()], pressed: None })
}

fn status(scene: &Scene, width: f64, dpi: f64) -> Layer {
    panel::status(width, &format!("PipeWire {} · {} devices", scene.version.as_deref().unwrap_or("?"), scene.devs.len()), "", dpi)
}

/// `patchcord --render out.png [rack] [demo] [split] [hover | held] [save | presets]
/// [only=NAME,NAME…] [size=WxH]`: draws the live graph to a PNG without opening a window, after
/// letting the cords settle (PATCHCORD_SCALE sets the scale, 1.5 by default). "rack" puts the
/// devices in the rack; "split" shows every stereo jack as L and R; "demo" adds a few cords to the
/// picture (not to PipeWire); "hover" rests the pointer on the jack the first cord comes from;
/// "held" pulls a cord out of the first output and holds its end in the air; "save" or "presets"
/// opens that drop-down. "only" shows just the devices whose names start with those, arranged by
/// signal flow; "size" makes the picture W×H pixels, with the devices fitted into it.
pub fn render(path: &str, opts: &[String]) -> Result<(), String> {
    let menu = opts.iter().map(String::as_str).find(|o| matches!(*o, "save" | "presets"));
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
        Some((w, h)) => ((w, h), scene.fit(w as f64, h as f64 - top - panel::STATUS_H * s, top, 2.0 * s)),
        None => (
            (((x1 - x0 + 2.0 * m) * s).max(1200.0 * s) as u32, ((y1 - y0 + 2.0 * m + 80.0 + panel::H + panel::STATUS_H) * s) as u32),
            View { s, x: -(x0 - m) * s, y: top - (y0 - m) * s },
        ),
    };
    let t = Instant::now();
    scene.prepare(view.s, 0);
    let mut panel = new_panel(&scene);
    let presets = config::load_presets();
    panel.menu = match menu {
        Some("save") => Some(panel::Menu::Save { text: "LATE NIGHT".into(), placeholder: String::new() }),
        Some("presets") => Some(panel::Menu::Load { names: presets.iter().map(|p| p.name.clone()).collect(), current: (!presets.is_empty()).then_some(0), confirm: None }),
        _ => None,
    };
    let status = status(&scene, w as f64 / s, s);
    let mut out = compose(&scene, view, (w, h), s, held.map(|(seq, _)| (seq, None)), &mut panel, &status);
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
const SIZE: (u32, u32) = (1800, 1350);
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
        self.scene.fit(SIZE.0 as f64, SIZE.1 as f64 - top - panel::STATUS_H * DPI, top, 1.2 * DPI)
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
        levels(&mut self.scene, &self.conn);
        self.scene.tick(dt, None, held, self.dragged.is_some());
        self.scene.prepare(self.raster, (self.frame as f64 * 1000.0 / FPS) as u128);
        self.panel.state.rack = self.scene.rack;
        let magnet = self.held.as_ref().map(|(seq, _, m)| (*seq, m.as_deref()));
        let mut out = compose(&self.scene, self.view, SIZE, DPI, magnet, &mut self.panel, &self.status);
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
        // the view follows, as Fit does in the window
        self.view_to = self.fit();
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
        .flat_map(|d| d.defs.iter().filter(move |j| j.dir == Dir::In && d.dev.groups.get(j.group).is_some_and(|g| g.label == "PLAYBACK")))
        .map(|j| j.key.clone())
        .find(|k| !scene.cords.iter().any(|c| c.a.jack.as_ref() == Some(&out) && c.b.jack.as_ref() == Some(k)))?;
    Some((out, inp))
}

/// `patchcord --record DIR`: plays a short session on the live graph — cords patched, a device
/// dragged into line, the rack and back — and saves it as numbered frames (FPS a second) in DIR,
/// for the README's animation. Nothing is linked in PipeWire.
pub fn record(dir: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let (mut scene, conn, _) = live_scene()?;
    start_meters(&mut scene, &conn);
    scene.set_rack(false);
    settle(&mut scene, &conn, 6.0, None);
    let cursor = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><path d="M3 2 L3 19.5 L7.6 15.2 L10.7 22 L13.6 20.7 L10.6 14 L16.8 14 Z" fill="#fff" stroke="#111" stroke-width="1.4" stroke-linejoin="round"/></svg>"##;
    let cursor = rasterize(&usvg::Options::default(), cursor, DPI).ok_or("cursor")?;
    let status = status(&scene, SIZE.0 as f64 / DPI, DPI);
    let panel = new_panel(&scene);
    let mut rec = Rec { scene, conn, view: View { x: 0.0, y: 0.0, s: 1.0 }, view_to: View { x: 0.0, y: 0.0, s: 1.0 }, raster: 0.0, panel, status, cursor, ptr: (SIZE.0 as f64 * 0.55, SIZE.1 as f64 * 0.7), down: false, held: None, dragged: None, dir: Path::new(dir).to_owned(), frame: 0 };
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
    rec.wait(2.0)?;
    eprintln!("{dir}: {} frames, {FPS} a second", rec.frame);
    Ok(())
}
