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

#[derive(Clone, Debug)]
pub struct Module {
    pub key: String,
    pub di: usize,
    pub name: String,
    pub finish: Finish,
    /// a card's profile in use (its PROFILE key), and whether it is off
    pub profile: Option<String>,
    pub app: bool,
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
/// one gap between every two modules, devices and apps alike
const GAP: f64 = 6.0;
/// a module's heading: its name; a card's name in the left half, its PROFILE key in the right
const HEAD: f64 = 34.0;
/// every strip is as wide: a module is one strip wide, two, three…
const STRIP_W: f64 = 112.0;
const BAND: f64 = 24.0;
const ROUTE_Y: f64 = 31.0;
const KEYS_Y: f64 = 188.0;
const CHAN_Y: f64 = 219.0;
const FBOX_Y: f64 = 250.0;
const SLOT_Y: f64 = FBOX_Y + 12.0;
const TRAVEL: f64 = 160.0;
const READ_Y: f64 = 440.0;
const STRIP_H: f64 = 462.0;
const MOD_H: f64 = HEAD + STRIP_H;
const RADIUS: f64 = 8.0;
/// a fader's cap
const CAP: (f64, f64) = (26.0, 32.0);
/// the lit LEDs' column: its height, a LED's
const LEDS_H: f64 = 172.0;

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

/// Where a strip's scale, slots and meter go, from its left. Two faders (SPLIT) fit the strip as
/// it is, a little closer together.
fn fader_x(s: &Strip) -> (f64, Vec<f64>, f64) {
    let n = if s.chans { 2 } else { 1 };
    let gap = if n == 2 { 4.0 } else { 5.0 };
    let meter = s.cols as f64 * 8.0 + 4.0;
    let content = 20.0 + gap + n as f64 * 26.0 + (n - 1) as f64 * gap + gap + meter;
    let x0 = ((s.w - content) / 2.0).max(4.0);
    let slots = (0..n).map(|i| x0 + 20.0 + gap + i as f64 * (26.0 + gap)).collect();
    (x0, slots, x0 + content - meter)
}

/// The strip a key with a LED is on (those go down while held and act when let go): PLAY ON and
/// RECORD FROM, MAIN, CUE, MUTE, SPLIT.
fn key_of(h: &Hit) -> Option<(usize, usize)> {
    match h {
        Hit::To(at, _) | Hit::Main(at) | Hit::Cue(at) | Hit::Mute(at) | Hit::Chan(at) => Some(*at),
        _ => None,
    }
}

/// A card's PROFILE key in its heading (the right half of a module `w` wide).
fn profile_key(w: f64) -> [f64; 4] {
    [w / 2.0 + 2.0, (HEAD - 21.0) / 2.0 + 2.0, w / 2.0 - 10.0, 21.0]
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
    hits: Vec<(Hit, [f64; 4])>,
    pics: HashMap<String, ModPic>,
    inbox: painter::Inbox,
    /// the cap, the lit LEDs (by columns and how many lit), the numbers, the sections' names
    cap: Option<(u64, Layer)>,
    leds: HashMap<(usize, u8, u8, u64), Layer>,
    texts: HashMap<(String, u64), Layer>,
    drag: Option<Drag>,
    last_click: Option<(Instant, Hit)>,
}

impl Mixer {
    pub fn new(on: bool) -> Self {
        Mixer { on, ..Mixer::default() }
    }

    /// The modules for what the scene shows now, and where everything is on the console.
    fn layout(&mut self, scene: &Scene, pins: &[String]) {
        let out_default = scene.default_name(true);
        let in_default = scene.default_name(false);
        let mut mods: Vec<Module> = Vec::new();
        for (di, d) in scene.devs.iter().enumerate() {
            let dev = &d.dev;
            let mut strips: Vec<Strip> = (0..dev.groups.len())
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
                    Some(Strip { at: (di, gi), role, node, name: face::name(dev, gi), kind: g.kind, mix, stereo: g.stereo, chans, held, targets: Vec::new(), main, node_name, port, cue, cols: if two { 2 } else { 1 }, level: d.level(gi), x: 0.0, w: STRIP_W })
                })
                .collect();
            // what takes sound in at the left, what gives it out at the right (as the rack's frames)
            strips.sort_by_key(|s| !s.role.takes());
            let profile = dev.card.as_ref().map(|c| c.profile().map_or_else(|| "Off".to_owned(), |p| p.description.clone()));
            if strips.is_empty() && profile.is_none() {
                continue;
            }
            mods.push(Module { key: dev.key.clone(), di, name: dev.name.to_uppercase(), finish: dev.finish, profile, app: dev.finish == Finish::Black, strips, x: 0.0, w: 0.0 });
        }
        // a static order, whatever plays or is the default: the devices by name, then the apps
        mods.sort_by(|a, b| (a.app, &a.name, &a.key).cmp(&(b.app, &b.name, &b.key)));
        // an app's keys: every sink (source) shown, lit where it is linked
        let ends: Vec<End> = mods
            .iter()
            .flat_map(|m| {
                let dev = &scene.devs[m.di].dev;
                m.strips.iter().filter(|s| matches!(s.role, Role::Sink | Role::Source) && s.mix.is_some()).map(move |s| {
                    let several = m.strips.iter().filter(|o| o.role == s.role).count() > 1;
                    let label = if several { format!("{} · {}", dev.name, s.name) } else { dev.name.clone() };
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
        // where everything is
        self.hits.clear();
        let mut x = LEFT;
        for m in &mut mods {
            let mut sx = 6.0;
            for s in &mut m.strips {
                s.x = sx;
                sx += s.w;
            }
            // a fixed width: one strip's, two strips'…; a card that is off, one strip's
            m.w = m.strips.len().max(1) as f64 * STRIP_W + 12.0;
            m.x = x;
            x += m.w + GAP;
            let (mx, my) = (m.x, TOP);
            if m.profile.is_some() {
                let [px, py, pw, ph] = profile_key(m.w);
                self.hits.push((Hit::Profile(m.di), [mx + px, my + py, pw, ph]));
            }
            let sy = my + HEAD;
            for s in &m.strips {
                let x0 = mx + s.x;
                let gw = 10.0 + SMALL.width(&format!("#{}", s.node));
                self.hits.push((Hit::Graph(s.node), [x0 + 8.0, sy + 5.0, gw, 14.0]));
                let (kx, kw) = (x0 + 6.0, s.w - 12.0);
                match s.role {
                    Role::Play | Role::Rec => {
                        for (i, t) in s.targets.iter().enumerate().take(6) {
                            self.hits.push((Hit::To(s.at, t.at), [kx, sy + ROUTE_Y + 17.0 + i as f64 * 23.0, kw, 17.0]));
                        }
                    }
                    Role::Sink | Role::Source => {
                        if s.mix.is_some() && s.node_name.is_some() {
                            self.hits.push((Hit::Main(s.at), [kx, sy + ROUTE_Y + 3.0, kw, 19.0]));
                            self.hits.push((Hit::All(s.at), [kx, sy + ROUTE_Y + 28.0, kw, 19.0]));
                        }
                        if s.port.is_some() {
                            self.hits.push((Hit::Port(m.di), [kx, sy + ROUTE_Y + 53.0, kw, 19.0]));
                        }
                    }
                }
                let half = (s.w - 12.0 - 7.0) / 2.0;
                self.hits.push((Hit::Cue(s.at), [kx, sy + KEYS_Y, half, 24.0]));
                if s.mix.is_some() {
                    self.hits.push((Hit::Mute(s.at), [kx + half + 7.0, sy + KEYS_Y, half, 24.0]));
                    if s.stereo {
                        self.hits.push((Hit::Chan(s.at), [kx, sy + CHAN_Y, kw, 24.0]));
                    }
                    let (_, slots, _) = fader_x(s);
                    for (i, slot) in slots.iter().enumerate() {
                        let ch = if s.chans { Some(i) } else { None };
                        self.hits.push((Hit::Fader(s.at, ch, sy + SLOT_Y), [x0 + slot, sy + SLOT_Y, 26.0, TRAVEL + 12.0]));
                    }
                }
            }
        }
        self.size = (x - GAP + LEFT, TOP + MOD_H + 16.0);
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
            Hit::Main(at) => format!("Make {} the default: new apps go there", name(at)),
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
        self.inbox.busy()
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
        self.layout(scene, pins);
        self.clamp(size, dpi);
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
        for m in &self.modules {
            let (skin, print) = face::paint(m.finish, m.w, MOD_H, RADIUS);
            let svg = module_svg(m, print);
            let sig = painter::sig(&svg, dpi);
            let p = self.pics.entry(m.key.clone()).or_insert(ModPic { sig: 0, layer: None, want: None });
            if p.layer.is_none() {
                // the first one at once: nothing to show meanwhile
                let pm = face::raster(&svg_options(), &svg, &skin, dpi);
                p.layer = Some(Layer { id: next_id(), pm, x: -FACE_PAD, y: -FACE_PAD, w: m.w + 2.0 * FACE_PAD, h: MOD_H + 2.0 * FACE_PAD });
                p.sig = sig;
            } else if p.sig != sig && p.want.is_none_or(|w| w.0 != sig) {
                self.inbox.ask_skinned(painter::key("mixer", &m.key), sig, svg, dpi, Some(skin));
                p.want = Some((sig, m.w, MOD_H));
            }
        }
        // the small ones: the cap, the lit LEDs, the numbers
        let scale = dpi.to_bits();
        if self.cap.as_ref().is_none_or(|(s, _)| *s != scale) {
            self.cap = Some((scale, small(&cap_svg(), CAP.0, CAP.1, dpi)));
        }
        let mut wanted_leds = Vec::new();
        let mut wanted_texts = Vec::new();
        for s in self.modules.iter().flat_map(|m| &m.strips) {
            let lit = s.level.map(|l| l.round().clamp(0.0, 10.0) as u8);
            wanted_leds.push((s.cols, lit[0], if s.cols == 2 { lit[1] } else { 0 }, scale));
            wanted_texts.push((readout(s), scale));
        }
        for k in wanted_leds {
            self.leds.entry(k).or_insert_with(|| small(&leds_svg(k.0, k.1, k.2), k.0 as f64 * 8.0 + 4.0, LEDS_H, dpi));
        }
        for name in ["DEVICES", "APPS"] {
            wanted_texts.push((name.to_owned(), scale));
        }
        for k in wanted_texts {
            if !self.texts.contains_key(&k) {
                let w = SECTION.width(&k.0) + 8.0;
                let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="14" font-family="Noto Sans" font-weight="bold">{}</svg>"#, label_text(&k.0));
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
        let (fw, top, bottom) = Mixer::field(size, dpi);
        let clip = Some([0.0, top * dpi, fw * dpi, (bottom - top) * dpi]);
        let (ox, oy) = (-self.sx, panel::H - self.sy);
        let mut out: Vec<Pic> = Vec::new();
        let px = |x: f64| (x * dpi).round();
        let mut first_app = None;
        let mut first_dev = None;
        for m in &self.modules {
            if m.app { first_app.get_or_insert(m.x) } else { first_dev.get_or_insert(m.x) };
            if let Some(l) = self.pics.get(&m.key).and_then(|p| p.layer.as_ref()) {
                out.push((l, px(ox + m.x + l.x), px(oy + TOP + l.y), l.w * dpi, l.h * dpi, clip, 1.0));
            }
        }
        // the sections' names over the modules
        for (name, x) in [("DEVICES", first_dev), ("APPS", first_app)] {
            if let (Some(x), Some(l)) = (x, self.texts.get(&(name.to_owned(), scale))) {
                out.push((l, px(ox + x + 2.0), px(oy + 6.0), l.w * dpi, l.h * dpi, clip, 1.0));
            }
        }
        for m in &self.modules {
            let sy = TOP + HEAD;
            for s in &m.strips {
                let x0 = m.x + s.x;
                let (_, slots, meter) = fader_x(s);
                if let Some((vol, bal, _)) = s.mix
                    && let Some((_, cap)) = &self.cap
                {
                    let lr = channels((vol, bal));
                    for (i, slot) in slots.iter().enumerate() {
                        let v = if s.chans { lr[i] } else { vol };
                        out.push((cap, px(ox + x0 + slot), px(oy + sy + SLOT_Y + cap_y(v) - CAP.1 / 2.0), CAP.0 * dpi, CAP.1 * dpi, clip, 1.0));
                    }
                }
                let lit = s.level.map(|l| l.round().clamp(0.0, 10.0) as u8);
                if let Some(l) = self.leds.get(&(s.cols, lit[0], if s.cols == 2 { lit[1] } else { 0 }, scale)) {
                    out.push((l, px(ox + x0 + meter), px(oy + sy + FBOX_Y + 12.0), l.w * dpi, l.h * dpi, clip, 1.0));
                }
                if let Some(l) = self.texts.get(&(readout(s), scale)) {
                    out.push((l, px(ox + x0 + (s.w - l.w) / 2.0), px(oy + sy + READ_Y), l.w * dpi, l.h * dpi, clip, 1.0));
                }
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
        self.layout(scene, &[]);
        &self.modules
    }
}

/// A small picture made here (the cap, the lit LEDs, a number), `w`×`h` logical pixels.
fn small(svg: &str, w: f64, h: f64, dpi: f64) -> Layer {
    Layer { id: next_id(), pm: rasterize(&svg_options(), svg, dpi).or_else(|| Pixmap::new(1, 1)), x: 0.0, y: 0.0, w, h }
}

/// What a strip's number says: its volume, or L and R.
fn readout(s: &Strip) -> String {
    match s.mix {
        None => String::new(),
        Some((v, b, _)) if s.chans => {
            let [l, r] = channels((v, b));
            format!("{:.0}% {:.0}%", l * 100.0, r * 100.0)
        }
        Some((v, ..)) => format!("{:.0}%", v * 100.0),
    }
}

/// A number under a fader, or a section's name: light print on the console.
fn label_text(t: &str) -> String {
    let red = t.split(' ').any(|p| p.trim_end_matches('%').parse::<f64>().is_ok_and(|v| v > 100.0));
    let colour = if t.ends_with('%') { if red { "#ff4a32" } else { "#d5d7d9" } } else { "#8a8d91" };
    SECTION.text(4.0, 11.0, colour, "start", t)
}

fn cap_svg() -> String {
    let (w, h) = CAP;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5a5b5f"/><stop offset=".45" stop-color="#2b2c2f"/><stop offset=".55" stop-color="#1b1c1e"/><stop offset="1" stop-color="#3c3d41"/></linearGradient></defs><rect x=".5" y=".5" width="{}" height="{}" rx="3" fill="url(#g)" stroke="#0b0b0c"/><rect x="3" y="{}" width="{}" height="2" fill="#e6e7e8"/></svg>"##,
        w - 1.0,
        h - 1.0,
        h / 2.0 - 1.0,
        w - 6.0
    )
}

/// The lit LEDs of a meter (`cols` columns, `l` and `r` lit from the bottom): green, then amber,
/// then red, as on the rack's faces.
fn leds_svg(cols: usize, l: u8, r: u8) -> String {
    let w = cols as f64 * 8.0 + 4.0;
    let mut s = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{LEDS_H}">"#);
    let step = (LEDS_H - 6.0) / 10.0;
    for c in 0..cols {
        let n = if c == 0 { l } else { r };
        for i in 0..n.min(10) {
            let colour = if i >= 9 { "#ff3b2f" } else if i >= 7 { "#ffb02e" } else { "#4cff6a" };
            let y = LEDS_H - 3.0 - (i as f64 + 1.0) * step + 1.0;
            let _ = write!(s, r#"<rect x="{}" y="{y}" width="6" height="{}" rx="1" fill="{colour}"/>"#, 3.0 + c as f64 * 8.0, step - 2.0);
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
fn module_svg(m: &Module, print: &str) -> String {
    let (w, h) = (m.w, MOD_H);
    let (dw, dh) = (w + 2.0 * FACE_PAD, h + 2.0 * FACE_PAD);
    let mut o = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}" font-family="Noto Sans" font-weight="bold"><defs>{}</defs><g transform="translate({FACE_PAD},{FACE_PAD})">"##, panel::KEY_DEFS);
    let _ = write!(o, r##"<rect x=".5" y=".5" width="{}" height="{}" rx="{RADIUS}" fill="none" stroke="#000" stroke-opacity=".7"/>"##, w - 1.0, h - 1.0);
    // the heading: a card's name in the left half and its PROFILE key, named by the profile, in
    // the right; anything else its name across it
    match &m.profile {
        Some(p) => {
            o.push_str(&SECTION.text(w / 4.0 + 2.0, 22.0, print, "middle", &fit(&m.name, SECTION, w / 2.0 - 14.0)));
            let k = profile_key(w);
            key(&mut o, k, &fit(p, SMALL, k[2] - 16.0), None, None, false);
        }
        None => o.push_str(&SECTION.text(w / 2.0, 22.0, print, "middle", &fit(&m.name, SECTION, w - 20.0))),
    }
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
        match s.role {
            Role::Play | Role::Rec => {
                o.push_str(&SMALL.text(x0 + s.w / 2.0, ry + 8.0, print, "middle", if s.role == Role::Play { "PLAY ON" } else { "RECORD FROM" }));
                for (i, t) in s.targets.iter().enumerate().take(6) {
                    let down = s.held.as_ref().is_some_and(|h| matches!(h, Hit::To(_, to) if *to == t.at));
                    push_key(&mut o, [kx, ry + 17.0 + i as f64 * 23.0, kw, 17.0], &t.label, t.lit, down);
                }
                if s.targets.is_empty() {
                    o.push_str(&SMALL.text(x0 + s.w / 2.0, ry + 30.0, print, "middle", "NOWHERE"));
                }
            }
            Role::Sink | Role::Source => {
                if s.mix.is_some() && s.node_name.is_some() {
                    let out = s.role == Role::Sink;
                    key(&mut o, [kx, ry + 3.0, kw, 19.0], if out { "MAIN OUT" } else { "MAIN IN" }, Some(("#ffb02e", s.main)), None, held(|h| matches!(h, Hit::Main(_))));
                    key(&mut o, [kx, ry + 28.0, kw, 19.0], if out { "PLAY ALL HERE" } else { "RECORD ALL HERE" }, None, None, false);
                }
                if let Some(p) = &s.port {
                    key(&mut o, [kx, ry + 53.0, kw, 19.0], "PORT", None, Some(p), false);
                }
            }
        }
        let half = (s.w - 12.0 - 7.0) / 2.0;
        led_key(&mut o, [kx, sy + KEYS_Y, half, 24.0], "CUE", "#5fd4ff", s.cue, false, held(|h| matches!(h, Hit::Cue(_))));
        let muted = s.mix.is_some_and(|m| m.2);
        led_key(&mut o, [kx + half + 7.0, sy + KEYS_Y, half, 24.0], "MUTE", "#ff4a32", muted, s.mix.is_none(), held(|h| matches!(h, Hit::Mute(_))));
        led_key(&mut o, [kx, sy + CHAN_Y, kw, 24.0], "SPLIT", "#ffb02e", s.chans, !s.stereo || s.mix.is_none(), held(|h| matches!(h, Hit::Chan(_))));
        // the faders: their scale, its zones and lines, the slots; the meter's dark LEDs
        let (sx, slots, mx) = fader_x(s);
        let top = sy + SLOT_Y;
        if s.mix.is_some() {
            for m in [150.0, 100.0, 75.0, 50.0, 25.0, 0.0] {
                o.push_str(&SMALL.text(x0 + sx + 18.0, top + cap_y(m / 100.0) + 2.6, print, "end", &format!("{m}")));
            }
            // (on the pixel grid: each line one sharp pixel)
            let px = |v: f64| (top + cap_y(v)).round() + 0.5;
            for (i, slot) in slots.iter().enumerate() {
                let cx = x0 + slot + 13.0;
                if s.chans {
                    o.push_str(&SMALL.text(cx, top - 3.0, print, "middle", if i == 0 { "L" } else { "R" }));
                }
                // the zones behind the lines: up to 100% the print's grey, past it to the top a dull
                // red; narrower than the cap
                let (y0, y100, ytop) = (px(0.0), px(1.0), px(VOL_MAX));
                let _ = write!(o, r##"<rect x="{}" y="{y100}" width="16" height="{}" fill="{print}" fill-opacity=".26"/><rect x="{}" y="{ytop}" width="16" height="{}" fill="#b4483c" fill-opacity=".7"/>"##, cx - 8.0, y0 - y100, cx - 8.0, y100 - ytop);
                // a line at every 12.5%: long at the numbers (100% the strongest), shorter at 125%,
                // short between
                for k in 0..=12 {
                    let v = k as f64 * 12.5;
                    let major = [0.0, 25.0, 50.0, 75.0, 100.0, 150.0].contains(&v);
                    let mid = v == 125.0;
                    let half = if major { 8.0 } else if mid { 5.5 } else { 4.0 };
                    let op = if v == 100.0 { 0.95 } else if major { 0.75 } else if mid { 0.6 } else { 0.5 };
                    let _ = write!(o, r#"<path d="M{},{} h{}" stroke="{print}" stroke-opacity="{op}"/>"#, cx - half, px(v / 100.0), half * 2.0);
                }
                let _ = write!(o, r##"<rect x="{}" y="{}" width="4" height="{TRAVEL}" rx="2" fill="#0b0b0c"/>"##, cx - 2.0, top + 6.0);
            }
        } else {
            let _ = write!(o, r#"<text transform="translate({},{}) rotate(-90)" font-size="7.5" letter-spacing="1" fill="{print}" fill-opacity=".6" text-anchor="middle">NO VOLUME IN PIPEWIRE</text>"#, x0 + sx + 40.0, top + TRAVEL / 2.0 + 6.0);
        }
        let mw = s.cols as f64 * 8.0 + 4.0;
        let my = sy + FBOX_Y + 12.0;
        let _ = write!(o, r##"<rect x="{}" y="{my}" width="{mw}" height="{LEDS_H}" rx="3" fill="#0b0b0c"/>"##, x0 + mx);
        let step = (LEDS_H - 6.0) / 10.0;
        for c in 0..s.cols {
            for i in 0..10 {
                let y = my + LEDS_H - 3.0 - (i as f64 + 1.0) * step + 1.0;
                let dark = if i >= 9 { "#351612" } else if i >= 7 { "#2f2810" } else { "#1c2a1f" };
                let _ = write!(o, r#"<rect x="{}" y="{y}" width="6" height="{}" rx="1" fill="{dark}"/>"#, x0 + mx + 3.0 + c as f64 * 8.0, step - 2.0);
            }
        }
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
