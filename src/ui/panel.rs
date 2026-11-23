//! The top panel (a strip of painted metal with hardware keys, the CABLES switch and the LCD), the
//! status bar at the bottom, and the short notices. Sizes are in logical pixels.

use std::fmt::Write;

use super::face::paint_uri;
use super::print::{SECTION, SMALL};
use super::scene::{Layer, next_id, rasterize};
use super::text::{self, esc};
use crate::graph::{Card, PortChoice};

/// height of the top panel; its shadow falls a little below
pub const H: f64 = 64.0;
const SHADOW: f64 = 10.0;
pub const STATUS_H: f64 = 22.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Key {
    Undo,
    Redo,
    Save,
    Desk,
    /// every device and app as a mixing console
    Mixer,
    Rack,
    /// the real PipeWire graph
    Engine,
    Lock,
    Align,
    /// the cords switch: the knob, or one of its printed positions
    Cables,
    CablesAt(u8),
    Arrange,
    Fit,
    /// the list of hidden devices
    Hidden,
    /// the light in the cords, showing which way they go
    Flow,
    Lcd,
    /// the screen of the default output and input
    Defaults,
    /// CUE mode on and off, and the cue's level
    Cue,
    CueLevel,
    /// the groups the window is too narrow for, dropped below the panel
    More,
}

impl Key {
    /// A key for the desk and the rack only, off in ENGINE. (CUE and its LEVEL work there too:
    /// ENGINE shows the cue, and its key turns it off.)
    pub fn desk_only(self) -> bool {
        matches!(self, Key::Lock | Key::Align | Key::Cables | Key::CablesAt(_) | Key::Arrange | Key::Flow | Key::Hidden)
    }

    /// What it does, for the status bar.
    pub fn hint(self) -> &'static str {
        match self {
            Key::Undo => "Undo (Ctrl+Z)",
            Key::Redo => "Redo (Ctrl+Shift+Z)",
            Key::Save => "Save as a new preset (Ctrl+S)",
            Key::Desk => "Devices anywhere on a desk",
            Key::Mixer => "Every device and app as a mixing console: volumes, where each app plays, the defaults (M)",
            Key::Rack => "Devices in a 19\" rack (R)",
            Key::Engine => "What PipeWire really has: every node, port and link, drivers, timings, issues (E)",
            Key::Lock => "Lock devices in place (L)",
            Key::Align => "Line devices up while dragging them on the desk; off: place them freely (G)",
            Key::Cables => "Cords: show · fade · hide: drag, turn the wheel or click (C)",
            Key::CablesAt(0) => "Show the cords",
            Key::CablesAt(1) => "Fade the cords",
            Key::CablesAt(_) => "Hide the cords",
            Key::Arrange => "Arrange devices by signal flow (A)",
            Key::Fit => "Fit everything in the window; in the rack: back to its top (F)",
            Key::Flow => "Show which way the cords go: a light runs along each from its output to its input (W)",
            Key::Hidden => "Hidden devices: click to show them again (a device's HIDE key, or H over it, hides an unused one)",
            Key::Lcd => "Presets",
            Key::Defaults => "The default output and input, where new apps play and record: click to choose others or find them",
            Key::Cue => "Listen (cue): CUE mode on/off: hover a jack to hear it, click to keep it on, patch through its hole, or hold Alt over it; in ENGINE click a port",
            Key::CueLevel => "How loud the cue is: drag up/down or turn the wheel",
            Key::More => "The keys the window is too narrow for",
        }
    }
}

/// What the panel shows.
#[derive(Clone, PartialEq, Hash)]
pub struct State {
    pub rack: bool,
    /// ENGINE is shown: the desk's keys are off
    pub engine: bool,
    /// MIXER is shown: the desk's keys are off too
    pub mixer: bool,
    pub locked: bool,
    /// devices dragged on the desk stick to each other's edges and centres
    pub align: bool,
    /// 0 show, 1 fade, 2 hide
    pub cables: u8,
    pub lcd: [String; 2],
    /// the defaults screen's lines: the default output's name and the default input's
    pub defaults: [String; 2],
    /// the CUE key's LED (lit in CUE mode, blinking while probing with Alt) and the cue's level,
    /// 0…100
    pub cue_led: bool,
    pub cue_level: u8,
    /// how many devices are hidden by hand, printed on the HIDDEN key
    pub hidden: usize,
    /// the light in the cords
    pub flow: bool,
    pub pressed: Option<Key>,
    /// MORE's drop-down is open
    pub more: bool,
}

impl State {
    /// A key this screen can't use, greyed: the desk's keys in ENGINE, ALIGN in the rack.
    pub fn off(&self, k: Key) -> bool {
        ((self.engine || self.mixer) && k.desk_only()) || (self.rack && !self.engine && !self.mixer && k == Key::Align)
    }
}

/// A part of the panel: a group of keys, the logo or the screen (or, folded into MORE, one of
/// the screen's halves).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Part {
    Logo,
    Edit,
    /// DESK · MIXER · RACK · ENGINE, and LOCK · ALIGN while they are in its group
    Layout,
    LockAlign,
    Cables,
    View,
    Cue,
    Lcd,
    LcdDefaults,
    LcdPreset,
}

/// keys and where they are (x, y, w, h)
type Keys = Vec<(Key, [f64; 4])>;

/// the room between groups
const GAP: f64 = 22.0;
/// the screen is narrower than this: it folds into MORE
const LCD_MIN: f64 = 260.0;
const MORE_W: f64 = 70.0;

pub struct Panel {
    pub state: State,
    /// hit areas, in logical pixels
    rects: Vec<(Key, [f64; 4])>,
    layer: Option<(u64, Layer)>,
    /// the panel being drawn on the painter's threads (the old one shows meanwhile): what for, its
    /// width
    want: Option<(u64, f64)>,
    inbox: super::painter::Inbox,
    /// the drop-down under the LCD, while open
    pub menu: Option<Menu>,
    menu_rects: Vec<(MenuHit, [f64; 4])>,
    menu_layer: Option<(u64, Layer)>,
    /// what is folded into MORE, MORE's keys and its drop-down (with where it is), while open
    folded: Vec<Part>,
    more_rects: Vec<(Key, [f64; 4])>,
    more_layer: Option<(u64, Layer)>,
    /// the bare panel (see `tile`), at a dpi
    tile: Option<(u64, Layer)>,
    /// MORE's drop-down being drawn on the painter's threads: what for, where
    more_want: Option<(u64, [f64; 4])>,
}

/// The drop-down under the LCD: a name field to save a new preset, or the list of presets.
#[derive(Clone, PartialEq, Hash)]
pub enum Menu {
    Save { text: String, placeholder: String },
    /// `confirm`: the row whose delete button was pressed once and now asks "DEL?"
    Load { names: Vec<String>, current: Option<usize>, confirm: Option<usize> },
    /// the outputs and inputs that can be the default, by name, and which ones are
    Defaults { outs: Vec<String>, ins: Vec<String>, out: Option<usize>, inp: Option<usize> },
    /// the devices hidden by hand (name, model), and the nodes always left out (who, what)
    Hidden { mine: Vec<(String, String)>, system: Vec<(String, String)> },
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuHit {
    Row(usize),
    Delete(usize),
    Save,
    /// an output (true) or input to make the default, or to find on the desk
    Default(bool, usize),
    Find(bool, usize),
    /// a hidden device to show again
    Show(usize),
    /// somewhere else on the drop-down
    Inside,
}

const MENU_W: f64 = 300.0;
/// the defaults' drop-down is wider: device names are long
const DEFAULTS_W: f64 = 360.0;
/// width of a card's profile list
pub const PROFILE_W: f64 = 360.0;

/// What a click in a card's profile list hits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProfileHit {
    /// a family's row (by its place in `Card::families`)
    Row(usize),
    /// a codec of the profile in use: that profile's index
    Codec(u32),
    /// in a card's port list: a port (group, route index) and a latency step (group, ms)
    Port(usize, u32),
    Latency(usize, i64),
    Inside,
}

/// A card's profile list, amber like the LCDs: a row per family (the one in use dark), its codecs
/// as choices under it, profiles with nothing plugged in dimmed and marked. The layer is at (0, 0),
/// `PROFILE_W` wide; the hit areas are relative to it.
pub fn profile_menu(card: &Card, dpi: f64) -> (Layer, Vec<(ProfileHit, [f64; 4])>) {
    const INK: &str = "#3a2408";
    let w = PROFILE_W;
    // (no title: it drops out of the PROFILE key that names it)
    let mut o = String::new();
    let mut hits = Vec::new();
    let mut y = 8.0;
    let current = card.profile().map(|p| Card::split(&p.description).0);
    for (fi, f) in card.families().iter().enumerate() {
        let cur = current.as_deref() == Some(f.name.as_str());
        let ink = if cur { "#f7bb62" } else { INK };
        if cur {
            let _ = write!(o, r#"<rect x="7" y="{}" width="{}" height="22" rx="3" fill="{INK}"/>"#, y + 1.0, w - 14.0);
        }
        let room = if f.unplugged { 37 } else { 47 };
        let name: String = if f.name.chars().count() > room { format!("{}…", f.name.chars().take(room - 1).collect::<String>()) } else { f.name.clone() };
        let _ = write!(o, r#"<text x="12" y="{}" font-size="11.5" fill="{ink}" opacity="{}" xml:space="preserve">{}</text>"#, y + 16.0, if f.unplugged && !cur { 0.55 } else { 1.0 }, esc(&name));
        if f.unplugged {
            let tw = 70.0;
            let _ = write!(
                o,
                r#"<rect x="{}" y="{}" width="{tw}" height="14" rx="2" fill="none" stroke="{ink}" stroke-opacity=".7"/><text x="{}" y="{}" font-size="8.5" letter-spacing=".8" fill="{ink}" opacity=".8" text-anchor="middle">UNPLUGGED</text>"#,
                w - 12.0 - tw,
                y + 5.0,
                w - 12.0 - tw / 2.0,
                y + 15.0
            );
        }
        hits.push((ProfileHit::Row(fi), [0.0, y, w, 24.0]));
        y += 24.0;
        // the codecs of the family in use, as choices under it
        if cur && f.members.iter().any(|(_, c)| c.is_some()) {
            let mut x = 12.0;
            let now = card.current;
            for (index, codec) in &f.members {
                let Some(c) = codec else { continue };
                let cw = c.chars().count() as f64 * 6.6 + 14.0;
                let on = now == Some(*index);
                let _ = write!(
                    o,
                    r#"<rect x="{x}" y="{}" width="{cw}" height="17" rx="3" fill="{}" stroke="{INK}" stroke-width="1.5"/><text x="{}" y="{}" font-size="11" text-anchor="middle" fill="{}">{}</text>"#,
                    y + 4.0,
                    if on { INK } else { "none" },
                    x + cw / 2.0,
                    y + 16.5,
                    if on { "#f7bb62" } else { INK },
                    esc(c)
                );
                hits.push((ProfileHit::Codec(*index), [x, y + 4.0, cw, 17.0]));
                x += cw + 6.0;
            }
            y += 26.0;
        }
    }
    let h = y + 8.0;
    hits.push((ProfileHit::Inside, [0.0, 0.0, w, h]));
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#121214"/><rect x="3" y="3" width="{}" height="{}" rx="3" fill="url(#g)"/>{o}</svg>"##,
        w - 6.0,
        h - 6.0
    );
    (Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h }, hits)
}

/// A section's heading in an amber list: its name, and a rule running to the right edge.
fn section(name: &str, y: f64, w: f64) -> String {
    const INK: &str = "#3a2408";
    let tw = text::width(text::MONO_BOLD, name, 10.0, 1.5);
    format!(
        r#"<text x="12" y="{}" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".8">{}</text><path d="M{},{} H{}" stroke="{INK}" stroke-opacity=".4" stroke-width="1.5"/>"#,
        y + 13.0,
        esc(name),
        12.0 + tw + 8.0,
        y + 9.5,
        w - 12.0
    )
}

/// One output's or input's ports in a card's PORT list.
#[derive(Debug)]
pub struct PortSection {
    pub group: usize,
    /// its frame's name (SPEAKERS, BUILT-IN MIC…)
    pub name: String,
    pub ports: Vec<PortChoice>,
    /// the route index in use
    pub current: Option<u32>,
    pub latency_ns: i64,
}

/// A card's port list, under its PORT key, as amber as the profile list: for each of its outputs
/// and inputs with several ports, its name (when it has several), the ports (the one in use dark,
/// the unplugged dimmed and marked), and its latency offset with − and + (10 ms steps with Shift).
pub fn port_menu(sections: &[PortSection], dpi: f64) -> (Layer, Vec<(ProfileHit, [f64; 4])>) {
    const INK: &str = "#3a2408";
    let w = PROFILE_W;
    let mut o = String::new();
    let mut hits = Vec::new();
    let mut y = 8.0;
    for (si, PortSection { group: gi, name: label, ports, current, latency_ns: latency }) in sections.iter().enumerate() {
        // its name under a rule, when the card has several outputs and inputs with ports
        if sections.len() > 1 {
            if si > 0 {
                y += 8.0;
            }
            o += &section(label, y, w);
            y += 22.0;
        }
        for p in ports {
            let cur = *current == Some(p.index);
            let ink = if cur { "#f7bb62" } else { INK };
            if cur {
                let _ = write!(o, r#"<rect x="7" y="{}" width="{}" height="22" rx="3" fill="{INK}"/>"#, y + 1.0, w - 14.0);
            }
            let room = if p.unplugged { 37 } else { 47 };
            let shown: String = if p.description.chars().count() > room { format!("{}…", p.description.chars().take(room - 1).collect::<String>()) } else { p.description.clone() };
            let _ = write!(o, r#"<text x="12" y="{}" font-size="11.5" fill="{ink}" opacity="{}" xml:space="preserve">{}</text>"#, y + 16.0, if p.unplugged && !cur { 0.55 } else { 1.0 }, esc(&shown));
            if p.unplugged {
                let tw = 70.0;
                let _ = write!(
                    o,
                    r#"<rect x="{}" y="{}" width="{tw}" height="14" rx="2" fill="none" stroke="{ink}" stroke-opacity=".7"/><text x="{}" y="{}" font-size="8.5" letter-spacing=".8" fill="{ink}" opacity=".8" text-anchor="middle">UNPLUGGED</text>"#,
                    w - 12.0 - tw,
                    y + 5.0,
                    w - 12.0 - tw / 2.0,
                    y + 15.0
                );
            }
            hits.push((ProfileHit::Port(*gi, p.index), [0.0, y, w, 24.0]));
            y += 24.0;
        }
        // its latency offset: − value +
        y += 4.0;
        // a dotted line: the latency is the section's footer
        let _ = write!(o, r#"<path d="M14,{y} H{}" stroke="{INK}" stroke-opacity=".45" stroke-width="1.5" stroke-dasharray="1.5 3" stroke-linecap="round"/>"#, w - 14.0);
        let ly = y + 6.0;
        let _ = write!(o, r#"<text x="12" y="{}" font-size="10" letter-spacing=".8" fill="{INK}" opacity=".75">LATENCY OFFSET</text>"#, ly + 13.0);
        let ms = *latency as f64 / 1e6;
        let value = if ms.fract().abs() < 1e-9 { format!("{ms:.0} ms") } else { format!("{ms:.1} ms") };
        let (bm, bp) = (w - 12.0 - 22.0 - 70.0 - 22.0, w - 12.0 - 22.0);
        for (bx, sign) in [(bm, "−"), (bp, "+")] {
            let _ = write!(o, r##"<rect x="{bx}" y="{ly}" width="22" height="18" rx="3" fill="{INK}"/><text x="{}" y="{}" font-size="12" fill="#f7bb62" text-anchor="middle">{sign}</text>"##, bx + 11.0, ly + 13.5);
        }
        let _ = write!(o, r#"<text x="{}" y="{}" font-size="11.5" fill="{INK}" text-anchor="middle">{value}</text>"#, bm + 22.0 + 35.0, ly + 13.5);
        hits.push((ProfileHit::Latency(*gi, -1), [bm, ly, 22.0, 18.0]));
        hits.push((ProfileHit::Latency(*gi, 1), [bp, ly, 22.0, 18.0]));
        y = ly + 26.0;
    }
    let h = y + 4.0;
    hits.push((ProfileHit::Inside, [0.0, 0.0, w, h]));
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#121214"/><rect x="3" y="3" width="{}" height="{}" rx="3" fill="url(#g)"/>{o}</svg>"##,
        w - 6.0,
        h - 6.0
    );
    (Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h }, hits)
}

// icons on a 24-unit grid, drawn with round 2-unit strokes
const ICONS: &[(Key, &str)] = &[
    (Key::Undo, r#"<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>"#),
    (Key::Redo, r#"<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>"#),
    (Key::Save, r#"<path d="M5 3h11l3 3v15H5z"/><path d="M8 3v5h7V3"/><path d="M8 21v-7h8v7"/>"#),
    (Key::Lock, r#"<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>"#),
    (Key::Align, r#"<path d="M4 3v18"/><rect x="7.5" y="5" width="12" height="5" rx="1"/><rect x="7.5" y="14" width="7.5" height="5" rx="1"/>"#),
    (Key::Arrange, r#"<rect x="3.5" y="4" width="7" height="5" rx="1"/><rect x="3.5" y="15" width="7" height="5" rx="1"/><rect x="13.5" y="9.5" width="7" height="5" rx="1"/><path d="M10.5 6.5h1.5v11h-1.5M12 12h1.5"/>"#),
    (Key::Fit, r#"<path d="M4 9V4h5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/><path d="M20 15v5h-5"/>"#),
    (Key::Flow, r#"<path d="M3 17c3-6 6-6 9 0s6 6 9 0"/><path d="M4 7h14"/><path d="m15 4 3 3-3 3"/>"#),
    (Key::Hidden, r#"<path d="M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6z"/><circle cx="12" cy="12" r="2.6"/><path d="M4 20 20 4"/>"#),
    (Key::Cue, r#"<path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4.5" height="7" rx="1.5"/><rect x="16.5" y="14" width="4.5" height="7" rx="1.5"/>"#),
];

const KEY_Y: f64 = 10.0;
/// the knobs' centre line, a little low so the label above the cords switch clears the edge
const KNOB_Y: f64 = 30.0;
const KEY_H: f64 = 30.0;
const LABEL: &str = "#26272a";

impl Panel {
    pub fn new(state: State) -> Self {
        Panel { state, tile: None, rects: Vec::new(), layer: None, want: None, inbox: Default::default(), menu: None, menu_rects: Vec::new(), menu_layer: None, folded: Vec::new(), more_rects: Vec::new(), more_layer: None, more_want: None }
    }

    /// Where a key is (x, y, w, h), once the panel has been drawn.
    pub fn rect(&self, key: Key) -> Option<[f64; 4]> {
        self.keys().find(|(k, _)| *k == key).map(|(_, r)| *r)
    }

    /// The keys on the panel and in MORE's drop-down while it is open.
    fn keys(&self) -> impl Iterator<Item = &(Key, [f64; 4])> {
        self.rects.iter().chain(self.more_rects.iter().filter(|_| self.state.more))
    }

    /// The key under a point in logical pixels.
    pub fn hit(&self, x: f64, y: f64) -> Option<Key> {
        self.keys().find(|(_, [rx, ry, rw, rh])| x >= *rx && x <= rx + rw && y >= *ry && y <= ry + rh).map(|(k, _)| *k)
    }

    /// A point in logical pixels is on MORE's open drop-down.
    pub fn in_more(&self, x: f64, y: f64) -> bool {
        // where it is, or will be in a moment
        let at = self.more_want.map(|(_, at)| at).or_else(|| self.more_layer.as_ref().map(|(_, l)| [l.x, l.y, l.w, l.h]));
        self.state.more && at.is_some_and(|[lx, ly, lw, lh]| x >= lx && x <= lx + lw && y >= ly && y <= ly + lh - SHADOW)
    }

    /// How far down MORE's open drop-down reaches (logical pixels), if it is open.
    pub fn more_bottom(&self) -> Option<f64> {
        let at = self.more_want.map(|(_, at)| at).or_else(|| self.more_layer.as_ref().map(|(_, l)| [l.x, l.y, l.w, l.h]));
        at.filter(|_| self.state.more).map(|[_, ly, _, lh]| ly + lh - SHADOW)
    }

    /// The panel as a layer `width` logical pixels wide, drawn at `dpi`.
    pub fn layer(&mut self, width: f64, dpi: f64) -> &Layer {
        for done in self.inbox.take() {
            if let Some((sig, w)) = self.want
                && sig == done.sig
            {
                self.layer = Some((sig, Layer { id: next_id(), pm: done.pm, x: 0.0, y: 0.0, w, h: H + SHADOW }));
                self.want = None;
            } else if let Some((sig, [x, y, w, h])) = self.more_want
                && sig == done.sig
            {
                self.more_layer = Some((sig, Layer { id: next_id(), pm: done.pm, x, y, w, h }));
                self.more_want = None;
            }
        }
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(&self.state, width.to_bits(), dpi.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if self.layer.as_ref().is_none_or(|(s, _)| *s != sig) && self.want.map(|w| w.0) != Some(sig) {
            // where the keys are, at once; their picture on the painter's threads (the first one
            // here: there is nothing to show meanwhile; a window grown meanwhile shows the bare
            // panel, `tile`, beyond the old one)
            let (svg, rects, folded) = self.svg(width);
            self.rects = rects;
            self.folded = folded;
            if self.layer.is_none() {
                let pm = rasterize(&text::svg_options(), &svg, dpi);
                self.layer = Some((sig, Layer { id: next_id(), pm, x: 0.0, y: 0.0, w: width, h: H + SHADOW }));
            } else {
                self.want = Some((sig, width));
                self.inbox.ask(super::painter::key("panel", 0u8), sig, svg, dpi);
            }
        }
        if self.tile.as_ref().is_none_or(|(d, _)| *d != dpi.to_bits()) {
            let pm = rasterize(&text::svg_options(), &bare_svg(TILE_W), dpi);
            self.tile = Some((dpi.to_bits(), Layer { id: next_id(), pm, x: 0.0, y: 0.0, w: TILE_W, h: H + SHADOW }));
        }
        // MORE's drop-down, on the painter's threads too (the old one shows meanwhile); where its
        // keys are, at once
        if self.folded.is_empty() {
            self.state.more = false;
        }
        if !self.state.more {
            (self.more_layer, self.more_want) = (None, None);
        } else {
            let mut h = std::hash::DefaultHasher::new();
            std::hash::Hash::hash(&(&self.state, &self.folded, width.to_bits(), dpi.to_bits()), &mut h);
            let sig = std::hash::Hasher::finish(&h);
            if self.more_layer.as_ref().is_none_or(|(s, _)| *s != sig) && self.more_want.map(|w| w.0) != Some(sig) {
                let (svg, rects, at) = self.more_svg(width);
                self.more_rects = rects;
                self.more_want = Some((sig, at));
                self.inbox.ask(super::painter::key("panel", 1u8), sig, svg, dpi);
            }
        }
        &self.layer.as_ref().unwrap().1
    }

    /// The panel is being drawn again.
    pub fn busy(&self) -> bool {
        self.inbox.busy()
    }

    /// The layer's pixels went to the GPU.
    pub fn uploaded(&mut self) {
        for (_, l) in [self.layer.as_mut(), self.more_layer.as_mut(), self.menu_layer.as_mut()].into_iter().flatten() {
            l.pm = None;
        }
    }

    /// The bare panel, `TILE_W` wide (its paint repeats that often): laid side by side under the
    /// panel's picture, it fills what a window that has grown shows beyond it until the new one
    /// comes.
    pub fn tile(&self) -> Option<&Layer> {
        self.tile.as_ref().map(|(_, l)| l)
    }

    /// The panel and drop-down layers as last prepared.
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        [self.layer.as_ref(), self.more_layer.as_ref(), self.menu_layer.as_ref()].into_iter().flatten().map(|(_, l)| l)
    }

    /// What of the drop-down is under a point in logical pixels; None when it is outside it.
    pub fn menu_hit(&self, x: f64, y: f64) -> Option<MenuHit> {
        self.menu.as_ref()?;
        let inside = |[rx, ry, rw, rh]: [f64; 4]| x >= rx && x <= rx + rw && y >= ry && y <= ry + rh;
        let hit = self.menu_rects.iter().skip(1).find(|(_, r)| inside(*r)).map(|(h, _)| *h);
        hit.or_else(|| self.menu_rects.first().filter(|(_, r)| inside(*r)).map(|_| MenuHit::Inside))
    }

    /// The drop-down as a layer, placed under the LCD of a panel `width` logical pixels wide.
    pub fn menu_layer(&mut self, width: f64, dpi: f64) -> Option<&Layer> {
        let Some(menu) = &self.menu else {
            self.menu_layer = None;
            return None;
        };
        // under its screen: the defaults' under theirs, the presets' against the right edge; below
        // MORE's drop-down while that is open
        let mw = if matches!(menu, Menu::Defaults { .. } | Menu::Hidden { .. }) { DEFAULTS_W } else { MENU_W };
        let x = match (menu, self.rect(Key::Defaults), self.rect(Key::Hidden), self.rect(Key::Lcd)) {
            (Menu::Defaults { .. }, Some([dx, ..]), _, _) => dx.min(width - 6.0 - mw).max(6.0),
            (Menu::Hidden { .. }, _, Some([hx, ..]), _) => (hx - 20.0).min(width - 6.0 - mw).max(6.0),
            (Menu::Load { .. } | Menu::Save { .. }, _, _, Some([lx, _, lw, _])) if self.state.more && self.more_rects.iter().any(|(k, _)| *k == Key::Lcd) => (lx + lw - mw).min(width - 6.0 - mw).max(6.0),
            _ => width - 18.0 - mw,
        };
        let more_bottom = self.more_want.map(|(_, at)| at).or_else(|| self.more_layer.as_ref().map(|(_, l)| [l.x, l.y, l.w, l.h])).filter(|_| self.state.more).map(|[_, y, _, h]| y + h - SHADOW);
        let y = more_bottom.unwrap_or(H) + 4.0;
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(menu, width.to_bits(), dpi.to_bits(), x.to_bits(), y.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if self.menu_layer.as_ref().is_none_or(|(s, _)| *s != sig) {
            let (svg, mh, rects) = menu_svg(menu);
            self.menu_rects = std::iter::once((MenuHit::Inside, [x, y, mw, mh])).chain(rects.into_iter().map(|(h, [rx, ry, rw, rh])| (h, [x + rx, y + ry, rw, rh]))).collect();
            let pm = rasterize(&text::svg_options(), &svg, dpi);
            self.menu_layer = Some((sig, Layer { id: next_id(), pm, x, y, w: mw, h: mh }));
        }
        self.menu_layer.as_ref().map(|(_, l)| l)
    }

    /// The panel's SVG `w` wide, where its keys are, and the groups folded into MORE.
    fn svg(&self, w: f64) -> (String, Keys, Vec<Part>) {
        let (parts, attached, bare, folded, lcd_w) = self.plan(w);
        let mut o = String::new();
        let mut rects = Vec::new();
        let mut x = 18.0;
        for p in parts {
            // the screen against the right edge, as wide as there is room for
            let at = if p == Part::Lcd { w - 18.0 - lcd_w } else { x };
            let (s, r, pw) = self.part(p, at, attached, bare, lcd_w);
            o += &s;
            rects.extend(r);
            if p != Part::Lcd {
                x += pw + if p == Part::Logo { 26.0 } else { GAP };
            }
        }
        if !folded.is_empty() {
            let mx = w - 10.0 - MORE_W;
            o += &more_key(mx, self.state.more || self.state.pressed == Some(Key::More));
            rects.push((Key::More, [mx - 3.0, KEY_Y - 3.0, MORE_W + 6.0, KEY_H + 6.0]));
        }
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{th}" viewBox="0 0 {w} {th}" font-family="Noto Sans" font-weight="bold">
{defs}
<rect y="{H}" width="{w}" height="{SHADOW}" fill="url(#gDrop)"/>
<rect width="{w}" height="{H}" fill="#8c8e91"/><rect width="{w}" height="{H}" fill="url(#pt)"/><rect width="{w}" height="{H}" fill="url(#gStrip)"/>
<path d="M0,.5 H{w}" stroke="#fff" stroke-opacity=".35"/><path d="M0,{b} H{w}" stroke="#000" stroke-opacity=".5"/>
{o}
</svg>"##,
            th = H + SHADOW,
            defs = defs(),
            b = H - 0.5,
        );
        (svg, rects, folded)
    }

    /// What the panel holds when `w` wide: its parts in order, whether LOCK and ALIGN stay in
    /// LAYOUT's group, whether the grey keys are left out, what folds into MORE (in the panel's
    /// order), and the screen's width. When the panel is too wide, first the logo goes, then the
    /// grey keys (what this screen can't use), then groups fold into MORE from the right: the
    /// screen, CUE · LEVEL, VIEW, CABLES, LOCK · ALIGN, EDIT. DESK · RACK · ENGINE stay.
    /// The narrowest the window may be: the panel folded all it can, DESK · RACK · ENGINE and MORE.
    pub fn min_w(&self) -> f64 {
        18.0 + self.part(Part::Layout, 0.0, false, true, 0.0).2 + 12.0 + MORE_W + 10.0
    }

    fn plan(&self, w: f64) -> (Vec<Part>, bool, bool, Vec<Part>, f64) {
        let width = |p: Part, attached: bool, bare: bool| self.part(p, 0.0, attached, bare, 0.0).2;
        let main = [Part::Edit, Part::Layout, Part::Cables, Part::View, Part::Cue];
        for (logo, bare) in [(true, false), (false, false), (false, true)] {
            let mut parts: Vec<Part> = if logo { vec![Part::Logo] } else { Vec::new() };
            parts.extend(main.iter().copied().filter(|p| width(*p, true, bare) > 0.0));
            let used = 18.0 + parts.iter().map(|p| width(*p, true, bare) + if *p == Part::Logo { 26.0 } else { GAP }).sum::<f64>();
            let room = w - 18.0 - used;
            if room >= LCD_MIN {
                parts.push(Part::Lcd);
                return (parts, true, bare, Vec::new(), room.min(600.0));
            }
        }
        let used = |parts: &[Part], attached: bool| {
            18.0 + parts
                .iter()
                .map(|p| match width(*p, attached, true) {
                    0.0 => 0.0,
                    pw => pw + GAP,
                })
                .sum::<f64>()
        };
        let mut parts = main.to_vec();
        let mut attached = true;
        let mut folded = vec![Part::LcdDefaults, Part::LcdPreset];
        for f in [Part::Cue, Part::View, Part::Cables, Part::LockAlign, Part::Edit] {
            // MORE sits closer to its neighbours and the edge than groups do to each other
            if used(&parts, attached) - GAP + 12.0 + MORE_W + 10.0 <= w {
                break;
            }
            if f == Part::LockAlign {
                attached = false;
            } else {
                parts.retain(|p| *p != f);
            }
            if width(f, false, true) > 0.0 {
                folded.push(f);
            }
        }
        parts.retain(|p| width(*p, attached, true) > 0.0);
        folded.sort_by_key(|p| *p as u8);
        (parts, attached, true, folded, 0.0)
    }

    /// A part of the panel drawn from `x`, on the panel's row: its SVG, where its keys are and its
    /// width (none when all its keys are grey and `bare` leaves those out). `attached`: LOCK and
    /// ALIGN are in LAYOUT's group; `lcd_w`: a screen's width.
    fn part(&self, p: Part, x: f64, attached: bool, bare: bool, lcd_w: f64) -> (String, Keys, f64) {
        let st = &self.state;
        let mut o = String::new();
        let mut rects = Vec::new();
        let keep = |k: &(Key, Option<&str>, Option<bool>)| !(bare && st.off(k.0));
        let lock_align = [(Key::Lock, None, Some(st.locked)), (Key::Align, None, Some(st.align))];
        let pw = match p {
            Part::Logo => {
                // the maker's name, pressed into the metal
                let _ = write!(
                    o,
                    r##"<g transform="translate({x},34) scale(1.06,1)" font-size="26" font-weight="900" letter-spacing="1"><text y="1" fill="#fff" opacity=".35">PATCHCORD</text><text fill="#2a2b2e">PATCHCORD</text></g>{sub}"##,
                    sub = SECTION.text(x, 50.0, LABEL, "start", "PC-1 · PIPEWIRE PATCHBAY")
                );
                text::width(text::BLACK, "PATCHCORD", 26.0, 1.0) * 1.06
            }
            Part::Edit => group(st, &mut o, &mut rects, x, &[(Key::Undo, None, None), (Key::Redo, None, None), (Key::Save, None, None)], "EDIT"),
            Part::Layout => {
                let desk = !st.engine && !st.mixer;
                let mut keys = vec![
                    (Key::Desk, Some("DESK"), Some(desk && !st.rack)),
                    (Key::Mixer, Some("MIXER"), Some(st.mixer && !st.engine)),
                    (Key::Rack, Some("RACK"), Some(desk && st.rack)),
                    (Key::Engine, Some("ENGINE"), Some(st.engine)),
                ];
                if attached {
                    keys.extend(lock_align.into_iter().filter(keep));
                }
                group(st, &mut o, &mut rects, x, &keys, "LAYOUT")
            }
            Part::LockAlign if attached => 0.0,
            Part::LockAlign => group(st, &mut o, &mut rects, x, &lock_align.into_iter().filter(keep).collect::<Vec<_>>(), ""),
            Part::Cables if bare && st.off(Key::Cables) => 0.0,
            Part::Cables => {
                // the cords switch: three click positions with their names printed around it
                let (kx, ky) = (x + 44.0, KNOB_Y);
                for (i, (name, anchor, dx, dy)) in [("SHOW", "end", -19.0, 0.0), ("FADE", "middle", 0.0, -20.5), ("HIDE", "start", 19.0, 0.0)].into_iter().enumerate() {
                    let a = SWITCH[i].to_radians();
                    let _ = write!(o, r#"<path d="M{},{} L{},{}" stroke="{LABEL}" stroke-width="1.4"/>"#, kx + 15.0 * a.sin(), ky - 15.0 * a.cos(), kx + 16.8 * a.sin(), ky - 16.8 * a.cos());
                    // the labels stay as printed: the pointer shows the position
                    o.push_str(&SMALL.text(kx + dx * 1.15, ky + dy + 2.5, LABEL, anchor, name));
                    let w = SMALL.width(name) + 4.0;
                    let lx = match anchor {
                        "end" => kx + dx * 1.15 - w,
                        "middle" => kx - w / 2.0,
                        _ => kx + dx * 1.15 - 2.0,
                    };
                    rects.push((Key::CablesAt(i as u8), [lx, ky + dy - 6.0, w, 11.0]));
                }
                o.push_str(&knob_svg(kx, ky, SWITCH[st.cables.min(2) as usize], ""));
                o.push_str(&SECTION.text(kx, 54.0, LABEL, "middle", "CABLES"));
                rects.push((Key::Cables, [kx - 16.0, ky - 16.0, 32.0, 32.0]));
                88.0
            }
            Part::View => {
                let keys: Vec<_> = [(Key::Arrange, None, None), (Key::Fit, None, None), (Key::Flow, None, Some(st.flow)), (Key::Hidden, None, None)].into_iter().filter(keep).collect();
                let pw = group(st, &mut o, &mut rects, x, &keys, "VIEW");
                // how many are hidden, small in the HIDDEN key's corner
                if st.hidden > 0
                    && let Some((_, [hx, hy, hw, _])) = rects.iter().find(|(k, _)| *k == Key::Hidden)
                {
                    let _ = write!(o, r##"<text x="{}" y="{}" font-size="9" font-family="Noto Sans Mono" fill="#f7bb62" text-anchor="end">{}</text>"##, hx + hw - 7.0, hy + 13.0, st.hidden);
                }
                pw
            }
            Part::Cue if bare && st.off(Key::Cue) => 0.0,
            Part::Cue => {
                // CUE: its key (a cyan LED) and, as big as the CABLES knob, the cue's LEVEL
                let kw = 38.0;
                o.push_str(&key_svg(x, kw, st.pressed == Some(Key::Cue)));
                let icon = ICONS.iter().find(|(k, _)| *k == Key::Cue).map_or("", |(_, s)| s);
                let _ = write!(
                    o,
                    r##"<g transform="translate({},{}) scale(.7083)" fill="none" stroke="#e6e7e8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{icon}</g>"##,
                    x + kw / 2.0 - 8.5 + 2.0,
                    KEY_Y + KEY_H / 2.0 - 8.5
                );
                o.push_str(&led_svg(x + 8.0, KEY_Y + 7.0, st.cue_led, "#5fd4ff"));
                rects.push((Key::Cue, [x - 3.0, KEY_Y - 3.0, kw + 6.0, KEY_H + 6.0]));
                let (lx, ly) = (x + kw + 13.0 + 18.0, KEY_Y + KEY_H / 2.0);
                let deg = -135.0 + 270.0 * st.cue_level.min(100) as f64 / 100.0;
                let mut scale = String::new();
                for i in 0..=8 {
                    let b = (-135.0 + 270.0 * i as f64 / 8.0_f64).to_radians();
                    let _ = write!(scale, r#"<path d="M{},{} L{},{}" stroke="{LABEL}" stroke-width="1.2"/>"#, lx + 15.5 * b.sin(), ly - 15.5 * b.cos(), lx + 17.5 * b.sin(), ly - 17.5 * b.cos());
                }
                o.push_str(&knob_svg(lx, ly, deg, &scale).replace("#ffb02e", "#5fd4ff"));
                rects.push((Key::CueLevel, [lx - 18.0, ly - 18.0, 36.0, 36.0]));
                let pw = lx + 18.0 - x;
                o.push_str(&SECTION.text(x + pw / 2.0, 54.0, LABEL, "middle", "CUE · LEVEL"));
                pw
            }
            // the screen in two halves, the default output and input on the left, the preset and
            // the clock on the right, a thin line between them; each half opens its own list. In
            // MORE each half is a screen of its own.
            Part::Lcd => screen(&mut o, &mut rects, x, lcd_w, &[(self.defaults_lines(), true, Key::Defaults), (st.lcd.clone(), false, Key::Lcd)]),
            Part::LcdDefaults => screen(&mut o, &mut rects, x, lcd_w, &[(self.defaults_lines(), true, Key::Defaults)]),
            Part::LcdPreset => screen(&mut o, &mut rects, x, lcd_w, &[(st.lcd.clone(), false, Key::Lcd)]),
        };
        // what this screen can't use is greyed over with the panel's paint
        for (k, [rx, ry, rw, rh]) in &rects {
            if st.off(*k) {
                let _ = write!(o, r##"<rect x="{rx}" y="{ry}" width="{rw}" height="{rh}" rx="5" fill="#8c8e91" opacity=".7"/>"##);
            }
        }
        (o, rects, pw)
    }

    fn defaults_lines(&self) -> [String; 2] {
        [format!("OUT ▸ {}", self.state.defaults[0]), format!("IN  ▸ {}", self.state.defaults[1])]
    }

    /// MORE's drop-down: the folded groups under the panel `w` wide, as they look on it, in as
    /// many rows as they need, against the right edge. Its SVG, where its keys are (in the
    /// window) and where it is.
    fn more_svg(&self, w: f64) -> (String, Keys, [f64; 4]) {
        const PAD: f64 = 14.0;
        const ROW: f64 = 60.0;
        let maxw = (w - 12.0).max(160.0);
        // the screen whole while it fits, as wide as its longest line needs; else its halves
        let half = |lines: &[String; 2]| lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * 6.95 + 28.0;
        let (hd, hp) = (half(&self.defaults_lines()), half(&self.state.lcd));
        let joined = (2.0 * hd.max(hp)).clamp(LCD_MIN, 600.0);
        let mut items: Vec<(Part, f64)> = Vec::new();
        for &p in &self.folded {
            match p {
                Part::LcdPreset if items.last().is_some_and(|(q, _)| *q == Part::Lcd) => {}
                Part::LcdDefaults if joined <= maxw - 2.0 * PAD => items.push((Part::Lcd, joined)),
                Part::LcdDefaults => items.push((p, hd.min(maxw - 2.0 * PAD))),
                Part::LcdPreset => items.push((p, hp.min(maxw - 2.0 * PAD))),
                _ => items.push((p, 0.0)),
            }
        }
        let mut placed = Vec::new();
        let (mut x, mut row, mut dw) = (PAD, 0usize, 0.0f64);
        for (p, lw) in items {
            let pw = self.part(p, 0.0, false, true, lw).2;
            if x > PAD && x + pw + PAD > maxw {
                row += 1;
                x = PAD;
            }
            placed.push((p, x, row, lw));
            dw = dw.max(x + pw + PAD);
            x += pw + GAP;
        }
        let dh = (row + 1) as f64 * ROW + 4.0;
        let x0 = (w - 6.0 - dw).max(6.0);
        let mut o = String::new();
        let mut rects = Vec::new();
        for (p, px, r, lw) in placed {
            let dy = r as f64 * ROW;
            let (s, rs, _) = self.part(p, px, false, true, lw);
            let _ = write!(o, r#"<g transform="translate(0,{dy})">{s}</g>"#);
            rects.extend(rs.into_iter().map(|(k, [rx, ry, rw, rh])| (k, [x0 + rx, H + dy + ry, rw, rh])));
        }
        let th = dh + SHADOW;
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{th}" viewBox="0 0 {dw} {th}" font-family="Noto Sans" font-weight="bold">
{defs}
<rect x="2" y="2" width="{sw}" height="{dh}" rx="6" fill="#000" opacity=".35"/>
<rect width="{dw}" height="{dh}" rx="6" fill="#8c8e91"/><rect width="{dw}" height="{dh}" rx="6" fill="url(#pt)"/><rect width="{dw}" height="{dh}" rx="6" fill="url(#gStrip)"/>
<rect x=".5" y="-6" width="{iw}" height="{ih}" rx="6" fill="none" stroke="#000" stroke-opacity=".5"/>
{o}
</svg>"##,
            defs = defs(),
            sw = dw - 2.0,
            iw = dw - 1.0,
            ih = dh + 5.5,
        );
        (svg, rects, [x0, H, dw, th])
    }
}

/// A group of keys from `x0`, its name printed under them: its width.
fn group(st: &State, o: &mut String, rects: &mut Vec<(Key, [f64; 4])>, x0: f64, keys: &[(Key, Option<&str>, Option<bool>)], label: &str) -> f64 {
    if keys.is_empty() {
        return 0.0;
    }
    let mut x = x0;
    for (i, (key, txt, led)) in keys.iter().enumerate() {
        if i > 0 {
            x += 13.0;
        }
        let kw = match txt {
            Some(t) => (text::width(text::BOLD, t, 10.5, 0.4) + 20.0).max(62.0),
            None => 38.0,
        };
        o.push_str(&key_svg(x, kw, st.pressed == Some(*key)));
        match txt {
            Some(t) => {
                let tx = x + kw / 2.0 + if led.is_some() { 4.0 } else { 0.0 };
                let _ = write!(o, r##"<text x="{tx}" y="{}" font-size="10.5" letter-spacing=".4" fill="#e6e7e8" text-anchor="middle">{t}</text>"##, KEY_Y + 19.0);
            }
            None => {
                let icon = ICONS.iter().find(|(k, _)| k == key).map_or("", |(_, s)| s);
                let _ = write!(
                    o,
                    r##"<g transform="translate({},{}) scale(.7083)" fill="none" stroke="#e6e7e8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{icon}</g>"##,
                    x + kw / 2.0 - 8.5 + if led.is_some() { 2.0 } else { 0.0 },
                    KEY_Y + KEY_H / 2.0 - 8.5
                );
            }
        }
        if let Some(on) = led {
            o.push_str(&led_svg(x + 8.0, KEY_Y + 7.0, *on, "#ff4a32"));
        }
        rects.push((*key, [x - 3.0, KEY_Y - 3.0, kw + 6.0, KEY_H + 6.0]));
        x += kw;
    }
    if !label.is_empty() {
        o.push_str(&SECTION.text((x0 + x) / 2.0, 54.0, LABEL, "middle", label));
    }
    x - x0
}

/// The amber screen from `sx`, `sw` wide, in as many halves as `halves` (two lines each, the
/// second bold or not, and the key a click on it is): its width.
fn screen(o: &mut String, rects: &mut Vec<(Key, [f64; 4])>, sx: f64, sw: f64, halves: &[([String; 2], bool, Key)]) -> f64 {
    let _ = write!(
        o,
        r##"<rect x="{}" y="5" width="{}" height="50" rx="6" fill="#fff" opacity=".2"/><rect x="{}" y="6" width="{}" height="48" rx="5" fill="#121214"/>
<rect x="{sx}" y="9" width="{sw}" height="42" rx="4" fill="url(#gPanelLcd)"/><rect x="{sx}" y="9" width="{sw}" height="42" rx="4" fill="none" stroke="#783200" stroke-opacity=".35" stroke-width="3"/>"##,
        sx - 4.0,
        sw + 8.0,
        sx - 3.0,
        sw + 6.0
    );
    let hw = sw / halves.len() as f64;
    for (i, (lines, bold2, key)) in halves.iter().enumerate() {
        let hx = sx + i as f64 * hw;
        if i > 0 {
            let _ = write!(o, r##"<path d="M{hx},16 V44" stroke="#3a2408" stroke-opacity=".35" stroke-width="1.5"/>"##);
        }
        let chars = ((hw - 24.0) / 6.95) as usize;
        let line = |t: &str| {
            let n = t.chars().count();
            esc(&if n > chars { format!("{}…", t.chars().take(chars.saturating_sub(1)).collect::<String>()) } else { t.to_owned() })
        };
        let _ = write!(
            o,
            r##"<g font-family="Noto Sans Mono" font-size="11.5" fill="#3a2408" xml:space="preserve"><text x="{}" y="26">{}</text><text x="{}" y="43" font-weight="{}">{}</text></g>"##,
            hx + 12.0,
            line(&lines[0]),
            hx + 12.0,
            if *bold2 { "bold" } else { "normal" },
            line(&lines[1])
        );
        rects.push((*key, [hx, 9.0, hw, 42.0]));
    }
    sw
}

/// The MORE key: its name and a chevron pointing down.
fn more_key(x: f64, pressed: bool) -> String {
    let mut o = key_svg(x, MORE_W, pressed);
    let _ = write!(
        o,
        r##"<text x="{}" y="{}" font-size="10.5" letter-spacing=".4" fill="#e6e7e8" text-anchor="middle">MORE</text><path d="M{},{} l3.5,3.5 l3.5,-3.5" fill="none" stroke="#e6e7e8" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"##,
        x + MORE_W / 2.0 - 7.0,
        KEY_Y + 19.0,
        x + MORE_W - 22.0,
        KEY_Y + KEY_H / 2.0 - 1.5
    );
    o
}

/// How wide the bare panel's tile is: as its paint's pattern, so tiles meet seamlessly.
pub const TILE_W: f64 = 80.0;

/// The panel with nothing on it, `w` wide: its paint, its light and its shadow.
fn bare_svg(w: f64) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{th}" viewBox="0 0 {w} {th}">
{defs}
<rect y="{H}" width="{w}" height="{SHADOW}" fill="url(#gDrop)"/>
<rect width="{w}" height="{H}" fill="#8c8e91"/><rect width="{w}" height="{H}" fill="url(#pt)"/><rect width="{w}" height="{H}" fill="url(#gStrip)"/>
<path d="M0,.5 H{w}" stroke="#fff" stroke-opacity=".35"/><path d="M0,{b} H{w}" stroke="#000" stroke-opacity=".5"/>
</svg>"##,
        th = H + SHADOW,
        defs = defs(),
        b = H - 0.5,
    )
}

/// The panel's paint, gradients and the keys' shading.
fn defs() -> String {
    format!(
        r##"<defs>
<pattern id="pt" width="80" height="80" patternUnits="userSpaceOnUse"><image href="{paint}" width="80" height="80"/></pattern>
<linearGradient id="gStrip" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".10"/><stop offset="1" stop-color="#000" stop-opacity=".18"/></linearGradient>
<linearGradient id="gDrop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity=".55"/><stop offset="1" stop-color="#000" stop-opacity="0"/></linearGradient>
{keys}
<radialGradient id="gKnob" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient>
<linearGradient id="gPanelLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient>
</defs>"##,
        paint = paint_uri(),
        keys = KEY_DEFS
    )
}


/// The drop-down, in the LCD's amber: its SVG, height and hit areas (relative to its corner).
fn menu_svg(menu: &Menu) -> (String, f64, Vec<(MenuHit, [f64; 4])>) {
    const INK: &str = "#3a2408";
    let mut o = String::new();
    let mut rects = Vec::new();
    let w = if matches!(menu, Menu::Defaults { .. } | Menu::Hidden { .. }) { DEFAULTS_W } else { MENU_W };
    let h;
    let title = |o: &mut String, t: &str| {
        let _ = write!(o, r#"<text x="12" y="20" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".7">{t}</text>"#);
    };
    match menu {
        Menu::Save { text, placeholder } => {
            h = 70.0;
            title(&mut o, "SAVE AS NEW PRESET");
            let shown = if text.is_empty() { placeholder.clone() } else { text.to_uppercase() };
            let _ = write!(
                o,
                r##"<rect x="12" y="30" width="{}" height="26" rx="3" fill="#fff" fill-opacity=".35" stroke="{INK}" stroke-width="2"/><text x="20" y="48" font-size="12" fill="{INK}" opacity="{}" xml:space="preserve">{}</text>"##,
                w - 92.0,
                if text.is_empty() { 0.5 } else { 1.0 },
                esc(&shown)
            );
            if !text.is_empty() {
                // the caret, after the text
                let cx = 20.0 + text.chars().count() as f64 * 7.22 + 1.0;
                let _ = write!(o, r#"<rect x="{cx}" y="36" width="2" height="15" fill="{INK}"/>"#);
            }
            let _ = write!(o, r##"<rect x="{}" y="30" width="62" height="26" rx="3" fill="{INK}"/><text x="{}" y="48" font-size="12" fill="#f7bb62" text-anchor="middle">SAVE</text>"##, w - 74.0, w - 43.0);
            rects.push((MenuHit::Save, [w - 74.0, 30.0, 62.0, 26.0]));
        }
        Menu::Defaults { outs, ins, out, inp } => {
            let mut y = 8.0;
            for (output, names, cur, head) in [(true, outs, out, "DEFAULT OUTPUT"), (false, ins, inp, "DEFAULT INPUT")] {
                o += &section(head, y, w);
                y += 22.0;
                if names.is_empty() {
                    let _ = write!(o, r#"<text x="12" y="{}" font-size="12" font-weight="normal" fill="{INK}" opacity=".7">none</text>"#, y + 16.0);
                    y += 24.0;
                }
                for (i, name) in names.iter().enumerate() {
                    let on = *cur == Some(i);
                    let ink = if on { "#f7bb62" } else { INK };
                    if on {
                        let _ = write!(o, r#"<rect x="7" y="{}" width="{}" height="22" rx="3" fill="{INK}"/>"#, y + 1.0, w - 14.0);
                    }
                    let shown: String = if name.chars().count() > 42 { format!("{}…", name.chars().take(41).collect::<String>()) } else { name.clone() };
                    let _ = write!(o, r#"<text x="12" y="{}" font-size="12" fill="{ink}" xml:space="preserve">{}</text>"#, y + 16.5, esc(&shown));
                    // find it on the desk: a crosshair
                    let (cx, cy) = (w - 20.0, y + 12.0);
                    let _ = write!(
                        o,
                        r#"<g stroke="{ink}" stroke-width="1.6" fill="none" opacity=".8"><circle cx="{cx}" cy="{cy}" r="5"/><path d="M{},{cy} h4 M{},{cy} h4 M{cx},{} v4 M{cx},{} v4"/></g>"#,
                        cx - 9.0,
                        cx + 5.0,
                        cy - 9.0,
                        cy + 5.0
                    );
                    rects.push((MenuHit::Find(output, i), [w - 40.0, y, 40.0, 24.0]));
                    rects.push((MenuHit::Default(output, i), [0.0, y, w - 40.0, 24.0]));
                    y += 24.0;
                }
                y += 6.0;
            }
            h = y + 2.0;
        }
        Menu::Hidden { mine, system } => {
            let cut = |s: &str, n: usize| if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>()) } else { s.to_owned() };
            let mut y = 8.0;
            o += &section("HIDDEN BY YOU", y, w);
            y += 22.0;
            if mine.is_empty() {
                let _ = write!(o, r#"<text x="12" y="{}" font-size="12" font-weight="normal" fill="{INK}" opacity=".7">none</text>"#, y + 16.0);
                y += 24.0;
            }
            for (i, (name, model)) in mine.iter().enumerate() {
                let _ = write!(
                    o,
                    r#"<text x="12" y="{}" font-size="12" fill="{INK}" xml:space="preserve">{}</text><text x="{}" y="{}" font-size="9.5" fill="{INK}" opacity=".7" text-anchor="end">{}</text>"#,
                    y + 16.5,
                    esc(&cut(name, 26)),
                    w - 12.0,
                    y + 16.0,
                    esc(&cut(model, 18))
                );
                rects.push((MenuHit::Show(i), [0.0, y, w, 24.0]));
                y += 24.0;
            }
            y += 6.0;
            // what Patchcord always leaves out: listed for reference, dimmed
            o += &section("SYSTEM", y, w);
            y += 22.0;
            for (who, what) in system {
                let _ = write!(
                    o,
                    r#"<g opacity=".6"><text x="12" y="{}" font-size="12" fill="{INK}" xml:space="preserve">{} <tspan font-weight="normal" font-size="10.5">· {}</tspan></text><text x="{}" y="{}" font-size="9.5" fill="{INK}" text-anchor="end">SYSTEM</text></g>"#,
                    y + 16.5,
                    esc(&cut(who, 25)),
                    esc(&cut(what, 22)),
                    w - 12.0,
                    y + 16.0
                );
                y += 24.0;
            }
            for line in ["A device's HIDE key (the struck-through eye", "by its name) hides it when unused; H too.", "Click one above to show it again."] {
                let _ = write!(o, r#"<text x="12" y="{}" font-size="10.5" font-weight="normal" fill="{INK}" opacity=".75">{line}</text>"#, y + 16.0);
                y += 15.0;
            }
            h = y + 12.0;
        }
        Menu::Load { names, current, confirm } => {
            let mut y = 8.0;
            if names.is_empty() {
                let _ = write!(o, r#"<text x="12" y="{}" font-size="12" font-weight="normal" fill="{INK}" opacity=".7">No presets yet. Save one first.</text>"#, y + 16.0);
                y += 24.0;
            }
            for (i, name) in names.iter().enumerate() {
                let cur = *current == Some(i);
                let ink = if cur { "#f7bb62" } else { INK };
                if cur {
                    let _ = write!(o, r#"<rect x="7" y="{}" width="{}" height="22" rx="3" fill="{INK}"/>"#, y + 1.0, w - 14.0);
                }
                // long names end in "…" before the delete button
                let shown: String = if name.chars().count() > 26 { format!("{}…", name.chars().take(25).collect::<String>()) } else { name.clone() };
                let _ = write!(o, r#"<text x="12" y="{}" font-size="12" fill="{ink}" xml:space="preserve">{:02}  {}</text>"#, y + 16.5, i + 1, esc(&shown));
                if *confirm == Some(i) {
                    let _ = write!(o, r#"<text x="{}" y="{}" font-size="12" fill="{ink}" text-anchor="end">DEL?</text>"#, w - 12.0, y + 16.5);
                } else {
                    // a cross: the mono font has no ✕
                    let (cx, cy) = (w - 18.0, y + 12.0);
                    let _ = write!(o, r#"<path d="M{},{} l8,8 M{},{} l-8,8" stroke="{ink}" stroke-width="1.8" stroke-linecap="round" opacity=".75"/>"#, cx - 4.0, cy - 4.0, cx + 4.0, cy - 4.0);
                }
                rects.push((MenuHit::Delete(i), [w - 50.0, y, 50.0, 24.0]));
                rects.push((MenuHit::Row(i), [0.0, y, w - 50.0, 24.0]));
                y += 24.0;
            }
            h = y + 8.0;
        }
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#121214"/><rect x="3" y="3" width="{}" height="{}" rx="3" fill="url(#g)"/>{o}</svg>"##,
        w - 6.0,
        h - 6.0
    );
    (svg, h, rects)
}

/// A dark key as on Kadence: a cap in a hole in the panel. The hole's rim catches the light at
/// the bottom right; the cap casts a soft shadow into it and has a fine lit edge and a sheen.
/// Pressed, the cap sinks a little into the hole and darkens.
fn key_svg(x: f64, w: f64, pressed: bool) -> String {
    cap_svg([x, KEY_Y, w, KEY_H], pressed, false)
}

/// A key's cap in its hole at `[x, y, w, h]`, as the panel's (MIXER's keys are these too).
/// `deep`: held down, it sinks a little further and darkens more.
pub fn cap_svg([x, y, w, h]: [f64; 4], pressed: bool, deep: bool) -> String {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut o = format!(r#"<rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="url(#hole)" stroke="url(#edgeHole)" stroke-width="1.2"/>"#, x - 3.0, y - 3.0, w + 6.0, h + 6.0);
    let (tr, shadow, sheen) = if pressed {
        let s = if deep { 0.94 } else { 0.955 };
        (format!(r#" transform="translate({},{}) scale({s})""#, cx * (1.0 - s), cy * (1.0 - s)), 0.15 * 0.75, 0.35)
    } else {
        (String::new(), 0.75, 1.0)
    };
    // its shadow: soft at the edge from two plain shapes (a blur filter costs ~4 ms a key)
    let soft = format!(r##"<rect x="{}" y="{}" width="{}" height="{}" rx="5" fill="#000" opacity="{}"/>"##, x + 0.2, y + 1.4, w + 2.0, h + 2.0, shadow * 0.45);
    let _ = write!(
        o,
        r##"<g{tr}>{soft}<rect x="{}" y="{}" width="{w}" height="{h}" rx="4" fill="#000" opacity="{}"/><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="url(#btnGrad)" stroke="#070708" stroke-width="1"/><g opacity="{sheen}"><rect x="{}" y="{}" width="{}" height="{}" rx="3.4" fill="none" stroke="url(#capEdge)" stroke-width="1.4"/><rect x="{}" y="{}" width="{}" height="{}" rx="2.5" fill="url(#capTop)"/></g>"##,
        x + 1.2,
        y + 2.4,
        shadow * 0.6,
        x + 0.8,
        y + 0.8,
        w - 1.6,
        h - 1.6,
        x + 3.0,
        y + 1.6,
        w - 6.0,
        h * 0.4
    );
    if pressed {
        let _ = write!(o, r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="#000" opacity="{}"/>"##, if deep { 0.3 } else { 0.2 });
    }
    o += "</g>";
    o
}

/// The gradients a key's cap and hole are drawn with (`cap_svg`), for a `<defs>`.
pub const KEY_DEFS: &str = r##"<linearGradient id="hole" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#050506"/><stop offset="1" stop-color="#232427"/></linearGradient>
<linearGradient id="edgeHole" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2a2c2e"/><stop offset="0.5" stop-color="#55585b"/><stop offset="1" stop-color="#a7aaad"/></linearGradient>
<linearGradient id="btnGrad" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#303134"/><stop offset="0.6" stop-color="#1d1e20"/><stop offset="1" stop-color="#131314"/></linearGradient>
<linearGradient id="capEdge" x1="0" y1="0" x2="0.3" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.28"/><stop offset="0.45" stop-color="#fff" stop-opacity="0.04"/><stop offset="1" stop-color="#000" stop-opacity="0.5"/></linearGradient>
<linearGradient id="capTop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.13"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>"##;

fn led_svg(cx: f64, cy: f64, on: bool, color: &str) -> String {
    if on {
        format!(r#"<circle cx="{cx}" cy="{cy}" r="5" fill="{color}" opacity=".35"/><circle cx="{cx}" cy="{cy}" r="2.6" fill="{color}"/>"#)
    } else {
        format!(r##"<circle cx="{cx}" cy="{cy}" r="2.6" fill="#3a1512"/>"##)
    }
}

/// The angles (degrees from straight up) of the cords switch's three positions.
pub const SWITCH: [f64; 3] = [-90.0, 0.0, 90.0];

/// A black knob with an amber pointer at `deg` (from straight up), over its printed `scale`.
fn knob_svg(cx: f64, cy: f64, deg: f64, scale: &str) -> String {
    let mut o = scale.to_owned();
    let a = deg.to_radians();
    let _ = write!(
        o,
        r##"<circle cx="{cx}" cy="{}" r="14.5" fill="#111" opacity=".5"/><circle cx="{cx}" cy="{cy}" r="14" fill="url(#gKnob)" stroke="#0c0c0d"/><circle cx="{cx}" cy="{cy}" r="10.5" fill="none" stroke="#fff" stroke-opacity=".08"/><path d="M{},{} L{},{}" stroke="#ffb02e" stroke-width="2.4" stroke-linecap="round"/>"##,
        cy + 1.5,
        cx + 4.0 * a.sin(),
        cy - 4.0 * a.cos(),
        cx + 12.0 * a.sin(),
        cy - 12.0 * a.cos()
    );
    o
}

/// The badge that rides beside the pointer while listening (the pointer stays the desktop's own):
/// a dark round chip with a pair of cyan headphones, `CUE_BADGE` across.
pub const CUE_BADGE: f64 = 20.0;
pub fn cue_badge(dpi: f64) -> Layer {
    let s = CUE_BADGE;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><circle cx="10" cy="10" r="9" fill="#121315" stroke="#5fd4ff" stroke-width="1.5"/><g transform="translate(3,2.75) scale(.5833)" fill="none" stroke="#5fd4ff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4.5" height="7" rx="1.5" fill="#5fd4ff"/><rect x="16.5" y="14" width="4.5" height="7" rx="1.5" fill="#5fd4ff"/></g></svg>"##
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w: s, h: s }
}

/// The status bar: what PipeWire has on the left, a hint for what is under the pointer on the right.
/// When they would meet, the hint goes, then the left text ends in "…".
pub fn status(w: f64, left: &str, right: &str, dpi: f64) -> Layer {
    let width = |t: &str| text::width(text::MONO, t, 11.5, 0.0);
    let right = if width(left) + 24.0 + width(right) > w - 28.0 { "" } else { right };
    let mut left = left.to_owned();
    if width(&left) > w - 28.0 {
        let mut n = left.chars().count();
        while n > 1 && width(&left) > w - 28.0 {
            n -= 1;
            left = format!("{}…", left.chars().take(n).collect::<String>().trim_end());
        }
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{STATUS_H}" viewBox="0 0 {w} {STATUS_H}" font-family="Noto Sans Mono" font-size="11.5"><rect width="{w}" height="{STATUS_H}" fill="#0e0f10"/><path d="M0,.5 H{w}" stroke="#000"/><text x="14" y="15" fill="#7d8085">{}</text><text x="{}" y="15" fill="#a8abae" text-anchor="end">{}</text></svg>"##,
        esc(&left),
        w - 14.0,
        esc(right)
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h: STATUS_H }
}

/// A tooltip: a title in bold, then lines of PipeWire names in mono.
pub fn tooltip(lines: &[String], dpi: f64) -> Layer {
    let (title, rest) = lines.split_first().map_or(("", &[][..]), |(t, r)| (t.as_str(), r));
    let w = rest.iter().map(|l| text::width(text::MONO, l, 11.0, 0.0)).fold(text::width(text::BOLD, title, 12.0, 0.0), f64::max) + 24.0;
    let h = 30.0 + rest.len() as f64 * 16.0;
    let mut body = format!(r##"<text x="12" y="20" font-size="12" font-weight="bold" fill="#eceded">{}</text>"##, esc(title));
    for (i, l) in rest.iter().enumerate() {
        body += &format!(r##"<text x="12" y="{}" font-family="Noto Sans Mono" font-size="11" fill="#a9acb0">{}</text>"##, 37.0 + i as f64 * 16.0, esc(l));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans"><rect x=".5" y=".5" width="{}" height="{}" rx="7" fill="#141517" fill-opacity=".95" stroke="#000" stroke-opacity=".6"/>{body}</svg>"##,
        w - 1.0,
        h - 1.0
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h }
}

/// A short notice, a dark pill with a line of text.
pub fn notice(msg: &str, dpi: f64) -> Layer {
    let w = text::width(text::BOLD, msg, 13.0, 0.0) + 32.0;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="34" viewBox="0 0 {w} 34" font-family="Noto Sans" font-weight="bold"><rect width="{w}" height="34" rx="9" fill="#141517" fill-opacity=".94"/><text x="{}" y="22" font-size="13" fill="#e6e7e8" text-anchor="middle">{}</text></svg>"##,
        w / 2.0,
        esc(msg)
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h: 34.0 }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn state(rack: bool, engine: bool) -> State {
        State { rack, engine, mixer: false, locked: false, align: true, cables: 0, lcd: ["NO PRESET".into(), "48 kHz · 1024 · 21.3 ms".into()], defaults: ["JLab GO Work 2".into(), "Ryzen HD Audio Controller".into()], cue_led: false, cue_level: 70, hidden: 2, flow: true, pressed: None, more: false }
    }

    /// what folds, at a width, on each screen
    fn folded(rack: bool, engine: bool, w: f64) -> (Vec<Part>, Vec<Part>) {
        let p = Panel::new(state(rack, engine));
        let (parts, ..) = p.plan(w);
        (parts, p.plan(w).3)
    }

    #[test]
    fn a_wide_window_has_everything_and_a_narrow_one_folds_from_the_right() {
        let (parts, more) = folded(false, false, 1700.0);
        assert_eq!(parts.first(), Some(&Part::Logo));
        assert!(parts.contains(&Part::Lcd) && more.is_empty());
        // the logo goes first
        let (parts, more) = folded(false, false, 1350.0);
        assert!(!parts.contains(&Part::Logo) && parts.contains(&Part::Lcd) && more.is_empty(), "{parts:?} {more:?}");
        // a rack's width: DESK · RACK · ENGINE stay, EDIT too; the rest in MORE in the panel's order
        let (parts, more) = folded(false, false, 615.0);
        assert_eq!(parts, vec![Part::Edit, Part::Layout]);
        assert_eq!(more, vec![Part::LockAlign, Part::Cables, Part::View, Part::Cue, Part::LcdDefaults, Part::LcdPreset]);
        // ENGINE's grey keys go first; CUE · LEVEL works there and folds as on the desk
        let (parts, more) = folded(false, true, 728.0);
        assert!(parts.contains(&Part::View) && !parts.contains(&Part::Cables), "{parts:?}");
        assert_eq!(more, vec![Part::Cue, Part::LcdDefaults, Part::LcdPreset]);
        // in the rack ALIGN is off: grey, then gone
        let (_, more) = folded(true, false, 575.0);
        assert_eq!(more[0], Part::LockAlign);
        let mut p = Panel::new(state(true, false));
        let _ = p.layer(540.0, 1.0);
        p.state.more = true;
        let _ = p.layer(540.0, 1.0);
        assert!(p.rect(Key::Lock).is_some() && p.rect(Key::Align).is_none());
    }

    /// PATCHCORD_MIXER_PANEL=out.png [PATCHCORD_MIXER_PANEL_W=1440]: the panel as MIXER shows
    /// it, at twice the scale, for the mockup (docs/design/mixer). Run with --ignored.
    #[test]
    #[ignore]
    fn mixer_panel() {
        let Ok(out) = std::env::var("PATCHCORD_MIXER_PANEL") else { return };
        let w: f64 = std::env::var("PATCHCORD_MIXER_PANEL_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1440.0);
        let mut st = state(false, false);
        st.mixer = true;
        st.lcd = ["PRESET 01 · DEFAULT *".into(), "48 kHz · 1024 · 21.3 ms".into()];
        st.defaults = ["easyeffects".into(), "JLab GO Work 2".into()];
        st.cue_led = true;
        st.hidden = 0;
        let mut p = Panel::new(st);
        let _ = p.layer(w, 2.0);
        let (svg, ..) = p.svg(w);
        rasterize(&text::svg_options(), &svg, 2.0).unwrap().save_png(out).unwrap();
    }

    /// PATCHCORD_PANELS=dir draws the panel at several widths, MORE open, to look at.
    #[test]
    fn pictures() {
        let Ok(dir) = std::env::var("PATCHCORD_PANELS") else { return };
        for (name, rack, engine) in [("desk", false, false), ("rack", true, false), ("engine", false, true)] {
            for w in [1500.0, 1100.0, 900.0, 700.0, 540.0, 420.0] {
                let mut p = Panel::new(state(rack, engine));
                let _ = p.layer(w, 2.0);
                let (svg, ..) = p.svg(w);
                rasterize(&text::svg_options(), &svg, 2.0).unwrap().save_png(format!("{dir}/{name}-{w}.png")).unwrap();
                if !p.folded.is_empty() {
                    p.state.more = true;
                    let _ = p.layer(w, 2.0);
                    let (svg, ..) = p.more_svg(w);
                    rasterize(&text::svg_options(), &svg, 2.0).unwrap().save_png(format!("{dir}/{name}-{w}-more.png")).unwrap();
                }
            }
        }
    }
}
