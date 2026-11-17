//! Everything on the patch field that doesn't need a window: the devices and where they are,
//! the cords and their physics, and the pictures to draw. The window (app.rs) and the
//! `--render` snapshot both use it.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
use resvg::usvg;

use super::cord::{Cord, Held, RATE};
use super::face::{self, BADGE, BAL_R, BalKnob, DEV_H, Geometry, KNOB_R, Leds, MIX_H, MIX_Y, JP, JackDef, JackKind, LCD_W, LCD_X, LCD_Y, PIC, RACK_MIN, SHADOW};
use super::{ease, paint, text};
use crate::config::Preset;

/// A change to the links, for undo and redo: the (output, input) port pairs added and removed.
type Change = (Vec<(u32, u32)>, Vec<(u32, u32)>);
use crate::graph::{Device, Dir, Graph, Media, VOL_MAX, volume_text};
use crate::pw::{Cmd, Event};

/// What the pointer is over, for highlighting cords and the tooltip.
#[derive(Clone, PartialEq, Debug, Default)]
pub enum Hover {
    #[default]
    None,
    /// a jack, or a plug in it: by jack key
    Jack(String),
    /// a cord, by number
    Cord(u64),
    /// a device, by key
    Device(String),
}

/// A picture of something on the field: (x, y, w, h) is the world area it covers.
pub struct Layer {
    /// unique for every picture made, so the GPU knows which textures it already has
    pub id: u64,
    /// the pixels, until the GPU has them
    pub pm: Option<Pixmap>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// space between units in the rack
const RACK_GAP: f64 = 3.0;
/// the rack's frame around the stack of devices
const FRAME: f64 = 7.0;
/// pixels per world unit for the faces' shadows, whatever the zoom
const SHADE_SCALE: f64 = 0.5;
/// how long a frame may spend drawing pictures again at a new scale after zooming
const RESCALE_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);

/// World → window pixels: `x * s + (x, y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub s: f64,
}

pub struct DevView {
    pub dev: Device,
    /// stereo groups shown as separate L and R jacks
    pub split: Vec<bool>,
    pub defs: Vec<JackDef>,
    /// cords plugged into each jack (its mult has one more jack, free)
    pub counts: Vec<usize>,
    pub geo: Geometry,
    pub x: f64,
    pub y: f64,
    pub tx: f64,
    pub ty: f64,
    /// its place on the desk, kept while it sits in the rack
    pub dx: f64,
    pub dy: f64,
    pub face: Option<Layer>,
    /// its shadow on the desk, and the width it was drawn for
    pub shade: Option<(u64, Layer)>,
    /// what the face was drawn for (jack counts, split switches…) and at what scale
    face_sig: u64,
    face_scale: f64,
    pub lcd: Option<(String, Layer)>,
    /// the meter's lit LEDs, left and right (0…10, falling smoothly)
    pub level: [f32; 2],
    pub meter: Option<((u8, u8), Layer)>,
    /// each app stream's volume, balance and mute as last set here (by group), shown until
    /// PipeWire has caught up
    mix_local: HashMap<usize, (Instant, (f64, f64, bool))>,
    /// what the LCD's second line says for a moment after a volume changes, and until when
    mix_note: Option<(Instant, String)>,
    /// each BAL knob (by group): pushed in, held down or out
    pub bal: HashMap<usize, BalKnob>,
    /// each group's knobs and MUTE LED, and what they were drawn for
    mix_pics: Vec<(usize, u64, Layer)>,
    /// a group's own meter (on a device with two app streams): its lit LEDs, falling smoothly
    glevel: HashMap<usize, [f32; 2]>,
    gmeters: Vec<(usize, (u8, u8), Layer)>,
}

/// How long a volume set here is shown before PipeWire's own is, and the LCD shows it.
const MIX_HOLD: Duration = Duration::from_millis(600);
const MIX_NOTE: Duration = Duration::from_millis(1500);

impl DevView {
    /// A group's volume, balance and mute: as last set here, else as PipeWire has them.
    pub fn mix(&self, gi: usize) -> Option<(f64, f64, bool)> {
        let g = self.dev.groups.get(gi)?;
        let m = g.mix.as_ref()?;
        match self.mix_local.get(&gi) {
            Some((at, v)) if at.elapsed() < MIX_HOLD => Some(*v),
            // only a stereo pair has a balance
            _ => Some((m.volume(), if g.stereo { m.balance() } else { 0.0 }, m.mute)),
        }
    }

    /// A group's BAL knob.
    pub fn bal(&self, gi: usize) -> BalKnob {
        self.bal.get(&gi).copied().unwrap_or_default()
    }

    /// Its pictures, bottom to top.
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        [self.shade.as_ref().map(|(_, l)| l), self.face.as_ref(), self.lcd.as_ref().map(|(_, l)| l), self.meter.as_ref().map(|(_, l)| l)]
            .into_iter()
            .flatten()
            .chain(self.mix_pics.iter().map(|(_, _, l)| l))
            .chain(self.gmeters.iter().map(|(_, _, l)| l))
    }

    pub fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        [self.shade.as_mut().map(|(_, l)| l), self.face.as_mut(), self.lcd.as_mut().map(|(_, l)| l), self.meter.as_mut().map(|(_, l)| l)]
            .into_iter()
            .flatten()
            .chain(self.mix_pics.iter_mut().map(|(_, _, l)| l))
            .chain(self.gmeters.iter_mut().map(|(_, _, l)| l))
    }
}

/// The volume as the LCD shows it: "VOL  85%  -4.23 dB" ("GAIN" on an input), or MUTED.
fn vol_text(volume: f64, mute: bool, input: bool) -> String {
    if mute {
        return "MUTED".into();
    }
    let (pct, db) = volume_text(volume);
    format!("{} {pct:>4}  {db}", if input { "GAIN" } else { "VOL" })
}

/// The balance as the LCD shows it: "BAL R 30%  L -9.29 dB" (the quieter side), or CENTRE.
fn bal_text(volume: f64, balance: f64) -> String {
    if balance.abs() < 0.005 {
        return "BAL CENTRE".into();
    }
    let (near, far) = if balance < 0.0 { ("L", "R") } else { ("R", "L") };
    format!("BAL {near} {:.0}%  {far} {}", balance.abs() * 100.0, volume_text(volume * (1.0 - balance.abs())).1)
}

/// What a meter measures for: a device's SIG meter, or a group's own one (`group`). One measuring
/// what a recording app takes in (`input`) has its channels mapped to the app's (`map`), and the
/// app's gain applied: that happens inside the app, where no meter can listen.
#[derive(Clone, Debug, PartialEq)]
struct MeterFor {
    dev: String,
    group: Option<usize>,
    input: Option<usize>,
    map: Vec<usize>,
}

pub struct Scene {
    pub graph: Graph,
    /// in drawing order: the last is on top
    pub devs: Vec<DevView>,
    /// jack key → (device, jack)
    jacks: HashMap<String, (usize, usize)>,
    /// PipeWire port → jack key
    port_jack: HashMap<u32, String>,
    pub cords: Vec<Cord>,
    seq: u64,
    slot_seq: u64,
    /// devices have been placed once (after the first full graph)
    pub laid_out: bool,
    /// physics time not yet stepped
    acc: f32,
    opt: Arc<usvg::Options<'static>>,
    /// commands for PipeWire (meters to start and stop), sent by whoever owns the connection
    pub outbox: Vec<Cmd>,
    /// meter id by the ports it measures, and what each meter is for, and its peaks
    meters: HashMap<Vec<u32>, u32>,
    meter_dev: HashMap<u32, MeterFor>,
    peaks: HashMap<u32, Vec<f32>>,
    next_meter: u32,
    /// young streams waiting to be shown, at the last look
    young: usize,
    /// cords: 0 shown, 1 faded, 2 hidden
    pub cables: u8,
    /// where devices sat last time (settings) or in a preset, used when they appear
    pub saved_pos: HashMap<String, (f64, f64)>,
    /// devices in a 19" rack instead of on the desk; the rack's order, width and height
    pub rack: bool,
    pub rack_order: Vec<String>,
    rack_w: Option<f64>,
    rack_h: f64,
    /// the rack's rails and inside, behind the devices
    pub rack_bg: Option<(u64, Layer)>,
    /// a device on the desk grew (more jacks): the ones it now lies over make room on the next tick
    crowded: bool,
    /// while a device is dragged on the desk, the lines it is aligned along: (x0, y0, x1, y1)
    pub guides: Vec<[f64; 4]>,
    /// where the dragged device sticks (x, y), each while the hand keeps near it
    snapped: (Option<f64>, Option<f64>),
    /// devices dragged on the desk stick to each other's edges and centres (else they go
    /// exactly where they are held)
    pub align: bool,
    rack_scale: f64,
    /// pictures of a mono, a stereo and a MIDI jack and of the plus badge, laid over the faces
    pub jack_pics: [Option<Layer>; 4],
    pics_scale: f64,
    /// what the pointer is over; its cords stand out, the others fade (`lit` eases in and out,
    /// `lit_for` keeps what was hovered while it fades out)
    pub hover: Hover,
    lit_for: Hover,
    lit: f32,
    /// the links changed since the last preset was saved or loaded
    pub edited: bool,
    /// PipeWire's settings (clock.rate, clock.quantum…) and version
    pub settings: HashMap<String, String>,
    pub version: Option<String>,
    /// jack pairs just pulled apart, not to be plugged back in from links PipeWire hasn't removed yet
    suppress: Vec<((String, String), f32)>,
    /// link changes, for undo and redo: (added, removed) port pairs
    undo: Vec<Change>,
    redo: Vec<Change>,
}


/// How close devices on the desk may come to each other.
const GAP: f64 = 8.0;
/// How near (in pixels on the screen) an edge or centre of a dragged device must come to
/// another's to stick to it, and how far the hand must then pull it to come free.
const SNAP: f64 = 16.0;
const UNSNAP: f64 = 32.0;

/// Whether two devices on the desk, at (x, y) and `w` wide, are at least `gap` apart.
fn apart((ax, ay, aw): (f64, f64, f64), (bx, by, bw): (f64, f64, f64), gap: f64) -> bool {
    ax + aw + gap <= bx || bx + bw + gap <= ax || ay + DEV_H + gap <= by || by + DEV_H + gap <= ay
}

/// The furthest point on the way from `from` to `to` where `free` still holds (`from` must).
fn reach(free: &impl Fn((f64, f64)) -> bool, from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    let at = |m: f64| (from.0 + (to.0 - from.0) * m, from.1 + (to.1 - from.1) * m);
    if free(to) {
        return to;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..14 {
        let m = (lo + hi) / 2.0;
        if free(at(m)) { lo = m } else { hi = m }
    }
    at(lo)
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            graph: Graph::default(),
            devs: Vec::new(),
            jacks: HashMap::new(),
            port_jack: HashMap::new(),
            cords: Vec::new(),
            seq: 0,
            slot_seq: 0,
            laid_out: false,
            acc: 0.0,
            opt: text::svg_options(),
            outbox: Vec::new(),
            meters: HashMap::new(),
            meter_dev: HashMap::new(),
            peaks: HashMap::new(),
            next_meter: 1,
            young: 0,
            cables: 0,
            saved_pos: HashMap::new(),
            rack: false,
            rack_order: Vec::new(),
            rack_w: None,
            rack_h: 0.0,
            rack_bg: None,
            crowded: false,
            guides: Vec::new(),
            snapped: (None, None),
            align: true,
            rack_scale: 0.0,
            jack_pics: [None, None, None, None],
            pics_scale: 0.0,
            hover: Hover::None,
            lit_for: Hover::None,
            lit: 0.0,
            edited: false,
            settings: HashMap::new(),
            version: None,
            suppress: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Takes in what PipeWire reported; true when something to draw changed.
    pub fn apply(&mut self, events: impl IntoIterator<Item = Event>) -> bool {
        let mut changed = false;
        let mut graph = false;
        for ev in events {
            match &ev {
                Event::Error(e) => eprintln!("patchcord: {e}"),
                Event::Levels(l) => {
                    for (id, p) in l {
                        self.peaks.insert(*id, p.clone());
                    }
                    if std::env::var_os("PATCHCORD_DEBUG").is_some() && l.iter().any(|(_, p)| p.iter().any(|v| *v > 0.001)) {
                        let named: Vec<String> = l.iter().map(|(id, p)| format!("{}={:.2?}", self.meter_dev.get(id).map_or("?", |m| m.dev.as_str()), p)).collect();
                        eprintln!("levels: {}", named.join(" "));
                    }
                    changed = true;
                }
                Event::Setting { key, value } => {
                    self.settings.insert(key.clone(), value.clone());
                    changed = true;
                }
                Event::Version(v) => {
                    self.version = Some(v.clone());
                    changed = true;
                }
                _ => {}
            }
            graph |= self.graph.apply(&ev);
        }
        if graph && self.graph.synced {
            self.sync();
        }
        changed || graph
    }

    /// Devices and cords to match the graph: devices keep their place and switches.
    fn sync(&mut self) {
        let order: Vec<String> = self.devs.iter().map(|d| d.dev.key.clone()).collect();
        let mut old: HashMap<String, DevView> = self.devs.drain(..).map(|d| (d.dev.key.clone(), d)).collect();
        let mut fresh = Vec::new();
        for dev in self.graph.devices() {
            let dv = match old.remove(&dev.key) {
                Some(mut dv) => {
                    if dv.split.len() != dev.groups.len() {
                        dv.split = vec![false; dev.groups.len()];
                    }
                    dv.dev = dev;
                    dv
                }
                None => {
                    let split = vec![false; dev.groups.len()];
                    fresh.push(dev.key.clone());
                    DevView { dev, split, defs: Vec::new(), counts: Vec::new(), geo: Geometry::default(), x: 0.0, y: 0.0, tx: 0.0, ty: 0.0, dx: 0.0, dy: 0.0, face: None, shade: None, face_sig: 0, face_scale: 0.0, lcd: None, level: [0.0; 2], meter: None, mix_local: HashMap::new(), mix_note: None, bal: HashMap::new(), mix_pics: Vec::new(), glevel: HashMap::new(), gmeters: Vec::new() }
                }
            };
            self.devs.push(dv);
        }
        // keep the drawing order the user made by dragging
        self.devs.sort_by_key(|d| order.iter().position(|k| *k == d.dev.key).unwrap_or(usize::MAX));
        self.follow_splits();
        self.index();
        self.meters();
        self.reconcile();
        self.slots();
        if !self.laid_out {
            self.laid_out = true;
            self.arrange();
        } else {
            self.place_new(&fresh);
        }
        // devices that have a remembered place go back to it, unless another device sits there
        // now (an app that comes back after the others moved): then it goes below them all
        let bottom = self.devs.iter().filter(|d| !fresh.contains(&d.dev.key)).map(|d| d.ty + DEV_H + 46.0).fold(40.0, f64::max);
        let mut below = bottom;
        for i in 0..self.devs.len() {
            if !fresh.contains(&self.devs[i].dev.key) {
                continue;
            }
            if let Some(&(x, y)) = self.saved_pos.get(&self.devs[i].dev.key) {
                let w = self.devs[i].geo.w;
                let taken = !self.others(i).all(|o| apart((x, y, w), (o.tx, o.ty, o.geo.w), 0.0));
                if taken {
                    (self.devs[i].tx, self.devs[i].ty) = (40.0, below);
                    below += DEV_H + 46.0;
                } else {
                    (self.devs[i].tx, self.devs[i].ty) = (x, y);
                }
            }
            let d = &mut self.devs[i];
            (d.dx, d.dy) = (d.tx, d.ty);
        }
        if self.rack {
            self.layout_rack(None);
        }
        for d in self.devs.iter_mut().filter(|d| fresh.contains(&d.dev.key)) {
            (d.x, d.y) = (d.tx, d.ty);
        }
        self.crowded = true;
    }

    /// The stereo group (device, group) of each port of a stereo jack shown.
    fn stereo_groups(&self) -> HashMap<u32, (usize, usize)> {
        let mut group_of = HashMap::new();
        for (di, d) in self.devs.iter().enumerate() {
            for (gi, g) in d.dev.groups.iter().enumerate().filter(|(_, g)| g.stereo) {
                for p in &g.ports {
                    group_of.insert(p.id, (di, gi));
                }
            }
        }
        group_of
    }

    /// A stereo jack that links tie channel by channel to a split pair is split too (a stream
    /// that starts playing to split headphones, say): as one jack it would show two cords that
    /// each carry only L or R. A jack linked to L with both its channels (a cord pulled from L
    /// into it) stays whole.
    fn follow_splits(&mut self) {
        let links = self.graph.links();
        let group_of = self.stereo_groups();
        let split = |devs: &[DevView], (di, gi): (usize, usize)| devs[di].split.get(gi).copied().unwrap_or(false);
        for l in &links {
            let (Some(&a), Some(&b)) = (group_of.get(&l.out_port), group_of.get(&l.in_port)) else { continue };
            for (x, px, y) in [(a, l.out_port, b), (b, l.in_port, a)] {
                // how many of y's channels the split side's port is linked to
                let to_y = links.iter().filter(|m| (m.out_port == px && group_of.get(&m.in_port) == Some(&y)) || (m.in_port == px && group_of.get(&m.out_port) == Some(&y))).count();
                if split(&self.devs, x) && !split(&self.devs, y) && to_y == 1 {
                    self.devs[y.0].split[y.1] = true;
                }
            }
        }
    }

    /// Faces' geometry; in a rack every face is as wide as the widest needs.
    fn regeo(&mut self) {
        self.rack_w = self.rack.then(|| self.devs.iter().map(|d| face::natural_w(&d.dev, &d.defs, &d.counts, true)).fold(RACK_MIN, f64::max));
        for d in &mut self.devs {
            d.geo = face::face_geometry(&d.dev, &d.defs, &d.counts, self.rack_w);
        }
    }

    /// Stacks the devices in the rack in its order. While one is dragged (`dragged`), it goes where
    /// it is held and the others make room for it.
    fn layout_rack(&mut self, dragged: Option<&str>) {
        let mut order: Vec<String> = self.rack_order.iter().filter(|k| self.devs.iter().any(|d| d.dev.key == **k)).cloned().collect();
        for d in &self.devs {
            if !order.contains(&d.dev.key) {
                order.push(d.dev.key.clone());
            }
        }
        if let Some(k) = dragged
            && let Some(d) = self.devs.iter().find(|d| d.dev.key == k)
        {
            order.retain(|o| o != k);
            let (mut y, mut idx) = (0.0, order.len());
            for (i, _) in order.iter().enumerate() {
                if d.y + DEV_H / 2.0 < y + DEV_H / 2.0 {
                    idx = i;
                    break;
                }
                y += DEV_H + RACK_GAP;
            }
            order.insert(idx, k.to_owned());
        }
        let mut y = 0.0;
        for k in &order {
            if let Some(d) = self.devs.iter_mut().find(|d| d.dev.key == *k)
                && Some(k.as_str()) != dragged
            {
                (d.tx, d.ty) = (0.0, y);
            }
            y += DEV_H + RACK_GAP;
        }
        self.rack_order = order;
        self.rack_h = y;
    }

    /// Devices into the rack, or back to their places on the desk.
    pub fn set_rack(&mut self, on: bool) {
        if on == self.rack {
            return;
        }
        self.rack = on;
        self.regeo();
        if on {
            self.layout_rack(None);
        } else {
            for d in &mut self.devs {
                (d.tx, d.ty) = (d.dx, d.dy);
            }
        }
    }

    /// A device dragged by the hand towards (x, y). On the desk devices don't go over each other:
    /// one pushed against another stops at it and slides along its edge. One that already lies
    /// over another (from an older version) moves freely until it is clear.
    /// `px`: how much of the world one pixel of the screen shows.
    pub fn drag_device(&mut self, key: &str, x: f64, y: f64, px: f64) {
        let Some(i) = self.devs.iter().position(|d| d.dev.key == key).filter(|_| !self.rack) else {
            return self.move_device(key, x, y);
        };
        let from = (self.devs[i].dx, self.devs[i].dy);
        let mut to = self.snap(i, (x, y), px);
        let free = |at: (f64, f64)| self.free_at(i, at);
        if free(from) && !free(to) {
            // as far as it goes straight, then along the edge it met, whichever way goes further
            let s = reach(&free, from, to);
            let (ax, ay) = (reach(&free, s, (x, s.1)), reach(&free, s, (s.0, y)));
            to = if (ax.0 - s.0).abs() >= (ay.1 - s.1).abs() { ax } else { ay };
        }
        self.guides = self.guides_at(i, to);
        self.move_device(key, to.0, to.1);
    }

    /// The devices other than device `i`.
    fn others(&self, i: usize) -> impl Iterator<Item = &DevView> {
        self.devs.iter().enumerate().filter(move |(j, _)| *j != i).map(|(_, o)| o)
    }

    /// Whether device `i` at `at` on the desk keeps clear of the others.
    fn free_at(&self, i: usize, (x, y): (f64, f64)) -> bool {
        let w = self.devs[i].geo.w;
        self.others(i).all(|o| apart((x, y, w), (o.dx, o.dy, o.geo.w), GAP))
    }

    /// Where device `i`, held at `at` on the desk, settles: lined up with another device by the
    /// left edges, centres or right edges, or by the tops, or just beside, above or below one
    /// (as close as devices may come). It sticks there lightly when the hand brings it near,
    /// and holds while the hand stays near (so it doesn't flicker between stuck and free),
    /// until pulled well away. Only places where it keeps clear of the others count.
    fn snap(&mut self, i: usize, at: (f64, f64), px: f64) -> (f64, f64) {
        if !self.align {
            self.snapped = (None, None);
            return at;
        }
        let w = self.devs[i].geo.w;
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for o in self.others(i) {
            xs.extend([o.dx, o.dx + (o.geo.w - w) / 2.0, o.dx + o.geo.w - w, o.dx + o.geo.w + GAP, o.dx - GAP - w]);
            ys.extend([o.dy, o.dy + DEV_H + GAP, o.dy - DEV_H - GAP]);
        }
        // the choices on each axis, nearest first: where it already sticks, those near, or none
        let choices = |held: Option<f64>, raw: f64, all: &[f64]| {
            let mut near: Vec<f64> = all.iter().copied().filter(|v| (v - raw).abs() < SNAP * px).collect();
            near.sort_by(|a, b| (a - raw).abs().total_cmp(&(b - raw).abs()));
            let mut c: Vec<Option<f64>> = held.filter(|h| (h - raw).abs() < UNSNAP * px).into_iter().chain(near).map(Some).collect();
            c.push(None);
            c
        };
        let (cx, cy) = (choices(self.snapped.0, at.0, &xs), choices(self.snapped.1, at.1, &ys));
        let pick = cx.iter().flat_map(|x| cy.iter().map(move |y| (*x, *y))).find(|(x, y)| self.free_at(i, (x.unwrap_or(at.0), y.unwrap_or(at.1))));
        self.snapped = pick.unwrap_or((None, None));
        (self.snapped.0.unwrap_or(at.0), self.snapped.1.unwrap_or(at.1))
    }

    /// The guide lines for device `i` at `at`: every edge or centre it shares with another
    /// device on the axis it sticks on, drawn across both.
    fn guides_at(&self, i: usize, at: (f64, f64)) -> Vec<[f64; 4]> {
        let w = self.devs[i].geo.w;
        let lines = |x: f64, w: f64| [x, x + w / 2.0, x + w];
        let same = |a: f64, b: f64| (a - b).abs() < 0.5;
        let mut guides = Vec::new();
        for o in self.others(i) {
            let (y0, y1) = (at.1.min(o.dy) - 14.0, (at.1 + DEV_H).max(o.dy + DEV_H) + 14.0);
            let (x0, x1) = (at.0.min(o.dx) - 14.0, (at.0 + w).max(o.dx + o.geo.w) + 14.0);
            if self.snapped.0.is_some_and(|s| same(s, at.0)) {
                for v in lines(at.0, w).into_iter().filter(|v| lines(o.dx, o.geo.w).iter().any(|t| same(*t, *v))) {
                    guides.push([v, y0, v, y1]);
                }
            }
            if self.snapped.1.is_some_and(|s| same(s, at.1)) && same(at.1, o.dy) {
                for v in lines(at.1, DEV_H) {
                    guides.push([x0, v, x1, v]);
                }
            }
        }
        guides
    }

    /// Devices on the desk that lie over one another (one grew when its jacks were split, or got
    /// another cord) move apart: of two, the one further right moves right, or the lower one
    /// down, whichever is the shorter way, and on along the line they push. They glide there.
    fn make_room(&mut self) {
        for _ in 0..200 {
            let mut moved = false;
            for i in 0..self.devs.len() {
                for j in 0..self.devs.len() {
                    let (a, b) = (&self.devs[i], &self.devs[j]);
                    if i == j || (b.dx, b.dy) < (a.dx, a.dy) || apart((a.dx, a.dy, a.geo.w), (b.dx, b.dy, b.geo.w), GAP) {
                        continue;
                    }
                    // how far b must go right, or down, to clear a
                    let right = a.dx + a.geo.w + GAP - b.dx;
                    let down = a.dy + DEV_H + GAP - b.dy;
                    let push_down = b.dy >= a.dy && down < right;
                    let b = &mut self.devs[j];
                    if push_down {
                        b.dy += down;
                    } else {
                        b.dx += right;
                    }
                    if !self.rack {
                        (b.tx, b.ty) = (b.dx, b.dy);
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
    }

    /// A device dragged to (x, y): on the desk that is its place; in the rack the others make room.
    pub fn move_device(&mut self, key: &str, x: f64, y: f64) {
        if let Some(d) = self.devs.iter_mut().find(|d| d.dev.key == key) {
            (d.x, d.y, d.tx, d.ty) = (x, y, x, y);
            if !self.rack {
                (d.dx, d.dy) = (x, y);
            }
        }
        if self.rack {
            self.layout_rack(Some(key));
        }
    }

    /// A dragged device let go: in the rack it slides into its slot.
    pub fn drop_device(&mut self) {
        self.guides.clear();
        self.snapped = (None, None);
        if self.rack {
            self.layout_rack(None);
        }
    }

    fn index(&mut self) {
        self.jacks.clear();
        self.port_jack.clear();
        for (di, d) in self.devs.iter_mut().enumerate() {
            d.defs = face::jacks(&d.dev, &d.split);
            d.counts = vec![0; d.defs.len()];
            for (ji, j) in d.defs.iter().enumerate() {
                self.jacks.insert(j.key.clone(), (di, ji));
                for p in &j.ports {
                    self.port_jack.insert(*p, j.key.clone());
                }
            }
        }
        self.regeo();
    }

    /// Meters for the groups that have one: started for new ones, stopped for those gone.
    fn meters(&mut self) {
        // by the ports measured: the device, (node, port) of each, and the channel names
        type Want = (MeterFor, Vec<(u32, u32)>, Vec<String>);
        let mut want: HashMap<Vec<u32>, Want> = HashMap::new();
        let links = self.graph.links();
        for d in &self.devs {
            let own = face::own_meters(&d.dev);
            for (gi, g) in d.dev.groups.iter().enumerate() {
                let group = (own && g.mix.is_some()).then_some(gi);
                if g.meter {
                    let ports: Vec<u32> = g.ports.iter().map(|p| p.id).collect();
                    let targets = g.ports.iter().map(|p| (p.node, p.id)).collect();
                    let channels = g.ports.iter().map(|p| if p.channel.is_empty() { "MONO".to_owned() } else { p.channel.clone() }).collect();
                    let map = (0..g.ports.len()).collect();
                    want.insert(ports, (MeterFor { dev: d.dev.key.clone(), group, input: None, map }, targets, channels));
                }
                // a recording app: what comes into it, every port linked to each of its channels
                if g.dir == Dir::In && g.mix.is_some() {
                    let mut targets = Vec::new();
                    let mut map = Vec::new();
                    for (c, p) in g.ports.iter().enumerate() {
                        for l in links.iter().filter(|l| l.in_port == p.id) {
                            if let Some(node) = self.graph.port_node(l.out_port) {
                                targets.push((node, l.out_port));
                                map.push(c);
                            }
                        }
                    }
                    if targets.is_empty() {
                        continue;
                    }
                    // apart from a meter of the same ports for their own device: marked by the app's node
                    let mut key: Vec<u32> = targets.iter().map(|(_, p)| *p).collect();
                    key.extend([u32::MAX, g.ports[0].node]);
                    let channels = (0..targets.len()).map(|i| format!("AUX{i}")).collect();
                    want.insert(key, (MeterFor { dev: d.dev.key.clone(), group, input: Some(gi), map }, targets, channels));
                }
            }
        }
        let gone: Vec<Vec<u32>> = self.meters.keys().filter(|k| !want.contains_key(*k)).cloned().collect();
        for k in gone {
            if let Some(id) = self.meters.remove(&k) {
                self.meter_dev.remove(&id);
                self.peaks.remove(&id);
                self.outbox.push(Cmd::Unmeter(id));
            }
        }
        for (k, (dev, ports, channels)) in want {
            if let Some(id) = self.meters.get(&k) {
                self.meter_dev.insert(*id, dev);
                continue;
            }
            let id = self.next_meter;
            self.next_meter += 1;
            self.meters.insert(k, id);
            self.meter_dev.insert(id, dev);
            self.outbox.push(Cmd::Meter { id, ports, channels });
        }
    }

    pub fn jack(&self, key: &str) -> Option<(&DevView, &JackDef)> {
        let (di, ji) = *self.jacks.get(key)?;
        let d = &self.devs[di];
        Some((d, &d.defs[ji]))
    }

    /// Where the n-th jack of a mult is, in world units.
    pub fn slot_pos(&self, key: &str, n: usize) -> Option<(f32, f32)> {
        let (di, ji) = *self.jacks.get(key)?;
        let d = &self.devs[di];
        let (lx, ly) = *d.geo.jacks.get(ji)?;
        Some(((d.x + lx + n as f64 * JP) as f32, (d.y + ly) as f32))
    }

    /// The cords to match PipeWire's links: keep those that fit, pull out the rest, plug in what is missing.
    fn reconcile(&mut self) {
        let mut want: Vec<(String, String)> = Vec::new();
        for l in self.graph.links() {
            if let (Some(a), Some(b)) = (self.port_jack.get(&l.out_port), self.port_jack.get(&l.in_port)) {
                let pair = (a.clone(), b.clone());
                if !want.contains(&pair) {
                    want.push(pair);
                }
            }
        }
        let mut have: Vec<(String, String)> = Vec::new();
        for c in &mut self.cords {
            if c.dying.is_none()
                && let (Some(a), Some(b)) = (c.a.jack.clone().or(c.a.orig.clone()), c.b.jack.clone().or(c.b.orig.clone()))
                && (c.a.held || c.b.held)
            {
                // a plug in the hand: its links stay until it is let go
                have.push((a, b));
                continue;
            }
            if !c.live() {
                continue;
            }
            let pair = (c.a.jack.clone().unwrap(), c.b.jack.clone().unwrap());
            let here = self.jacks.contains_key(&pair.0) && self.jacks.contains_key(&pair.1);
            if want.contains(&pair) && !have.contains(&pair) && here {
                c.pending = 0.0;
                have.push(pair);
            } else if c.pending > 0.0 && here && !have.contains(&pair) {
                // just plugged in: PipeWire hasn't made the links yet
                have.push(pair);
            } else {
                // the end whose device went away falls with the cord; else the input end comes out
                let (ga, gb) = (!self.jacks.contains_key(&pair.0), !self.jacks.contains_key(&pair.1));
                c.kill_ends(ga, gb || !ga);
            }
        }
        for (a, b) in want {
            let pair = (a.clone(), b.clone());
            if have.contains(&pair) || self.suppress.iter().any(|(p, _)| *p == pair) {
                continue;
            }
            let (Some(pa), Some(pb)) = (self.slot_pos(&a, 0), self.slot_pos(&b, 0)) else { continue };
            let (ka, kb) = (self.jack(&a).map(|j| j.1.kind).unwrap(), self.jack(&b).map(|j| j.1.kind).unwrap());
            let color = self.jack(&a).map(|j| j.0.dev.color).unwrap();
            self.seq += 1;
            self.slot_seq += 2;
            self.cords.push(Cord::new(self.seq, (self.slot_seq - 1, self.slot_seq), (a, ka, pa), (b, kb, pb), color));
        }
    }

    /// One plug per jack: each cord on a port gets its own jack of the mult, in the order they
    /// were plugged in. Faces whose jack counts changed get new geometry.
    fn slots(&mut self) {
        let before: Vec<Vec<usize>> = self.devs.iter().map(|d| d.counts.clone()).collect();
        for d in &mut self.devs {
            d.counts.iter_mut().for_each(|c| *c = 0);
        }
        let mut per: HashMap<String, Vec<(u64, usize, bool)>> = HashMap::new();
        for (ci, c) in self.cords.iter().enumerate() {
            // a cord being pulled out keeps the jack its other plug is still in until it has faded
            // away: the mult closes up after it, not under it
            for (b, e) in [(false, &c.a), (true, &c.b)] {
                if e.seated() {
                    per.entry(e.jack.clone().unwrap()).or_default().push((e.slot, ci, b));
                }
            }
        }
        for (key, mut list) in per {
            list.sort();
            if let Some(&(di, ji)) = self.jacks.get(&key) {
                self.devs[di].counts[ji] = list.len();
            }
            for (rank, (_, ci, b)) in list.into_iter().enumerate() {
                let e = if b { &mut self.cords[ci].b } else { &mut self.cords[ci].a };
                e.rank = rank;
            }
        }
        if self.devs.iter().zip(&before).any(|(d, old)| d.counts != *old) {
            self.regeo();
            self.crowded = true;
            if self.rack {
                self.layout_rack(None);
            }
        }
    }

    /// Advances `dt` seconds; true while something is still moving.
    pub fn tick(&mut self, dt: f32, hand: Option<(f32, f32)>, held: Option<(u64, Held)>, dragging: bool) -> bool {
        let mut moving = false;
        self.suppress.retain_mut(|(_, t)| {
            *t -= dt;
            *t > 0.0
        });
        // new streams show once old enough: look again when one has grown up (or gone)
        let young = self.graph.young();
        if young < self.young && self.graph.synced {
            self.sync();
        }
        self.young = young;
        moving |= young > 0;
        let glide = ease(0.18, dt) as f64;
        for d in &mut self.devs {
            let (dx, dy) = (d.tx - d.x, d.ty - d.y);
            if dx.abs() + dy.abs() > 0.3 {
                d.x += dx * glide;
                d.y += dy * glide;
                moving = true;
            } else {
                (d.x, d.y) = (d.tx, d.ty);
            }
        }
        self.slots();
        // (not while a device is in the hand: it goes where it is held)
        if self.crowded && !dragging {
            self.crowded = false;
            self.make_room();
        }
        // meters: peaks to lit LEDs on a dB scale (-54…0 dB → 0…10), quick up and slow down
        let mut target: HashMap<(&str, Option<usize>), [f32; 2]> = HashMap::new();
        let index: HashMap<&str, usize> = self.devs.iter().enumerate().map(|(i, d)| (d.dev.key.as_str(), i)).collect();
        for (id, p) in &self.peaks {
            let Some(m) = self.meter_dev.get(id) else { continue };
            let led = |v: f32| if v <= 0.0 { 0.0 } else { ((20.0 * v.log10() + 54.0) / 54.0 * 10.0).clamp(0.0, 10.0) };
            // each of the app's channels: the loudest of what comes into it, times its gain
            let mut ch = [0.0f32; 2];
            for (i, v) in p.iter().enumerate() {
                let c = m.map.get(i).copied().unwrap_or(i).min(1);
                ch[c] = ch[c].max(*v);
            }
            if let Some(gi) = m.input {
                let gains = index.get(m.dev.as_str()).and_then(|&di| {
                    let d = &self.devs[di];
                    let (vol, bal, mute) = d.mix(gi)?;
                    let mx = d.dev.groups[gi].mix.as_ref()?;
                    Some(if mute { vec![0.0; 2] } else { mx.volumes_for(vol, bal) })
                });
                let gains = gains.unwrap_or_else(|| vec![1.0; 2]);
                for (c, v) in ch.iter_mut().enumerate() {
                    *v *= gains.get(c).or(gains.first()).copied().unwrap_or(1.0);
                }
            }
            if p.len() == 1 || m.map.iter().all(|c| *c == 0) {
                ch[1] = ch[0];
            }
            let t = target.entry((m.dev.as_str(), m.group)).or_default();
            (t[0], t[1]) = (t[0].max(led(ch[0])), t[1].max(led(ch[1])));
        }
        let fall = |level: &mut f32, t: f32| {
            *level = if t >= *level { t } else { (*level - 18.0 * dt).max(t) };
            *level > 0.01
        };
        for d in &mut self.devs {
            let t = target.get(&(d.dev.key.as_str(), None)).copied().unwrap_or_default();
            for (level, t) in d.level.iter_mut().zip(t) {
                moving |= fall(level, t);
            }
            for (gi, m) in &d.geo.mix {
                if m.meter.is_none() {
                    continue;
                }
                let t = target.get(&(d.dev.key.as_str(), Some(*gi))).copied().unwrap_or_default();
                let lv = d.glevel.entry(*gi).or_default();
                for (level, t) in lv.iter_mut().zip(t) {
                    moving |= fall(level, t);
                }
            }
        }
        self.acc = (self.acc + dt).min(4.0 / RATE);
        let steps = (self.acc * RATE) as usize;
        self.acc -= steps as f32 / RATE;
        let cords = std::mem::take(&mut self.cords);
        let mut kept = Vec::with_capacity(cords.len());
        for mut c in cords {
            let h = held.filter(|(seq, _)| *seq == c.seq).map(|(_, h)| h);
            c.frame(dt, steps, &|k, n| self.slot_pos(k, n), hand, h, dragging);
            moving |= c.moving();
            if !c.gone() {
                kept.push(c);
            }
        }
        self.cords = kept;
        // the hovered thing's cords light up quickly and fade back a little slower; something
        // without cords lights none, and leaves the others as they are
        let has = self.hover != Hover::None && self.cords.iter().any(|c| self.lights_for(c, &self.hover));
        if has {
            self.lit_for = self.hover.clone();
        }
        let to = if has { 1.0 } else { 0.0 };
        let was = self.lit;
        self.lit = ease_lit(self.lit, to, dt);
        moving |= self.lit != was;
        moving
    }

    /// Columns by signal flow: sources on the left, then what they feed, and so on. In the rack:
    /// sources at the top, then what they feed.
    pub fn arrange(&mut self) {
        let n = self.devs.len();
        let mut rank = vec![0usize; n];
        let edges: Vec<(usize, usize)> = self
            .cords
            .iter()
            .filter(|c| c.live())
            .filter_map(|c| Some((self.jacks.get(c.a.jack.as_ref()?)?.0, self.jacks.get(c.b.jack.as_ref()?)?.0)))
            .collect();
        for _ in 0..n {
            for &(a, b) in &edges {
                if a != b && rank[b] <= rank[a] {
                    rank[b] = (rank[a] + 1).min(4);
                }
            }
        }
        if self.rack {
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|a, b| rank[*a].cmp(&rank[*b]).then_with(|| self.devs[*a].dev.key.cmp(&self.devs[*b].dev.key)));
            self.rack_order = order.into_iter().map(|i| self.devs[i].dev.key.clone()).collect();
            return self.layout_rack(None);
        }
        let mut x = 40.0;
        for r in 0..=4 {
            let mut col: Vec<usize> = (0..n).filter(|i| rank[*i] == r).collect();
            if col.is_empty() {
                continue;
            }
            col.sort_by(|a, b| self.devs[*a].dev.key.cmp(&self.devs[*b].dev.key));
            let mut y = 40.0;
            let mut w: f64 = 0.0;
            for i in col {
                let d = &mut self.devs[i];
                (d.tx, d.ty) = (x, y);
                (d.dx, d.dy) = (x, y);
                y += DEV_H + 46.0;
                w = w.max(d.geo.w);
            }
            x += w + 110.0;
        }
    }

    /// Devices that appeared later go below everything else.
    fn place_new(&mut self, keys: &[String]) {
        let mut y = self.devs.iter().filter(|d| !keys.contains(&d.dev.key)).map(|d| d.dy + DEV_H + 46.0).fold(40.0, f64::max);
        for d in self.devs.iter_mut().filter(|d| keys.contains(&d.dev.key)) {
            (d.tx, d.ty) = (40.0, y);
            (d.x, d.y) = (d.tx, d.ty);
            y += DEV_H + 46.0;
        }
    }

    /// World area the devices take (where they are going).
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut it = self.devs.iter();
        let f = it.next()?;
        let mut b = (f.tx, f.ty, f.tx + f.geo.w, f.ty + DEV_H);
        for d in it {
            b = (b.0.min(d.tx), b.1.min(d.ty), b.2.max(d.tx + d.geo.w), b.3.max(d.ty + DEV_H));
        }
        Some(b)
    }

    /// A view that shows every device in the window area `top` pixels down, w × h pixels.
    pub fn fit(&self, w: f64, h: f64, top: f64, max_scale: f64) -> View {
        let Some((x0, y0, x1, y1)) = self.bounds() else { return View { x: 0.0, y: top, s: 1.0 } };
        let m = 60.0;
        let s = ((w - 2.0 * m) / (x1 - x0)).min((h - 2.0 * m) / (y1 - y0 + 60.0)).clamp(0.3, max_scale);
        View { s, x: (w - (x1 - x0) * s) / 2.0 - x0 * s, y: top + ((h - (y1 - y0 + 60.0) * s) / 2.0).max(m * 0.6) - y0 * s }
    }

    /// Draws the rack, faces and LCDs that are missing, out of date or drawn at another scale.
    /// `ms`: time, for the scrolling names. Pictures that only need drawing at a new scale are
    /// drawn a few at a time, frame by frame (the old ones stretch meanwhile), so zooming never
    /// stops a frame for long. True when some are still waiting.
    pub fn prepare(&mut self, scale: f64, ms: u128) -> bool {
        let start = std::time::Instant::now();
        let mut waiting = false;
        // one picture at a new scale per frame at least, more while the frame has time for them
        let mut may_rescale = || {
            let ok = start.elapsed() < RESCALE_BUDGET;
            waiting |= !ok;
            ok
        };
        if self.pics_scale != scale {
            // small: always drawn sharp at once
            for (i, kind) in [Some(JackKind::Mono), Some(JackKind::Stereo), Some(JackKind::Midi), None].into_iter().enumerate() {
                let pm = rasterize(&self.opt, &face::jack_pic(kind), scale);
                self.jack_pics[i] = Some(Layer { id: next_id(), pm, x: -PIC, y: -PIC, w: 2.0 * PIC, h: 2.0 * PIC });
            }
            self.pics_scale = scale;
        }
        match self.rack_w {
            Some(rw) => {
                let mut h = DefaultHasher::new();
                (rw.to_bits(), self.rack_h.to_bits()).hash(&mut h);
                let sig = h.finish();
                let changed = self.rack_bg.as_ref().is_none_or(|(s, _)| *s != sig);
                if changed || (self.rack_scale != scale && may_rescale()) {
                    // tight around the stack (the last unit's gap is not part of it)
                    let (w, ht) = (rw + 2.0 * FRAME, (self.rack_h - RACK_GAP).max(0.0) + 2.0 * FRAME);
                    let pm = rasterize(&self.opt, &rack_svg(w, ht), scale);
                    self.rack_bg = Some((sig, Layer { id: next_id(), pm, x: -FRAME, y: -FRAME, w, h: ht }));
                    self.rack_scale = scale;
                }
            }
            None => self.rack_bg = None,
        }
        let links: HashMap<usize, usize> = {
            let mut m = HashMap::new();
            for c in self.cords.iter().filter(|c| c.live()) {
                for e in [&c.a, &c.b] {
                    if let Some(&(di, _)) = e.jack.as_ref().and_then(|k| self.jacks.get(k)) {
                        *m.entry(di).or_insert(0) += 1;
                    }
                }
            }
            m
        };
        for (di, d) in self.devs.iter_mut().enumerate() {
            let mut h = DefaultHasher::new();
            (&d.counts, &d.split, &d.dev.name, &d.dev.model, d.dev.state, self.rack_w.map(f64::to_bits)).hash(&mut h);
            d.dev.groups.iter().for_each(|g| g.mix.as_ref().map(|m| m.volumes.len()).hash(&mut h));
            d.defs.iter().for_each(|j| (&j.key, &j.label).hash(&mut h));
            let sig = h.finish();
            let changed = d.face.is_none() || sig != d.face_sig;
            if changed || (d.face_scale != scale && may_rescale()) {
                let (svg, geo) = face::face(&d.dev, &d.defs, &d.counts, &d.split, self.rack_w);
                d.geo = geo;
                let pm = rasterize(&self.opt, &svg, scale);
                d.face = Some(Layer { id: next_id(), pm, x: -SHADOW, y: -SHADOW, w: d.geo.w + 2.0 * SHADOW, h: DEV_H + 2.0 * SHADOW });
                (d.face_sig, d.face_scale) = (sig, scale);
                d.lcd = None;
                d.meter = None;
                d.mix_pics.clear();
                d.gmeters.clear();
            }
            // each group's knobs and MUTE LED, drawn again when they move
            let mut pics = Vec::new();
            for (gi, m) in &d.geo.mix {
                let Some((vol, bal, mute)) = d.mix(*gi) else { continue };
                let mut h = DefaultHasher::new();
                (vol.to_bits(), bal.to_bits(), mute, d.bal(*gi), scale.to_bits(), m.x.to_bits()).hash(&mut h);
                let sig = h.finish();
                match d.mix_pics.iter().position(|(g, s, _)| g == gi && *s == sig) {
                    Some(i) => pics.push(d.mix_pics.swap_remove(i)),
                    None => {
                        let pm = rasterize(&self.opt, &face::mix_pic(m, vol, bal, mute, d.bal(*gi)), scale);
                        pics.push((*gi, sig, Layer { id: next_id(), pm, x: m.x, y: MIX_Y, w: m.w, h: MIX_H }));
                    }
                }
            }
            d.mix_pics = pics;
            let wsig = d.geo.w.to_bits() ^ self.rack_w.is_some() as u64;
            if d.shade.as_ref().is_none_or(|(s, _)| *s != wsig) {
                // a blur: drawn at a small fixed scale and stretched, so zooming never redraws it
                let pm = rasterize(&self.opt, &face::shadow(d.geo.w, self.rack_w.is_some()), SHADE_SCALE);
                d.shade = Some((wsig, Layer { id: next_id(), pm, x: -SHADOW, y: -SHADOW, w: d.geo.w + 2.0 * SHADOW, h: DEV_H + 2.0 * SHADOW }));
            }
            let n = links.get(&di).copied().unwrap_or(0);
            let note = d.mix_note.as_ref().filter(|(until, _)| Instant::now() < *until).map(|(_, s)| s.clone());
            let line2 = if let Some(note) = note {
                note
            } else if d.dev.groups.is_empty() && d.dev.card.is_some() {
                "NO INPUTS OR OUTPUTS".to_owned()
            } else {
                format!(
                    "{:<11}{}",
                    d.dev.state.to_uppercase(),
                    match n {
                        0 => "NO LINKS".to_owned(),
                        1 => "1 LINK".to_owned(),
                        n => format!("{n} LINKS"),
                    }
                )
            };
            // line 3: a card's profile (and codec), scrolling when long; else what it carries
            let line3 = match &d.dev.card {
                Some(c) => face::marquee(&c.summary(), ms),
                None if d.dev.groups.iter().any(|g| g.ports.iter().any(|p| p.media == Media::Midi)) => "MIDI".to_owned(),
                None if d.dev.groups.iter().any(|g| g.stereo) => "STEREO".to_owned(),
                None => "MONO".to_owned(),
            };
            let text = format!("{}\n{line2}\n{line3}", face::marquee(&d.dev.name, ms));
            if d.lcd.as_ref().is_none_or(|(t, _)| *t != text) {
                let l: Vec<&str> = text.split('\n').collect();
                let card = d.dev.card.is_some();
                let pm = rasterize(&self.opt, &face::lcd([l[0], l[1], l[2]], card), scale);
                let x = d.geo.left + LCD_X;
                d.lcd = Some((text, Layer { id: next_id(), pm, x, y: LCD_Y, w: LCD_W, h: face::lcd_h(card) }));
            }
            let lit = (d.level[0].round() as u8, d.level[1].round() as u8);
            if lit == (0, 0) || !d.geo.sig {
                d.meter = None;
            } else if d.meter.as_ref().is_none_or(|(l, _)| *l != lit) {
                d.meter = Some((lit, meter_layer(lit, Leds::sig(d.geo.meter), scale)));
            }
            // a group's own meter
            let mut meters = Vec::new();
            for (gi, m) in &d.geo.mix {
                let (Some(x), Some(lv)) = (m.meter, d.glevel.get(gi)) else { continue };
                let lit = (lv[0].round() as u8, lv[1].round() as u8);
                if lit == (0, 0) {
                    continue;
                }
                match d.gmeters.iter().position(|(g, l, _)| g == gi && *l == lit) {
                    Some(i) => meters.push(d.gmeters.swap_remove(i)),
                    None => meters.push((*gi, lit, meter_layer(lit, Leds::strip(x), scale))),
                }
            }
            d.gmeters = meters;
        }
        waiting
    }

    /// The cords in the window area (x, y, w, h) in pixels, or None when there are none.
    /// `rings`: jacks the end in the hand may go into (world position, the one it is pulled to).
    pub fn cord_layer(&self, view: View, win: (u32, u32), rings: &[((f32, f32), bool)], pulse: f32) -> Option<(Pixmap, i32, i32)> {
        if self.cords.is_empty() {
            return None;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for c in &self.cords {
            // its ends are its first and last points
            for p in &c.pts {
                (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
            }
        }
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return None;
        }
        // room for the cords' width and their shadows (12 down, 13 wide)
        let m = 40.0;
        let s = view.s as f32;
        let px0 = (((x0 - m) * s) as f64 + view.x).floor().max(0.0) as i32;
        let py0 = (((y0 - m) * s) as f64 + view.y).floor().max(0.0) as i32;
        let px1 = (((x1 + m) * s) as f64 + view.x).ceil().min(win.0 as f64) as i32;
        let py1 = (((y1 + m) * s) as f64 + view.y).ceil().min(win.1 as f64) as i32;
        // while a cord's end is in the hand, the target rings can be anywhere: use the whole window
        let (px0, py0, px1, py1) = if rings.is_empty() { (px0, py0, px1, py1) } else { (0, 0, win.0 as i32, win.1 as i32) };
        if px1 <= px0 || py1 <= py0 {
            return None;
        }
        let mut pm = Pixmap::new((px1 - px0) as u32, (py1 - py0) as u32)?;
        let ts = Transform::from_row(s, 0.0, 0.0, s, view.x as f32 - px0 as f32, view.y as f32 - py0 as f32);
        paint::rings(&mut pm, rings, pulse, ts);
        // faded or hidden cords; a cord in the hand always shows, and so do the hovered thing's
        // cords, while the others fade
        let fade = [1.0, 0.22, 0.0][self.cables.min(2) as usize];
        paint::cords(
            &mut pm,
            &self.cords,
            |c| {
                let a = if c.a.held || c.b.held || c.grab.is_some() { c.alpha } else { c.alpha * fade };
                if self.lit == 0.0 {
                    a
                } else if self.lights(c) {
                    a + (c.alpha - a) * self.lit
                } else {
                    a * (1.0 - 0.75 * self.lit)
                }
            },
            ts,
        );
        Some((pm, px0, py0))
    }

    // ---------------- patching ----------------

    /// The seated plug under a world point: (cord, its input end?).
    pub fn plug_at(&self, x: f64, y: f64) -> Option<(usize, bool)> {
        let mut best = None;
        let mut bd = 15.0f32;
        for (i, c) in self.cords.iter().enumerate() {
            if c.dying.is_some() {
                continue;
            }
            for (b, e) in [(false, &c.a), (true, &c.b)] {
                let d = ((e.x - x as f32).powi(2) + (e.y - y as f32).powi(2)).sqrt();
                if e.seated() && d < bd {
                    (bd, best) = (d, Some((i, b)));
                }
            }
        }
        best
    }

    /// The jack whose free slot is under a world point.
    pub fn free_jack_at(&self, x: f64, y: f64) -> Option<String> {
        self.jacks.iter().find_map(|(key, &(di, ji))| {
            let (sx, sy) = self.slot_pos(key, self.devs[di].counts[ji])?;
            (((sx as f64 - x).powi(2) + (sy as f64 - y).powi(2)).sqrt() < 16.0).then(|| key.clone())
        })
    }

    /// The jacks to lay over device `di`'s face: picture (in `jack_pics`), the world position of
    /// its centre, and opacity. A mult's free jack is faded, as is everything a plug in the hand
    /// (`targets`: the jacks it fits) can't go into; the free jacks it can go into are solid.
    pub fn jack_sprites(&self, di: usize, targets: Option<&[String]>) -> Vec<(usize, f64, f64, f32)> {
        let d = &self.devs[di];
        let mut v = Vec::new();
        for (ji, j) in d.defs.iter().enumerate() {
            let (Some(&(jx, jy)), Some(&n)) = (d.geo.jacks.get(ji), d.counts.get(ji)) else { continue };
            // the L and R of a split stereo jack keep its gold ring: still the same pair
            let stereo = d.dev.groups.get(j.group).is_some_and(|g| g.stereo);
            let pic = match j.kind {
                JackKind::Mono if stereo => 1,
                JackKind::Mono => 0,
                JackKind::Stereo => 1,
                JackKind::Midi => 2,
            };
            let k = face::slots(n);
            for s in 0..k {
                let (x, y) = (d.x + jx + s as f64 * JP, d.y + jy);
                let spare = k > 1 && s == k - 1;
                let a = match targets {
                    None if spare => 0.4,
                    None => 1.0,
                    Some(t) if s == n && t.contains(&j.key) => 1.0,
                    Some(_) => 0.4,
                };
                v.push((pic, x, y, a));
                // its badge; while a plug is in the hand a free jack it fits glows instead
                let badge = match targets {
                    None => 1.0,
                    Some(_) if a == 1.0 => 0.0,
                    Some(_) => a,
                };
                if spare && badge > 0.0 {
                    v.push((3, x + BADGE.0, y + BADGE.1, badge));
                }
            }
        }
        v
    }

    /// Where the free slot of a jack is.
    pub fn spare(&self, key: &str) -> Option<(f32, f32)> {
        let &(di, ji) = self.jacks.get(key)?;
        self.slot_pos(key, self.devs[di].counts[ji])
    }

    pub fn jack_kind(&self, key: &str) -> Option<JackKind> {
        self.jack(key).map(|(_, j)| j.kind)
    }

    /// The control of an app stream under a world point: (device, group).
    fn mix_hit(&self, x: f64, y: f64, hit: impl Fn(&face::MixGeo, f64, f64) -> bool) -> Option<(usize, usize)> {
        let di = self.device_at(x, y)?;
        let d = &self.devs[di];
        d.geo.mix.iter().find(|(_, m)| hit(m, x - d.x, y - d.y)).map(|(gi, _)| (di, *gi))
    }

    /// The VOLUME (or GAIN) knob under a world point: (device, group).
    pub fn knob_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.mix_hit(x, y, |m, x, y| (x - m.knob.0).hypot(y - m.knob.1) <= KNOB_R)
    }

    /// The BAL knob under a world point.
    pub fn bal_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.mix_hit(x, y, |m, x, y| m.bal.is_some_and(|(bx, by)| (x - bx).hypot(y - by) <= BAL_R))
    }

    /// The MUTE key under a world point.
    pub fn mute_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.mix_hit(x, y, |m, x, y| {
            let [kx, ky, kw, kh] = m.mute;
            x >= kx && x <= kx + kw && y >= ky && y <= ky + kh
        })
    }

    /// Any of an app stream's controls under a world point.
    pub fn mix_control_at(&self, x: f64, y: f64) -> bool {
        self.knob_at(x, y).or(self.bal_at(x, y)).or(self.mute_at(x, y)).is_some()
    }

    /// Sets an app stream's volume, balance and mute (PipeWire is told; the LCD says what
    /// changed, and which stream on a device with two: IN or OUT).
    fn set_mix(&mut self, (di, gi): (usize, usize), volume: f64, balance: f64, mute: bool) {
        let Some(d) = self.devs.get_mut(di) else { return };
        let Some(g) = d.dev.groups.get(gi) else { return };
        let Some(m) = &g.mix else { return };
        let input = g.dir == Dir::In;
        // only a stereo pair has a balance
        let balance = if g.stereo { balance } else { 0.0 };
        let was = d.mix(gi);
        self.outbox.push(Cmd::SetMix { node: m.node, volumes: m.volumes_for(volume, balance), mute });
        d.mix_local.insert(gi, (Instant::now(), (volume, balance, mute)));
        let mut note = if was.is_some_and(|(_, b, _)| (b - balance).abs() > 0.001) { bal_text(volume, balance) } else { vol_text(volume, mute, input) };
        if face::own_meters(&d.dev) {
            note = format!("{} {note}", if input { "IN" } else { "OUT" });
        }
        d.mix_note = Some((Instant::now() + MIX_NOTE, note));
        self.edited = true;
    }

    pub fn set_volume(&mut self, at: (usize, usize), volume: f64) {
        let Some((_, bal, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        self.set_mix(at, (volume.clamp(0.0, VOL_MAX) * 100.0).round() / 100.0, bal, mute);
    }

    pub fn set_balance(&mut self, at: (usize, usize), balance: f64) {
        let Some((vol, _, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        self.set_mix(at, vol, (balance.clamp(-1.0, 1.0) * 100.0).round() / 100.0, mute);
    }

    pub fn toggle_mute(&mut self, at: (usize, usize)) {
        let Some((vol, bal, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        self.set_mix(at, vol, bal, !mute);
    }

    /// The LCD still shows a volume just changed: it has to be drawn again when that ends.
    pub fn noting(&self) -> bool {
        self.devs.iter().any(|d| d.mix_note.as_ref().is_some_and(|(until, _)| Instant::now() < *until + Duration::from_millis(50)))
    }

    /// The SPLIT/JOIN switch under a world point: (device, group).
    /// The device whose PROFILE key is under a world point.
    pub fn profile_key_at(&self, x: f64, y: f64) -> Option<usize> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| d.geo.profile.filter(|[kx, ky, kw, kh]| x >= d.x + kx && x <= d.x + kx + kw && y >= d.y + ky && y <= d.y + ky + kh).map(|_| di))
    }

    pub fn switch_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| {
            d.geo.switches.iter().find_map(|(g, [bx, by, bw, bh])| (x >= d.x + bx && x <= d.x + bx + bw && y >= d.y + by && y <= d.y + by + bh).then_some((di, *g)))
        })
    }

    /// One stereo jack ⇄ separate L and R jacks. The cords re-plug themselves to match.
    /// The stereo jacks its cords lead to follow, and the ones theirs lead to, and so on: else a
    /// stereo jack would show two cords that each carry only L or R and look like they carry both.
    pub fn toggle_split(&mut self, di: usize, g: usize) {
        let Some(&was) = self.devs[di].split.get(g) else { return };
        let group_of = self.stereo_groups();
        // links to hidden streams (Patchcord's own meters on app outputs, say) don't count: their
        // ports aren't in `group_of`
        let links = self.graph.links();
        let mut flip = vec![(di, g)];
        let mut k = 0;
        while k < flip.len() {
            let here = flip[k];
            for l in &links {
                let (a, b) = (group_of.get(&l.out_port), group_of.get(&l.in_port));
                let far = if a == Some(&here) { b } else if b == Some(&here) { a } else { None };
                if let Some(&f) = far
                    && !flip.contains(&f)
                {
                    flip.push(f);
                }
            }
            k += 1;
        }
        for (dj, gj) in flip {
            if let Some(v) = self.devs[dj].split.get_mut(gj) {
                *v = !was;
            }
        }
        self.index();
        self.reconcile();
        self.slots();
    }

    /// Plugs a cord between two jacks in the picture only, without linking them in PipeWire:
    /// for snapshots of a patch the graph doesn't have (`--render ... demo`).
    pub fn show_cord(&mut self, out: &str, inp: &str) {
        let (Some(pa), Some(pb)) = (self.spare(out), self.spare(inp)) else { return };
        let (ka, kb) = (self.jack(out).map(|j| j.1.kind).unwrap(), self.jack(inp).map(|j| j.1.kind).unwrap());
        let color = self.jack(out).map(|j| j.0.dev.color).unwrap();
        self.seq += 1;
        self.slot_seq += 2;
        let c = Cord::new(self.seq, (self.slot_seq - 1, self.slot_seq), (out.to_owned(), ka, pa), (inp.to_owned(), kb, pb), color);
        self.cords.push(c);
        self.slots();
    }

    /// Shows only the devices whose names start with one of `names`, and the cords between them
    /// (for pictures).
    pub fn keep_only(&mut self, names: &[&str]) {
        self.devs.retain(|d| names.iter().any(|n| d.dev.name.starts_with(n)));
        self.index();
        self.cords.retain(|c| [&c.a, &c.b].iter().all(|e| e.jack.as_ref().is_some_and(|k| self.jacks.contains_key(k))));
        self.slots();
    }

    /// The first output jack, for snapshots that show a plug in the hand.
    pub fn first_output(&self) -> Option<String> {
        self.devs.iter().flat_map(|d| d.defs.iter()).find(|j| j.dir == Dir::Out).map(|j| j.key.clone())
    }

    /// Pulls a new cord out of a jack's free slot, into the hand. Returns the cord's number.
    pub fn pull_new(&mut self, key: &str) -> Option<u64> {
        let at = self.spare(key)?;
        let (d, j) = self.jack(key)?;
        let output = j.dir == Dir::Out;
        let color = if output { d.dev.color } else { [0x8d, 0x90, 0x94] };
        let kind = j.kind;
        self.seq += 1;
        self.slot_seq += 1;
        self.cords.push(Cord::fresh(self.seq, self.slot_seq, key.to_owned(), kind, at, color, output));
        Some(self.seq)
    }

    /// Takes a seated plug into the hand. Returns the cord's number.
    pub fn unplug(&mut self, ci: usize, b: bool) -> u64 {
        let c = &mut self.cords[ci];
        let e = if b { &mut c.b } else { &mut c.a };
        e.orig = e.jack.take();
        e.held = true;
        c.seq
    }

    fn cord_index(&self, seq: u64) -> Option<usize> {
        self.cords.iter().position(|c| c.seq == seq)
    }

    /// Jacks the plug in the hand fits: inputs for the input end, outputs for the output end,
    /// of the same kind of signal as the jack at the cord's other end, and not patched to that
    /// jack already (it takes no second cord; see `drop_held`). The jack it was pulled out of
    /// always fits.
    pub fn targets(&self, seq: u64) -> Vec<String> {
        let Some(c) = self.cord_index(seq).map(|i| &self.cords[i]) else { return Vec::new() };
        let (dir, other, orig) = if c.b.held { (Dir::In, c.a.jack.as_ref(), c.b.orig.as_ref()) } else { (Dir::Out, c.b.jack.as_ref(), c.a.orig.as_ref()) };
        let media = other.and_then(|k| self.jack(k)).map_or(Media::Audio, |(_, j)| j.media);
        let free = |k: &String| {
            Some(k) == orig
                || other.is_none_or(|o| {
                    let (x, y) = if dir == Dir::In { (o, k) } else { (k, o) };
                    !self.patched(x, y)
                })
        };
        self.jacks.keys().filter(|k| self.jack(k).is_some_and(|(_, j)| j.dir == dir && j.media == media) && free(k)).cloned().collect()
    }

    /// Whether a cord is plugged in from jack `out` to jack `inp`.
    fn patched(&self, out: &str, inp: &str) -> bool {
        self.cords.iter().any(|c| c.live() && c.a.jack.as_deref() == Some(out) && c.b.jack.as_deref() == Some(inp))
    }

    /// Port pairs one cord between two jacks stands for: L→L and R→R between stereo jacks,
    /// every combination when one side is mono (mono to both sides, or both sides summed).
    fn pairs(&self, out: &str, inp: &str) -> Vec<(u32, u32)> {
        let (Some((_, a)), Some((_, b))) = (self.jack(out), self.jack(inp)) else { return Vec::new() };
        if a.ports.len() == b.ports.len() {
            a.ports.iter().copied().zip(b.ports.iter().copied()).collect()
        } else {
            a.ports.iter().flat_map(|&o| b.ports.iter().map(move |&i| (o, i))).collect()
        }
    }

    /// The PipeWire links between two jacks now.
    fn links_between(&self, out: &str, inp: &str) -> Vec<(u32, u32)> {
        let (Some((_, a)), Some((_, b))) = (self.jack(out), self.jack(inp)) else { return Vec::new() };
        self.graph.links().into_iter().filter(|l| a.ports.contains(&l.out_port) && b.ports.contains(&l.in_port)).map(|l| (l.out_port, l.in_port)).collect()
    }

    fn link(&mut self, add: &[(u32, u32)], del: &[(u32, u32)]) {
        let links = self.graph.links();
        for &(o, i) in del {
            if let Some(l) = links.iter().find(|l| l.out_port == o && l.in_port == i) {
                self.outbox.push(Cmd::Unlink(l.id));
            }
        }
        for &(o, i) in add {
            if links.iter().any(|l| l.out_port == o && l.in_port == i) {
                continue;
            }
            if let (Some(on), Some(inn)) = (self.graph.port_node(o), self.graph.port_node(i)) {
                self.outbox.push(Cmd::Link { out_node: on, out_port: o, in_node: inn, in_port: i });
            }
        }
    }

    fn record(&mut self, add: Vec<(u32, u32)>, del: Vec<(u32, u32)>) {
        if !add.is_empty() || !del.is_empty() {
            self.undo.push((add, del));
            self.redo.clear();
            self.edited = true;
        }
    }

    /// Where every device sits on the desk, by device key.
    pub fn positions(&self) -> Vec<(String, (f64, f64))> {
        self.devs.iter().map(|d| (d.dev.key.clone(), (d.dx, d.dy))).collect()
    }

    /// The patch as it is now, as a preset.
    pub fn snapshot(&self, name: &str) -> Preset {
        let mut links: Vec<_> = self.graph.links().iter().filter_map(|l| Some((self.graph.port_name(l.out_port)?, self.graph.port_name(l.in_port)?))).collect();
        links.sort();
        let split = self
            .devs
            .iter()
            .flat_map(|d| {
                d.dev.groups.iter().zip(&d.split).filter(|(_, s)| **s).map(|(g, _)| (d.dev.key.clone(), g.label.clone(), if g.dir == Dir::In { "in" } else { "out" }.to_owned()))
            })
            .collect();
        // what the faces set: the apps' volumes, the cards' profiles
        let mix = self
            .devs
            .iter()
            .flat_map(|d| d.dev.groups.iter().enumerate().filter_map(move |(gi, g)| Some(((d.dev.key.clone(), g.label.clone(), if g.dir == Dir::In { "in" } else { "out" }.to_owned()), d.mix(gi)?))))
            .collect();
        let profiles = self.devs.iter().filter_map(|d| Some((d.dev.key.clone(), d.dev.card.as_ref()?.profile()?.description.clone()))).collect();
        Preset { name: name.to_owned(), nodes: self.graph.node_names(), links, split, pos: self.positions().into_iter().collect(), mix, profiles }
    }

    /// Puts a preset in place: split switches, device places and links, as one undo step. Links
    /// between devices the preset knows that it doesn't have are taken away; devices it doesn't
    /// know (started later) keep theirs. Returns how many links it has, and how many it couldn't
    /// make because a device isn't here.
    pub fn apply_preset(&mut self, p: &Preset) -> (usize, usize) {
        for d in &mut self.devs {
            for (gi, g) in d.dev.groups.iter().enumerate() {
                let dir = if g.dir == Dir::In { "in" } else { "out" };
                if let Some(s) = d.split.get_mut(gi) {
                    *s = p.split.iter().any(|(k, l, di)| *k == d.dev.key && *l == g.label && di == dir);
                }
            }
            if let Some(&(x, y)) = p.pos.get(&d.dev.key) {
                (d.dx, d.dy) = (x, y);
                if !self.rack {
                    (d.tx, d.ty) = (x, y);
                }
            }
        }
        for (k, xy) in &p.pos {
            self.saved_pos.insert(k.clone(), *xy);
        }
        // cards to their profiles (by name: the numbers can change), apps to their volumes
        for d in &self.devs {
            if let (Some(c), Some(name)) = (&d.dev.card, p.profiles.get(&d.dev.key))
                && let Some(pr) = c.profiles.iter().find(|pr| pr.description == *name)
                && Some(pr.index) != c.current
            {
                self.outbox.push(Cmd::SetProfile { device: c.id, index: pr.index });
            }
        }
        for di in 0..self.devs.len() {
            for gi in 0..self.devs[di].dev.groups.len() {
                let d = &self.devs[di];
                let g = &d.dev.groups[gi];
                let key = (d.dev.key.clone(), g.label.clone(), if g.dir == Dir::In { "in" } else { "out" }.to_owned());
                // only what differs (the LCD says what changed)
                if let Some(&(vol, bal, mute)) = p.mix.get(&key)
                    && d.mix(gi).is_some_and(|(v, b, m)| (v - vol).abs() > 0.005 || (b - bal).abs() > 0.005 || m != mute)
                {
                    self.set_mix((di, gi), vol.clamp(0.0, VOL_MAX), bal.clamp(-1.0, 1.0), mute);
                }
            }
        }
        let mut want = Vec::new();
        let mut missing = 0;
        for ((on, op), (inn, ip)) in &p.links {
            match (self.graph.find_port(on, op), self.graph.find_port(inn, ip)) {
                (Some(o), Some(i)) => want.push((o, i)),
                _ => missing += 1,
            }
        }
        let known = |port: u32| self.graph.port_name(port).is_some_and(|(n, _)| p.nodes.contains(&n));
        let now: Vec<(u32, u32)> = self.graph.links().iter().map(|l| (l.out_port, l.in_port)).collect();
        let del: Vec<(u32, u32)> = now.iter().copied().filter(|l| !want.contains(l) && known(l.0) && known(l.1)).collect();
        let add: Vec<(u32, u32)> = want.iter().copied().filter(|l| !now.contains(l)).collect();
        self.link(&add, &del);
        self.record(add, del);
        self.edited = false;
        self.index();
        self.reconcile();
        self.slots();
        (want.len(), missing)
    }

    /// Lets go of the cord's end in the hand: into `target` when given and free to take it, else
    /// it falls out. True when it went in.
    pub fn drop_held(&mut self, seq: u64, target: Option<String>) -> bool {
        let Some(ci) = self.cord_index(seq) else { return false };
        let b = self.cords[ci].b.held;
        let (orig, other) = {
            let c = &self.cords[ci];
            if b { (c.b.orig.clone(), c.a.jack.clone()) } else { (c.a.orig.clone(), c.b.jack.clone()) }
        };
        let pair = |held: &str, other: &str| if b { (other.to_owned(), held.to_owned()) } else { (held.to_owned(), other.to_owned()) };
        // a jack pair already patched takes no second cord
        let target = target.filter(|t| {
            Some(t) == orig.as_ref()
                || other.as_ref().is_none_or(|o| {
                    let (x, y) = pair(t, o);
                    !self.patched(&x, &y)
                })
        });
        let (mut add, mut del) = (Vec::new(), Vec::new());
        if let (Some(o), Some(other)) = (&orig, &other)
            && target.as_ref() != Some(o)
        {
            let (x, y) = pair(o, other);
            del = self.links_between(&x, &y);
            self.suppress.push(((x, y), 2.0));
        }
        if let (Some(t), Some(other)) = (&target, &other)
            && Some(t) != orig.as_ref()
        {
            let (x, y) = pair(t, other);
            add = self.pairs(&x, &y);
        }
        self.link(&add, &del);
        let color = target.as_ref().filter(|_| !b).and_then(|t| self.jack(t)).map(|(d, _)| d.dev.color);
        self.slot_seq += 1;
        let slot = self.slot_seq;
        let c = &mut self.cords[ci];
        let e = if b { &mut c.b } else { &mut c.a };
        e.held = false;
        let went_in = target.is_some();
        match target {
            Some(t) => {
                if Some(&t) != e.orig.as_ref() {
                    e.slot = slot;
                }
                e.jack = Some(t);
                e.orig = None;
                if let Some(col) = color {
                    c.color = col;
                }
                if !add.is_empty() {
                    c.pending = 2.0;
                }
            }
            None => {
                e.orig = None;
                c.kill(b);
            }
        }
        self.record(add, del);
        self.slots();
        went_in
    }

    /// Pulls a cord out (double-click): its links go, the input end drops.
    pub fn pull_out(&mut self, ci: usize, b: bool) {
        let c = &self.cords[ci];
        if !c.live() {
            return;
        }
        let (x, y) = (c.a.jack.clone().unwrap(), c.b.jack.clone().unwrap());
        let del = self.links_between(&x, &y);
        self.link(&[], &del);
        self.suppress.push(((x, y), 2.0));
        self.cords[ci].kill(b);
        self.record(Vec::new(), del);
    }

    /// Undoes the last link change; true when there was one.
    pub fn undo(&mut self) -> bool {
        let Some((add, del)) = self.undo.pop() else { return false };
        self.link(&del, &add);
        self.redo.push((add, del));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some((add, del)) = self.redo.pop() else { return false };
        self.link(&add, &del);
        self.undo.push((add, del));
        true
    }

    /// The device under a world point, topmost first.
    pub fn device_at(&self, x: f64, y: f64) -> Option<usize> {
        (0..self.devs.len()).rev().find(|&i| {
            let d = &self.devs[i];
            x >= d.x && x <= d.x + d.geo.w && y >= d.y && y <= d.y + DEV_H
        })
    }

    /// The cord under a world point (within a few units), and its nearest point.
    pub fn cord_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        (0..self.cords.len()).rev().find_map(|i| {
            let c = &self.cords[i];
            if c.dying.is_some() {
                return None;
            }
            c.hit(x as f32, y as f32).filter(|(d, _)| *d < 7.0).map(|(_, k)| (i, k))
        })
    }

    /// What is under a world point: a plug or a jack (the jack, either way), a cord, a device.
    pub fn hover_at(&self, x: f64, y: f64) -> Hover {
        if let Some((ci, b)) = self.plug_at(x, y) {
            let c = &self.cords[ci];
            if let Some(k) = if b { &c.b.jack } else { &c.a.jack } {
                return Hover::Jack(k.clone());
            }
        }
        if let Some(k) = self.jack_at(x, y) {
            return Hover::Jack(k);
        }
        if self.cables < 2
            && let Some((ci, _)) = self.cord_at(x, y)
        {
            return Hover::Cord(self.cords[ci].seq);
        }
        match self.device_at(x, y) {
            Some(i) => Hover::Device(self.devs[i].dev.key.clone()),
            None => Hover::None,
        }
    }

    /// Any jack of a mult (taken or free) under a world point, on the topmost device there.
    fn jack_at(&self, x: f64, y: f64) -> Option<String> {
        let di = self.device_at(x, y)?;
        let d = &self.devs[di];
        d.defs.iter().enumerate().find_map(|(ji, j)| {
            let n = d.counts.get(ji).copied().unwrap_or(0);
            (0..face::slots(n)).find_map(|s| {
                let (sx, sy) = self.slot_pos(&j.key, s)?;
                (((sx as f64 - x).powi(2) + (sy as f64 - y).powi(2)).sqrt() < 16.0).then(|| j.key.clone())
            })
        })
    }

    /// Whether a cord belongs to what was hovered last.
    fn lights(&self, c: &Cord) -> bool {
        self.lights_for(c, &self.lit_for)
    }

    /// Whether a cord belongs to `h`: its own cord, or a cord of its jack or device.
    fn lights_for(&self, c: &Cord, h: &Hover) -> bool {
        // a plug in the hand still counts for the jack it came from
        let mut ends = [&c.a, &c.b].into_iter().filter_map(|e| e.jack.as_ref().or(e.orig.as_ref()));
        match h {
            Hover::None => false,
            Hover::Cord(seq) => c.seq == *seq,
            Hover::Jack(key) => ends.any(|k| k == key),
            Hover::Device(key) => ends.any(|k| self.jack(k).is_some_and(|(d, _)| d.dev.key == *key)),
        }
    }

    /// The tooltip for what is hovered: lines of text, the first one a title, or none.
    pub fn tooltip(&self, h: &Hover) -> Vec<String> {
        let ports = |key: &str| -> Option<(String, String, String)> {
            let (d, j) = self.jack(key)?;
            let names: Vec<&str> = j.ports.iter().filter_map(|id| d.dev.groups.iter().flat_map(|g| &g.ports).find(|p| p.id == *id)).map(|p| p.name.as_str()).collect();
            let group = d.dev.groups.get(j.group).map_or(String::new(), |g| g.label.clone());
            Some((d.dev.name.clone(), group, names.join(" + ")))
        };
        match h {
            Hover::None => Vec::new(),
            Hover::Jack(key) => {
                let Some((dev, group, names)) = ports(key) else { return Vec::new() };
                let mut v = vec![dev, format!("{group} · {names}")];
                let out = self.jack(key).is_some_and(|(_, j)| j.dir == Dir::Out);
                for c in self.cords.iter().filter(|c| c.live()) {
                    let (a, b) = (c.a.jack.as_deref().unwrap_or(""), c.b.jack.as_deref().unwrap_or(""));
                    let other = if a == key { b } else if b == key { a } else { continue };
                    if let Some((od, _, on)) = ports(other) {
                        v.push(format!("{} {od}: {on}", if out { "to" } else { "from" }));
                    }
                }
                v
            }
            Hover::Cord(seq) => {
                let Some(c) = self.cords.iter().find(|c| c.seq == *seq) else { return Vec::new() };
                let (Some((ad, _, an)), Some((bd, _, bn))) = (c.a.jack.as_deref().and_then(&ports), c.b.jack.as_deref().and_then(&ports)) else { return Vec::new() };
                vec![format!("{ad} to {bd}"), format!("from {an}"), format!("to {bn}")]
            }
            Hover::Device(key) => {
                let Some(d) = self.devs.iter().find(|d| d.dev.key == *key) else { return Vec::new() };
                vec![d.dev.name.clone(), format!("{} · {}", d.dev.model, d.dev.state)]
            }
        }
    }

    /// The PipeWire names of a jack's ports: "Firefox: output_FL + output_FR".
    fn jack_name(&self, key: &str) -> String {
        let Some((d, j)) = self.jack(key) else { return "?".into() };
        let names: Vec<&str> = j.ports.iter().filter_map(|id| d.dev.groups.iter().flat_map(|g| &g.ports).find(|p| p.id == *id)).map(|p| p.name.as_str()).collect();
        format!("{}: {}", d.dev.name, names.join(" + "))
    }

    /// What is under a world point, for the status bar.
    pub fn hint_at(&self, x: f64, y: f64, locked: bool) -> String {
        let cord = |c: &Cord| match (&c.a.jack, &c.b.jack) {
            (Some(a), Some(b)) => format!("{} → {}", self.jack_name(a), self.jack_name(b)),
            _ => String::new(),
        };
        if let Some((ci, _)) = self.plug_at(x, y) {
            return format!("{}  ·  drag: move or pull out, double-click: pull out", cord(&self.cords[ci]));
        }
        if let Some(k) = self.free_jack_at(x, y) {
            return format!("{}  ·  drag: pull out a cord", self.jack_name(&k));
        }
        if let Some((di, gi)) = self.knob_at(x, y)
            && let Some((v, _, mute)) = self.devs[di].mix(gi)
        {
            let g = &self.devs[di].dev.groups[gi];
            let input = g.dir == Dir::In;
            return format!("{} · {} · {}  ·  drag up/down: {}, Shift: fine · wheel: 5% (Shift: 1%) · double-click: 100%", self.devs[di].dev.name, g.label, vol_text(v, mute, input), if input { "gain" } else { "volume" });
        }
        if let Some((di, gi)) = self.bal_at(x, y)
            && let Some((v, b, _)) = self.devs[di].mix(gi)
        {
            let d = &self.devs[di];
            let label = &d.dev.groups[gi].label;
            return if d.bal(gi) == BalKnob::Out {
                format!("{} · {label} · {}  ·  drag up/down: balance (up is right), Shift: fine · wheel · click: push it in", d.dev.name, bal_text(v, b))
            } else {
                format!("{} · {label} · {}  ·  click: pop the knob out to turn it", d.dev.name, bal_text(v, b))
            };
        }
        if let Some((di, gi)) = self.mute_at(x, y)
            && let Some((_, _, mute)) = self.devs[di].mix(gi)
        {
            return format!("{} · {}  ·  click: {}", self.devs[di].dev.name, self.devs[di].dev.groups[gi].label, if mute { "unmute" } else { "mute" });
        }
        if let Some(di) = self.profile_key_at(x, y) {
            let card = self.devs[di].dev.card.as_ref().map_or(String::new(), |c| c.summary());
            return format!("{card}  ·  click: choose a profile");
        }
        if let Some((di, g)) = self.switch_at(x, y) {
            return if self.devs[di].split.get(g).copied().unwrap_or(false) { "Join L and R into one stereo jack".into() } else { "Split into separate L and R jacks".into() };
        }
        if self.cables < 2
            && let Some((ci, _)) = self.cord_at(x, y)
        {
            return format!("{}  ·  drag: swing, double-click: pull out", cord(&self.cords[ci]));
        }
        if let Some(i) = self.device_at(x, y) {
            let d = &self.devs[i].dev;
            return format!("{} · {}{}", d.name, d.model, if locked { "" } else { "  ·  drag: move" });
        }
        String::new()
    }

    /// Puts a device on top of the others.
    pub fn raise(&mut self, i: usize) -> usize {
        let d = self.devs.remove(i);
        self.devs.push(d);
        self.index_only();
        self.devs.len() - 1
    }

    /// Re-index jacks after reordering devices (keys stay, positions in the list change).
    fn index_only(&mut self) {
        self.jacks.clear();
        for (di, d) in self.devs.iter().enumerate() {
            for (ji, j) in d.defs.iter().enumerate() {
                self.jacks.insert(j.key.clone(), (di, ji));
            }
        }
    }
}

/// The hover highlight one frame on, towards `to`: up quickly, down a little slower, and still
/// once there (it used to step down from 1 and back up every frame, and the cords blinked).
fn ease_lit(lit: f32, to: f32, dt: f32) -> f32 {
    if lit < to { (lit + dt * 8.0).min(to) } else { (lit - dt * 5.0).max(to) }
}

/// The rack: just a thin metal frame hugging the stack of devices, dark inside (between the
/// units).
fn rack_svg(w: f64, h: f64) -> String {
    let i = FRAME - 2.0;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="steel" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#a6a9ad"/><stop offset=".5" stop-color="#8a8d91"/><stop offset="1" stop-color="#6e7175"/></linearGradient></defs><rect x=".5" y=".5" width="{}" height="{}" rx="4" fill="url(#steel)" stroke="#000"/><rect x="{i}" y="{i}" width="{}" height="{}" rx="2" fill="#0c0d0e" stroke="#2a2b2e"/></svg>"##,
        w - 1.0,
        h - 1.0,
        w - 2.0 * i,
        h - 2.0 * i
    )
}

/// The lit LEDs of a signal meter (the dark ones are part of the face), with a little glow.
fn meter_layer(lit: (u8, u8), leds: Leds, scale: f64) -> Layer {
    let (x0, y0) = (leds.x - 4.0, leds.y - 9.0 * leds.step - 4.0);
    let (w, h) = (leds.col + leds.w + 8.0, 9.0 * leds.step + leds.h + 8.0);
    let s = scale as f32;
    let mut pm = Pixmap::new((w as f32 * s).ceil() as u32, (h as f32 * s).ceil() as u32);
    if let Some(pm) = &mut pm {
        let ts = Transform::from_row(s, 0.0, 0.0, s, -(x0 as f32) * s, -(y0 as f32) * s);
        for (c, n) in [lit.0, lit.1].into_iter().enumerate() {
            for i in 0..n.min(10) {
                let rgb: [u8; 3] = if i >= 9 {
                    [0xff, 0x3b, 0x2f]
                } else if i >= 7 {
                    [0xff, 0xb0, 0x2e]
                } else {
                    [0x4c, 0xff, 0x6a]
                };
                let (x, y) = ((leds.x + c as f64 * leds.col) as f32, (leds.y - i as f64 * leds.step) as f32);
                let (lw, lh) = (leds.w as f32, leds.h as f32);
                for (grow, a) in [(2.0, 50u8), (0.0, 255u8)] {
                    if let Some(r) = Rect::from_xywh(x - grow, y - grow, lw + 2.0 * grow, lh + 2.0 * grow) {
                        let mut p = Paint { anti_alias: true, ..Default::default() };
                        p.set_color_rgba8(rgb[0], rgb[1], rgb[2], a);
                        let path = PathBuilder::from_rect(r);
                        pm.fill_path(&path, &p, FillRule::Winding, ts, None);
                    }
                }
            }
        }
    }
    Layer { id: next_id(), pm, x: x0, y: y0, w, h }
}

/// Rasterizes an SVG document at `scale`.
pub fn rasterize(opt: &usvg::Options, doc: &str, scale: f64) -> Option<Pixmap> {
    let tree = match usvg::Tree::from_str(doc, opt) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("patchcord: svg error: {e}");
            return None;
        }
    };
    let s = scale as f32;
    let size = tree.size();
    let mut pm = Pixmap::new((size.width() * s).ceil().max(1.0) as u32, (size.height() * s).ceil().max(1.0) as u32)?;
    resvg::render(&tree, Transform::from_scale(s, s), &mut pm.as_mut());
    Some(pm)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene with devices on the desk at (x, y), `w` wide.
    fn desk(at: &[(f64, f64, f64)]) -> Scene {
        let mut s = Scene::new();
        s.rack = false;
        for (k, &(x, y, w)) in at.iter().enumerate() {
            let dev = Device { key: format!("d{k}"), name: format!("d{k}"), model: String::new(), finish: crate::graph::Finish::Metal, color: [0; 3], groups: Vec::new(), state: "idle", card: None };
            let geo = Geometry { w, ..Default::default() };
            s.devs.push(DevView { dev, split: Vec::new(), defs: Vec::new(), counts: Vec::new(), geo, x, y, tx: x, ty: y, dx: x, dy: y, face: None, shade: None, face_sig: 0, face_scale: 0.0, lcd: None, level: [0.0; 2], meter: None, mix_local: HashMap::new(), mix_note: None, bal: HashMap::new(), mix_pics: Vec::new(), glevel: HashMap::new(), gmeters: Vec::new() });
        }
        s
    }

    fn overlap(s: &Scene) -> bool {
        let d = &s.devs;
        (0..d.len()).any(|i| (0..d.len()).any(|j| i != j && !apart((d[i].dx, d[i].dy, d[i].geo.w), (d[j].dx, d[j].dy, d[j].geo.w), 0.0)))
    }

    /// A device dragged near another's edge or centre sticks to it, with a guide to show it;
    /// pulled further it comes free.
    #[test]
    fn a_dragged_device_lines_up_with_another() {
        let mut s = desk(&[(100.0, 100.0, 400.0), (600.0, 500.0, 300.0)]);
        // left edges
        s.drag_device("d1", 106.0, 500.0, 1.0);
        assert_eq!((s.devs[1].dx, s.devs[1].dy), (100.0, 500.0));
        assert!(s.guides.iter().any(|g| g[0] == 100.0 && g[2] == 100.0), "no guide along the left edges: {:?}", s.guides);
        // from the other side too
        s.drag_device("d1", 95.0, 500.0, 1.0);
        assert_eq!(s.devs[1].dx, 100.0);
        s.drag_device("d1", 650.0, 107.0 + DEV_H + 40.0, 1.0);
        assert_eq!(s.devs[1].dx, 650.0, "far from every line, it goes where it is held");
        assert!(s.guides.is_empty());
        // tops, side by side
        s.drag_device("d1", 700.0, 94.0, 1.0);
        assert_eq!(s.devs[1].dy, 100.0);
        assert!(s.guides.iter().any(|g| g[1] == 100.0 && g[3] == 100.0));
        s.drop_device();
        assert!(s.guides.is_empty());
        // with lining up switched off it goes exactly where it is held
        s.align = false;
        s.drag_device("d1", 106.0, 500.0, 1.0);
        assert_eq!((s.devs[1].dx, s.devs[1].dy), (106.0, 500.0));
        assert!(s.guides.is_empty());
    }

    /// Once stuck, a device stays stuck while the hand wavers near the line, so the guide doesn't
    /// flicker; pulled well away it comes free. The distances are pixels on the screen.
    #[test]
    fn a_device_stays_lined_up_until_pulled_away() {
        let mut s = desk(&[(100.0, 100.0, 400.0), (600.0, 500.0, 300.0)]);
        for (x, px, want) in [(112.0, 1.0, 100.0), (125.0, 1.0, 100.0), (88.0, 1.0, 100.0), (175.0, 1.0, 175.0), (180.0, 1.0, 180.0), (125.0, 2.0, 100.0)] {
            s.drag_device("d1", x, 500.0, px);
            assert_eq!(s.devs[1].dx, want, "held at {x} (a pixel showing {px})");
            assert_eq!(s.guides.is_empty(), want != 100.0);
        }
    }

    /// Right under another device, as close as devices may come, a device still lines up with
    /// it by the centres, even with the hand a little too high.
    #[test]
    fn a_device_right_below_another_lines_up_by_the_centres() {
        let mut s = desk(&[(100.0, 100.0, 400.0), (600.0, 500.0, 300.0)]);
        let below = 100.0 + DEV_H + GAP;
        for y in [below + 6.0, below, below - 5.0] {
            s.drag_device("d1", 157.0, y, 1.0);
            assert_eq!((s.devs[1].dx, s.devs[1].dy), (150.0, below), "held at (157, {y})");
            assert!(s.guides.iter().any(|g| g[0] == 300.0 && g[2] == 300.0), "no guide along the centres: {:?}", s.guides);
        }
    }

    /// A new cord from a jack doesn't offer the jacks that jack is already patched to: they take
    /// no second cord.
    #[test]
    fn a_new_cord_is_not_offered_jacks_it_is_patched_to() {
        use crate::graph::{Group, Port};
        let group = |label: &str, dir: Dir, ids: [u32; 2]| Group {
            label: label.into(),
            dir,
            ports: ids.iter().zip(["FL", "FR"]).map(|(&id, ch)| Port { id, node: id, name: format!("p{id}"), label: ch.into(), dir, media: Media::Audio, channel: ch.into() }).collect(),
            stereo: true,
            meter: false,
            mix: None,
        };
        let mut s = desk(&[(0.0, 0.0, 300.0), (400.0, 0.0, 300.0), (400.0, 200.0, 300.0)]);
        for (d, g) in s.devs.iter_mut().zip([group("OUTPUT", Dir::Out, [1, 2]), group("PLAYBACK", Dir::In, [3, 4]), group("PLAYBACK", Dir::In, [5, 6])]) {
            d.dev.groups = vec![g];
            d.split = vec![false];
        }
        s.index();
        let key = |s: &Scene, di: usize| s.devs[di].defs[0].key.clone();
        let (out, b, c) = (key(&s, 0), key(&s, 1), key(&s, 2));
        s.show_cord(&out, &b);
        let seq = s.pull_new(&out).unwrap();
        let t = s.targets(seq);
        assert!(t.contains(&c), "the free input is offered: {t:?}");
        assert!(!t.contains(&b), "the input it is already patched to is offered: {t:?}");
    }

    /// Devices that come to lie over one another (one grew) move apart.
    #[test]
    fn devices_that_overlap_move_apart() {
        for at in [vec![(0.0, 0.0, 500.0), (400.0, 40.0, 300.0)], vec![(0.0, 0.0, 500.0), (100.0, 100.0, 300.0), (450.0, 20.0, 300.0)], vec![(0.0, 0.0, 300.0), (0.0, 0.0, 300.0)]] {
            let mut s = desk(&at);
            s.make_room();
            assert!(!overlap(&s), "devices at {at:?} still overlap");
        }
    }

    /// The hover highlight eases to where it is going and stays there, frame after frame.
    #[test]
    fn the_hover_highlight_stays_still_once_there() {
        for to in [0.0, 1.0] {
            let mut lit = 1.0 - to;
            for _ in 0..30 {
                lit = ease_lit(lit, to, 1.0 / 60.0);
            }
            for _ in 0..10 {
                let next = ease_lit(lit, to, 1.0 / 60.0);
                assert_eq!(next, to, "the highlight moves from {lit} to {next} while it should stay at {to}");
                lit = next;
            }
        }
    }
}
