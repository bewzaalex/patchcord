//! Everything on the patch field that doesn't need a window: the devices and where they are,
//! the cords and their physics, and the pictures to draw. The window (app.rs) and the
//! `--render` snapshot both use it.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
use resvg::usvg;

use super::cord::{Cord, Held, RATE};
use super::face::{self, DEV_H, Geometry, JP, JackDef, JackKind, LCD_H, LCD_W, LCD_X, LCD_Y, RACK_MIN, SHADOW};
use super::{paint, text};
use crate::config::Preset;
use crate::graph::{Device, Dir, Graph, Media};
use crate::pw::{Cmd, Event};

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
/// the rails each side of the rack, and the margin above and below
const RAIL: f64 = 34.0;
const RACK_PAD: f64 = 14.0;

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
    /// what the face was drawn for (jack counts, split switches, scale)
    face_sig: u64,
    pub lcd: Option<(String, Layer)>,
    /// the meter's lit LEDs, left and right (0…10, falling smoothly)
    pub level: [f32; 2],
    pub meter: Option<((u8, u8), Layer)>,
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
    pub sag: f32,
    /// devices have been placed once (after the first full graph)
    pub laid_out: bool,
    /// physics time not yet stepped
    acc: f32,
    opt: Arc<usvg::Options<'static>>,
    /// commands for PipeWire (meters to start and stop), sent by whoever owns the connection
    pub outbox: Vec<Cmd>,
    /// meter id by the ports it measures, and the device and peaks of each meter
    meters: HashMap<Vec<u32>, u32>,
    meter_dev: HashMap<u32, String>,
    peaks: HashMap<u32, Vec<f32>>,
    next_meter: u32,
    /// young streams waiting to be shown, at the last look
    young: usize,
    /// cords: 0 shown, 1 faded, 2 hidden (plugs stay)
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
    /// the links changed since the last preset was saved or loaded
    pub edited: bool,
    /// PipeWire's settings (clock.rate, clock.quantum…) and version
    pub settings: HashMap<String, String>,
    pub version: Option<String>,
    /// jack pairs just pulled apart, not to be plugged back in from links PipeWire hasn't removed yet
    suppress: Vec<((String, String), f32)>,
    /// link changes, for undo and redo: (added, removed) port pairs
    undo: Vec<(Vec<(u32, u32)>, Vec<(u32, u32)>)>,
    redo: Vec<(Vec<(u32, u32)>, Vec<(u32, u32)>)>,
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
            sag: 0.45,
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
                        let named: Vec<String> = l.iter().map(|(id, p)| format!("{}={:.2?}", self.meter_dev.get(id).map_or("?", |s| s.as_str()), p)).collect();
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
                    DevView { dev, split, defs: Vec::new(), counts: Vec::new(), geo: Geometry::default(), x: 0.0, y: 0.0, tx: 0.0, ty: 0.0, dx: 0.0, dy: 0.0, face: None, face_sig: 0, lcd: None, level: [0.0; 2], meter: None }
                }
            };
            self.devs.push(dv);
        }
        // keep the drawing order the user made by dragging
        self.devs.sort_by_key(|d| order.iter().position(|k| *k == d.dev.key).unwrap_or(usize::MAX));
        self.index();
        self.meters();
        self.reconcile(!self.laid_out);
        self.slots();
        if !self.laid_out {
            self.laid_out = true;
            self.arrange();
        } else {
            self.place_new(&fresh);
        }
        // devices that have a remembered place go back to it
        for d in self.devs.iter_mut().filter(|d| fresh.contains(&d.dev.key)) {
            if let Some(&(x, y)) = self.saved_pos.get(&d.dev.key) {
                (d.tx, d.ty) = (x, y);
            }
            (d.dx, d.dy) = (d.tx, d.ty);
        }
        if self.rack {
            self.layout_rack(None);
        }
        for d in self.devs.iter_mut().filter(|d| fresh.contains(&d.dev.key)) {
            (d.x, d.y) = (d.tx, d.ty);
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
        let mut want: HashMap<Vec<u32>, (String, Vec<(u32, u32)>, Vec<String>)> = HashMap::new();
        for d in &self.devs {
            for g in d.dev.groups.iter().filter(|g| g.meter) {
                let ports: Vec<u32> = g.ports.iter().map(|p| p.id).collect();
                let targets = g.ports.iter().map(|p| (p.node, p.id)).collect();
                let channels = g.ports.iter().map(|p| if p.channel.is_empty() { "MONO".to_owned() } else { p.channel.clone() }).collect();
                want.insert(ports, (d.dev.key.clone(), targets, channels));
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
    fn reconcile(&mut self, quiet: bool) {
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
            let mut c = Cord::new(self.seq, (self.slot_seq - 1, self.slot_seq), (a, ka, pa), (b, kb, pb), color, self.sag);
            if !quiet {
                (c.a.ins, c.b.ins) = (1.0, 1.0);
            }
            self.cords.push(c);
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
            // a cord being pulled out no longer takes a jack of the mult
            if c.dying.is_some() {
                continue;
            }
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
        for d in &mut self.devs {
            let (dx, dy) = (d.tx - d.x, d.ty - d.y);
            if dx.abs() + dy.abs() > 0.3 {
                d.x += dx * 0.18;
                d.y += dy * 0.18;
                moving = true;
            } else {
                (d.x, d.y) = (d.tx, d.ty);
            }
        }
        self.slots();
        // meters: peaks to lit LEDs on a dB scale (-54…0 dB → 0…10), quick up and slow down
        let mut target: HashMap<&str, [f32; 2]> = HashMap::new();
        for (id, p) in &self.peaks {
            let Some(dev) = self.meter_dev.get(id) else { continue };
            let led = |v: f32| if v <= 0.0 { 0.0 } else { ((20.0 * v.log10() + 54.0) / 54.0 * 10.0).clamp(0.0, 10.0) };
            let (l, r) = (p.first().copied().unwrap_or(0.0), p.get(1).or(p.first()).copied().unwrap_or(0.0));
            let t = target.entry(dev.as_str()).or_default();
            (t[0], t[1]) = (t[0].max(led(l)), t[1].max(led(r)));
        }
        for d in &mut self.devs {
            let t = target.get(d.dev.key.as_str()).copied().unwrap_or_default();
            for c in 0..2 {
                d.level[c] = if t[c] >= d.level[c] { t[c] } else { (d.level[c] - 18.0 * dt).max(t[c]) };
                moving |= d.level[c] > 0.01;
            }
        }
        self.acc = (self.acc + dt).min(4.0 / RATE);
        let steps = (self.acc * RATE) as usize;
        self.acc -= steps as f32 / RATE;
        let cords = std::mem::take(&mut self.cords);
        let mut kept = Vec::with_capacity(cords.len());
        for mut c in cords {
            let h = held.filter(|(seq, _)| *seq == c.seq).map(|(_, h)| h);
            c.frame(dt, steps, &|k, n| self.slot_pos(k, n), hand, h, self.sag, dragging);
            moving |= c.moving();
            if !c.gone() {
                kept.push(c);
            }
        }
        self.cords = kept;
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

    /// Draws faces and LCDs that are missing, out of date or drawn at another scale.
    /// `ms`: time, for the scrolling names. True when something new was drawn.
    pub fn prepare(&mut self, scale: f64, ms: u128) -> bool {
        let mut drew = false;
        match self.rack_w {
            Some(rw) => {
                let mut h = DefaultHasher::new();
                (rw.to_bits(), self.rack_h.to_bits(), scale.to_bits()).hash(&mut h);
                let sig = h.finish();
                if self.rack_bg.as_ref().is_none_or(|(s, _)| *s != sig) {
                    let (w, ht) = (rw + 2.0 * RAIL, self.rack_h + 2.0 * RACK_PAD);
                    let pm = rasterize(&self.opt, &rack_svg(w, ht), scale);
                    self.rack_bg = Some((sig, Layer { id: next_id(), pm, x: -RAIL, y: -RACK_PAD, w, h: ht }));
                    drew = true;
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
            (&d.counts, &d.split, scale.to_bits(), &d.dev.name, &d.dev.model, d.dev.state, self.rack_w.map(f64::to_bits)).hash(&mut h);
            d.defs.iter().for_each(|j| (&j.key, &j.label).hash(&mut h));
            let sig = h.finish();
            if d.face.is_none() || sig != d.face_sig {
                let (svg, geo) = face::face(&d.dev, &d.defs, &d.counts, &d.split, self.rack_w);
                d.geo = geo;
                let pm = rasterize(&self.opt, &svg, scale);
                d.face = Some(Layer { id: next_id(), pm, x: -SHADOW, y: -SHADOW, w: d.geo.w + 2.0 * SHADOW, h: DEV_H + 2.0 * SHADOW });
                d.face_sig = sig;
                d.lcd = None;
                d.meter = None;
                drew = true;
            }
            let n = links.get(&di).copied().unwrap_or(0);
            let line2 = format!(
                "{:<11}{}",
                d.dev.state.to_uppercase(),
                match n {
                    0 => "NO LINKS".to_owned(),
                    1 => "1 LINK".to_owned(),
                    n => format!("{n} LINKS"),
                }
            );
            let text = format!("{}\n{line2}", face::marquee(&d.dev.name, ms));
            if d.lcd.as_ref().is_none_or(|(t, _)| *t != text) {
                let (l1, l2) = text.split_once('\n').unwrap();
                let pm = rasterize(&self.opt, &face::lcd(l1, l2), scale);
                let x = d.geo.left + LCD_X;
                d.lcd = Some((text, Layer { id: next_id(), pm, x, y: LCD_Y, w: LCD_W, h: LCD_H }));
                drew = true;
            }
            let lit = (d.level[0].round() as u8, d.level[1].round() as u8);
            if lit == (0, 0) {
                d.meter = None;
            } else if d.meter.as_ref().is_none_or(|(l, _)| *l != lit) {
                d.meter = Some((lit, meter_layer(lit, d.geo.meter, scale)));
                drew = true;
            }
        }
        drew
    }

    /// The cords and plugs in the window area (x, y, w, h) in pixels, or None when there are none.
    /// `rings`: jacks a plug in the hand may go into (world position, the one it is pulled to).
    pub fn cord_layer(&self, view: View, win: (u32, u32), rings: &[((f32, f32), bool)], pulse: f32) -> Option<(Pixmap, i32, i32)> {
        if self.cords.is_empty() {
            return None;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for c in &self.cords {
            for p in &c.pts {
                (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
            }
        }
        // room for plugs in the hand and shadows
        let m = 170.0;
        let s = view.s as f32;
        let px0 = (((x0 - m) * s) as f64 + view.x).floor().max(0.0) as i32;
        let py0 = (((y0 - m) * s) as f64 + view.y).floor().max(0.0) as i32;
        let px1 = (((x1 + m) * s) as f64 + view.x).ceil().min(win.0 as f64) as i32;
        let py1 = (((y1 + m) * s) as f64 + view.y).ceil().min(win.1 as f64) as i32;
        // while a plug is in the hand, the target rings can be anywhere: use the whole window
        let (px0, py0, px1, py1) = if rings.is_empty() { (px0, py0, px1, py1) } else { (0, 0, win.0 as i32, win.1 as i32) };
        if px1 <= px0 || py1 <= py0 {
            return None;
        }
        let mut pm = Pixmap::new((px1 - px0) as u32, (py1 - py0) as u32)?;
        let ts = Transform::from_row(s, 0.0, 0.0, s, view.x as f32 - px0 as f32, view.y as f32 - py0 as f32);
        paint::rings(&mut pm, rings, pulse, ts);
        // faded or hidden cords; a cord in the hand always shows
        let fade = [1.0, 0.22, 0.0][self.cables.min(2) as usize];
        paint::cords(&mut pm, &self.cords, |c| if c.a.held || c.b.held || c.grab.is_some() { c.alpha } else { c.alpha * fade }, ts);
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

    /// Where the free slot of a jack is.
    pub fn spare(&self, key: &str) -> Option<(f32, f32)> {
        let &(di, ji) = self.jacks.get(key)?;
        self.slot_pos(key, self.devs[di].counts[ji])
    }

    pub fn jack_kind(&self, key: &str) -> Option<JackKind> {
        self.jack(key).map(|(_, j)| j.kind)
    }

    /// The SPLIT/JOIN switch under a world point: (device, group).
    pub fn switch_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| {
            d.geo.switches.iter().find_map(|(g, [bx, by, bw, bh])| (x >= d.x + bx && x <= d.x + bx + bw && y >= d.y + by && y <= d.y + by + bh).then_some((di, *g)))
        })
    }

    /// One stereo jack ⇄ separate L and R jacks. The cords re-plug themselves to match.
    pub fn toggle_split(&mut self, di: usize, g: usize) {
        if let Some(v) = self.devs[di].split.get_mut(g) {
            *v = !*v;
        }
        self.index();
        self.reconcile(false);
        self.slots();
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
    /// of the same kind of signal as the jack at the cord's other end.
    pub fn targets(&self, seq: u64) -> Vec<String> {
        let Some(c) = self.cord_index(seq).map(|i| &self.cords[i]) else { return Vec::new() };
        let (dir, other) = if c.b.held { (Dir::In, c.a.jack.as_ref()) } else { (Dir::Out, c.b.jack.as_ref()) };
        let media = other.and_then(|k| self.jack(k)).map_or(Media::Audio, |(_, j)| j.media);
        self.jacks.keys().filter(|k| self.jack(k).is_some_and(|(_, j)| j.dir == dir && j.media == media)).cloned().collect()
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
        Preset { name: name.to_owned(), nodes: self.graph.node_names(), links, split, pos: self.positions().into_iter().collect() }
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
        self.reconcile(false);
        self.slots();
        (want.len(), missing)
    }

    /// Lets go of the plug in the hand: into `target` when given and free to take it, else it
    /// falls out. `moved`: it travelled (a plug only clicked returns quietly). True when it went in.
    pub fn drop_held(&mut self, seq: u64, target: Option<String>, moved: bool) -> bool {
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
                    !self.cords.iter().any(|c| c.live() && c.a.jack.as_deref() == Some(x.as_str()) && c.b.jack.as_deref() == Some(y.as_str()))
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
                e.ins = if moved { 1.0 } else { 0.0 };
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

/// The rack: a dark inside between two metal rails with a row of mounting holes each.
fn rack_svg(w: f64, h: f64) -> String {
    let mut holes = String::new();
    let mut y = 16.5;
    while y < h - 6.0 {
        for x in [RAIL / 2.0 - 2.0, w - RAIL / 2.0 + 2.0] {
            holes += &format!(r##"<circle cx="{x}" cy="{y}" r="4" fill="#000" opacity=".4"/><circle cx="{x}" cy="{y}" r="3.2" fill="#050506"/>"##);
        }
        y += 21.0;
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="in" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0c0d0e"/><stop offset="1" stop-color="#17181a"/></linearGradient><linearGradient id="rail" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#6e7175"/><stop offset=".45" stop-color="#c4c7cb"/><stop offset=".7" stop-color="#8a8d91"/><stop offset="1" stop-color="#5d6064"/></linearGradient></defs><rect width="{w}" height="{h}" rx="4" fill="url(#in)" stroke="#000"/><rect x="0" width="{r}" height="{h}" fill="url(#rail)"/><rect x="{x2}" width="{r}" height="{h}" fill="url(#rail)"/>{holes}</svg>"##,
        r = RAIL - 4.0,
        x2 = w - RAIL + 4.0
    )
}

/// The lit LEDs of a signal meter (the dark ones are part of the face), with a little glow.
fn meter_layer(lit: (u8, u8), mx: f64, scale: f64) -> Layer {
    let (x0, y0, w, h) = (mx - 4.0, 19.6, 27.0, 82.0);
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
                let (x, y) = (mx as f32 + c as f32 * 11.0, 92.0 - i as f32 * 7.6);
                for (grow, a) in [(2.0, 50u8), (0.0, 255u8)] {
                    if let Some(r) = Rect::from_xywh(x - grow, y - grow, 8.0 + 2.0 * grow, 5.4 + 2.0 * grow) {
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
