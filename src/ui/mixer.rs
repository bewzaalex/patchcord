//! MIXER: the graph as a mixing console (ADR 0015). Every device and every program the rack shows
//! is a module: its name and its PROFILE once on top, then a strip for each thing it does that
//! carries sound, what takes sound in at the left (a sink, an app's INPUT), what gives it out at
//! the right (a source, an app's OUTPUT). A strip is a group of the device (`graph::Group`): its
//! volume, MUTE, CUE, its meter, and where its sound goes: an app's PLAY ON (RECORD FROM) keys
//! are its links, a device's MAIN is the default. MIXER creates no node: it shows and changes what
//! the graph has.
//!
//! Each module is one picture drawn on the painter's threads; a fader's cap, the lit LEDs and the
//! numbers under the faders are small pictures over it, so moving a fader draws no module again.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::time::{Duration, Instant};

use resvg::tiny_skia::Pixmap;

use super::face::{self, FACE_PAD};
use super::painter;
use super::panel;
use super::print::{SECTION, SMALL};
use super::scene::{Layer, Scene, next_id, rasterize};
use super::text::svg_options;
use crate::graph::{Device, Finish, VOL_MAX};

/// A picture to draw (see `engine::Pic`).
pub type Pic<'a> = super::engine::Pic<'a>;

/// What a strip does with sound: a device's sink (it plays what comes in) or source (a mic), an
/// app's output (it plays) or input (it records).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Role {
    Sink,
    Source,
    Play,
    Rec,
}

impl Role {
    /// It takes sound in: at a module's left.
    fn takes(self) -> bool {
        matches!(self, Role::Sink | Role::Rec)
    }
}

/// A group `gi` of `dev` as a strip, if it is one: audio that is no MONITOR, with a volume, or a
/// node with ports only one way (a DAW's engine, a filter that only plays). A node that takes sound
/// in and gives it out without a volume (an effect inside Easy Effects' chain) is no strip: the
/// rack has it.
pub fn role_of(dev: &Device, gi: usize) -> Option<Role> {
    let g = dev.groups.get(gi)?;
    if !matches!(g.role, "PLAYBACK" | "CAPTURE" | "INPUT" | "OUTPUT") {
        return None;
    }
    if g.mix.is_none() {
        let node = g.ports.first()?.node;
        if dev.groups.iter().any(|o| o.role != "MONITOR" && o.dir != g.dir && o.ports.first().map(|p| p.node) == Some(node)) {
            return None;
        }
    }
    Some(match g.role {
        "PLAYBACK" => Role::Sink,
        "CAPTURE" => Role::Source,
        "OUTPUT" => Role::Play,
        _ => Role::Rec,
    })
}

/// Where an app's sound can go (a sink), or what it can record (a source).
#[derive(Clone, Debug, Hash)]
pub struct Target {
    pub at: (usize, usize),
    pub label: String,
    pub color: [u8; 3],
    pub lit: bool,
}

#[derive(Clone, Debug)]
pub struct Strip {
    pub at: (usize, usize),
    pub role: Role,
    pub node: u32,
    pub name: String,
    pub kind: &'static str,
    /// volume (1 is 100%), balance, mute; none: PipeWire has no volume for it
    pub mix: Option<(f64, f64, bool)>,
    pub stereo: bool,
    /// SPLIT: a fader for L and one for R
    pub chans: bool,
    /// a key of it held down now: it sinks deeper until it is let go
    held: Option<Hit>,
    /// an app's: PLAY ON or RECORD FROM
    pub targets: Vec<Target>,
    /// a device's: it is the default output (input)
    pub main: bool,
    /// a device's: its `node.name`, to make it the default
    pub node_name: Option<String>,
    /// a card's: the port in use, when it has more than one to choose
    pub port: Option<String>,
    pub cue: bool,
    /// its meter: one column or two, and the LEDs lit in each
    pub cols: usize,
    pub level: [f32; 2],
    /// within the module
    pub x: f64,
    pub w: f64,
}

/// The console's groups, left to right, as on a hardware one: the microphones, the apps, the
/// other outputs, and the master (the default output) at the right.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Section {
    Inputs,
    Apps,
    Outputs,
    Master,
}

impl Section {
    fn name(self) -> &'static str {
        match self {
            Section::Inputs => "INPUTS",
            Section::Apps => "APPS",
            Section::Outputs => "OUTPUTS",
            Section::Master => "MASTER",
        }
    }
}

/// A module of the console: one strip (a device's input or output, an app's INPUT or OUTPUT),
/// under its device's name; or a card that is off, its name and PROFILE only.
#[derive(Clone, Debug)]
pub struct Module {
    /// its picture's key: the device's, and the group's
    pub key: String,
    pub di: usize,
    pub name: String,
    pub finish: Finish,
    pub section: Section,
    /// the card's profile in use, on its PROFILE key: on the card's output (else its input)
    pub profile: Option<String>,
    pub strips: Vec<Strip>,
    pub x: f64,
    pub w: f64,
    /// MASTER held at the window's right edge, the console going on under it
    pub pinned: bool,
}

/// A sink (source) an app's key can go to: its role, where it is, its label, its cords' colour.
type End = (Role, (usize, usize), String, [u8; 3]);

/// A turn of the wheel: its clicks (for steps), how far it scrolls (logical pixels), Shift held.
pub struct Wheel {
    pub clicks: i32,
    pub d: (f64, f64),
    pub fine: bool,
}

/// What a press lands on.
#[derive(Clone, Debug, PartialEq)]
enum Hit {
    Graph(u32),
    Profile(usize),
    Port(usize),
    /// an app's key to a sink (source)
    To((usize, usize), (usize, usize)),
    Main((usize, usize)),
    All((usize, usize)),
    Cue((usize, usize)),
    Mute((usize, usize)),
    Chan((usize, usize)),
    /// a fader's slot: which channel (none: the fader for all), the slot's top
    Fader((usize, usize), Option<usize>, f64),
}

/// What MIXER asks the window to do.
pub enum Action {
    /// ENGINE, that node chosen
    Graph(u32),
    /// a card's PROFILE or PORT list, under the key at this rectangle (window pixels)
    Profile(usize, [f64; 4]),
    Port(usize, [f64; 4]),
    /// listen to a group (its jacks' keys)
    Cue(Vec<String>),
    /// something changed in PipeWire: send it
    Changed,
}

enum Drag {
    Fader { at: (usize, usize), ch: Option<usize>, top: f64 },
    /// a key with a LED held down: it does what it does when let go over it
    Key(Hit),
    Pan { from: (f64, f64), x: f64, y: f64 },
}

// the console's measures, in logical pixels
const LEFT: f64 = 14.0;
const TOP: f64 = 26.0;
const BOTTOM: f64 = 14.0;
/// the gap between two modules of a group
const GAP: f64 = 6.0;
/// between two groups: wider, a groove down its middle
const SEP: f64 = 22.0;
/// a module's heading: its name
const HEAD: f64 = 34.0;
/// a module is one strip, of one width
const STRIP_W: f64 = 112.0;
const BAND: f64 = 24.0;
/// the first group of keys (OUTPUT, INPUT, PLAY ON, RECORD FROM): its title, then its keys in rows
const ROUTE_Y: f64 = 31.0;
const KEY_H: f64 = 17.0;
fn row(i: usize) -> f64 {
    ROUTE_Y + 17.0 + i as f64 * 23.0
}
const TRAVEL: f64 = 160.0;
/// The window's height, for the modules' (each new one draws every module again): the last height
/// once the window has kept still this long.
const ROWS_DELAY: Duration = Duration::from_millis(150);
const RADIUS: f64 = 8.0;

/// Where the lower part of a strip is (CHANNEL's keys, the fader block), from the strip's top:
/// the modules are as tall as the window lets them (no shorter than these), what they gain the
/// room under the first keys.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rows {
    keys_y: f64,
    chan_y: f64,
    strip_h: f64,
}

impl Rows {
    const MIN: Rows = Rows { keys_y: 200.0, chan_y: 231.0, strip_h: 474.0 };

    /// For a field `h` tall.
    fn fit(h: f64) -> Rows {
        let m = Rows::MIN;
        let extra = (h - TOP - BOTTOM - HEAD - m.strip_h).floor().max(0.0);
        Rows { keys_y: m.keys_y + extra, chan_y: m.chan_y + extra, strip_h: m.strip_h + extra }
    }

    fn mod_h(&self) -> f64 {
        HEAD + self.strip_h
    }

    /// The fader block, under SPLIT (clear of its key's hole): where the caps go by (`cap_y` from
    /// it; the slot from 6 under it), and the line of its name (VOLUME, GAIN) under it all.
    fn fader_y(&self) -> f64 {
        self.chan_y + 44.0
    }

    fn level_y(&self) -> f64 {
        self.chan_y + 230.0
    }
}

impl Default for Rows {
    fn default() -> Self {
        Rows::MIN
    }
}
/// a fader's cap, its volume on it (and its shadow, a little lower)
const CAP: (f64, f64) = (34.0, 22.0);
/// the lit LEDs' column: as tall as the fader's slot, level with it; a LED's
const LEDS_H: f64 = TRAVEL;
/// the meter's LEDs: how many, how wide (thin, split or not)
const LEDS: usize = 12;
const LED_W: f64 = 4.0;

/// Where the cap's middle is for a volume (1 is 100%), from the slot's top.
fn cap_y(v: f64) -> f64 {
    6.0 + (1.0 - (v / VOL_MAX).clamp(0.0, 1.0)) * TRAVEL
}

/// The volume at a height from the slot's top; near 100% it rests there.
fn vol_at(y: f64) -> f64 {
    let v = ((1.0 - (y - 6.0) / TRAVEL) * VOL_MAX).clamp(0.0, VOL_MAX);
    if (v - 1.0).abs() < 0.03 { 1.0 } else { v }
}

/// A stereo pair's L and R from its volume and balance (as `Mix::volumes_for` makes them).
fn channels((v, b): (f64, f64)) -> [f64; 2] {
    [if b > 0.0 { v * (1.0 - b) } else { v }, if b < 0.0 { v * (1.0 + b) } else { v }]
}

/// The middles of a strip's slots, from its left: one, or L and R (SPLIT).
fn slots_x(s: &Strip) -> Vec<f64> {
    if s.chans { vec![38.0, 74.0] } else { vec![50.0] }
}

/// The meter's left, from the strip's.
fn meter_x(s: &Strip) -> f64 {
    if s.chans { 93.0 } else { 84.0 }
}

/// How many of the meter's LEDs a level (0…10, the rack's) lights.
fn lit(level: f32) -> u8 {
    (level as f64 / 10.0 * LEDS as f64).round().clamp(0.0, LEDS as f64) as u8
}

/// The role of the strip at `at`.
fn at_role(m: &Mixer, at: (usize, usize)) -> Option<Role> {
    m.strip(at).map(|s| s.role)
}

/// The strip a key with a LED is on (those go down while held and act when let go): PLAY ON and
/// RECORD FROM, MAIN, CUE, MUTE, SPLIT.
fn key_of(h: &Hit) -> Option<(usize, usize)> {
    match h {
        Hit::To(at, _) | Hit::Main(at) | Hit::Cue(at) | Hit::Mute(at) | Hit::Chan(at) => Some(*at),
        _ => None,
    }
}

/// A device's second group of keys, DEVICE SETUP (its card's PROFILE, its PORT), under the first:
/// where its title is, from the strip's top.
fn setup_y() -> f64 {
    row(2) + 4.0
}

/// The keys of DEVICE SETUP in rows under its title.
fn setup_row(i: usize) -> f64 {
    setup_y() + 17.0 + i as f64 * 23.0
}

/// A card's PROFILE key, first in DEVICE SETUP, in a module `w` wide.
fn profile_key(w: f64) -> [f64; 4] {
    [12.0, HEAD + setup_row(0), w - 24.0, KEY_H]
}

/// `t` cut to `w` wide in a style, with an ellipsis.
fn fit(t: &str, style: super::print::Style, w: f64) -> String {
    if style.width(t) <= w {
        return t.to_owned();
    }
    let mut s: String = t.to_owned();
    while !s.is_empty() && style.width(&format!("{s}…")) > w {
        s.pop();
    }
    format!("{}…", s.trim_end())
}

/// A module's picture: what it was drawn from, the picture shown, one being drawn.
struct ModPic {
    sig: u64,
    layer: Option<Layer>,
    want: Option<(u64, f64, f64)>,
}

#[derive(Default)]
pub struct Mixer {
    pub on: bool,
    /// how far the console is scrolled (logical pixels)
    sx: f64,
    sy: f64,
    /// the console's size
    size: (f64, f64),
    /// stereo strips showing a fader for each channel: device key, group
    chans: HashSet<(String, usize)>,
    modules: Vec<Module>,
    /// the grooves between the groups: their x, and whether MASTER's
    grooves: Vec<(f64, bool)>,
    rows: Rows,
    /// a new height the window asks for, and since when (see `ROWS_DELAY`)
    rows_want: Option<(Rows, Instant)>,
    hits: Vec<(Hit, [f64; 4])>,
    pics: HashMap<String, ModPic>,
    inbox: painter::Inbox,
    /// the cap, the lit LEDs (by columns and how many lit), the numbers, the sections' names
    /// the faders' caps, by the volume on them (%)
    caps: HashMap<(u32, u64), Layer>,
    leds: HashMap<(usize, u8, u8, u64), Layer>,
    texts: HashMap<(String, &'static str, u64), Layer>,
    /// a groove, MASTER's backdrop, the shadow it casts on what goes under it
    deco: Option<(u64, [Layer; 3])>,
    drag: Option<Drag>,
    last_click: Option<(Instant, Hit)>,
}

impl Mixer {
    pub fn new(on: bool) -> Self {
        Mixer { on, ..Mixer::default() }
    }

    /// The modules for what the scene shows now, and where everything is on the console (a field
    /// `size` pixels at `dpi`: MASTER held at its right edge, the rows as tall as it lets them).
    fn layout(&mut self, scene: &Scene, pins: &[String], size: (u32, u32), dpi: f64) {
        let (fw, top, bottom) = Mixer::field(size, dpi);
        // a new height (every module drawn again) once the window keeps still (`ROWS_DELAY`), as
        // the rack takes a new width; at once the first time
        let want = Rows::fit(bottom - top);
        if want != self.rows {
            match self.rows_want {
                Some((w, at)) if w == want && at.elapsed() >= ROWS_DELAY => {
                    self.rows = want;
                    self.rows_want = None;
                }
                Some((w, _)) if w == want => {}
                _ if self.modules.is_empty() => self.rows = want,
                _ => self.rows_want = Some((want, Instant::now())),
            }
        } else {
            self.rows_want = None;
        }
        let rows = self.rows;
        let out_default = scene.default_name(true);
        let in_default = scene.default_name(false);
        // (section, first, name, its order within the device) for the order
        let mut mods: Vec<(Section, u8, usize, Module)> = Vec::new();
        for (di, d) in scene.devs.iter().enumerate() {
            let dev = &d.dev;
            let strips: Vec<Strip> = (0..dev.groups.len())
                .filter_map(|gi| {
                    let role = role_of(dev, gi)?;
                    let g = &dev.groups[gi];
                    let node = g.ports.first()?.node;
                    let mix = d.mix(gi);
                    let node_name = scene.graph.node_name(node).map(str::to_owned);
                    let main = match role {
                        Role::Sink => g.mix.is_some() && node_name.is_some() && node_name == out_default,
                        Role::Source => g.mix.is_some() && node_name.is_some() && node_name == in_default,
                        _ => false,
                    };
                    let port = scene.port_sections(di).into_iter().find(|p| p.group == gi).and_then(|p| p.ports.iter().find(|c| Some(c.index) == p.current).map(|c| c.description.clone()));
                    let jacks: Vec<&String> = d.defs.iter().filter(|j| j.group == gi).map(|j| &j.key).collect();
                    let cue = !jacks.is_empty() && jacks.iter().all(|k| pins.contains(k));
                    let two = g.stereo || g.ports.len() == 2;
                    let chans = g.stereo && mix.is_some() && self.chans.contains(&(dev.key.clone(), gi));
                    let held = match &self.drag {
                        Some(Drag::Key(h)) if key_of(h) == Some((di, gi)) => Some(h.clone()),
                        _ => None,
                    };
                    Some(Strip { at: (di, gi), role, node, name: face::name(dev, gi), kind: g.kind, mix, stereo: g.stereo, chans, held, targets: Vec::new(), main, node_name, port, cue, cols: if two { 2 } else { 1 }, level: d.level(gi), x: 6.0, w: STRIP_W })
                })
                .collect();
            let profile = dev.card.as_ref().map(|c| c.profile().map_or_else(|| "Off".to_owned(), |p| p.description.clone()));
            // a card's PROFILE once: on its output, else its input
            let profile_on = profile.as_ref().and_then(|_| strips.iter().find(|s| s.role == Role::Sink).or_else(|| strips.iter().find(|s| s.role == Role::Source)).map(|s| s.at));
            let module = |key: String, section: Section, strips: Vec<Strip>, profile: Option<String>| Module { key, di, name: dev.name.to_uppercase(), finish: dev.finish, section, profile, strips, x: 0.0, w: STRIP_W + 12.0, pinned: false };
            // an app's INPUT before its OUTPUT
            let mut order: Vec<Strip> = strips.iter().filter(|s| s.role.takes()).cloned().chain(strips.iter().filter(|s| !s.role.takes()).cloned()).collect();
            if order.is_empty() {
                // a card that is off: its name and PROFILE, last of the outputs
                if profile.is_some() {
                    mods.push((Section::Outputs, 2, 0, module(dev.key.clone(), Section::Outputs, Vec::new(), profile.clone())));
                }
                continue;
            }
            for (n, s) in order.drain(..).enumerate() {
                let (section, first) = match s.role {
                    Role::Source => (Section::Inputs, if s.main { 0 } else { 1 }),
                    Role::Sink if s.main => (Section::Master, 0),
                    Role::Sink => (Section::Outputs, 1),
                    _ => (Section::Apps, 1),
                };
                let p = if profile_on == Some(s.at) { profile.clone() } else { None };
                mods.push((section, first, n, module(format!("{}#{}", dev.key, s.at.1), section, vec![s], p)));
            }
        }
        // a static order within each group (the default input first): by name, A to Z
        mods.sort_by(|a, b| (a.0, a.1, &a.3.name, &a.3.key, a.2).cmp(&(b.0, b.1, &b.3.name, &b.3.key, b.2)));
        let mut mods: Vec<Module> = mods.into_iter().map(|m| m.3).collect();
        // an app's keys: every sink (source) shown, lit where it is linked, in the console's order
        let ends: Vec<End> = mods
            .iter()
            .flat_map(|m| {
                let dev = &scene.devs[m.di].dev;
                let several = |role: Role| dev.groups.iter().enumerate().filter(|(gi, g)| g.mix.is_some() && role_of(dev, *gi) == Some(role)).count() > 1;
                m.strips.iter().filter(|s| matches!(s.role, Role::Sink | Role::Source) && s.mix.is_some()).map(move |s| {
                    let label = if several(s.role) { format!("{} · {}", dev.name, s.name) } else { dev.name.clone() };
                    (s.role, s.at, label.to_uppercase(), dev.color)
                })
            })
            .collect();
        for m in &mut mods {
            for s in &mut m.strips {
                let want = match s.role {
                    Role::Play => Role::Sink,
                    Role::Rec => Role::Source,
                    _ => continue,
                };
                s.targets = ends
                    .iter()
                    .filter(|e| e.0 == want)
                    .map(|(_, at, label, color)| {
                        let lit = if s.role == Role::Play { !scene.group_links(s.at, *at).is_empty() } else { !scene.group_links(*at, s.at).is_empty() };
                        Target { at: *at, label: label.clone(), color: *color, lit }
                    })
                    .collect();
            }
        }
        // where everything is: a gap between modules, a groove between groups, MASTER held at the
        // window's right edge whatever the console scrolls to
        self.hits.clear();
        self.grooves.clear();
        let mut x = LEFT;
        let mut was = None;
        for m in &mut mods {
            let master = m.section == Section::Master;
            if was.is_some_and(|w| w != m.section) {
                x += SEP - GAP;
            }
            // MASTER at the window's right edge, always: over what scrolls under it while the
            // console is wider than the window; its groove right before it
            if master {
                m.x = self.sx + fw - LEFT - m.w;
                m.pinned = m.x < x - 0.5;
            }
            if was.is_some_and(|w| w != m.section) {
                self.grooves.push((if master { m.x - SEP / 2.0 } else { x - SEP / 2.0 }, master));
            }
            was = Some(m.section);
            if !master {
                m.x = x;
            }
            x += m.w + GAP;
            let (mx, my) = (m.x, TOP);
            if m.profile.is_some() {
                let [px, py, pw, ph] = profile_key(m.w);
                self.hits.push((Hit::Profile(m.di), [mx + px, my + py, pw, ph]));
            }
            let sy = my + HEAD;
            let below = usize::from(m.profile.is_some());
            for s in &m.strips {
                let x0 = mx + s.x;
                let gw = 10.0 + SMALL.width(&format!("#{}", s.node));
                self.hits.push((Hit::Graph(s.node), [x0 + 8.0, sy + 5.0, gw, 14.0]));
                let (kx, kw) = (x0 + 6.0, s.w - 12.0);
                match s.role {
                    Role::Play | Role::Rec => {
                        for (i, t) in s.targets.iter().enumerate().take(6) {
                            self.hits.push((Hit::To(s.at, t.at), [kx, sy + row(i), kw, KEY_H]));
                        }
                    }
                    Role::Sink | Role::Source => {
                        if s.mix.is_some() && s.node_name.is_some() {
                            self.hits.push((Hit::Main(s.at), [kx, sy + row(0), kw, KEY_H]));
                            self.hits.push((Hit::All(s.at), [kx, sy + row(1), kw, KEY_H]));
                        }
                        if s.port.is_some() {
                            self.hits.push((Hit::Port(m.di), [kx, sy + setup_row(below), kw, KEY_H]));
                        }
                    }
                }
                let half = (s.w - 12.0 - 7.0) / 2.0;
                self.hits.push((Hit::Cue(s.at), [kx, sy + rows.keys_y, half, 24.0]));
                if s.mix.is_some() {
                    self.hits.push((Hit::Mute(s.at), [kx + half + 7.0, sy + rows.keys_y, half, 24.0]));
                    if s.stereo {
                        self.hits.push((Hit::Chan(s.at), [kx, sy + rows.chan_y, kw, 24.0]));
                    }
                    for (i, cx) in slots_x(s).iter().enumerate() {
                        let ch = if s.chans { Some(i) } else { None };
                        let top = sy + rows.fader_y();
                        self.hits.push((Hit::Fader(s.at, ch, top), [x0 + cx - CAP.0 / 2.0, top, CAP.0, TRAVEL + 12.0]));
                    }
                }
            }
        }
        // (MASTER held at the edge: what it covers is where the console ends)
        self.size = (x - GAP + LEFT, TOP + rows.mod_h() + BOTTOM);
        // MASTER last: drawn over what goes under it
        mods.sort_by_key(|m| m.section == Section::Master);
        self.modules = mods;
    }

    /// The console's field in the window (logical pixels): below the panel, above the status bar.
    fn field(size: (u32, u32), dpi: f64) -> (f64, f64, f64) {
        let (w, h) = (size.0 as f64 / dpi, size.1 as f64 / dpi);
        (w, panel::H, h - panel::STATUS_H)
    }

    /// Keeps the console's scroll inside it.
    fn clamp(&mut self, size: (u32, u32), dpi: f64) {
        let (w, top, bottom) = Mixer::field(size, dpi);
        self.sx = self.sx.clamp(0.0, (self.size.0 - w).max(0.0));
        self.sy = self.sy.clamp(0.0, (self.size.1 - (bottom - top)).max(0.0));
    }

    /// A point on the console from the window (pixels).
    fn at(&self, p: (f64, f64), dpi: f64) -> (f64, f64) {
        (p.0 / dpi + self.sx, p.1 / dpi - panel::H + self.sy)
    }

    fn hit(&self, p: (f64, f64), dpi: f64) -> Option<Hit> {
        let (x, y) = self.at(p, dpi);
        self.hits.iter().find(|(_, [hx, hy, hw, hh])| x >= *hx && x <= hx + hw && y >= *hy && y <= hy + hh).map(|(h, _)| h.clone())
    }

    /// A rectangle on the console as window pixels.
    fn to_window(&self, [x, y, w, h]: [f64; 4], dpi: f64) -> [f64; 4] {
        [(x - self.sx) * dpi, (y - self.sy + panel::H) * dpi, w * dpi, h * dpi]
    }

    fn strip(&self, at: (usize, usize)) -> Option<&Strip> {
        self.modules.iter().flat_map(|m| &m.strips).find(|s| s.at == at)
    }

    /// A press at `p` (window pixels).
    pub fn press(&mut self, scene: &mut Scene, p: (f64, f64), dpi: f64) -> Option<Action> {
        let Some(hit) = self.hit(p, dpi) else {
            let (x, y) = (p.0 / dpi, p.1 / dpi);
            self.drag = Some(Drag::Pan { from: (x, y), x: self.sx, y: self.sy });
            return None;
        };
        let double = self.last_click.as_ref().is_some_and(|(t, h)| t.elapsed() < Duration::from_millis(350) && *h == hit);
        self.last_click = Some((Instant::now(), hit.clone()));
        let rect = self.hits.iter().find(|(h, _)| *h == hit).map(|(_, r)| *r).unwrap_or_default();
        // a key with a LED goes down while it is held, and does what it does when let go over it
        if key_of(&hit).is_some() {
            self.drag = Some(Drag::Key(hit));
            return None;
        }
        match hit {
            Hit::Graph(node) => Some(Action::Graph(node)),
            Hit::Profile(di) => Some(Action::Profile(di, self.to_window(rect, dpi))),
            Hit::Port(di) => Some(Action::Port(di, self.to_window(rect, dpi))),
            Hit::All(at) => self.play_all(scene, at),
            Hit::Fader(at, ch, top) => {
                if double {
                    self.set_fader(scene, at, ch, 1.0);
                } else {
                    self.drag = Some(Drag::Fader { at, ch, top });
                    let (_, y) = self.at(p, dpi);
                    self.set_fader(scene, at, ch, vol_at(y - top));
                }
                Some(Action::Changed)
            }
            _ => None,
        }
    }

    /// A key with a LED let go over itself: it does what it does.
    fn key_up(&mut self, scene: &mut Scene, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Cue((di, gi)) => Some(Action::Cue(scene.devs.get(di).map(|d| d.defs.iter().filter(|j| j.group == gi).map(|j| j.key.clone()).collect()).unwrap_or_default())),
            Hit::To(from, to) => {
                let s = self.strip(from)?;
                let play = s.role == Role::Play;
                let pair = |t: (usize, usize)| if play { (from, t) } else { (t, from) };
                // a push-push key: in, the app plays (records) there too; out, no longer
                let lit = s.targets.iter().any(|t| t.at == to && t.lit);
                if lit { scene.reroute(&[], &[pair(to)]) } else { scene.reroute(&[pair(to)], &[]) }
                Some(Action::Changed)
            }
            Hit::Main(at) => {
                let s = self.strip(at)?;
                scene.set_default(s.role == Role::Sink, s.node_name.as_deref()?);
                Some(Action::Changed)
            }
            Hit::Mute(at) => {
                scene.toggle_mute(at);
                Some(Action::Changed)
            }
            Hit::Chan(at) => {
                let key = (scene.devs.get(at.0)?.dev.key.clone(), at.1);
                if !self.chans.remove(&key) {
                    self.chans.insert(key);
                }
                None
            }
            _ => None,
        }
    }

    /// PLAY ALL HERE (RECORD ALL HERE): every app that plays (records) somewhere, here only; and
    /// new ones too (it becomes MAIN).
    fn play_all(&mut self, scene: &mut Scene, at: (usize, usize)) -> Option<Action> {
        let s = self.strip(at)?;
        let want = if s.role == Role::Sink { Role::Play } else { Role::Rec };
        let (mut add, mut del) = (Vec::new(), Vec::new());
        for a in self.modules.iter().flat_map(|m| &m.strips).filter(|a| a.role == want && a.targets.iter().any(|t| t.lit)) {
            let pair = |t: (usize, usize)| if want == Role::Play { (a.at, t) } else { (t, a.at) };
            add.push(pair(at));
            del.extend(a.targets.iter().filter(|t| t.lit && t.at != at).map(|t| pair(t.at)));
        }
        let name = s.node_name.clone();
        scene.reroute(&add, &del);
        if let Some(n) = name {
            scene.set_default(want == Role::Play, &n);
        }
        Some(Action::Changed)
    }

    /// A fader moved to `v` (1 is 100%): the volume, or one channel's.
    fn set_fader(&self, scene: &mut Scene, at: (usize, usize), ch: Option<usize>, v: f64) {
        match ch {
            None => scene.set_volume(at, v),
            Some(c) => {
                let Some((vol, bal, _)) = scene.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
                let mut lr = channels((vol, bal));
                lr[c.min(1)] = v;
                scene.set_channels(at, lr[0], lr[1]);
            }
        }
    }

    /// The pointer moved with a press held: a fader, or the console panned.
    pub fn drag(&mut self, scene: &mut Scene, p: (f64, f64), size: (u32, u32), dpi: f64) -> bool {
        match self.drag {
            Some(Drag::Fader { at, ch, top }) => {
                let (_, y) = self.at(p, dpi);
                self.set_fader(scene, at, ch, vol_at(y - top));
                true
            }
            Some(Drag::Pan { from, x, y }) => {
                (self.sx, self.sy) = (x - (p.0 / dpi - from.0), y - (p.1 / dpi - from.1));
                self.clamp(size, dpi);
                true
            }
            // (a key held: it stays down; the release says whether it was let go over it)
            Some(Drag::Key(_)) | None => false,
        }
    }

    /// The press let go at `p`: a key held down does what it does if it is let go over itself.
    pub fn release(&mut self, scene: &mut Scene, p: (f64, f64), dpi: f64) -> Option<Action> {
        match self.drag.take() {
            Some(Drag::Key(h)) if self.hit(p, dpi).as_ref() == Some(&h) => self.key_up(scene, h),
            _ => None,
        }
    }

    /// Lets go of what is held without doing anything (switching screens).
    pub fn let_go(&mut self) {
        self.drag = None;
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The wheel: over a fader it steps the volume (`clicks`, finer with Shift); elsewhere it
    /// scrolls the console (`d`: logical pixels, sideways first when it is wider than the window).
    pub fn wheel(&mut self, scene: &mut Scene, p: (f64, f64), Wheel { clicks, d, fine }: Wheel, size: (u32, u32), dpi: f64) -> bool {
        if let Some(Hit::Fader(at, ch, _)) = self.hit(p, dpi) {
            if clicks == 0 {
                return false;
            }
            let Some((vol, bal, _)) = scene.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return false };
            let now = match ch {
                None => vol,
                Some(c) => channels((vol, bal))[c.min(1)],
            };
            self.set_fader(scene, at, ch, now + clicks as f64 * if fine { 0.01 } else { 0.05 });
            return true;
        }
        let (w, top, bottom) = Mixer::field(size, dpi);
        let wide = self.size.0 > w;
        let (dx, dy) = if d.0 == 0.0 && wide && self.size.1 <= bottom - top { (d.1, 0.0) } else { d };
        (self.sx, self.sy) = (self.sx - dx, self.sy - dy);
        self.clamp(size, dpi);
        true
    }

    /// What the pointer is over, for the status bar.
    pub fn hover(&self, scene: &Scene, p: (f64, f64), dpi: f64) -> Option<String> {
        let name = |at: (usize, usize)| scene.devs.get(at.0).map(|d| format!("{} · {}", d.dev.name, face::name(&d.dev, at.1))).unwrap_or_default();
        Some(match self.hit(p, dpi)? {
            Hit::Graph(n) => format!("#{n}: its block in ENGINE"),
            Hit::Profile(_) => "The card's profile: which of its outputs and inputs it has".into(),
            Hit::Port(_) => "The card's port: where it plays or records from".into(),
            Hit::To(from, to) => {
                let lit = self.strip(from).is_some_and(|s| s.targets.iter().any(|t| t.at == to && t.lit));
                format!("{} {} {}. Click: {}", name(from), if lit { "goes to" } else { "doesn't go to" }, name(to), if lit { "not there any more" } else { "there too" })
            }
            Hit::Main(at) if at_role(self, at) == Some(Role::Sink) => format!("Make {} the master: everything plays here, new apps too", name(at)),
            Hit::Main(at) => format!("Make {} the default input: new apps record from here", name(at)),
            Hit::All(at) => format!("Every app to {}, and it becomes the default", name(at)),
            Hit::Cue(at) => format!("Listen to {} in the default output; nothing else changes", name(at)),
            Hit::Mute(at) => format!("Mute {}", name(at)),
            Hit::Chan(_) => "CHANNELS: a fader for L and one for R".into(),
            Hit::Fader(at, ..) => format!("{}: drag or wheel; double-click 100%", name(at)),
        })
    }

    /// The device whose PROFILE (`ports`: PORT) key is at `p`.
    pub fn list_key_at(&self, p: (f64, f64), ports: bool, dpi: f64) -> Option<usize> {
        match self.hit(p, dpi)? {
            Hit::Profile(di) if !ports => Some(di),
            Hit::Port(di) if ports => Some(di),
            _ => None,
        }
    }

    /// Over a key (a hand would say so; the system's arrow stays).
    pub fn over_key(&self, p: (f64, f64), dpi: f64) -> bool {
        self.hit(p, dpi).is_some()
    }

    /// The pictures still being drawn.
    pub fn settling(&self) -> bool {
        self.inbox.busy() || self.rows_want.is_some()
    }

    /// The GPU has these: keep them while another screen shows.
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.pics.values().filter_map(|p| p.layer.as_ref().map(|l| l.id))
    }

    /// Frees the modules' pixels the GPU has: their memory back.
    pub fn uploaded(&mut self, has: impl Fn(u64) -> bool) -> usize {
        let mut freed = 0;
        for l in self.pics.values_mut().filter_map(|p| p.layer.as_mut()) {
            if has(l.id) {
                freed += l.pm.take().map_or(0, |pm| pm.data().len());
            }
        }
        freed
    }

    /// Everything to draw, in window pixels.
    pub fn sprites(&mut self, scene: &Scene, pins: &[String], size: (u32, u32), dpi: f64) -> Vec<Pic<'_>> {
        // (scrolled first: MASTER's place follows the scroll)
        self.clamp(size, dpi);
        self.layout(scene, pins, size, dpi);
        self.clamp(size, dpi);
        let rows = self.rows;
        let mod_h = rows.mod_h();
        for done in self.inbox.take() {
            for p in self.pics.values_mut() {
                if let Some((sig, w, h)) = p.want
                    && sig == done.sig
                {
                    p.layer = Some(Layer { id: next_id(), pm: done.pm, x: -FACE_PAD, y: -FACE_PAD, w: w + 2.0 * FACE_PAD, h: h + 2.0 * FACE_PAD });
                    p.sig = sig;
                    p.want = None;
                    break;
                }
            }
        }
        // the modules' pictures: drawn again when what they show changes
        let keys: HashSet<&str> = self.modules.iter().map(|m| m.key.as_str()).collect();
        self.pics.retain(|k, _| keys.contains(k.as_str()));
        let mut prints = Vec::new();
        for m in &self.modules {
            let (skin, print) = face::paint(m.finish, m.w, mod_h, RADIUS);
            prints.push(print);
            let svg = module_svg(m, print, &rows);
            let sig = painter::sig(&svg, dpi);
            let p = self.pics.entry(m.key.clone()).or_insert(ModPic { sig: 0, layer: None, want: None });
            // (one of a new height too goes to the painter: the old one shows meanwhile; drawing every
            // module here took 200 ms a frame)
            if p.layer.is_none() {
                // the first one at once: nothing to show meanwhile
                let pm = face::raster(&svg_options(), &svg, &skin, dpi);
                p.layer = Some(Layer { id: next_id(), pm, x: -FACE_PAD, y: -FACE_PAD, w: m.w + 2.0 * FACE_PAD, h: mod_h + 2.0 * FACE_PAD });
                p.sig = sig;
            } else if p.sig != sig && p.want.is_none_or(|w| w.0 != sig) {
                self.inbox.ask_skinned(painter::key("mixer", &m.key), sig, svg, dpi, Some(skin));
                p.want = Some((sig, m.w, mod_h));
            }
        }
        // the small ones: the caps, the lit LEDs, the groups' names, the grooves
        let scale = dpi.to_bits();
        if self.deco.as_ref().is_none_or(|(s, _)| *s != scale) {
            let groove = r##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="1" height="1" fill="#000" fill-opacity=".6"/><rect x="1" width="1" height="1" fill="#fff" fill-opacity=".08"/></svg>"##;
            let back = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="#1b1c1e"/></svg>"##;
            let shade = r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="1"><defs><linearGradient id="g"><stop offset="0" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".45"/></linearGradient></defs><rect width="14" height="1" fill="url(#g)"/></svg>"##;
            self.deco = Some((scale, [small(groove, 2.0, 1.0, dpi), small(back, 1.0, 1.0, dpi), small(shade, 14.0, 1.0, dpi)]));
        }
        let mut wanted_leds = Vec::new();
        let mut wanted_caps = Vec::new();
        let mut wanted_texts: Vec<(String, &'static str, u64)> = Vec::new();
        for s in self.modules.iter().flat_map(|m| &m.strips) {
            let l = s.level.map(lit);
            wanted_leds.push((s.cols, l[0], if s.cols == 2 { l[1] } else { 0 }, scale));
            wanted_caps.extend(cap_values(s).into_iter().map(|v| (v, scale)));
        }
        for k in wanted_leds {
            self.leds.entry(k).or_insert_with(|| small(&leds_svg(k.0, k.1, k.2), k.0 as f64 * (LED_W + 2.0) + 2.0, LEDS_H, dpi));
        }
        for k in wanted_caps {
            self.caps.entry(k).or_insert_with(|| small(&cap_svg(k.0), CAP.0 + 1.0, CAP.1 + 2.0, dpi));
        }
        for sec in [Section::Inputs, Section::Apps, Section::Outputs, Section::Master] {
            wanted_texts.push((sec.name().to_owned(), LABEL, scale));
        }
        for k in wanted_texts {
            if !self.texts.contains_key(&k) {
                let w = SECTION.width(&k.0) + 8.0;
                let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="14" font-family="Noto Sans" font-weight="bold">{}</svg>"#, label_text(&k.0, k.1));
                self.texts.insert(k.clone(), small(&svg, w, 14.0, dpi));
            }
        }
        // (not to pile up: only those wanted now)
        if self.leds.len() > 400 {
            self.leds.clear();
        }
        if self.texts.len() > 400 {
            self.texts.clear();
        }
        if self.caps.len() > 400 {
            self.caps.clear();
        }
        let (fw, top, bottom) = Mixer::field(size, dpi);
        let clip = Some([0.0, top * dpi, fw * dpi, (bottom - top) * dpi]);
        let (ox, oy) = (-self.sx, panel::H - self.sy);
        let mut out: Vec<Pic> = Vec::new();
        let px = |x: f64| (x * dpi).round();
        let Some((_, [groove, back, shade])) = &self.deco else { return out };
        let mut named = HashSet::new();
        for m in &self.modules {
            // before MASTER: the grooves; if it is held at the edge, what it covers goes under it
            if m.section == Section::Master {
                for (gx, master) in &self.grooves {
                    if !(*master && m.pinned) {
                        out.push((groove, px(ox + gx - 1.0), px(oy + TOP), 2.0 * dpi, mod_h * dpi, clip, 1.0));
                    }
                }
                if m.pinned {
                    out.push((shade, px(ox + m.x - GAP - 14.0), px(oy), 14.0 * dpi, self.size.1 * dpi, clip, 1.0));
                    out.push((back, px(ox + m.x - GAP), px(oy), (m.w + GAP + LEFT) * dpi, self.size.1 * dpi, clip, 1.0));
                }
            }
            // a group's name over its first module
            if named.insert(m.section)
                && let Some(l) = self.texts.get(&(m.section.name().to_owned(), LABEL, scale))
            {
                out.push((l, px(ox + m.x + 2.0), px(oy + 6.0), l.w * dpi, l.h * dpi, clip, 1.0));
            }
            if let Some(l) = self.pics.get(&m.key).and_then(|p| p.layer.as_ref()) {
                out.push((l, px(ox + m.x + l.x), px(oy + TOP + l.y), l.w * dpi, l.h * dpi, clip, 1.0));
            }
            // the caps (their volume on them) and the lit LEDs, over the module
            let sy = TOP + HEAD;
            for s in &m.strips {
                let x0 = m.x + s.x;
                let top = sy + rows.fader_y();
                if let Some((vol, bal, _)) = s.mix {
                    let lr = channels((vol, bal));
                    for (i, cx) in slots_x(s).iter().enumerate() {
                        let v = if s.chans { lr[i] } else { vol };
                        if let Some(l) = self.caps.get(&(pct(v), scale)) {
                            out.push((l, px(ox + x0 + cx - CAP.0 / 2.0), px(oy + top + cap_y(v) - CAP.1 / 2.0), l.w * dpi, l.h * dpi, clip, 1.0));
                        }
                    }
                }
                let l = s.level.map(lit);
                if let Some(layer) = self.leds.get(&(s.cols, l[0], if s.cols == 2 { l[1] } else { 0 }, scale)) {
                    out.push((layer, px(ox + x0 + meter_x(s)), px(oy + top + cap_y(VOL_MAX)), layer.w * dpi, layer.h * dpi, clip, 1.0));
                }
            }
        }
        // no MASTER (no default output): the grooves all the same
        if !self.modules.iter().any(|m| m.section == Section::Master) {
            for (gx, _) in &self.grooves {
                out.push((groove, px(ox + gx - 1.0), px(oy + TOP), 2.0 * dpi, mod_h * dpi, clip, 1.0));
            }
        }
        out
    }

    /// For a picture without a window: everything drawn, waiting for the painter.
    pub fn finish(&mut self, scene: &Scene, size: (u32, u32), dpi: f64) {
        let _ = self.sprites(scene, &[], size, dpi);
        while self.inbox.busy() {
            let Some(done) = self.inbox.wait(Duration::from_secs(5)) else { break };
            for p in self.pics.values_mut() {
                if let Some((sig, w, h)) = p.want
                    && sig == done.sig
                {
                    p.layer = Some(Layer { id: next_id(), pm: done.pm, x: -FACE_PAD, y: -FACE_PAD, w: w + 2.0 * FACE_PAD, h: h + 2.0 * FACE_PAD });
                    p.sig = sig;
                    p.want = None;
                    break;
                }
            }
        }
    }

    /// The modules now (for tests).
    #[cfg(test)]
    pub fn modules(&mut self, scene: &Scene) -> &[Module] {
        self.layout(scene, &[], (1440, 900), 1.0);
        &self.modules
    }
}

/// A small picture made here (the cap, the lit LEDs, a number), `w`×`h` logical pixels.
fn small(svg: &str, w: f64, h: f64, dpi: f64) -> Layer {
    Layer { id: next_id(), pm: rasterize(&svg_options(), svg, dpi).or_else(|| Pixmap::new(1, 1)), x: 0.0, y: 0.0, w, h }
}

/// A volume as the % on its cap.
fn pct(v: f64) -> u32 {
    (v * 100.0).round().max(0.0) as u32
}

/// The volumes on a strip's caps (%): one, or L and R.
fn cap_values(s: &Strip) -> Vec<u32> {
    match s.mix {
        None => Vec::new(),
        Some((v, b, _)) if s.chans => channels((v, b)).iter().map(|v| pct(*v)).collect(),
        Some((v, ..)) => vec![pct(v)],
    }
}

/// A group's name over the console.
const LABEL: &str = "#8a8d91";

/// A number under a fader, in its module's print (red past 100%), or a group's name.
fn label_text(t: &str, colour: &str) -> String {
    let red = t.split(' ').any(|p| p.trim_end_matches('%').parse::<f64>().is_ok_and(|v| v > 100.0));
    SECTION.text(4.0, 11.0, if red { "#ff4a32" } else { colour }, "start", t)
}

/// A fader's cap with its volume on it (red past 100%), and its shadow.
fn cap_svg(pct: u32) -> String {
    let (w, h) = CAP;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5a5b5f"/><stop offset=".45" stop-color="#2b2c2f"/><stop offset=".55" stop-color="#1b1c1e"/><stop offset="1" stop-color="#3c3d41"/></linearGradient></defs><rect x="1" y="2" width="{w}" height="{h}" rx="3" fill="#000" opacity=".5"/><rect x=".5" y=".5" width="{}" height="{}" rx="3" fill="url(#g)" stroke="#0b0b0c"/><text x="{}" y="14.5" font-family="Noto Sans Mono" font-weight="bold" font-size="8.5" fill="{}" text-anchor="middle">{pct}</text></svg>"##,
        w + 1.0,
        h + 2.0,
        w - 1.0,
        h - 1.0,
        w / 2.0,
        if pct > 100 { "#ff6a52" } else { "#e6e7e8" }
    )
}

/// The lit LEDs of a meter (`cols` columns, `l` and `r` of `LEDS` lit from the bottom): green, then
/// amber, then red, as on the rack's faces; thin.
fn leds_svg(cols: usize, l: u8, r: u8) -> String {
    let w = cols as f64 * (LED_W + 2.0) + 2.0;
    let mut s = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{LEDS_H}">"#);
    let step = (LEDS_H - 6.0) / LEDS as f64;
    for c in 0..cols {
        let n = if c == 0 { l } else { r };
        for i in 0..(n as usize).min(LEDS) {
            let f = i as f64 / LEDS as f64;
            let colour = if f >= 0.9 { "#ff3b2f" } else if f >= 0.7 { "#ffb02e" } else { "#4cff6a" };
            let y = LEDS_H - 3.0 - (i as f64 + 1.0) * step + 1.0;
            let _ = write!(s, r#"<rect x="{}" y="{y}" width="{LED_W}" height="{}" rx="1" fill="{colour}"/>"#, 3.0 + c as f64 * (LED_W + 2.0), step - 2.0);
        }
    }
    s.push_str("</svg>");
    s
}

/// A key's LED at its left, as the panel's (`panel::led_svg`): lit, a glow round it; off, dark in
/// its colour.
fn led(o: &mut String, cx: f64, cy: f64, on: bool, colour: &str) {
    if on {
        let _ = write!(o, r#"<circle cx="{cx}" cy="{cy}" r="5" fill="{colour}" opacity=".35"/><circle cx="{cx}" cy="{cy}" r="2.6" fill="{colour}"/>"#);
    } else {
        let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="2.6" fill="#0a0a0b"/><circle cx="{cx}" cy="{cy}" r="2.6" fill="{colour}" opacity=".2"/>"##);
    }
}

/// A key as the panel's (`panel::cap_svg`), its name at the left (after its LED, if it has one: then
/// it is in while it is on, deeper while `held`), a value at the right (PORT).
fn key(o: &mut String, r @ [x, y, w, h]: [f64; 4], label: &str, led_on: Option<(&str, bool)>, value: Option<&str>, held: bool) {
    o.push_str(&panel::cap_svg(r, led_on.is_some_and(|(_, on)| on) || held, held));
    let ty = y + h / 2.0 + 3.2;
    let lx = if led_on.is_some() { x + 15.0 } else { x + 8.0 };
    if let Some((colour, on)) = led_on {
        led(o, x + 8.0, y + h / 2.0, on, colour);
    }
    o.push_str(&SMALL.text(lx, ty, "#e6e7e8", "start", label));
    if let Some(v) = value {
        let room = w - 16.0 - (lx - x) - SMALL.width(label) - 6.0;
        o.push_str(&SMALL.text(x + w - 8.0, ty, "#9a9da1", "end", &fit(v, SMALL, room)));
    }
}

/// A push-push key (PLAY ON, RECORD FROM): in, pressed into its hole, its red LED lit; out, up;
/// held down, deeper.
fn push_key(o: &mut String, r @ [x, y, w, h]: [f64; 4], label: &str, on: bool, held: bool) {
    o.push_str(&panel::cap_svg(r, on || held, held));
    led(o, x + 8.0, y + h / 2.0, on, "#ff4a32");
    o.push_str(&SMALL.text(x + 15.0, y + h / 2.0 + 3.1, if on { "#f3efe6" } else { "#c9cbcd" }, "start", &fit(label, SMALL, w - 22.0)));
}

/// A key with its LED and its name in the middle (CUE, MUTE, SPLIT): in while it is on, deeper while
/// held; greyed where it does nothing.
fn led_key(o: &mut String, r @ [x, y, w, h]: [f64; 4], label: &str, colour: &str, on: bool, off: bool, held: bool) {
    o.push_str(if off { r#"<g opacity=".35">"# } else { "<g>" });
    o.push_str(&panel::cap_svg(r, on || held, held));
    let tw = SMALL.width(label) + 11.0;
    let lx = x + (w - tw) / 2.0;
    led(o, lx + 3.0, y + h / 2.0, on, colour);
    o.push_str(&SMALL.text(lx + 11.0, y + h / 2.0 + 3.2, "#e6e7e8", "start", label));
    o.push_str("</g>");
}

/// A module: its plate's outline, its heading (a card's name and PROFILE side by side), its strips
/// (all but the caps, the lit LEDs and the numbers).
fn module_svg(m: &Module, print: &str, rows: &Rows) -> String {
    let (w, h) = (m.w, rows.mod_h());
    let (dw, dh) = (w + 2.0 * FACE_PAD, h + 2.0 * FACE_PAD);
    let mut o = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}" font-family="Noto Sans" font-weight="bold"><defs>{}</defs><g transform="translate({FACE_PAD},{FACE_PAD})">"##, panel::KEY_DEFS);
    let _ = write!(o, r##"<rect x=".5" y=".5" width="{}" height="{}" rx="{RADIUS}" fill="none" stroke="#000" stroke-opacity=".7"/>"##, w - 1.0, h - 1.0);
    // the heading: whose it is
    o.push_str(&SECTION.text(w / 2.0, 22.0, print, "middle", &fit(&m.name, SECTION, w - 20.0)));
    // DEVICE SETUP, a group of its own under the first: the card's PROFILE and the PORT, each with
    // what is in use at its right
    let has_port = m.strips.iter().any(|s| s.port.is_some());
    if m.profile.is_some() || has_port {
        o.push_str(&SMALL.text(w / 2.0, HEAD + setup_y() + 8.0, print, "middle", "DEVICE SETUP"));
    }
    if let Some(p) = &m.profile {
        let k = profile_key(w);
        key(&mut o, k, "PROFILE", None, Some(p), false);
    }
    let below = usize::from(m.profile.is_some());
    let sy = HEAD;
    for (i, s) in m.strips.iter().enumerate() {
        let x0 = s.x;
        let held = |what: fn(&Hit) -> bool| s.held.as_ref().is_some_and(what);
        if i > 0 {
            let _ = write!(o, r#"<path d="M{x0},{sy} V{}" stroke="{print}" stroke-opacity=".35" stroke-dasharray="2 3"/>"#, h - 8.0);
        }
        // the band: its block's kind, #id, the name
        let (kc, ink) = (face::kind_colour(s.kind), face::band_ink(s.kind));
        let _ = write!(o, r#"<path d="M{},{} V{} a5,5 0 0 1 5,-5 H{} a5,5 0 0 1 5,5 V{} Z" fill="{kc}"/>"#, x0 + 2.0, sy + BAND, sy + 5.0, x0 + s.w - 7.0, sy + BAND);
        let id = format!("#{}", s.node);
        let gw = 10.0 + SMALL.width(&id);
        let _ = write!(o, r#"<rect x="{}" y="{}" width="{gw}" height="14" rx="2.5" fill="none" stroke="{ink}" stroke-width="1.1"/>"#, x0 + 8.0, sy + 5.0);
        o.push_str(&SMALL.text(x0 + 8.0 + gw / 2.0, sy + 15.0, ink, "middle", &id));
        o.push_str(&SECTION.text(x0 + 8.0 + gw + 6.0, sy + 16.0, ink, "start", &fit(&s.name, SECTION, s.w - gw - 26.0)));
        // where it goes, or the device's keys
        let (kx, kw) = (x0 + 6.0, s.w - 12.0);
        let ry = sy + ROUTE_Y;
        // every group of keys under its title, its keys in rows: where an app goes, or the
        // device's own keys; then the channel itself
        let title = match s.role {
            Role::Play => "PLAY ON",
            Role::Rec => "RECORD FROM",
            Role::Sink => "OUTPUT",
            Role::Source => "INPUT",
        };
        o.push_str(&SMALL.text(x0 + s.w / 2.0, ry + 8.0, print, "middle", title));
        match s.role {
            Role::Play | Role::Rec => {
                for (i, t) in s.targets.iter().enumerate().take(6) {
                    let down = s.held.as_ref().is_some_and(|h| matches!(h, Hit::To(_, to) if *to == t.at));
                    push_key(&mut o, [kx, sy + row(i), kw, KEY_H], &t.label, t.lit, down);
                }
                if s.targets.is_empty() {
                    o.push_str(&SMALL.text(x0 + s.w / 2.0, sy + row(0) + 12.0, print, "middle", "NOWHERE"));
                }
            }
            Role::Sink | Role::Source => {
                if s.mix.is_some() && s.node_name.is_some() {
                    let out = s.role == Role::Sink;
                    key(&mut o, [kx, sy + row(0), kw, KEY_H], if out { "MASTER" } else { "MAIN IN" }, Some(("#ffb02e", s.main)), None, held(|h| matches!(h, Hit::Main(_))));
                    key(&mut o, [kx, sy + row(1), kw, KEY_H], if out { "PLAY ALL HERE" } else { "RECORD ALL HERE" }, None, None, false);
                }
                if let Some(p) = &s.port {
                    key(&mut o, [kx, sy + setup_row(below), kw, KEY_H], "PORT", None, Some(p), false);
                }
            }
        }
        o.push_str(&SMALL.text(x0 + s.w / 2.0, sy + rows.keys_y - 9.0, print, "middle", "CHANNEL"));
        let half = (s.w - 12.0 - 7.0) / 2.0;
        led_key(&mut o, [kx, sy + rows.keys_y, half, 24.0], "CUE", "#5fd4ff", s.cue, false, held(|h| matches!(h, Hit::Cue(_))));
        let muted = s.mix.is_some_and(|m| m.2);
        led_key(&mut o, [kx + half + 7.0, sy + rows.keys_y, half, 24.0], "MUTE", "#ff4a32", muted, s.mix.is_none(), held(|h| matches!(h, Hit::Mute(_))));
        led_key(&mut o, [kx, sy + rows.chan_y, kw, 24.0], "SPLIT", "#ffb02e", s.chans, !s.stereo || s.mix.is_none(), held(|h| matches!(h, Hit::Chan(_))));
        // the fader block under SPLIT: the scale's numbers at the left, lines left of each slot
        // (long at the numbers), the slot red past 100% (the caps, their volume on them, are
        // pictures over it); the meter's dark LEDs at the right, as tall as the slot and level with it; its name under it all
        let top = sy + rows.fader_y();
        let slots = slots_x(s);
        if s.mix.is_some() {
            for m in [150.0, 100.0, 75.0, 50.0, 25.0, 0.0] {
                o.push_str(&SMALL.text(x0 + if s.chans { 17.0 } else { 30.0 }, top + cap_y(m / 100.0) + 2.6, print, "end", &format!("{m}")));
            }
            for (i, c) in slots.iter().enumerate() {
                let cx = x0 + c;
                if s.chans {
                    o.push_str(&SMALL.text(cx, top - 4.0, print, "middle", if i == 0 { "L" } else { "R" }));
                }
                // (on the pixel grid: each line one sharp pixel)
                for k in 0..=12 {
                    let v = k as f64 * 12.5;
                    let long = [0.0, 25.0, 50.0, 75.0, 100.0, 150.0].contains(&v);
                    let _ = write!(o, r#"<path d="M{},{} h{}" stroke="{print}" stroke-opacity=".6"/>"#, cx - 12.0, (top + cap_y(v / 100.0)).round() + 0.5, if long { 7.0 } else { 4.0 });
                }
                let _ = write!(o, r##"<rect x="{}" y="{}" width="4" height="{TRAVEL}" rx="2" fill="#0b0b0c"/><rect x="{}" y="{}" width="4" height="{}" rx="2" fill="#b4483c"/>"##, cx - 2.0, top + 6.0, cx - 2.0, top + cap_y(VOL_MAX), cap_y(1.0) - cap_y(VOL_MAX));
            }
        } else {
            let _ = write!(o, r#"<text transform="translate({},{}) rotate(-90)" font-size="7.5" letter-spacing="1" fill="{print}" fill-opacity=".6" text-anchor="middle">NO VOLUME IN PIPEWIRE</text>"#, x0 + 48.0, top + TRAVEL / 2.0 + 6.0);
        }
        let (mx, my) = (x0 + meter_x(s), top + cap_y(VOL_MAX));
        let _ = write!(o, r##"<rect x="{mx}" y="{my}" width="{}" height="{LEDS_H}" rx="3" fill="#0b0b0c"/>"##, s.cols as f64 * (LED_W + 2.0) + 2.0);
        let step = (LEDS_H - 6.0) / LEDS as f64;
        for c in 0..s.cols {
            for i in 0..LEDS {
                let f = i as f64 / LEDS as f64;
                let y = my + LEDS_H - 3.0 - (i as f64 + 1.0) * step + 1.0;
                let dark = if f >= 0.9 { "#351612" } else if f >= 0.7 { "#2f2810" } else { "#1c2a1f" };
                let _ = write!(o, r#"<rect x="{}" y="{y}" width="{LED_W}" height="{}" rx="1" fill="{dark}"/>"#, mx + 3.0 + c as f64 * (LED_W + 2.0), step - 2.0);
            }
        }
        // named as on the rack: VOLUME on what plays, GAIN on what comes in
        let name = if matches!(s.role, Role::Source | Role::Rec) { "GAIN" } else { "VOLUME" };
        o.push_str(&SMALL.text(x0 + s.w / 2.0, sy + rows.level_y(), print, "middle", name));
    }
    o.push_str("</g></svg>");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fader_goes_from_silence_to_its_top() {
        assert!((vol_at(cap_y(0.0)) - 0.0).abs() < 1e-9);
        assert!((vol_at(cap_y(VOL_MAX)) - VOL_MAX).abs() < 1e-9);
        // near 100% it rests there
        assert_eq!(vol_at(cap_y(1.02)), 1.0);
        // L and R from a balance, and back the way the scene works it out
        assert_eq!(channels((1.0, 0.0)), [1.0, 1.0]);
        let [l, r] = channels((0.8, 0.5));
        assert!((l - 0.4).abs() < 1e-9 && (r - 0.8).abs() < 1e-9);
    }

    #[test]
    fn a_long_name_is_cut_to_fit() {
        let t = fit("ALSA PLUG-IN [SOUNDCRAFT]", SECTION, 80.0);
        assert!(t.ends_with('…') && SECTION.width(&t) <= 80.0, "{t}");
        assert_eq!(fit("HYDROGEN", SECTION, 200.0), "HYDROGEN");
    }
}
