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

use super::face::{self, BalKnob, FACE_PAD};
use super::painter;
use super::panel;
use super::print::{SECTION, SMALL};
use super::scene::{Layer, Scene, next_id, rasterize};
use super::text::{esc, svg_options};
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
/// outputs (the default one too).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Section {
    Inputs,
    Apps,
    Outputs,
}

impl Section {
    fn name(self) -> &'static str {
        match self {
            Section::Inputs => "INPUTS",
            Section::Apps => "APPS",
            Section::Outputs => "OUTPUTS",
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
    /// a device's PORT: its card, and the group (its input or its output) whose ports it lists
    Port((usize, usize)),
    /// an app's key to a sink (source)
    To((usize, usize), (usize, usize)),
    Main((usize, usize)),
    All((usize, usize)),
    Cue((usize, usize)),
    Mute((usize, usize)),
    Chan((usize, usize)),
    /// a fader's slot: which channel (none: the fader for all), the slot's top
    Fader((usize, usize), Option<usize>, f64),
    /// the knobs, when the window is too low for the faders: VOLUME (GAIN), BAL
    Knob((usize, usize)),
    Bal((usize, usize)),
}

/// What MIXER asks the window to do.
pub enum Action {
    /// ENGINE, that node chosen
    Graph(u32),
    /// a card's PROFILE or PORT list, under the key at this rectangle (window pixels)
    Profile(usize, [f64; 4]),
    Port(usize, usize, [f64; 4]),
    /// listen to a group (its jacks' keys)
    Cue(Vec<String>),
    /// something changed in PipeWire: send it
    Changed,
}

enum Drag {
    Fader { at: (usize, usize), ch: Option<usize>, top: f64 },
    /// the VOLUME knob turned: up is louder (`raw`: where it would be, before it rests at 100%)
    Knob { at: (usize, usize), last: f64, raw: f64 },
    /// the BAL knob pressed: it turns once the pointer moves (if it was out), else the release pops
    /// it out or pushes it in, as the rack's
    Bal { at: (usize, usize), y0: f64, last: f64, raw: f64, was: BalKnob, turned: bool },
    /// a key with a LED held down: it does what it does when let go over it
    Key(Hit),
    Pan { from: (f64, f64), x: f64, y: f64 },
    /// a scroll bar's thumb carried: down (or along) it, from where it was, the scroll then
    Bar { down: bool, from: f64, s0: f64 },
}

// the console's measures, in logical pixels
const LEFT: f64 = 14.0;
const TOP: f64 = 26.0;
const BOTTOM: f64 = 14.0;
/// the gap between two modules of a group
const GAP: f64 = 3.0;
/// A scroll bar's lane, at the window's right edge or bottom, only while the console goes on
/// beyond the window that way: the console stands back from it, nothing under the bar
const BAR: f64 = 10.0;
/// between two groups: wider, a groove down its middle
const SEP: f64 = 22.0;
/// over the strip's frame: nothing, the frame is the module's edge
const HEAD: f64 = 0.0;
/// a module is one strip, of one width
const STRIP_W: f64 = 112.0;
/// the frame's band: two lines on it, the device's name and the frame's
const BAND: f64 = 38.0;
/// GRAPH (`#id`, opens ENGINE): on the band's second line, at its right
const GRAPH_Y: f64 = 20.0;
const GRAPH_H: f64 = 13.0;
/// the first group of keys (OUTPUT, INPUT, PLAY ON, RECORD FROM): its title, then its keys in rows
const ROUTE_Y: f64 = BAND + 7.0;
const KEY_H: f64 = 17.0;
/// Between two keys side by side: the hole's dark between them.
const KEY_GAP: f64 = 2.0;
fn row(i: usize) -> f64 {
    ROUTE_Y + 17.0 + i as f64 * 23.0
}
const TRAVEL: f64 = 160.0;
/// The window's height, for the modules' (each new one draws every module again): the last height
/// once the window has kept still this long.
const ROWS_DELAY: Duration = Duration::from_millis(150);
const RADIUS: f64 = 8.0;

/// How far a knob turns for a pointer's move (logical pixels for its whole way), as the rack's.
const KNOB_TRAVEL: f64 = 200.0;

/// Where the lower part of a strip is, from the strip's top: the VOLUME (GAIN) block, its name
/// at `keys_y - 9`. The modules are all as tall as the tallest (the most keys), not the window: the
/// block right under the lowest key of any module, no empty room over it. Under the block's name,
/// the faders while the window has room for them (`TAIL_FADERS`), else the rack's knobs
/// (`TAIL_KNOBS`); lower still, the console scrolls.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rows {
    keys_y: f64,
    strip_h: f64,
    knobs: bool,
}

const TAIL_FADERS: f64 = 228.0;
const TAIL_KNOBS: f64 = 80.0;

impl Rows {
    /// For a field `h` tall, the lowest key's bottom on any module at `need`.
    fn fit(h: f64, need: f64) -> Rows {
        // (the block's name as far under the lowest key as a group's title under the keys over it)
        let keys = need + 27.0;
        let room = (h - TOP - BOTTOM - HEAD).floor();
        let knobs = room < keys + TAIL_FADERS;
        let tail = if knobs { TAIL_KNOBS } else { TAIL_FADERS };
        let keys_y = keys;
        Rows { keys_y, strip_h: keys_y + tail, knobs }
    }

    fn mod_h(&self) -> f64 {
        HEAD + self.strip_h
    }

    /// The faders, under the block's keys: where the caps go by (`cap_y` from it; the slot from 6
    /// under it).
    fn fader_y(&self) -> f64 {
        self.keys_y + 37.0
    }
}

impl Default for Rows {
    fn default() -> Self {
        Rows { keys_y: 200.0, strip_h: 200.0 + TAIL_FADERS, knobs: false }
    }
}

/// The lowest key's bottom on any module (from the strip's top): an app's every PLAY ON (RECORD
/// FROM) key (`sinks`, `sources` of them), a device's MAIN row and SETUP's LCDs.
fn lowest(mods: &[Module], sinks: usize, sources: usize) -> f64 {
    let mut b = row(0) + KEY_H;
    for m in mods {
        let setup = usize::from(m.profile.is_some()) + usize::from(m.strips.iter().any(|s| s.port.is_some()));
        if setup > 0 {
            b = b.max(setup_row(setup - 1) + LCD_H);
        }
        for s in &m.strips {
            let n = match s.role {
                Role::Play => sinks.max(1),
                Role::Rec => sources.max(1),
                _ => 1,
            };
            b = b.max(row(n - 1) + KEY_H);
        }
    }
    b
}

/// a fader's cap, its volume on it (and its shadow, a little lower)
const CAP: (f64, f64) = (34.0, 22.0);
/// the lit LEDs' column: as tall as the fader's slot, level with it; under the knobs, from the
/// block's name down to its keys' bottom
const LEDS_H: f64 = TRAVEL;
/// The meter beside the knobs: under the block's name (`KNOB_LEDS_Y` under its line; the name and
/// its rule go across the whole strip over it), its bottom level with the keys' (their shadow
/// under them): the keys at the name's line + 57, 17 high, their shadow 3 under them.
const KNOB_LEDS_Y: f64 = 6.0;
const KNOB_LEDS_H: f64 = 57.0 + KEY_H + 3.0 - KNOB_LEDS_Y;
/// the meter's LEDs: how many, how wide (thin, split or not)
const LEDS: usize = 12;
const LED_W: f64 = 4.0;

/// A meter's width for `cols` columns of LEDs: 3 of dark round them on every side, 2 between.
fn meter_w(cols: usize) -> f64 {
    cols as f64 * (LED_W + 2.0) + 4.0
}

/// From one LED to the next in a meter `h` high (each 2 shorter than this, 3 of dark at the top
/// and the bottom).
fn led_step(h: f64) -> f64 {
    (h - 4.0) / LEDS as f64
}

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

/// The strip a key is on (those go down while held and act when let go): PLAY ON and RECORD FROM,
/// MAIN, ALL HERE, CUE, MUTE, SPLIT.
fn key_of(h: &Hit) -> Option<(usize, usize)> {
    match h {
        Hit::To(at, _) | Hit::Main(at) | Hit::All(at) | Hit::Cue(at) | Hit::Mute(at) | Hit::Chan(at) => Some(*at),
        _ => None,
    }
}

/// A device's keys are under one title, SETUP: MAIN and ALL HERE in its first row (`row(0)`), each
/// in its own half (`halves`); then its card's PROFILE, its PORT, each a small LCD (`LCD_H` high)
/// with its name over it: where the `i`th LCD is, from the strip's top.
const LCD_H: f64 = 16.0;
fn setup_row(i: usize) -> f64 {
    row(0) + KEY_H + 6.0 + i as f64 * 30.0 + 9.0
}

/// MAIN's and ALL HERE's places in a row from `x`, `w` wide.
fn halves(x: f64, w: f64) -> [[f64; 2]; 2] {
    let hw = (w - 5.0) / 2.0;
    [[x, hw], [x + (w + 5.0) / 2.0, hw]]
}

/// A card's PROFILE LCD, first under MAIN, in a module `w` wide.
fn profile_key(w: f64) -> [f64; 4] {
    [12.0, HEAD + setup_row(0), w - 24.0, LCD_H]
}

/// The VOLUME (GAIN) block's keys in one row from `x`, `w` wide: CUE, MUTE, SPLIT (`split`; the
/// default output has no CUE). Every key has its own place, always the same, the places
/// equal; a key that would do nothing (no volume in PipeWire: no MUTE; mono: no SPLIT) isn't there,
/// its place empty.
fn row_keys(s: &Strip, x: f64, y: f64, w: f64, split: bool) -> Vec<(Hit, [f64; 4])> {
    let mut hs = Vec::new();
    // (no CUE on what CUE plays in, the default output: it would feed back, it can't go on)
    if !(s.role == Role::Sink && s.main) {
        hs.push(Hit::Cue(s.at));
    }
    if s.mix.is_some() {
        hs.push(Hit::Mute(s.at));
        if split && s.stereo {
            hs.push(Hit::Chan(s.at));
        }
    }
    let n = if split { 3.0 } else { 2.0 };
    // side by side in a block, a thin line of the hole between them
    let kw = (w - (n - 1.0) * KEY_GAP) / n;
    let place = |h: &Hit| match h {
        Hit::Cue(_) => 0.0,
        Hit::Mute(..) => 1.0,
        _ => 2.0,
    };
    hs.into_iter().map(|h| { let px = x + place(&h) * (kw + KEY_GAP); (h, [px, y, kw, KEY_H]) }).collect()
}

/// The block when the window is too low for the faders, as the rack's: its name on top (· BAL on a
/// stereo strip), the VOLUME (GAIN) knob, the BAL knob, CUE and MUTE small under them, the meter at
/// the right under the name; no SPLIT (BAL instead).
struct KnobGeo {
    /// the meter's left
    mx: f64,
    /// the name's line
    t: f64,
    vol: (f64, f64),
    bal: Option<(f64, f64)>,
    keys: Vec<(Hit, [f64; 4])>,
}

fn knob_geo(s: &Strip, x0: f64, sy: f64, rows: &Rows) -> KnobGeo {
    let mw = meter_w(s.cols);
    let mx = x0 + s.w - 8.0 - mw;
    let t = sy + rows.keys_y - 9.0;
    let kx = x0 + 6.0;
    let bal = s.stereo && s.mix.is_some();
    KnobGeo {
        mx,
        t,
        vol: (if bal { x0 + 30.0 } else { (kx + mx - 5.0) / 2.0 }, t + 29.0),
        bal: bal.then_some((x0 + 68.0, t + 25.0)),
        keys: row_keys(s, kx, t + 57.0, mx - 5.0 - kx, false),
    }
}

/// The block's name, as on the rack: VOLUME on what plays, GAIN on what comes in.
fn block_name(s: &Strip) -> &'static str {
    if matches!(s.role, Role::Source | Role::Rec) { "GAIN" } else { "VOLUME" }
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
    /// the window's (pixels), as last drawn
    win: (u32, u32),
    /// the scroll bars' lanes taken from the field: down the right (the console taller than the
    /// window), along the bottom (wider)
    lanes: (bool, bool),
    /// stereo strips showing a fader for each channel: device key, group
    chans: HashSet<(String, usize)>,
    modules: Vec<Module>,
    /// the grooves between the groups: their x
    grooves: Vec<f64>,
    rows: Rows,
    /// a new height the window asks for, and since when (see `ROWS_DELAY`)
    rows_want: Option<(Rows, Instant)>,
    hits: Vec<(Hit, [f64; 4])>,
    pics: HashMap<String, ModPic>,
    inbox: painter::Inbox,
    /// the cap, the lit LEDs (by columns and how many lit), the numbers, the sections' names
    /// the faders' caps, by the volume on them (%)
    caps: HashMap<(u32, u64), Layer>,
    leds: HashMap<(usize, u8, u8, bool, u64), Layer>,
    /// the knobs, by the volume (%), and BAL by its balance (%) and whether in, held or out
    knobs: HashMap<(u32, u64), Layer>,
    bals: HashMap<(i32, u8, u64), Layer>,
    texts: HashMap<(String, &'static str, u64), Layer>,
    /// a groove, the field's dark (the scroll bars' lanes)
    deco: Option<(u64, [Layer; 2])>,
    drag: Option<Drag>,
    last_click: Option<(Instant, Hit)>,
}

impl Mixer {
    pub fn new(on: bool) -> Self {
        Mixer { on, ..Mixer::default() }
    }

    /// The modules for what the scene shows now, and where everything is on the console (a field
    /// `size` pixels at `dpi`: the rows as tall as it lets them).
    fn layout(&mut self, scene: &Scene, pins: &[String], size: (u32, u32), dpi: f64) {
        let (fw, top, bottom) = Mixer::field(size, dpi);
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
                    // lit when any of it is heard: a jack of it, or a port of one (kept on in the
                    // rack, in ENGINE or here)
                    let cue = pins.iter().any(|p| scene.cue_jack(p).is_some_and(|j| jacks.iter().any(|k| k.as_str() == j)));
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
            let module = |key: String, section: Section, strips: Vec<Strip>, profile: Option<String>| Module { key, di, name: dev.name.to_uppercase(), finish: dev.finish, section, profile, strips, x: 0.0, w: STRIP_W + 12.0 };
            // an app's INPUT before its OUTPUT
            let mut order: Vec<Strip> = strips.iter().filter(|s| s.role.takes()).cloned().chain(strips.iter().filter(|s| !s.role.takes()).cloned()).collect();
            if order.is_empty() {
                // a card that is off: its name and PROFILE, among the outputs in its place by name
                // (nothing jumps when it goes on or off)
                if profile.is_some() {
                    mods.push((Section::Outputs, 1, 0, module(dev.key.clone(), Section::Outputs, Vec::new(), profile.clone())));
                }
                continue;
            }
            for (n, s) in order.drain(..).enumerate() {
                let (section, first) = match s.role {
                    Role::Source => (Section::Inputs, 1),
                    Role::Sink => (Section::Outputs, 1),
                    _ => (Section::Apps, 1),
                };
                let p = if profile_on == Some(s.at) { profile.clone() } else { None };
                mods.push((section, first, n, module(format!("{}#{}", dev.key, s.at.1), section, vec![s], p)));
            }
        }
        // a static order within each group: by name, A to Z (the default input too: nothing moves
        // when the default changes)
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
        // a new height (every module drawn again) once the window keeps still (`ROWS_DELAY`), as
        // the rack takes a new width; at once the first time
        let count = |role: Role| ends.iter().filter(|e| e.0 == role).count();
        let need = lowest(&mods, count(Role::Sink), count(Role::Source));
        // the scroll bars' lanes: along the bottom while the console is wider than the window, down
        // the right while taller (then narrower: wider, perhaps)
        let wide_at = |w: f64| {
            let (mut x, mut was) = (LEFT, None);
            for m in &mods {
                if was.is_some_and(|w| w != m.section) {
                    x += SEP - GAP;
                }
                was = Some(m.section);
                x += m.w + GAP;
            }
            x - GAP + LEFT > w + 0.5
        };
        let mut wide = wide_at(fw);
        let room = |wide: bool| bottom - top - if wide { BAR } else { 0.0 };
        let mut want = Rows::fit(room(wide), need);
        let tall = TOP + want.mod_h() + BOTTOM > room(wide) + 0.5;
        if tall && !wide && wide_at(fw - BAR) {
            wide = true;
            want = Rows::fit(room(true), need);
        }
        self.lanes = (tall, wide);
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
        // where everything is: a gap between modules, a groove between groups
        self.hits.clear();
        self.grooves.clear();
        let mut x = LEFT;
        let mut was = None;
        for m in &mut mods {
            if was.is_some_and(|w| w != m.section) {
                x += SEP - GAP;
                self.grooves.push(x - SEP / 2.0);
            }
            was = Some(m.section);
            m.x = x;
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
                let [[lx, lw], [rx, rw]] = halves(x0 + 6.0, s.w - 12.0);
                // the frame's name on the band: a click opens ENGINE with its block (as on the rack)
                let nw = SMALL.width(&s.name).min(s.w - 26.0);
                self.hits.push((Hit::Graph(s.node), [x0 + 7.0, sy + GRAPH_Y, nw + 6.0, GRAPH_H]));
                let (kx, kw) = (x0 + 6.0, s.w - 12.0);
                match s.role {
                    Role::Play | Role::Rec => {
                        for (i, t) in s.targets.iter().enumerate() {
                            self.hits.push((Hit::To(s.at, t.at), [kx, sy + row(i), kw, KEY_H]));
                        }
                    }
                    Role::Sink | Role::Source => {
                        if s.mix.is_some() && s.node_name.is_some() {
                            self.hits.push((Hit::Main(s.at), [lx, sy + row(0), lw, KEY_H]));
                            self.hits.push((Hit::All(s.at), [rx, sy + row(0), rw, KEY_H]));
                        }
                        if s.port.is_some() {
                            self.hits.push((Hit::Port(s.at), [kx, sy + setup_row(below), kw, LCD_H]));
                        }
                    }
                }
                // the VOLUME (GAIN) block: its keys, and the faders or the knobs
                if rows.knobs {
                    let g = knob_geo(s, x0, sy, &rows);
                    self.hits.extend(g.keys);
                    if s.mix.is_some() {
                        self.hits.push((Hit::Knob(s.at), [g.vol.0 - 21.0, g.vol.1 - 21.0, 42.0, 42.0]));
                        if let Some((bx, by)) = g.bal {
                            self.hits.push((Hit::Bal(s.at), [bx - 15.0, by - 15.0, 30.0, 30.0]));
                        }
                    }
                    continue;
                }
                self.hits.extend(row_keys(s, kx, sy + rows.keys_y, kw, true));
                if s.mix.is_some() {
                    for (i, cx) in slots_x(s).iter().enumerate() {
                        let ch = if s.chans { Some(i) } else { None };
                        let top = sy + rows.fader_y();
                        self.hits.push((Hit::Fader(s.at, ch, top), [x0 + cx - CAP.0 / 2.0, top, CAP.0, TRAVEL + 12.0]));
                    }
                }
            }
        }
        self.size = (x - GAP + LEFT, TOP + rows.mod_h() + BOTTOM);
        self.modules = mods;
    }

    /// The console's scroll bars where it goes on beyond the window, as the rack's: down the right
    /// edge, along the bottom. Whether down, its track and its thumb, in window pixels.
    fn bar_geo(&self, size: (u32, u32), dpi: f64) -> Vec<(bool, [f64; 4], [f64; 4])> {
        let (w, top, bottom) = self.view(size, dpi);
        let (fw, top, bottom) = (w * dpi, top * dpi, bottom * dpi);
        let (fh, t) = (bottom - top, 4.0 * dpi);
        // (in the middle of its lane)
        let inset = (BAR * dpi - t) / 2.0;
        let (cw, ch) = (self.size.0 * dpi, self.size.1 * dpi);
        let mut out = Vec::new();
        if self.lanes.0 && ch > fh + 1.0 {
            let th = (fh * fh / ch).max(24.0 * dpi);
            let at = (self.sy * dpi / (ch - fh)).clamp(0.0, 1.0);
            let x = fw + inset;
            out.push((true, [x, top + 2.0, t, fh - 4.0], [x, top + 2.0 + (fh - 4.0 - th) * at, t, th]));
        }
        if self.lanes.1 && cw > fw + 1.0 {
            let tw = (fw * fw / cw).max(24.0 * dpi);
            let at = (self.sx * dpi / (cw - fw)).clamp(0.0, 1.0);
            let y = bottom + inset;
            out.push((false, [2.0, y, fw - 4.0, t], [2.0 + (fw - 4.0 - tw) * at, y, tw, t]));
        }
        out
    }

    /// The field less the scroll bars' lanes: what shows the console (logical pixels).
    fn view(&self, size: (u32, u32), dpi: f64) -> (f64, f64, f64) {
        let (w, top, bottom) = Mixer::field(size, dpi);
        (w - if self.lanes.0 { BAR } else { 0.0 }, top, bottom - if self.lanes.1 { BAR } else { 0.0 })
    }

    /// The scroll bars to draw: track (0) and thumb (1), in window pixels (see `bar_geo`).
    pub fn bars(&self, size: (u32, u32), dpi: f64) -> Vec<(usize, [f64; 4])> {
        self.bar_geo(size, dpi).into_iter().flat_map(|(_, track, thumb)| [(0, track), (1, thumb)]).collect()
    }

    /// The console's field in the window (logical pixels): below the panel, above the status bar.
    fn field(size: (u32, u32), dpi: f64) -> (f64, f64, f64) {
        let (w, h) = (size.0 as f64 / dpi, size.1 as f64 / dpi);
        (w, panel::H, h - panel::STATUS_H)
    }

    /// Keeps the console's scroll inside it.
    fn clamp(&mut self, size: (u32, u32), dpi: f64) {
        let (w, top, bottom) = self.view(size, dpi);
        self.sx = self.sx.clamp(0.0, (self.size.0 - w).max(0.0));
        self.sy = self.sy.clamp(0.0, (self.size.1 - (bottom - top)).max(0.0));
    }

    /// A point on the console from the window (pixels).
    fn at(&self, p: (f64, f64), dpi: f64) -> (f64, f64) {
        (p.0 / dpi + self.sx, p.1 / dpi - panel::H + self.sy)
    }

    fn hit(&self, p: (f64, f64), dpi: f64) -> Option<Hit> {
        let (x, y) = self.at(p, dpi);
        self.hits
            .iter()
            .find(|(_, [hx, hy, hw, hh])| x >= *hx && x <= hx + hw && y >= *hy && y <= hy + hh)
            .map(|(h, _)| h.clone())
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
        // a scroll bar: its thumb is carried; pressed beside it, the thumb comes to the pointer
        for (down, track, thumb) in self.bar_geo(self.win, dpi) {
            let grab = BAR / 2.0 * dpi;
            let (along, at, lo, len, th) = if down { (p.0, p.1, track[1], track[3], thumb[3]) } else { (p.1, p.0, track[0], track[2], thumb[2]) };
            let across = if down { track[0] } else { track[1] };
            if (along - across - 2.0 * dpi).abs() > grab || at < lo || at > lo + len {
                continue;
            }
            let t0 = if down { thumb[1] } else { thumb[0] };
            if at < t0 || at > t0 + th {
                // (its middle under the pointer)
                let room = (len - th).max(1.0);
                let k = ((at - th / 2.0 - lo) / room).clamp(0.0, 1.0);
                let (w, top, bottom) = self.view(self.win, dpi);
                if down {
                    self.sy = k * (self.size.1 - (bottom - top)).max(0.0);
                } else {
                    self.sx = k * (self.size.0 - w).max(0.0);
                }
            }
            self.drag = Some(Drag::Bar { down, from: at, s0: if down { self.sy } else { self.sx } });
            return None;
        }
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
            Hit::Port((di, gi)) => Some(Action::Port(di, gi, self.to_window(rect, dpi))),
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
            // a knob: up is louder; a double click, 100%
            Hit::Knob(at) => {
                if double {
                    scene.set_volume(at, 1.0);
                    return Some(Action::Changed);
                }
                let raw = scene.devs.get(at.0).and_then(|d| d.mix(at.1)).map_or(1.0, |m| m.0);
                self.drag = Some(Drag::Knob { at, last: p.1 / dpi, raw });
                None
            }
            // BAL, as the rack's (the same knob: in or out on both)
            Hit::Bal(at) => {
                let d = scene.devs.get_mut(at.0)?;
                let raw = d.mix(at.1).map_or(0.0, |m| m.1);
                let was = d.bal(at.1);
                // down under the finger, below flush
                d.bal.insert(at.1, BalKnob::Held);
                let y = p.1 / dpi;
                self.drag = Some(Drag::Bal { at, y0: y, last: y, raw, was, turned: false });
                None
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
            Hit::All(at) => self.play_all(scene, at),
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
            // the thumb goes with the pointer: the console as much further as the thumb's room
            // stands for
            Some(Drag::Bar { down, from, s0 }) => {
                let Some((_, track, thumb)) = self.bar_geo(size, dpi).into_iter().find(|b| b.0 == down) else { return false };
                let (w, top, bottom) = self.view(size, dpi);
                let (len, th, at) = if down { (track[3], thumb[3], p.1) } else { (track[2], thumb[2], p.0) };
                let span = if down { self.size.1 - (bottom - top) } else { self.size.0 - w };
                let v = s0 + (at - from) / (len - th).max(1.0) * span.max(0.0);
                if down {
                    self.sy = v;
                } else {
                    self.sx = v;
                }
                self.clamp(size, dpi);
                true
            }
            // (as the rack's: it rests at 100% for a moment on the way)
            Some(Drag::Knob { at, last, raw }) => {
                let y = p.1 / dpi;
                let raw = (raw + (last - y) * VOL_MAX / KNOB_TRAVEL).clamp(0.0, VOL_MAX);
                self.drag = Some(Drag::Knob { at, last: y, raw });
                scene.set_volume(at, if (raw - 1.0).abs() < 0.04 { 1.0 } else { raw });
                true
            }
            Some(Drag::Bal { at, y0, last, raw, was, mut turned }) => {
                let y = p.1 / dpi;
                // pushed in, it can't turn; out, it turns once the pointer really moves
                if was != BalKnob::Out || (!turned && (y - y0).abs() < 3.0) {
                    return false;
                }
                if !turned {
                    // turning, not pressing: it comes back up without a click
                    turned = true;
                    if let Some(d) = scene.devs.get_mut(at.0) {
                        d.bal.insert(at.1, BalKnob::Out);
                    }
                }
                let raw = (raw + (last - y) * 2.0 / KNOB_TRAVEL).clamp(-1.0, 1.0);
                self.drag = Some(Drag::Bal { at, y0, last: y, raw, was, turned });
                scene.set_balance(at, if raw.abs() < 0.05 { 0.0 } else { raw });
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
            // a press without a turn: it comes back up, out if it was in, in if it was out
            Some(Drag::Bal { at, was, turned, .. }) => {
                if let Some(d) = scene.devs.get_mut(at.0) {
                    d.bal.insert(at.1, if turned || was == BalKnob::In { BalKnob::Out } else { BalKnob::In });
                }
                None
            }
            _ => None,
        }
    }

    /// Lets go of what is held without doing anything (switching screens); a BAL knob held comes
    /// back as it was.
    pub fn let_go(&mut self, scene: &mut Scene) {
        if let Some(Drag::Bal { at, was, .. }) = self.drag.take()
            && let Some(d) = scene.devs.get_mut(at.0)
        {
            d.bal.insert(at.1, was);
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The wheel: over a fader or the VOLUME knob it steps the volume (`clicks`, finer with Shift),
    /// over BAL when it is out the balance; elsewhere it scrolls the console (`d`: logical pixels,
    /// sideways first when it is wider than the window).
    pub fn wheel(&mut self, scene: &mut Scene, p: (f64, f64), Wheel { clicks, d, fine }: Wheel, size: (u32, u32), dpi: f64) -> bool {
        let step = clicks as f64 * if fine { 0.01 } else { 0.05 };
        match self.hit(p, dpi) {
            Some(Hit::Knob(at)) => {
                if clicks == 0 {
                    return false;
                }
                let Some((vol, ..)) = scene.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return false };
                scene.set_volume(at, (vol + step).clamp(0.0, VOL_MAX));
                return true;
            }
            Some(Hit::Bal(at)) if scene.devs.get(at.0).is_some_and(|d| d.bal(at.1) == BalKnob::Out) => {
                if clicks == 0 {
                    return false;
                }
                let Some((_, bal, _)) = scene.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return false };
                let b = (bal + step).clamp(-1.0, 1.0);
                scene.set_balance(at, if b.abs() < 0.03 { 0.0 } else { b });
                return true;
            }
            _ => {}
        }
        if let Some(Hit::Fader(at, ch, _)) = self.hit(p, dpi) {
            if clicks == 0 {
                return false;
            }
            let Some((vol, bal, _)) = scene.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return false };
            let now = match ch {
                None => vol,
                Some(c) => channels((vol, bal))[c.min(1)],
            };
            self.set_fader(scene, at, ch, now + step);
            return true;
        }
        let (w, top, bottom) = self.view(size, dpi);
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
            Hit::Graph(n) => format!("#{n} · {}  ·  click: open it in ENGINE", scene.graph.node_name(n).unwrap_or("?")),
            Hit::Profile(_) => "The card's profile: which of its outputs and inputs it has".into(),
            Hit::Port(_) => "The card's port: where it plays or records from".into(),
            Hit::To(from, to) => {
                let lit = self.strip(from).is_some_and(|s| s.targets.iter().any(|t| t.at == to && t.lit));
                format!("{} {} {}. Click: {}", name(from), if lit { "goes to" } else { "doesn't go to" }, name(to), if lit { "not there any more" } else { "there too" })
            }
            Hit::Main(at) if at_role(self, at) == Some(Role::Sink) => format!("MAIN: make {} the default output: new apps play here", name(at)),
            Hit::Main(at) => format!("MAIN: make {} the default input: new apps record from here", name(at)),
            Hit::All(at) if at_role(self, at) == Some(Role::Source) => format!("ALL HERE: every app records from {}, and it becomes the default", name(at)),
            Hit::All(at) => format!("ALL HERE: every app plays on {}, and it becomes the default", name(at)),
            Hit::Cue(at) => format!("Listen to {} in the default output; nothing else changes", name(at)),
            Hit::Mute(at) => format!("Mute {}", name(at)),
            Hit::Chan(..) => "SPLIT: a fader for L and one for R".into(),
            Hit::Fader(at, ..) | Hit::Knob(at) => format!("{}: drag or wheel; double-click 100%", name(at)),
            Hit::Bal(at) if scene.devs.get(at.0).is_some_and(|d| d.bal(at.1) == BalKnob::Out) => "BAL: drag or wheel turns it; a click pushes it in".into(),
            Hit::Bal(_) => "BAL: a click and it comes out, to turn".into(),
        })
    }

    /// The device whose PROFILE (`ports`: PORT, and its group) key is at `p`.
    pub fn list_key_at(&self, p: (f64, f64), ports: bool, dpi: f64) -> Option<(usize, Option<usize>)> {
        match self.hit(p, dpi)? {
            Hit::Profile(di) if !ports => Some((di, None)),
            Hit::Port((di, gi)) if ports => Some((di, Some(gi))),
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
        self.win = size;
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
            // (in whole pixels, whatever the scale: they are stretched anyway, and a pixel cut by a
            // fractional scale comes out see-through)
            self.deco = Some((scale, [small(groove, 2.0, 1.0, 1.0), small(back, 1.0, 1.0, 1.0)]));
        }
        let mut wanted_leds = Vec::new();
        let mut wanted_caps = Vec::new();
        let mut wanted_knobs = Vec::new();
        let mut wanted_bals = Vec::new();
        let mut wanted_texts: Vec<(String, &'static str, u64)> = Vec::new();
        for s in self.modules.iter().flat_map(|m| &m.strips) {
            let l = s.level.map(lit);
            wanted_leds.push((s.cols, l[0], if s.cols == 2 { l[1] } else { 0 }, rows.knobs, scale));
            if rows.knobs {
                if let Some((v, b, _)) = s.mix {
                    wanted_knobs.push((pct(v), scale));
                    if s.stereo {
                        wanted_bals.push((bal_pct(b), bal_state(scene, s.at), scale));
                    }
                }
            } else {
                wanted_caps.extend(cap_values(s).into_iter().map(|v| (v, scale)));
            }
        }
        for k in wanted_leds {
            let h = if k.3 { KNOB_LEDS_H } else { LEDS_H };
            self.leds.entry(k).or_insert_with(|| small(&leds_svg(k.0, k.1, k.2, h), meter_w(k.0), h, dpi));
        }
        for k in wanted_knobs {
            self.knobs.entry(k).or_insert_with(|| small(&vol_knob_svg(k.0), 40.0, 40.0, dpi));
        }
        for k in wanted_bals {
            self.bals.entry(k).or_insert_with(|| small(&bal_knob_svg(k.0, k.1), 30.0, 30.0, dpi));
        }
        for k in wanted_caps {
            self.caps.entry(k).or_insert_with(|| small(&cap_svg(k.0), CAP.0 + 1.0, CAP.1 + 2.0, dpi));
        }
        for sec in [Section::Inputs, Section::Apps, Section::Outputs] {
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
        if self.knobs.len() > 400 {
            self.knobs.clear();
        }
        if self.bals.len() > 400 {
            self.bals.clear();
        }
        let (fw, top, bottom) = self.view(size, dpi);
        let clip = Some([0.0, top * dpi, fw * dpi, (bottom - top) * dpi]);
        let (ox, oy) = (-self.sx, panel::H - self.sy);
        let mut out: Vec<Pic> = Vec::new();
        let px = |x: f64| (x * dpi).round();
        let Some((_, [groove, back])) = &self.deco else { return out };
        // the grooves between the groups
        for gx in &self.grooves {
            out.push((groove, px(ox + gx - 1.0), px(oy + TOP), 2.0 * dpi, mod_h * dpi, clip, 1.0));
        }
        let mut named = HashSet::new();
        for m in &self.modules {
            // a group's name over its first module
            if named.insert(m.section)
                && let Some(l) = self.texts.get(&(m.section.name().to_owned(), LABEL, scale))
            {
                out.push((l, px(ox + m.x + 2.0), px(oy + 6.0), l.w * dpi, l.h * dpi, clip, 1.0));
            }
            if let Some(l) = self.pics.get(&m.key).and_then(|p| p.layer.as_ref()) {
                out.push((l, px(ox + m.x + l.x), px(oy + TOP + l.y), l.w * dpi, l.h * dpi, clip, 1.0));
            }
            // the caps (their volume on them) or the knobs, and the lit LEDs, over the module
            let sy = TOP + HEAD;
            for s in &m.strips {
                let x0 = m.x + s.x;
                if rows.knobs {
                    let g = knob_geo(s, x0, sy, &rows);
                    if let Some((v, b, _)) = s.mix {
                        if let Some(l) = self.knobs.get(&(pct(v), scale)) {
                            out.push((l, px(ox + g.vol.0 - 20.0), px(oy + g.vol.1 - 20.0), l.w * dpi, l.h * dpi, clip, 1.0));
                        }
                        if let Some((bx, by)) = g.bal
                            && let Some(l) = self.bals.get(&(bal_pct(b), bal_state(scene, s.at), scale))
                        {
                            out.push((l, px(ox + bx - 15.0), px(oy + by - 15.0), l.w * dpi, l.h * dpi, clip, 1.0));
                        }
                    }
                    let l = s.level.map(lit);
                    if let Some(layer) = self.leds.get(&(s.cols, l[0], if s.cols == 2 { l[1] } else { 0 }, true, scale)) {
                        out.push((layer, px(ox + g.mx), px(oy + g.t + KNOB_LEDS_Y), layer.w * dpi, layer.h * dpi, clip, 1.0));
                    }
                    continue;
                }
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
                if let Some(layer) = self.leds.get(&(s.cols, l[0], if s.cols == 2 { l[1] } else { 0 }, false, scale)) {
                    out.push((layer, px(ox + x0 + meter_x(s)), px(oy + top + cap_y(VOL_MAX)), layer.w * dpi, layer.h * dpi, clip, 1.0));
                }
            }
        }
        // the scroll bars' lanes, the field's dark: the bars go in them (drawn by the window)
        let (full_w, _, full_bottom) = Mixer::field(size, dpi);
        if self.lanes.0 {
            out.push((back, px(fw), px(top), px(full_w) - px(fw), px(full_bottom) - px(top), None, 1.0));
        }
        if self.lanes.1 {
            out.push((back, 0.0, px(bottom), px(full_w), px(full_bottom) - px(bottom), None, 1.0));
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

    /// The console's whole size (logical pixels) in a window `size` pixels at `dpi`: how big a
    /// picture of all of it must be.
    pub fn content(&mut self, scene: &Scene, size: (u32, u32), dpi: f64) -> (f64, f64) {
        self.layout(scene, &[], size, dpi);
        self.size
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

/// A balance as a whole per cent (-100 all left … 100 all right).
fn bal_pct(b: f64) -> i32 {
    (b.clamp(-1.0, 1.0) * 100.0).round() as i32
}

/// The BAL knob of a group: in, held or out (the rack's, the same knob).
fn bal_state(scene: &Scene, at: (usize, usize)) -> u8 {
    match scene.devs.get(at.0).map(|d| d.bal(at.1)) {
        Some(BalKnob::Out) => 2,
        Some(BalKnob::Held) => 1,
        _ => 0,
    }
}

/// A point `r` from (x, y) at angle `a` (degrees, 0 straight up, clockwise).
fn polar(x: f64, y: f64, a: f64, r: f64) -> String {
    let a = a.to_radians();
    format!("{:.2},{:.2}", x + r * a.sin(), y - r * a.cos())
}

/// The VOLUME knob's angle for a volume: from -135° (silence) to 135° (150%), as the rack's.
fn vol_angle(v: f64) -> f64 {
    -135.0 + 270.0 * v.clamp(0.0, VOL_MAX) / VOL_MAX
}

/// A knob's shadow on the panel, the same for every knob, as the rack's (`face::mix_pic`): lit
/// from the top left, down and a little right of it, as far as it stands out.
fn knob_shadow(x: f64, y: f64, r: f64, lift: f64) -> String {
    format!(r##"<circle cx="{}" cy="{}" r="{}" fill="#000" opacity=".5"/>"##, x + 0.5 * lift, y + lift, r + 0.3 * lift)
}

/// A knob `r` round at (x, y), its pointer at `a`, as the rack's (`face::mix_pic`).
fn knob(x: f64, y: f64, r: f64, a: f64) -> String {
    format!(
        r##"<circle cx="{x}" cy="{y}" r="{r}" fill="url(#gVol)" stroke="#0c0c0d"/><circle cx="{x}" cy="{y}" r="{}" fill="none" stroke="rgba(255,255,255,.08)"/><path d="M{} L{}" stroke="#ffb02e" stroke-width="{}" stroke-linecap="round"/>"##,
        r * 0.75,
        polar(x, y, a, r * 0.29),
        polar(x, y, a, r * 0.86),
        if r > 12.0 { 2.4 } else { 2.0 }
    )
}

const KNOB_DEFS: &str = r##"<defs><radialGradient id="gVol" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient></defs>"##;

/// The VOLUME knob at a volume (%), in the middle of 40×40.
fn vol_knob_svg(pct: u32) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">{KNOB_DEFS}{}{}</svg>"#, knob_shadow(20.0, 20.0, 14.0, 2.5), knob(20.0, 20.0, 14.0, vol_angle(pct as f64 / 100.0)))
}

/// The BAL knob at a balance (%), in (0), held down (1) or out (2), in the middle of 30×30.
fn bal_knob_svg(bal: i32, state: u8) -> String {
    let (bx, by) = (15.0, 15.0);
    let a = bal as f64 / 100.0 * 135.0;
    let pointer = |dy: f64, colour: &str, w: f64, r0: f64, r1: f64| format!(r#"<path d="M{} L{}" stroke="{colour}" stroke-width="{w}" stroke-linecap="round"/>"#, polar(bx, by + dy, a, r0), polar(bx, by + dy, a, r1));
    let body = match state {
        // held down: below flush, deeper in its well
        1 => format!(r##"<circle cx="{bx}" cy="{}" r="8.3" fill="#151618" stroke="#050506"/><circle cx="{bx}" cy="{}" r="8.6" fill="none" stroke="rgba(0,0,0,.7)" stroke-width="1.6"/>{}"##, by + 0.6, by - 0.4, pointer(0.6, "#6a5028", 1.7, 2.4, 6.6)),
        // out: a taller cap with a shadow under it
        2 => format!(r##"{}<circle cx="{bx}" cy="{}" r="9.6" fill="#0d0e0f"/>{}"##, knob_shadow(bx, by, 9.6, 2.5), by + 2.0, knob(bx, by, 9.6, a)),
        // in: flush, a dim pointer
        _ => format!(r##"<circle cx="{bx}" cy="{by}" r="9" fill="#202124" stroke="#0c0c0d"/>{}"##, pointer(0.0, "#8a6a32", 1.8, 2.6, 7.4)),
    };
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="30">{KNOB_DEFS}{body}</svg>"#)
}

/// The lit LEDs of a meter (`cols` columns, `l` and `r` of `LEDS` lit from the bottom): green, then
/// amber, then red, as on the rack's faces; thin.
fn leds_svg(cols: usize, l: u8, r: u8, h: f64) -> String {
    let w = meter_w(cols);
    let mut s = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">"#);
    let step = led_step(h);
    for c in 0..cols {
        let n = if c == 0 { l } else { r };
        for i in 0..(n as usize).min(LEDS) {
            let f = i as f64 / LEDS as f64;
            let colour = if f >= 0.9 { "#ff3b2f" } else if f >= 0.7 { "#ffb02e" } else { "#4cff6a" };
            let y = h - 1.0 - (i as f64 + 1.0) * step;
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

/// A key that does something once (ALL HERE), as the panel's (`panel::cap_svg`): no light, its
/// name in the middle; down while `held`.
fn action_key(o: &mut String, r @ [x, y, w, h]: [f64; 4], label: &str, held: bool) {
    o.push_str(&panel::cap_svg(r, held, false));
    o.push_str(&SMALL.text(x + w / 2.0, y + h / 2.0 + 3.1, "#e6e7e8", "middle", label));
}

/// A choice with a list (PROFILE, PORT): its name small over a little amber LCD, as the top
/// panel's, the value in it at full width and ▾ at its right; a click opens the list.
fn lcd(o: &mut String, [x, y, w, h]: [f64; 4], label: &str, value: &str, print: &str) {
    o.push_str(&SMALL.text(x + 2.0, y - 4.0, print, "start", label));
    let _ = write!(
        o,
        r##"<rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="#121214"/><rect x="{}" y="{}" width="{}" height="{}" rx="4.5" fill="none" stroke="#fff" stroke-opacity=".18"/><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="2.5" fill="url(#gLcd)"/><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="2.5" fill="none" stroke="#783200" stroke-opacity=".35" stroke-width="2"/>"##,
        x - 2.0,
        y - 2.0,
        w + 4.0,
        h + 4.0,
        x - 2.5,
        y - 2.5,
        w + 5.0,
        h + 5.0
    );
    let chars = ((w - 20.0) / 5.75) as usize;
    let v = if value.chars().count() > chars { format!("{}…", value.chars().take(chars.saturating_sub(1)).collect::<String>()) } else { value.to_owned() };
    let _ = write!(o, r##"<text x="{}" y="{}" font-family="Noto Sans Mono" font-weight="400" font-size="9.5" fill="#3a2408" xml:space="preserve">{}</text><path d="M{},{} l3,3.5 l3,-3.5 z" fill="#3a2408"/>"##, x + 6.0, y + h / 2.0 + 3.4, esc(&v), x + w - 11.0, y + h / 2.0 - 1.5);
}

/// A key with a LED (PLAY ON, RECORD FROM): it doesn't latch, down only while held; its red LED
/// says whether it is on.
fn push_key(o: &mut String, r @ [x, y, w, h]: [f64; 4], label: &str, on: bool, held: bool) {
    o.push_str(&panel::cap_svg(r, held, false));
    led(o, x + 8.0, y + h / 2.0, on, "#ff4a32");
    o.push_str(&SMALL.text(x + 15.0, y + h / 2.0 + 3.1, if on { "#f3efe6" } else { "#c9cbcd" }, "start", &fit(label, SMALL, w - 22.0)));
}

/// CUE, MUTE, SPLIT's colours when lit, and their gradients' ids (`glass_defs`).
const GLASS: [(&str, &str); 3] = [("#5fd4ff", "gC"), ("#ff4a32", "gM"), ("#ffb02e", "gS")];

/// `a` toward `b` by `t` (colours #rrggbb).
fn mix(a: &str, b: &str, t: f64) -> String {
    let p = |c: &str, i: usize| u8::from_str_radix(&c[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0) as f64;
    let c = |i| (p(a, i) + (p(b, i) - p(a, i)) * t).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(0), c(1), c(2))
}

/// The keys' gradients: lit, a lamp behind frosted plastic in the key's colour (brightest a little
/// above and left of the middle, falling off unevenly to the edges, as a bulb lights it); unlit,
/// pale ivory frosted plastic, as a synth's lit keys are, a little warmer in the middle.
pub(crate) fn glass_defs() -> String {
    let mut o = String::new();
    for (c, id) in GLASS {
        let _ = write!(
            o,
            r#"<radialGradient id="{id}On" cx=".42" cy=".38" r=".78" fx=".38" fy=".3"><stop offset="0" stop-color="{}"/><stop offset=".3" stop-color="{}"/><stop offset=".72" stop-color="{c}"/><stop offset="1" stop-color="{}"/></radialGradient>"#,
            mix(c, "#ffffff", 0.62),
            mix(c, "#ffffff", 0.28),
            mix(c, "#000000", 0.28)
        );
    }
    o.push_str(r##"<radialGradient id="gFrost" cx=".44" cy=".4" r=".8" fx=".4" fy=".32"><stop offset="0" stop-color="#f6f1e3"/><stop offset=".55" stop-color="#e2dbc8"/><stop offset="1" stop-color="#bdb6a3"/></radialGradient>"##);
    o
}

/// The hole a row of keys sits in, milled in the panel round them all: its corners barely
/// rounded, a dark edge round the keys and between them, its far wall in shade at the top and the
/// panel's lip catching the light at the bottom. `[x, y, w, h]`: the keys' row.
pub(crate) fn key_hole(o: &mut String, [x, y, w, h]: [f64; 4]) {
    let _ = write!(
        o,
        r##"<rect x="{}" y="{}" width="{}" height="{}" rx="1.8" fill="#0b0c0d"/><path d="M{},{} h{}" stroke="#000" stroke-opacity=".5" stroke-width="1"/><path d="M{},{} h{}" stroke="#fff" stroke-opacity=".28" stroke-width=".8"/><path d="M{},{} v{}" stroke="#fff" stroke-opacity=".12" stroke-width=".8"/>"##,
        x - 1.5,
        y - 1.5,
        w + 3.0,
        h + 3.0,
        x - 0.5,
        y - 1.0,
        w + 1.0,
        x - 0.5,
        y + h + 1.9,
        w + 1.0,
        x + w + 1.9,
        y - 0.5,
        h + 1.0
    );
}

/// CUE, MUTE, SPLIT, MAIN: a key flush with the panel, in a hole milled for its row (`key_hole`,
/// drawn under the row), as a synth's lit keys side by side. Off, pale ivory frosted plastic; on,
/// lit from behind by a lamp in its colour, unevenly, brightest a little above the middle. Its
/// name dark. Held down, a shade darker and its name a little lower. No blur: fills and gradients.
pub(crate) fn lit_key(o: &mut String, [x, y, w, h]: [f64; 4], label: &str, colour: &str, on: bool, held: bool) {
    let id = GLASS.iter().find(|g| g.0 == colour).map_or("gC", |g| g.1);
    let fill = if on { format!("url(#{id}On)") } else { "url(#gFrost)".to_owned() };
    let _ = write!(o, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="1" fill="{fill}"/>"#);
    // its edge: a fine light line at its top, a dark one at its bottom (held: the other way, as
    // it sinks)
    let (top, bottom) = if held { (0.0, 0.15) } else { (if on { 0.5 } else { 0.6 }, 0.3) };
    let _ = write!(
        o,
        r##"<path d="M{},{} h{}" stroke="#fff" stroke-opacity="{top}" stroke-width=".7"/><path d="M{},{} h{}" stroke="#000" stroke-opacity="{bottom}" stroke-width=".7"/>"##,
        x + 1.0,
        y + 0.4,
        w - 2.0,
        x + 1.0,
        y + h - 0.4,
        w - 2.0
    );
    if held {
        let _ = write!(o, r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="1" fill="#000" fill-opacity=".22"/><path d="M{},{} h{}" stroke="#000" stroke-opacity=".35" stroke-width="1.2"/>"##, x + 0.6, y + 0.7, w - 1.2);
    }
    let dy = if held { 0.6 } else { 0.0 };
    o.push_str(&SMALL.text(x + w / 2.0, y + h / 2.0 + 2.9 + dy, if on { "#1d1408" } else { "#2a2720" }, "middle", label));
}

/// The VOLUME (GAIN) block's keys.
fn block_keys(o: &mut String, s: &Strip, keys: &[(Hit, [f64; 4])]) {
    // one hole for each run of keys side by side (a key missing leaves its place bare)
    let mut run: Option<[f64; 4]> = None;
    for (_, r) in keys {
        run = match run {
            Some(a) if (r[0] - (a[0] + a[2])) <= KEY_GAP + 0.5 => Some([a[0], a[1], r[0] + r[2] - a[0], a[3]]),
            Some(a) => {
                key_hole(o, a);
                Some(*r)
            }
            None => Some(*r),
        };
    }
    if let Some(a) = run {
        key_hole(o, a);
    }
    for (h, r) in keys {
        let held = s.held.as_ref() == Some(h);
        match h {
            Hit::Cue(_) => lit_key(o, *r, "CUE", "#5fd4ff", s.cue, held),
            Hit::Mute(..) => lit_key(o, *r, "MUTE", "#ff4a32", s.mix.is_some_and(|m| m.2), held),
            _ => lit_key(o, *r, "SPLIT", "#ffb02e", s.chans, held),
        }
    }
}

/// The block's dark meter: `cols` columns of `LEDS`, `h` high at (x, y).
fn dark_meter(o: &mut String, x: f64, y: f64, cols: usize, h: f64) {
    let _ = write!(o, r##"<rect x="{x}" y="{y}" width="{}" height="{h}" rx="3" fill="#0b0b0c"/>"##, meter_w(cols));
    let step = led_step(h);
    for c in 0..cols {
        for i in 0..LEDS {
            let f = i as f64 / LEDS as f64;
            let ly = y + h - 1.0 - (i as f64 + 1.0) * step;
            let dark = if f >= 0.9 { "#351612" } else if f >= 0.7 { "#2f2810" } else { "#1c2a1f" };
            let _ = write!(o, r#"<rect x="{}" y="{ly}" width="{LED_W}" height="{}" rx="1" fill="{dark}"/>"#, x + 3.0 + c as f64 * (LED_W + 2.0), step - 2.0);
        }
    }
}

/// A group's name (OUTPUT, PLAY ON, DEVICE SETUP, VOLUME...) in the middle of a rule from `x0` to
/// `x1`: a thin line at its left and its right (none where there is no room for one).
fn ruled(o: &mut String, x0: f64, x1: f64, y: f64, text: &str, print: &str) {
    let cx = (x0 + x1) / 2.0;
    let half = SMALL.width(text) / 2.0 + 6.0;
    o.push_str(&SMALL.text(cx, y, print, "middle", text));
    for (a, b) in [(x0, cx - half), (cx + half, x1)] {
        if b - a > 2.0 {
            let _ = write!(o, r#"<path d="M{a},{} H{b}" stroke="{print}" stroke-opacity=".5"/>"#, y - 2.8);
        }
    }
}

/// The block under the knobs: its name, the knobs' scales (the knobs are pictures over it), BAL's
/// well, CUE and MUTE, the dark meter.
fn knob_block(o: &mut String, s: &Strip, g: &KnobGeo, x0: f64, print: &str) {
    let cx0 = (x0 + 6.0 + g.mx - 5.0) / 2.0;
    ruled(o, x0 + 6.0, x0 + s.w - 6.0, g.t, &format!("{}{}", block_name(s), if g.bal.is_some() { " · BAL" } else { "" }), print);
    block_keys(o, s, &g.keys);
    if s.mix.is_none() {
        o.push_str(&SMALL.text(cx0, g.t + 24.0, print, "middle", "NO VOLUME"));
        o.push_str(&SMALL.text(cx0, g.t + 36.0, print, "middle", "IN PIPEWIRE"));
    } else {
        // VOLUME: silence to 150%, a long tick at 100% and red past it
        let (cx, cy) = g.vol;
        let _ = write!(o, r##"<path d="M{} A19.5,19.5 0 0 1 {}" fill="none" stroke="#d2402f" stroke-width="2.2" opacity=".85"/>"##, polar(cx, cy, vol_angle(1.0), 19.5), polar(cx, cy, vol_angle(VOL_MAX), 19.5));
        for i in 0..=12 {
            let a = vol_angle(i as f64 / 12.0 * VOL_MAX);
            let _ = write!(o, r#"<path d="M{} L{}" stroke="{print}" stroke-width=".9" opacity=".6"/>"#, polar(cx, cy, a, 17.5), polar(cx, cy, a, if i % 4 == 0 { 21.0 } else { 19.5 }));
        }
        let _ = write!(o, r#"<path d="M{} L{}" stroke="{print}" stroke-width="1.6"/>"#, polar(cx, cy, vol_angle(1.0), 16.5), polar(cx, cy, vol_angle(1.0), 23.5));
        // BAL: centre straight up with a long tick, L and R at the ends, and the knob's well
        if let Some((bx, by)) = g.bal {
            for i in -4i32..=4 {
                let a = i as f64 * 33.75;
                let _ = write!(o, r#"<path d="M{} L{}" stroke="{print}" stroke-width="{}" opacity="{}"/>"#, polar(bx, by, a, 11.5), polar(bx, by, a, if i == 0 { 16.0 } else { 13.5 }), if i == 0 { 1.5 } else { 0.9 }, if i == 0 { 1.0 } else { 0.6 });
            }
            let _ = write!(o, r#"<circle cx="{bx}" cy="{by}" r="10.8" fill="url(#gHoleK)" stroke="rgba(255,255,255,.12)" stroke-width=".7"/>{}{}"#, SMALL.text(bx - 12.0, by + 17.0, print, "middle", "L"), SMALL.text(bx + 12.0, by + 17.0, print, "middle", "R"));
        }
    }
    dark_meter(o, g.mx, g.t + KNOB_LEDS_Y, s.cols, KNOB_LEDS_H);
}

/// A module: its plate's outline, its heading (a card's name and PROFILE side by side), its strips
/// (all but the caps, the lit LEDs and the numbers).
fn module_svg(m: &Module, print: &str, rows: &Rows) -> String {
    let (w, h) = (m.w, rows.mod_h());
    let (dw, dh) = (w + 2.0 * FACE_PAD, h + 2.0 * FACE_PAD);
    let mut o = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}" font-family="Noto Sans" font-weight="bold"><defs>{}<radialGradient id="gHoleK" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient><linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient>{}</defs><g transform="translate({FACE_PAD},{FACE_PAD})">"##, panel::KEY_DEFS, glass_defs());
    let _ = write!(o, r##"<rect x=".5" y=".5" width="{}" height="{}" rx="{RADIUS}" fill="none" stroke="#000" stroke-opacity=".7"/>"##, w - 1.0, h - 1.0);
    // each strip's frame first, what is in it over it: as a frame on the rack (face.rs), here on the
    // module's very edge, its corners the module's; its edge and its band in the block kind's
    // colour, on the band the device's name, then the frame's with GRAPH at its right. A card with
    // its profile Off has no strip: a plain grey band, OFF on its second line, no GRAPH (no node)
    let frame = |o: &mut String, fx: f64, fw: f64, band: Option<(&str, &str, &str, Option<u32>)>| {
        let ink = band.map_or(print, |b| b.1);
        if let Some((c, _, _, _)) = band {
            let _ = write!(o, r#"<path d="M{fx},{BAND} V{RADIUS} a{RADIUS},{RADIUS} 0 0 1 {RADIUS},-{RADIUS} H{} a{RADIUS},{RADIUS} 0 0 1 {RADIUS},{RADIUS} V{BAND} Z" fill="{c}"/><path d="M{fx},{} H{}" stroke="rgba(0,0,0,.35)"/>"#, fx + fw - RADIUS, BAND + 0.5, fx + fw);
        }
        let _ = write!(o, r#"<rect x="{}" y=".6" width="{}" height="{}" rx="{}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="1.2"/>"#, fx + 0.6, fw - 1.2, h - 1.2, RADIUS - 0.6, band.map_or(print, |b| b.0), if band.is_some() { 0.9 } else { 0.35 });
        o.push_str(&SECTION.text(fx + 10.0, 16.0, ink, "start", &fit(&m.name, SECTION, fw - 20.0)));
        // (the frame's name opens its block in ENGINE: no key for it, as on the rack)
        if let Some((_, _, sub, _)) = band {
            o.push_str(&SMALL.text(fx + 10.0, 30.0, ink, "start", &fit(sub, SMALL, fw - 26.0)));
        }
    };
    if m.strips.is_empty() {
        frame(&mut o, 0.0, w, Some(("#6b6e72", "#f3efe6", "OFF", None)));
        ruled(&mut o, 12.0, w - 12.0, HEAD + ROUTE_Y + 8.0, "SETUP", print);
    }
    let last = m.strips.len().saturating_sub(1);
    for (i, s) in m.strips.iter().enumerate() {
        let l = if i == 0 { 0.0 } else { s.x };
        let r = if i == last { w } else { s.x + s.w };
        frame(&mut o, l, r - l, Some((face::kind_colour(s.kind), face::band_ink(s.kind), &s.name, Some(s.node))));
    }
    // the card's PROFILE: the first LCD under MAIN
    if let Some(p) = &m.profile {
        lcd(&mut o, profile_key(w), "PROFILE", p, print);
    }
    let below = usize::from(m.profile.is_some());
    let sy = HEAD;
    for (i, s) in m.strips.iter().enumerate() {
        let x0 = s.x;
        let held = |what: fn(&Hit) -> bool| s.held.as_ref().is_some_and(what);
        if i > 0 {
            let _ = write!(o, r#"<path d="M{x0},{sy} V{}" stroke="{print}" stroke-opacity=".35" stroke-dasharray="2 3"/>"#, h - 8.0);
        }
        // where it goes, or the device's keys
        let (kx, kw) = (x0 + 6.0, s.w - 12.0);
        let [[lx, lw], [rx, rw]] = halves(kx, kw);
        let ry = sy + ROUTE_Y;
        // every group of keys under its title, its keys in rows: where an app goes, or the
        // device's own keys (SETUP); then the channel itself
        let title = match s.role {
            Role::Play => "PLAY ON",
            Role::Rec => "RECORD FROM",
            Role::Sink | Role::Source => "SETUP",
        };
        ruled(&mut o, x0 + 6.0, x0 + s.w - 6.0, ry + 8.0, title, print);
        match s.role {
            Role::Play | Role::Rec => {
                for (i, t) in s.targets.iter().enumerate() {
                    let down = s.held.as_ref().is_some_and(|h| matches!(h, Hit::To(_, to) if *to == t.at));
                    push_key(&mut o, [kx, sy + row(i), kw, KEY_H], &t.label, t.lit, down);
                }
                if s.targets.is_empty() {
                    o.push_str(&SMALL.text(x0 + s.w / 2.0, sy + row(0) + 12.0, print, "middle", "NOWHERE"));
                }
            }
            Role::Sink | Role::Source => {
                if s.mix.is_some() && s.node_name.is_some() {
                    // MAIN: a state, lit like CUE and MUTE; ALL HERE: once, a plain key
                    key_hole(&mut o, [lx, sy + row(0), lw, KEY_H]);
                    lit_key(&mut o, [lx, sy + row(0), lw, KEY_H], "MAIN", "#ffb02e", s.main, held(|h| matches!(h, Hit::Main(_))));
                    action_key(&mut o, [rx, sy + row(0), rw, KEY_H], "ALL HERE", held(|h| matches!(h, Hit::All(_))));
                }
                if let Some(p) = &s.port {
                    lcd(&mut o, [kx, sy + setup_row(below), kw, LCD_H], "PORT", p, print);
                }
            }
        }
        // the VOLUME (GAIN) block, its name on top: the knobs when the window is low
        if rows.knobs {
            knob_block(&mut o, s, &knob_geo(s, x0, sy, rows), x0, print);
            continue;
        }
        ruled(&mut o, x0 + 6.0, x0 + s.w - 6.0, sy + rows.keys_y - 9.0, block_name(s), print);
        block_keys(&mut o, s, &row_keys(s, kx, sy + rows.keys_y, kw, true));
        // the faders under the keys: the scale's numbers at the left, lines left of each slot (long
        // at the numbers), the slot red past 100% (the caps, their volume on them, are pictures
        // over it); the meter's dark LEDs at the right, as tall as the slot and level with it
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
        dark_meter(&mut o, x0 + meter_x(s), top + cap_y(VOL_MAX), s.cols, LEDS_H);
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
