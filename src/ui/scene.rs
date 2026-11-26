//! Everything on the patch field that doesn't need a window: the devices and where they are,
//! the cords and their physics, and the pictures to draw. The window (app.rs) and the
//! `--render` snapshot both use it.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
use resvg::usvg;

use super::cord::{Cord, Held, RATE, Slot};
use super::face::{self, BADGE, BAL_R, BalKnob, DEV_H, Geometry, KNOB_R, Leds, MIX_H, MIX_Y, JP, JackDef, JackKind, LCD_H, LCD_W, LCD_X, LCD_Y, FACE_PAD, PIC, RACK_GAP, RACK_MIN, RACK_SCREW_X, RACK_U, SHADOW};
use super::{ease, paint, painter, text};
use crate::config::Preset;

/// A change to the links, for undo and redo: the (output, input) port pairs added and removed.
type Change = (Vec<(u32, u32)>, Vec<(u32, u32)>);
use crate::graph::{Device, Dir, Graph, Media, VOL_MAX, volume_text};
use super::panel::PortSection;
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

/// The box round some cords' points (world units).
fn cords_box<'a>(cords: impl Iterator<Item = &'a Cord>) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in cords.flat_map(|c| &c.pts) {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    [x0, y0, x1, y1].iter().all(|v| v.is_finite() && v.abs() < 1e6).then_some((x0 as f64, y0 as f64, x1 as f64, y1 as f64))
}

/// The rails' pictures at one scale (see `Scene::rails`): [left, right, left first, right first]
/// units, and the units' numbers, made as they come into sight.
struct Rails {
    scale: f64,
    tiles: [Layer; 4],
    nums: Vec<Layer>,
}

impl Rails {
    fn new(opt: &usvg::Options, scale: f64) -> Self {
        let tile = |right: bool, first: bool| {
            let svg = rail_svg(right, first);
            Layer { id: next_id(), pm: rasterize(opt, &svg, scale), x: 0.0, y: 0.0, w: RAIL_W, h: RACK_U }
        };
        Rails { scale, tiles: [tile(false, false), tile(true, false), tile(false, true), tile(true, true)], nums: Vec::new() }
    }

    fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.tiles.iter().chain(&self.nums).map(|l| l.id)
    }
}

/// A unit of a rail, its outer edge at x 0 (the right rail's mirrored): dark steel, three square
/// holes cut through it (the window's background shows in them; a notch by the middle one, as on
/// real rails), a white line over the unit (none over the first: the panel is there).
fn rail_svg(right: bool, first: bool) -> String {
    let (w, h) = (RAIL_W, RACK_U);
    let (hx, hs) = (RAIL_NUMS + RACK_SCREW_X, RAIL_HOLE);
    // the holes, as a mask cut out of the steel
    let mut cut = String::new();
    let mut edges = String::new();
    for (k, cy) in RAIL_PITCH.iter().enumerate() {
        let (x, y) = (hx - hs / 2.0, cy - hs / 2.0);
        let _ = std::fmt::Write::write_fmt(&mut cut, format_args!(r##"<rect x="{x}" y="{y}" width="{hs}" height="{hs}" rx="1.2" fill="#000"/>"##));
        if k == 1 {
            let _ = std::fmt::Write::write_fmt(&mut cut, format_args!(r##"<circle cx="{x}" cy="{cy}" r="2.6" fill="#000"/>"##));
        }
        // the steel's cut edge: dark round the hole, catching the light along its bottom
        let _ = std::fmt::Write::write_fmt(
            &mut edges,
            format_args!(r##"<rect x="{x}" y="{y}" width="{hs}" height="{hs}" rx="1.2" fill="none" stroke="#000" stroke-width=".6" opacity=".6"/><path d="M{},{} h{}" stroke="#5a5d61" stroke-width=".7" opacity=".8"/>"##, x + 1.0, y + hs + 0.35, hs - 2.0),
        );
    }
    let mut g = format!(
        r##"<defs><mask id="holes" maskUnits="userSpaceOnUse" x="0" y="0" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#fff"/>{cut}</mask></defs><g mask="url(#holes)"><rect width="{w}" height="{h}" fill="url(#steel)"/><rect x=".3" width=".8" height="{h}" fill="#6b6e72" opacity=".7"/><rect x="{}" width=".7" height="{h}" fill="#55585c" opacity=".6"/><rect x="{}" width=".9" height="{h}" fill="#0d0d0e" opacity=".75"/>{edges}</g>"##,
        w - 1.6,
        w - 0.9
    );
    if !first {
        let _ = std::fmt::Write::write_fmt(&mut g, format_args!(r##"<rect x="3" y="0" width="{}" height="1.3" fill="#e6e6e6" opacity=".9"/>"##, hx - hs / 2.0 - 3.0));
    }
    if right {
        g = format!(r#"<g transform="translate({w},0) scale(-1,1)">{g}</g>"#);
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="steel" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#2c2d30"/><stop offset=".5" stop-color="#36373a"/><stop offset="1" stop-color="#28292b"/></linearGradient></defs>{g}</svg>"##
    )
}

/// A unit's number, for the strip outside a rail.
fn rail_number_svg(n: usize) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{RAIL_NUMS}" height="16" viewBox="0 0 {RAIL_NUMS} 16"><text x="{}" y="12" font-family="Noto Sans" font-weight="bold" font-size="11" fill="#e8e8e8" text-anchor="middle">{n}</text></svg>"##,
        RAIL_NUMS / 2.0
    )
}

pub fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The rack's rails, at the window's sides: a strip with the units' numbers outside, then the
/// steel the faces are screwed to, its square holes in proportion to a unit (0.375", 0.25",
/// 0.875" and 1.5" into a 1.75" unit), a thin rim beside them inside.
pub const RAIL_NUMS: f64 = 20.0;
const RAIL_HOLE: f64 = RACK_U * 0.375 / 1.75;
const RAIL_PITCH: [f64; 3] = [RACK_U * 0.25 / 1.75, RACK_U * 0.875 / 1.75, RACK_U * 1.5 / 1.75];
const RAIL_RIM: f64 = 8.0;
/// a rail's width, from the window's edge; the faces start `RAIL_NUMS` in
const RAIL_W: f64 = RAIL_NUMS + RACK_SCREW_X + RAIL_HOLE / 2.0 + RAIL_RIM;
/// From one rack's left edge to the next one's: its faces and a rail's numbers either side.
fn rack_pitch(rack_w: f64) -> f64 {
    rack_w + 2.0 * RAIL_NUMS
}

/// The devices' heights, in order, into `n` racks, each a run of them: where each one goes, so the
/// tallest rack is as short as can be.
fn columns(hs: &[f64], n: usize) -> Vec<usize> {
    let fill = |cap: f64| {
        let (mut col, mut used, mut at) = (0, 0.0, Vec::with_capacity(hs.len()));
        for &h in hs {
            if used > 0.0 && used + h > cap + 1e-6 {
                col += 1;
                used = 0.0;
            }
            used += h;
            at.push(col);
        }
        at
    };
    let (mut lo, mut hi) = (hs.iter().copied().fold(0.0, f64::max), hs.iter().sum::<f64>());
    for _ in 0..50 {
        let mid = (lo + hi) / 2.0;
        if fill(mid).last().is_none_or(|c| *c < n) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    fill(hi).into_iter().map(|c| c.min(n.saturating_sub(1))).collect()
}

/// where unit `k` (from 0) starts: half a gap down, so the first face is a gap below the top
fn unit_top(k: i64) -> f64 {
    RACK_GAP / 2.0 + k as f64 * RACK_U
}
/// Where the ringed jacks start in `Scene::jack_pics` (after the plus badge): for each group
/// colour of `face::ACCENTS`, a mono, a stereo and a MIDI jack.
const JACK_PICS: usize = 1;
/// Where the cue's marks start in `Scene::jack_pics`: the disc under a jack kept on, the ring,
/// the red ring.
pub const CUE_PICS: usize = JACK_PICS + 3 * face::ACCENTS.len();
/// Where the target rings start in `Scene::jack_pics`: green, then gold.
pub const TARGET_PICS: usize = CUE_PICS + 3;
/// Where the half discs start (left, right): one channel of a stereo jack kept on.
pub const HALF_PICS: usize = TARGET_PICS + 2;
/// Where a split pair's marks start: the disc, the ring, the red ring.
pub const PAIR_PICS: usize = HALF_PICS + 2;

/// A jack picture as a layer centred on its jack: the cue's marks and the target rings are
/// larger than a jack.
fn pic_layer(i: usize, pm: Option<Pixmap>) -> Layer {
    if i >= PAIR_PICS {
        // centred on the L jack, reaching down past the R one
        let c = face::CUE_PIC;
        return Layer { id: next_id(), pm, x: -c, y: -c, w: 2.0 * c, h: 2.0 * c + face::SPLIT_DY };
    }
    let h = if i >= HALF_PICS { face::CUE_PIC } else if i >= TARGET_PICS { face::TARGET_PIC } else if i >= CUE_PICS { face::CUE_PIC } else { PIC };
    Layer { id: next_id(), pm, x: -h, y: -h, w: 2.0 * h, h: 2.0 * h }
}
/// pixels per world unit for the faces' shadows, whatever the zoom
const SHADE_SCALE: f64 = 0.5;

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
    /// a face being drawn (see `painter`): its sig, what it is drawn from, at what scale, its size
    face_want: Option<(u64, u64, f64, f64, f64)>,
    /// its face on the other screen (the desk's in the rack, the rack's on the desk): what it was
    /// drawn for, at what scale. The GPU keeps it, so going back shows it at once.
    spare: Option<(u64, f64, Layer)>,
    /// a shadow being drawn: what for, its size
    shade_want: Option<(u64, f64, f64)>,
    /// the LCD, and what it was drawn from (its text, place and scale)
    pub lcd: Option<(u64, Layer)>,
    /// an LCD, and knobs by group, being drawn: what from, where
    lcd_want: Option<(u64, f64, f64)>,
    mix_want: HashMap<usize, (u64, f64, f64, f64)>,
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
    /// a key in a frame's row held down (the frame's group, which key): it shows pressed, and
    /// does what it does when let go over it
    pub pressed: Option<(usize, face::RowKey)>,
    gmeters: Vec<(usize, (u8, u8), Layer)>,
}

/// How long a volume set here is shown before PipeWire's own is, and the LCD shows it.
const MIX_HOLD: Duration = Duration::from_millis(600);
const MIX_NOTE: Duration = Duration::from_millis(1500);

impl DevView {
    /// What its face is drawn from: drawn again when this changes.
    fn sig(&self, rack_w: Option<f64>, accents: bool) -> u64 {
        let mut h = DefaultHasher::new();
        (&self.counts, &self.split, &self.dev.name, &self.dev.model, self.dev.state, rack_w.map(f64::to_bits), accents, self.dev.chain).hash(&mut h);
        self.dev.groups.iter().for_each(|g| g.mix.as_ref().map(|m| m.volumes.len()).hash(&mut h));
        // the frames' names: they follow the port in use (SPEAKERS, HEADPHONES…), and the
        // layout their width
        (0..self.dev.groups.len()).for_each(|gi| face::name(&self.dev, gi).hash(&mut h));
        self.defs.iter().for_each(|j| (&j.key, &j.label).hash(&mut h));
        h.finish()
    }

    /// A device as first seen: at `at`, nothing drawn yet.
    fn new(dev: Device, split: Vec<bool>, geo: Geometry, (x, y): (f64, f64)) -> Self {
        DevView { dev, split, defs: Vec::new(), counts: Vec::new(), geo, x, y, tx: x, ty: y, dx: x, dy: y, face: None, shade: None, face_sig: 0, face_scale: 0.0, face_want: None, spare: None, shade_want: None, lcd: None, lcd_want: None, mix_want: HashMap::new(), mix_local: HashMap::new(), mix_note: None, bal: HashMap::new(), mix_pics: Vec::new(), glevel: HashMap::new(), pressed: None, gmeters: Vec::new() }
    }

    /// How high the face is: a unit for each row of frames (one before it is first drawn).
    pub fn h(&self) -> f64 {
        if self.geo.h > 0.0 { self.geo.h } else { DEV_H }
    }

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

    /// A group's own meter: its lit LEDs (0…10), L and R.
    pub fn level(&self, gi: usize) -> [f32; 2] {
        self.glevel.get(&gi).copied().unwrap_or_default()
    }

    /// A group's BAL knob.
    pub fn bal(&self, gi: usize) -> BalKnob {
        self.bal.get(&gi).copied().unwrap_or_default()
    }

    /// Its pictures, bottom to top.
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        [self.shade.as_ref().map(|(_, l)| l), self.face.as_ref(), self.lcd.as_ref().map(|(_, l)| l)]
            .into_iter()
            .flatten()
            .chain(self.mix_pics.iter().map(|(_, _, l)| l))
            .chain(self.gmeters.iter().map(|(_, _, l)| l))
    }

    pub fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        [self.shade.as_mut().map(|(_, l)| l), self.face.as_mut(), self.lcd.as_mut().map(|(_, l)| l)]
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
    format!("{} {pct:>4}  {db:>9}", if input { "GAIN" } else { "VOL" })
}

/// The balance as the LCD shows it: each channel's volume, as the volume is shown, L then R
/// ("BAL  L  60%  R  85%"), or CENTRE.
fn bal_text(volume: f64, balance: f64) -> String {
    if balance.abs() < 0.005 {
        return "BAL CENTRE".into();
    }
    let l = if balance > 0.0 { volume * (1.0 - balance) } else { volume };
    let r = if balance < 0.0 { volume * (1.0 + balance) } else { volume };
    // (as wide whatever the numbers: nothing on the line jumps as it turns)
    format!("BAL  L {:>4}  R {:>4}", volume_text(l).0, volume_text(r).0)
}

/// What the LCD says when a frame's volume, balance or mute is set: what the hand turns.
#[derive(Clone, Copy, PartialEq)]
enum Says {
    Volume,
    Balance,
    /// whichever changed (a preset put back, two faders apart)
    Changed,
}

/// What a meter measures for: the frames whose meters show it (`groups`). One measuring
/// what a recording app takes in has its channels mapped to the app's (`map`).
///
/// A meter shows the sound after the frame's volume or gain, everywhere (turned down, it falls;
/// muted, nothing): `vol_of`, the frame whose volume is put on a level measured before it (an
/// output's monitor, what comes into an app that records); `mute_of`, the frame whose mute
/// silences it (a monitor carries the sound even when muted).
#[derive(Clone, Debug, PartialEq)]
struct MeterFor {
    dev: String,
    /// the frames whose meters show it
    groups: Vec<usize>,
    /// measured before this group's volume (an output's monitor, what comes into an app that
    /// records): its volume is put on what is shown, so the meter falls with the fader as
    /// everywhere else (measured after it, an app's output, a microphone: shown as it is)
    vol_of: Option<usize>,
    mute_of: Option<usize>,
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
    /// how far down the devices reach in the rack: the bottom of the last one's units (of the
    /// tallest rack)
    rack_h: f64,
    /// how wide and tall the window's field is (its units): the rack is as wide; racks go side
    /// by side when the devices don't fit its height in one, the devices going down the first and
    /// on into the next
    rack_room: (f64, f64),
    racks: usize,
    /// the rails' pictures at a scale: a unit of the left and the right rail, each also without
    /// the line over it (the first unit's: the panel is there), and the units' numbers
    rails: Option<Rails>,
    /// faces and shadows being drawn on the painter's threads
    inbox: painter::Inbox,
    /// a device on the desk grew (more jacks): the ones it now lies over make room on the next tick
    crowded: bool,
    /// while a device is dragged on the desk, the lines it is aligned along: (x0, y0, x1, y1)
    pub guides: Vec<[f64; 4]>,
    /// where the dragged device sticks (x, y), each while the hand keeps near it
    snapped: (Option<f64>, Option<f64>),
    /// devices dragged on the desk stick to each other's edges and centres (else they go
    /// exactly where they are held)
    pub align: bool,
    /// each group on a band of its kind's colour (K)
    pub accents: bool,
    /// pictures laid over the faces: the plus badge of a mult's free jack, then (from
    /// `JACK_PICS`) for each group colour of `face::ACCENTS` a mono, a stereo and a MIDI jack
    /// ringed in it; then, from `CUE_PICS`, the cue's marks: the disc under a jack kept on, the ring
    /// round a jack heard, and the red ring of one that would feed back; then the rings round
    /// the jacks a cord's end in the hand may go into (`TARGET_PICS`): green, and gold for the
    /// one it is pulled to; last the halves of a disc (`HALF_PICS`: left, right) and a split
    /// pair's marks (`PAIR_PICS`: disc, ring, red ring)
    pub jack_pics: [Option<Layer>; PAIR_PICS + 3],
    /// counts what PipeWire said (but the levels) and the devices made from it: what is worked
    /// out from them is worked out again when it moves on
    pub rev: u64,
    /// what the last `tick` spent on each of its parts, for the frame log
    pub spent: Vec<(&'static str, Duration)>,
    /// what the last `prepare` spent on each of its parts, for the frame log
    pub prep_spent: Vec<(&'static str, Duration)>,
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
    /// devices hidden by hand (by key), and those of them PipeWire has now
    pub hidden: Vec<String>,
    pub hidden_devs: Vec<crate::graph::Device>,
    /// hidden devices that came back by themselves (they got a cord), by name, for the app to say
    pub shown_again: Vec<String>,
    /// a light runs inside each cord that carries sound (W)
    pub flow: bool,
    /// the window's desk area (logical pixels): ARRANGE shapes its rows to it
    pub view_w: f64,
    pub view_h: f64,
}


/// How close devices on the desk may come to each other.
const GAP: f64 = 8.0;
/// How long the light takes along a cord, in seconds.
const FLOW_TRIP: f32 = 1.4;
/// ARRANGE's spacing, on one step U: between devices (in a row, between rows) U; between kinds 2U;
/// between flow columns (room for cords) 4U; between the patched devices and the rest, and round
/// the whole desk when fitted, 2U. Close enough that the faces fill the window, apart enough to
/// read as separate gear.
const U: f64 = 12.0;
const ROW_GAP: f64 = U;
const DEV_GAP: f64 = U;
const KIND_GAP: f64 = 2.0 * U;
const COL_GAP: f64 = 4.0 * U;
pub const EDGE: f64 = 2.0 * U;
/// How near (in pixels on the screen) an edge or centre of a dragged device must come to
/// another's to stick to it, and how far the hand must then pull it to come free.
const SNAP: f64 = 16.0;
const UNSNAP: f64 = 32.0;

/// Whether two devices on the desk, at (x, y), `w` wide and `h` high, are at least `gap` apart.
fn apart((ax, ay, aw, ah): (f64, f64, f64, f64), (bx, by, bw, bh): (f64, f64, f64, f64), gap: f64) -> bool {
    ax + aw + gap <= bx || bx + bw + gap <= ax || ay + ah + gap <= by || by + bh + gap <= ay
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
            rack_room: (0.0, 0.0),
            racks: 1,
            rails: None,
            inbox: painter::Inbox::default(),
            crowded: false,
            guides: Vec::new(),
            snapped: (None, None),
            align: true,
            accents: true,
            jack_pics: std::array::from_fn(|_| None),
            spent: Vec::new(),
            prep_spent: Vec::new(),
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
            hidden: Vec::new(),
            hidden_devs: Vec::new(),
            shown_again: Vec::new(),
            flow: true,
            rev: 0,
            view_w: 1400.0,
            view_h: 820.0,
        }
    }

    /// Takes in what PipeWire reported; true when something to draw changed.
    pub fn apply(&mut self, events: impl IntoIterator<Item = Event>) -> bool {
        let mut changed = false;
        let mut graph = false;
        for ev in events {
            if !matches!(ev, Event::Levels(_) | Event::Profile(_)) {
                self.rev += 1;
            }
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
        self.rev += 1;
        let order: Vec<String> = self.devs.iter().map(|d| d.dev.key.clone()).collect();
        let mut old: HashMap<String, DevView> = self.devs.drain(..).map(|d| (d.dev.key.clone(), d)).collect();
        let mut fresh = Vec::new();
        // devices hidden by hand stay out, unless one has a cord now (to a device that is shown
        // or hidden; a meter or the cue doesn't count): a cord must never lead nowhere
        let devices = self.graph.devices();
        let ports_of = |d: &crate::graph::Device| -> HashSet<u32> { d.groups.iter().flat_map(|g| g.ports.iter().map(|p| p.id)).collect() };
        let all: HashSet<u32> = devices.iter().flat_map(ports_of).collect();
        let links = self.graph.links();
        self.hidden_devs.clear();
        for dev in devices {
            if self.hidden.contains(&dev.key) {
                let mine = ports_of(&dev);
                let linked = links.iter().any(|l| (mine.contains(&l.out_port) && all.contains(&l.in_port)) || (mine.contains(&l.in_port) && all.contains(&l.out_port)));
                if !linked {
                    self.hidden_devs.push(dev);
                    continue;
                }
                self.hidden.retain(|k| *k != dev.key);
                self.shown_again.push(dev.name.clone());
            }
            let dv = match old.remove(&dev.key) {
                Some(mut dv) => {
                    if dv.split.len() != dev.groups.len() {
                        dv.split = vec![false; dev.groups.len()];
                    }
                    // (its cords' colour stays)
                    let color = dv.dev.color;
                    dv.dev = dev;
                    dv.dev.color = color;
                    dv
                }
                None => {
                    let split = vec![false; dev.groups.len()];
                    fresh.push(dev.key.clone());
                    DevView::new(dev, split, Geometry::default(), (0.0, 0.0))
                }
            };
            self.devs.push(dv);
        }
        // a new device's colour: one no other device has (its own first), else the one fewest
        // have, so no two look alike while there are colours to go round
        for key in &fresh {
            let mut count = [0usize; crate::graph::PALETTE.len()];
            for d in self.devs.iter().filter(|d| !fresh.contains(&d.dev.key) || fresh.iter().position(|k| *k == d.dev.key) < fresh.iter().position(|k| k == key)) {
                if let Some(i) = crate::graph::PALETTE.iter().position(|c| *c == d.dev.color) {
                    count[i] += 1;
                }
            }
            let Some(d) = self.devs.iter_mut().find(|d| d.dev.key == *key) else { continue };
            let own = crate::graph::PALETTE.iter().position(|c| *c == d.dev.color).unwrap_or(0);
            let n = count.len();
            let best = (0..n).map(|k| (own + k) % n).min_by_key(|i| count[*i]).unwrap_or(own);
            d.dev.color = crate::graph::PALETTE[best];
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
        let bottom = self.devs.iter().filter(|d| !fresh.contains(&d.dev.key)).map(|d| d.ty + d.h() + 46.0).fold(40.0, f64::max);
        let mut below = bottom;
        for i in 0..self.devs.len() {
            if !fresh.contains(&self.devs[i].dev.key) {
                continue;
            }
            if let Some(&(x, y)) = self.saved_pos.get(&self.devs[i].dev.key) {
                let (w, h) = (self.devs[i].geo.w, self.devs[i].h());
                let taken = !self.others(i).all(|o| apart((x, y, w, h), (o.tx, o.ty, o.geo.w, o.h()), 0.0));
                if taken {
                    (self.devs[i].tx, self.devs[i].ty) = (40.0, below);
                    below += h + 46.0;
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

    /// Faces' geometry. In the rack every face is as wide as the rack: the window between its rails
    /// (vents take what a face's frames don't need, frames that don't fit go on to more units); as
    /// many racks side by side as there is room for 19" ones (and each as wide as the widest frame
    /// needs). Without a window (pictures): as wide as the widest face needs.
    fn regeo(&mut self) {
        self.rack_w = self.rack.then(|| {
            let least = self.devs.iter().map(|d| face::rack_min_w(&d.dev, &d.defs, &d.counts)).fold(0.0, f64::max);
            let (room, tall) = self.rack_room;
            if room <= 0.0 {
                self.racks = 1;
                return self.devs.iter().map(|d| face::natural_w(&d.dev, &d.defs, &d.counts, true)).fold(RACK_MIN, f64::max);
            }
            // one rack while the devices fit the window's height in it; else as many side by side
            // as it takes to fit (each still as wide as its widest device needs), or, if no number
            // of them does, the number whose tallest rack is the shortest
            let fit = |n: usize| room / n as f64 - 2.0 * RAIL_NUMS;
            let order = self.rack_keys();
            let tallest = |n: usize| {
                let hs: Vec<f64> = order.iter().filter_map(|k| self.devs.iter().find(|d| d.dev.key == *k)).map(|d| face::rack_units(&d.dev, &d.defs, &d.counts, fit(n)) as f64 * RACK_U).collect();
                let col = columns(&hs, n);
                (0..n).map(|c| hs.iter().zip(&col).filter(|(_, k)| **k == c).map(|(h, _)| h).sum::<f64>()).fold(0.0, f64::max) + RACK_GAP / 2.0
            };
            let (mut n, mut best) = (1, tallest(1));
            let mut k = 2;
            while best > tall && k <= self.devs.len() && fit(k) >= least {
                let t = tallest(k);
                if t < best - 0.5 {
                    (n, best) = (k, t);
                }
                k += 1;
            }
            self.racks = n;
            fit(n).max(least)
        });
        for d in &mut self.devs {
            d.geo = face::face_geometry(&d.dev, &d.defs, &d.counts, self.rack_w, self.accents);
        }
    }

    /// The devices' keys in the rack's order (the new ones last).
    fn rack_keys(&self) -> Vec<String> {
        let mut order: Vec<String> = self.rack_order.iter().filter(|k| self.devs.iter().any(|d| d.dev.key == **k)).cloned().collect();
        for d in &self.devs {
            if !order.contains(&d.dev.key) {
                order.push(d.dev.key.clone());
            }
        }
        order
    }

    /// Stacks the devices in the rack in its order. While one is dragged (`dragged`), it goes where
    /// it is held and the others make room for it.
    fn layout_rack(&mut self, dragged: Option<&str>) {
        let mut order = self.rack_keys();
        if let Some(k) = dragged
            && let Some(d) = self.devs.iter().find(|d| d.dev.key == k)
        {
            // the others stacked without it (down the racks as they would go), and it put before
            // the first one past it: a rack further right, or lower in its rack
            order.retain(|o| o != k);
            let hs: Vec<f64> = order.iter().map(|o| self.devs.iter().find(|x| x.dev.key == *o).map_or(DEV_H, DevView::h) + RACK_GAP).collect();
            let col = columns(&hs, self.racks);
            let pitch = rack_pitch(self.rack_w.unwrap_or(RACK_MIN));
            let held = (((d.x + d.geo.w / 2.0) / pitch).floor().max(0.0) as usize).min(self.racks.max(1) - 1);
            let mid = d.y + d.h() / 2.0;
            let (mut y, mut idx) = (RACK_GAP, order.len());
            for i in 0..order.len() {
                if i > 0 && col[i] != col[i - 1] {
                    y = RACK_GAP;
                }
                if col[i] > held || (col[i] == held && mid < y + (hs[i] - RACK_GAP) / 2.0) {
                    idx = i;
                    break;
                }
                y += hs[i];
            }
            order.insert(idx, k.to_owned());
        }
        // each face in its units, a gap below the one above (and the first a gap below the top);
        // down the first rack and on into the next
        let hs: Vec<f64> = order.iter().map(|k| self.devs.iter().find(|d| d.dev.key == *k).map_or(DEV_H, DevView::h) + RACK_GAP).collect();
        let col = columns(&hs, self.racks);
        let pitch = rack_pitch(self.rack_w.unwrap_or(RACK_MIN));
        let (mut y, mut bottom) = (RACK_GAP, 0.0f64);
        for (i, k) in order.iter().enumerate() {
            if i > 0 && col[i] != col[i - 1] {
                y = RACK_GAP;
            }
            if let Some(d) = self.devs.iter_mut().find(|d| d.dev.key == *k)
                && Some(k.as_str()) != dragged
            {
                (d.tx, d.ty) = (col[i] as f64 * pitch, y);
            }
            y += hs[i];
            bottom = bottom.max(y);
        }
        self.rack_order = order;
        self.rack_h = bottom.max(RACK_GAP) - RACK_GAP / 2.0;
    }

    /// The ids of every picture of the desk or rack (to keep on the GPU while ENGINE shows).
    pub fn layer_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.devs.iter().flat_map(|d| d.layers().map(|l| l.id)).chain(self.pic_ids()).chain(self.rails.iter().flat_map(Rails::ids))
    }

    /// The ids of the pictures laid over the faces now and then (the spare devices', the jacks'
    /// and their marks): kept on the GPU between the frames that show them.
    pub fn pic_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.spare_ids().chain(self.jack_pics.iter().flatten().map(|l| l.id))
    }

    /// The faces kept for the other screen: the GPU keeps them.
    pub fn spare_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.devs.iter().filter_map(|d| d.spare.as_ref().map(|(_, _, l)| l.id))
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
        // straight to their places: no gliding between the desk and the rack
        for d in &mut self.devs {
            (d.x, d.y) = (d.tx, d.ty);
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
        let (w, h) = (self.devs[i].geo.w, self.devs[i].h());
        self.others(i).all(|o| apart((x, y, w, h), (o.dx, o.dy, o.geo.w, o.h()), GAP))
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
        let (w, h) = (self.devs[i].geo.w, self.devs[i].h());
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for o in self.others(i) {
            xs.extend([o.dx, o.dx + (o.geo.w - w) / 2.0, o.dx + o.geo.w - w, o.dx + o.geo.w + GAP, o.dx - GAP - w]);
            ys.extend([o.dy, o.dy + o.h() + GAP, o.dy - h - GAP]);
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
        let (w, h) = (self.devs[i].geo.w, self.devs[i].h());
        let lines = |x: f64, w: f64| [x, x + w / 2.0, x + w];
        let same = |a: f64, b: f64| (a - b).abs() < 0.5;
        let mut guides = Vec::new();
        for o in self.others(i) {
            let (y0, y1) = (at.1.min(o.dy) - 14.0, (at.1 + h).max(o.dy + o.h()) + 14.0);
            let (x0, x1) = (at.0.min(o.dx) - 14.0, (at.0 + w).max(o.dx + o.geo.w) + 14.0);
            if self.snapped.0.is_some_and(|s| same(s, at.0)) {
                for v in lines(at.0, w).into_iter().filter(|v| lines(o.dx, o.geo.w).iter().any(|t| same(*t, *v))) {
                    guides.push([v, y0, v, y1]);
                }
            }
            if self.snapped.1.is_some_and(|s| same(s, at.1)) && same(at.1, o.dy) {
                for v in lines(at.1, h.min(o.h())) {
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
                    if i == j || (b.dx, b.dy) < (a.dx, a.dy) || apart((a.dx, a.dy, a.geo.w, a.h()), (b.dx, b.dy, b.geo.w, b.h()), GAP) {
                        continue;
                    }
                    // how far b must go right, or down, to clear a
                    let right = a.dx + a.geo.w + GAP - b.dx;
                    let down = a.dy + a.h() + GAP - b.dy;
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
        // (the jacks change: what is worked out from them is worked out again)
        self.rev += 1;
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
            // a frame muted: the meters it silences would only measure what is never shown, so
            // there are none (unmuted, they come again)
            let muted = |gi: Option<usize>| gi.and_then(|gi| d.dev.groups.get(gi)).and_then(|g| g.mix.as_ref()).is_some_and(|m| m.mute);
            for (gi, g) in d.dev.groups.iter().enumerate() {
                if !face::metered(g) {
                    continue;
                }
                let gain = g.mix.as_ref().is_some_and(|m| m.gain);
                // an output's monitor: the sound before the output's volume, its level shown on that
                // output (PLAYBACK) too; silent when the output is muted
                let output = (g.role == "MONITOR")
                    .then(|| d.dev.groups.iter().position(|o| o.dir == Dir::In && o.mix.is_some() && o.ports.first().map(|p| p.node) == g.ports.first().map(|p| p.node)))
                    .flatten();
                // what leaves: measured on its own ports (what an app sends, a device's input after
                // its gain, an output's monitor, a filter's output)
                if g.dir == Dir::Out {
                    let ports: Vec<u32> = g.ports.iter().map(|p| p.id).collect();
                    let targets = g.ports.iter().map(|p| (p.node, p.id)).collect();
                    let channels = g.ports.iter().map(|p| if p.channel.is_empty() { "MONO".to_owned() } else { p.channel.clone() }).collect();
                    let map = (0..g.ports.len()).collect();
                    let groups = std::iter::once(gi).chain(output).collect();
                    // an output's monitor is before the output's volume: that volume put on it; anything
                    // else measured after its own (an app's output, a microphone)
                    let vol_of = output;
                    let mute_of = output.or(g.mix.is_some().then_some(gi));
                    if muted(mute_of) {
                        continue;
                    }
                    want.insert(ports, (MeterFor { dev: d.dev.key.clone(), groups, vol_of, mute_of, map }, targets, channels));
                    continue;
                }
                // what comes in: a device's output (PLAYBACK) shows its monitor's level (above)
                if g.mix.is_some() && !gain {
                    continue;
                }
                // a recording app: its monitor, the sum of what comes in, before its gain
                let node = g.ports[0].node;
                let mons = if gain { self.graph.monitor_ports(node) } else { Vec::new() };
                if !mons.is_empty() {
                    if muted(Some(gi)) {
                        continue;
                    }
                    let map = mons.iter().enumerate().map(|(i, (_, c))| g.ports.iter().position(|p| !c.is_empty() && p.channel == *c).unwrap_or(i)).collect();
                    let key: Vec<u32> = mons.iter().map(|(p, _)| *p).collect();
                    let targets = mons.iter().map(|(p, _)| (node, *p)).collect();
                    let channels = mons.iter().enumerate().map(|(i, (_, c))| if c.is_empty() { format!("AUX{i}") } else { c.clone() }).collect();
                    want.insert(key, (MeterFor { dev: d.dev.key.clone(), groups: vec![gi], vol_of: Some(gi), mute_of: Some(gi), map }, targets, channels));
                    continue;
                }
                // anything else: every port linked to each of its channels
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
                if targets.is_empty() || muted(gain.then_some(gi)) {
                    continue;
                }
                // apart from a meter of the same ports for their own device: marked by this node
                let mut key: Vec<u32> = targets.iter().map(|(_, p)| *p).collect();
                key.extend([u32::MAX, g.ports[0].node]);
                // named as the ports it measures (MONO, FL…) when they tell and differ; AUX0… else
                let named: Vec<String> = targets.iter().map(|(_, p)| self.graph.props_of(*p).and_then(|q| q.get("audio.channel")).cloned().unwrap_or_default()).collect();
                let unique = named.iter().all(|c| !c.is_empty()) && named.iter().collect::<std::collections::HashSet<_>>().len() == named.len();
                let channels = if unique { named } else { (0..targets.len()).map(|i| format!("AUX{i}")).collect() };
                want.insert(key, (MeterFor { dev: d.dev.key.clone(), groups: vec![gi], vol_of: g.mix.is_some().then_some(gi), mute_of: gain.then_some(gi), map }, targets, channels));
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

    /// The device (in `devs`) a jack is on.
    pub fn jack_dev(&self, key: &str) -> Option<usize> {
        self.jacks.get(key).map(|j| j.0)
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

    /// A jack's n-th slot and the corner of its device, for a cord's end.
    fn slot(&self, key: &str, n: usize) -> Option<Slot> {
        let (di, ji) = *self.jacks.get(key)?;
        let d = &self.devs[di];
        let (lx, ly) = *d.geo.jacks.get(ji)?;
        Some(Slot { at: ((d.x + lx + n as f64 * JP) as f32, (d.y + ly) as f32), dev: (d.x as f32, d.y as f32) })
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
        for list in per.values_mut() {
            list.sort();
        }
        // a split stereo group's R row in the same columns as its L row: a cord on R whose other end
        // is the R of the same frame as a cord on L stands under it, so L and R of one source read
        // as a pair (in the order they were plugged in, they come out crossed)
        let peer = |ci: usize, b: bool| {
            let other = if b { &self.cords[ci].a } else { &self.cords[ci].b };
            let &(di, ji) = self.jacks.get(other.jack.as_ref()?)?;
            Some((di, self.devs[di].defs[ji].group))
        };
        for d in &self.devs {
            for (gi, _) in d.dev.groups.iter().enumerate().filter(|(gi, g)| g.stereo && d.split.get(*gi).copied().unwrap_or(false)) {
                let rows: Vec<&String> = d.defs.iter().filter(|j| j.group == gi).map(|j| &j.key).collect();
                let [l, r] = rows[..] else { continue };
                let Some(left) = per.get(l) else { continue };
                let column: HashMap<(usize, usize), usize> = left.iter().enumerate().filter_map(|(k, &(_, ci, b))| Some((peer(ci, b)?, k))).collect();
                if let Some(right) = per.get_mut(r) {
                    right.sort_by_key(|&(slot, ci, b)| (peer(ci, b).and_then(|p| column.get(&p).copied()).unwrap_or(usize::MAX), slot));
                }
            }
        }
        for (key, list) in per {
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
        // what each part took, for the frame log (`spent`)
        self.spent.clear();
        let mut t0 = Instant::now();
        let mut lap = |spent: &mut Vec<(&'static str, Duration)>, what: &'static str| {
            spent.push((what, t0.elapsed()));
            t0 = Instant::now();
        };
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
        lap(&mut self.spent, "tick.sync");
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
        lap(&mut self.spent, "tick.slots");
        // (not while a device is in the hand: it goes where it is held)
        if self.crowded && !dragging {
            self.crowded = false;
            self.make_room();
        }
        lap(&mut self.spent, "tick.room");
        // meters: peaks to lit LEDs on a dB scale (-48.6…+5.4 dB → 0…10), quick up and slow down;
        // one LED higher than -54…0 was, so that the red one lights only at a clip (0 dBFS and
        // over), not on a loud track at 100%
        let mut target: HashMap<(&str, Option<usize>), [f32; 2]> = HashMap::new();
        let index: HashMap<&str, usize> = self.devs.iter().enumerate().map(|(i, d)| (d.dev.key.as_str(), i)).collect();
        for (id, p) in &self.peaks {
            let Some(m) = self.meter_dev.get(id) else { continue };
            let led = |v: f32| if v <= 0.0 { 0.0 } else { ((20.0 * v.log10() + 48.6) / 54.0 * 10.0).clamp(0.0, 10.0) };
            // each frame channel: the loudest of what comes into it
            let mut ch = [0.0f32; 2];
            for (i, v) in p.iter().enumerate() {
                let c = m.map.get(i).copied().unwrap_or(i).min(1);
                ch[c] = ch[c].max(*v);
            }
            // what is shown is after the volume, everywhere: measured before it, the volume (as set
            // here just now, else PipeWire's) is put on it
            if let Some(gi) = m.vol_of
                && let Some(d) = index.get(m.dev.as_str()).map(|&di| &self.devs[di])
                && let (Some((vol, bal, _)), Some(mx)) = (d.mix(gi), d.dev.groups.get(gi).and_then(|g| g.mix.as_ref()))
            {
                let lin = mx.volumes_for(vol, bal);
                for (c, v) in ch.iter_mut().enumerate() {
                    *v *= lin.get(c).or(lin.first()).copied().unwrap_or(1.0);
                }
            }
            // muted: silent
            if let Some(gi) = m.mute_of
                && index.get(m.dev.as_str()).and_then(|&di| self.devs[di].mix(gi)).is_some_and(|(_, _, mute)| mute)
            {
                ch = [0.0; 2];
            }
            if p.len() == 1 || m.map.iter().all(|c| *c == 0) {
                ch[1] = ch[0];
            }
            for gi in &m.groups {
                let t = target.entry((m.dev.as_str(), Some(*gi))).or_default();
                (t[0], t[1]) = (t[0].max(led(ch[0])), t[1].max(led(ch[1])));
            }
        }
        let fall = |level: &mut f32, t: f32| {
            *level = if t >= *level { t } else { (*level - 18.0 * dt).max(t) };
            *level > 0.01
        };
        for d in &mut self.devs {
            for (gi, ..) in &d.geo.meters {
                let t = target.get(&(d.dev.key.as_str(), Some(*gi))).copied().unwrap_or_default();
                let lv = d.glevel.entry(*gi).or_default();
                for (level, t) in lv.iter_mut().zip(t) {
                    moving |= fall(level, t);
                }
            }
        }
        lap(&mut self.spent, "tick.meters");
        // the light in each plugged-in cord, showing which way it goes: one at a time, across in
        // FLOW_TRIP seconds whatever plays (it shows the patch, not the sound); fading in and out
        for c in &mut self.cords {
            let want = if c.live() && self.flow { 1.0 } else { 0.0 };
            c.flow += (want - c.flow) * (dt * 4.0).min(1.0);
            if c.flow > 0.02 {
                c.flow_at = (c.flow_at + dt / FLOW_TRIP) % 1.0;
                moving = true;
            }
        }
        self.acc = (self.acc + dt).min(4.0 / RATE);
        let steps = (self.acc * RATE) as usize;
        self.acc -= steps as f32 / RATE;
        let cords = std::mem::take(&mut self.cords);
        let mut kept = Vec::with_capacity(cords.len());
        for mut c in cords {
            let h = held.filter(|(seq, _)| *seq == c.seq).map(|(_, h)| h);
            c.frame(dt, steps, &|k, n| self.slot(k, n), hand, h, dragging);
            moving |= c.moving();
            if !c.gone() {
                kept.push(c);
            }
        }
        self.cords = kept;
        lap(&mut self.spent, "tick.cords");
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
        // the devices with cords, in columns by signal flow (sources left, what they feed right)
        let patched: Vec<bool> = self.devs.iter().map(|d| d.counts.iter().any(|c| *c > 0)).collect();
        let edges: Vec<(usize, usize)> = self
            .cords
            .iter()
            .filter(|c| c.live())
            .filter_map(|c| Some((self.jacks.get(c.a.jack.as_ref()?)?.0, self.jacks.get(c.b.jack.as_ref()?)?.0)))
            .collect();
        let mut rank = vec![0usize; n];
        for _ in 0..n {
            for &(a, b) in &edges {
                if a != b && rank[b] <= rank[a] {
                    rank[b] = (rank[a] + 1).min(4);
                }
            }
        }
        // the kinds, in the order they come: hardware, programs, PipeWire's own (MIDI bridges…)
        let kind = |d: &DevView| match d.dev.finish {
            crate::graph::Finish::Aluminium => 0,
            crate::graph::Finish::Black => 1,
            crate::graph::Finish::Olive => 2,
        };
        let by_kind = |a: &usize, b: &usize| kind(&self.devs[*a]).cmp(&kind(&self.devs[*b])).then_with(|| self.devs[*a].dev.name.to_lowercase().cmp(&self.devs[*b].dev.name.to_lowercase()));
        let mut cols: Vec<Vec<usize>> = Vec::new();
        for r in 0..=4 {
            let mut col: Vec<usize> = (0..n).filter(|i| patched[*i] && rank[*i] == r).collect();
            if col.is_empty() {
                continue;
            }
            // the first column by kind and name; the next ones by where their cords come from,
            // so they cross less
            let before: Vec<usize> = cols.iter().flatten().copied().collect();
            let at = |i: usize| {
                let ys: Vec<f64> = edges.iter().filter(|(a, b)| *b == i && before.contains(a)).map(|(a, _)| before.iter().position(|x| x == a).unwrap_or(0) as f64).collect();
                if ys.is_empty() { f64::MAX } else { ys.iter().sum::<f64>() / ys.len() as f64 }
            };
            col.sort_by(|a, b| at(*a).total_cmp(&at(*b)).then_with(|| by_kind(a, b)));
            cols.push(col);
        }
        // the rest, kind by kind
        let mut rest: Vec<usize> = (0..n).filter(|i| !patched[*i]).collect();
        rest.sort_by(by_kind);
        if self.rack {
            self.rack_order = cols.iter().flatten().chain(&rest).map(|i| self.devs[*i].dev.key.clone()).collect();
            return self.layout_rack(None);
        }
        let mut x = 40.0;
        let mut bottom: f64 = 40.0;
        for col in &cols {
            let mut y = 40.0;
            let mut w: f64 = 0.0;
            for &i in col {
                let d = &mut self.devs[i];
                (d.tx, d.ty) = (x, y);
                (d.dx, d.dy) = (x, y);
                y += d.h() + ROW_GAP;
                w = w.max(d.geo.w);
            }
            bottom = bottom.max(y - ROW_GAP);
            x += w + COL_GAP;
        }
        let flow_w = if cols.is_empty() { 0.0 } else { x - COL_GAP - 40.0 };
        let y0 = if cols.is_empty() { 40.0 } else { bottom + EDGE };
        // the rest in rows, kind after kind (a wider gap between kinds), wrapping at `wide`:
        // where each goes, and how wide and high it all is
        let sizes: Vec<(f64, f64, u8)> = rest.iter().map(|&i| (self.devs[i].geo.w, self.devs[i].h(), kind(&self.devs[i]))).collect();
        let pack = |wide: f64| -> (Vec<(f64, f64)>, f64, f64) {
            let (mut rx, mut y, mut row_h, mut last, mut w) = (40.0, y0, 0.0f64, None, 0.0f64);
            let mut at = Vec::new();
            for &(dw, dh, k) in &sizes {
                let gap = match last {
                    None => 0.0,
                    Some(l) if l != k => KIND_GAP,
                    _ => DEV_GAP,
                };
                if last.is_some() && rx + gap + dw > 40.0 + wide {
                    y += row_h + ROW_GAP;
                    (rx, row_h) = (40.0, 0.0);
                } else {
                    rx += gap;
                }
                last = Some(k);
                at.push((rx, y));
                rx += dw;
                row_h = row_h.max(dh);
                w = w.max(rx - 40.0);
            }
            let h = if sizes.is_empty() { bottom } else { y + row_h } - 40.0;
            (at, w.max(flow_w), h)
        };
        // the width whose shape fits the window best: the faces as large as they can be
        let (vw, vh) = (self.view_w - 2.0 * EDGE, self.view_h - 2.0 * EDGE);
        let mut wide = sizes.iter().map(|s| s.0).fold(flow_w, f64::max);
        let mut best = (wide, 0.0);
        while wide <= 6000.0 {
            let (_, w, h) = pack(wide);
            let s = (vw / w).min(vh / h);
            if s > best.1 + 1e-9 {
                best = (wide, s);
            }
            wide += 40.0;
        }
        let (at, ..) = pack(best.0);
        for (&i, (x, y)) in rest.iter().zip(at) {
            let d = &mut self.devs[i];
            (d.tx, d.ty) = (x, y);
            (d.dx, d.dy) = (x, y);
        }
    }

    /// Hides device `di` (by hand), when it has no cords: false when it has some.
    pub fn hide(&mut self, di: usize) -> bool {
        let Some(d) = self.devs.get(di) else { return false };
        if d.counts.iter().any(|c| *c > 0) {
            return false;
        }
        self.hidden.push(d.dev.key.clone());
        self.sync();
        true
    }

    /// Where device `key` is among those shown.
    pub fn dev_index(&self, key: &str) -> Option<usize> {
        self.devs.iter().position(|d| d.dev.key == key)
    }

    /// Shows a device hidden by hand again: where it is now.
    pub fn show(&mut self, key: &str) -> Option<usize> {
        self.hidden.retain(|k| k != key);
        self.sync();
        self.dev_index(key)
    }

    /// The HIDE key under (x, y): its device.
    pub fn hide_key_at(&self, x: f64, y: f64) -> Option<usize> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| {
            let [kx, ky, kw, kh] = d.geo.hide;
            (kw > 0.0 && x >= d.x + kx && x <= d.x + kx + kw && y >= d.y + ky && y <= d.y + ky + kh).then_some(di)
        })
    }

    /// The GRAPH key under (x, y): the node whose frame it is on (ADR 0014).
    pub fn graph_key_at(&self, x: f64, y: f64) -> Option<u32> {
        self.devs.iter().rev().find_map(|d| d.geo.graph.iter().find(|(_, [kx, ky, kw, kh])| x >= d.x + kx && x <= d.x + kx + kw && y >= d.y + ky && y <= d.y + ky + kh).map(|(n, _)| *n))
    }

    /// Devices that appeared later go below everything else.
    fn place_new(&mut self, keys: &[String]) {
        let mut y = self.devs.iter().filter(|d| !keys.contains(&d.dev.key)).map(|d| d.dy + d.h() + 46.0).fold(40.0, f64::max);
        for d in self.devs.iter_mut().filter(|d| keys.contains(&d.dev.key)) {
            (d.tx, d.ty) = (40.0, y);
            (d.x, d.y) = (d.tx, d.ty);
            y += d.h() + 46.0;
        }
    }

    /// World area the devices take (where they are going).
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut it = self.devs.iter();
        let f = it.next()?;
        let mut b = (f.tx, f.ty, f.tx + f.geo.w, f.ty + f.h());
        for d in it {
            b = (b.0.min(d.tx), b.1.min(d.ty), b.2.max(d.tx + d.geo.w), b.3.max(d.ty + d.h()));
        }
        Some(b)
    }

    /// A view that shows every device in the window area `top` pixels down, w × h pixels.
    /// The view that shows every device, as big as the window allows up to `max_scale`, with a
    /// margin of `m` (screen pixels) round them.
    /// In the rack: the rails at the window's edges, the first device at the top.
    pub fn fit(&self, w: f64, h: f64, top: f64, max_scale: f64, m: f64) -> View {
        if let Some((l, r, _)) = self.rack_extent() {
            let s = w / (r - l);
            return View { s, x: -l * s, y: top };
        }
        let Some((x0, y0, x1, y1)) = self.bounds() else { return View { x: 0.0, y: top, s: 1.0 } };
        let s = ((w - 2.0 * m) / (x1 - x0)).min((h - 2.0 * m) / (y1 - y0)).clamp(0.3, max_scale);
        View { s, x: (w - (x1 - x0) * s) / 2.0 - x0 * s, y: top + ((h - (y1 - y0) * s) / 2.0).max(m) - y0 * s }
    }

    /// How wide and tall the rack was last told the window's field is (0: not yet).
    pub fn rack_room(&self) -> (f64, f64) {
        self.rack_room
    }

    /// In the rack: how wide and tall the window's field is (its units). The rack is laid out
    /// again when it changes.
    pub fn set_rack_room(&mut self, w: f64, h: f64) {
        if (w - self.rack_room.0).abs() < 0.5 && (h - self.rack_room.1).abs() < 0.5 {
            return;
        }
        self.rack_room = (w, h);
        if self.rack {
            self.regeo();
            self.layout_rack(None);
        }
    }

    /// The rack's ends, in the rack: the left rail's outer edge, the right one's, and how far down
    /// it has something (the last device, or a cord hanging lower, but no more than one empty unit
    /// past the last device: a plug carried down doesn't take the rack on for ever). The scene
    /// never shows beyond them (but below, while the window is taller).
    pub fn rack_extent(&self) -> Option<(f64, f64, f64)> {
        let rw = self.rack_w.filter(|_| self.rack)?;
        let cords = self.cords.iter().flat_map(|c| c.pts.iter().map(|p| p.y as f64)).fold(0.0, f64::max);
        Some((-RAIL_NUMS, self.racks as f64 * rack_pitch(rw) - RAIL_NUMS, self.rack_h.max(cords).min(self.rack_h + RACK_U)))
    }

    /// The units' numbers up to the unit at `y` (world units) drawn, for `rails` to show.
    pub fn rail_numbers(&mut self, y: f64) {
        let Some(r) = &mut self.rails else { return };
        let upto = ((y - unit_top(0)) / RACK_U).ceil().max(0.0) as usize + 1;
        while r.nums.len() < upto {
            let svg = rail_number_svg(r.nums.len() + 1);
            r.nums.push(Layer { id: next_id(), pm: rasterize(&self.opt, &svg, r.scale), x: 0.0, y: 0.0, w: RAIL_NUMS, h: 16.0 });
        }
    }

    /// The rails' pictures between `y0` and `y1` (world units), and where each goes (x, y, w, h,
    /// world units): the rails run the whole way, past the last device.
    pub fn rails(&self, y0: f64, y1: f64) -> Vec<(&Layer, [f64; 4])> {
        let (Some(r), Some(rw)) = (&self.rails, self.rack_w.filter(|_| self.rack)) else { return Vec::new() };
        let first = ((y0 - unit_top(0)) / RACK_U).floor().max(-1.0) as i64;
        let last = ((y1 - unit_top(0)) / RACK_U).ceil() as i64;
        let mut out = Vec::new();
        for (l, rr) in (0..self.racks).map(|c| (c as f64 * rack_pitch(rw) - RAIL_NUMS, c as f64 * rack_pitch(rw) + rw + RAIL_NUMS)) {
            for k in first..=last {
                let y = unit_top(k);
                let t = if k <= 0 { 2 } else { 0 };
                out.push((&r.tiles[t], [l, y, RAIL_W, RACK_U]));
                out.push((&r.tiles[t + 1], [rr - RAIL_W, y, RAIL_W, RACK_U]));
                if let Some(n) = usize::try_from(k).ok().and_then(|k| r.nums.get(k)) {
                    let ny = y + RACK_U / 2.0 - 8.0;
                    out.push((n, [l, ny, RAIL_NUMS, 16.0]));
                    out.push((n, [rr - RAIL_NUMS, ny, RAIL_NUMS, 16.0]));
                }
            }
        }
        out
    }

    /// Brings the rack, faces and LCDs up to date where they are missing, out of date or drawn at
    /// another scale. `ms`: time, for the scrolling names. Faces and shadows are drawn on the
    /// painter's threads (the old ones show meanwhile, stretched or of the old shape), so a frame
    /// never waits for them; only a device seen for the first time is waited for. True while some
    /// are still being drawn.
    pub fn prepare(&mut self, scale: f64, ms: u128) -> bool {
        // faces and shadows drawn since the last frame
        for done in self.inbox.take() {
            self.install(done);
        }
        if self.pics_scale != scale {
            // the jacks' pictures: the first ones at once, side by side on all cores; at a new
            // scale on the painter's threads (the old ones stretch meanwhile)
            let mut svgs = vec![face::badge_pic()];
            for c in face::ACCENTS.iter() {
                svgs.extend([JackKind::Mono, JackKind::Stereo, JackKind::Midi].into_iter().map(|kind| face::jack_pic_ring(kind, c)));
            }
            svgs.extend([face::cue_disc_pic(), face::cue_ring_pic(false), face::cue_ring_pic(true), face::target_ring_pic(false), face::target_ring_pic(true)]);
            svgs.extend([face::cue_half_pic(false), face::cue_half_pic(true), face::cue_pair_pic(false, false), face::cue_pair_pic(true, false), face::cue_pair_pic(true, true)]);
            // (in the order of the indexes above: JACK_PICS, CUE_PICS, TARGET_PICS…)
            debug_assert_eq!(svgs.len(), self.jack_pics.len());
            if self.jack_pics.iter().any(Option::is_none) {
                let opt = &self.opt;
                let pms: Vec<Option<Pixmap>> = std::thread::scope(|sc| svgs.iter().map(|svg| sc.spawn(move || rasterize(opt, svg, scale))).collect::<Vec<_>>().into_iter().map(|t| t.join().ok().flatten()).collect());
                for (i, pm) in pms.into_iter().enumerate() {
                    self.jack_pics[i] = Some(pic_layer(i, pm));
                }
            } else {
                for (i, svg) in svgs.into_iter().enumerate() {
                    self.inbox.ask(painter::key("jack", i), painter::sig(i, scale), svg, scale);
                }
            }
            self.pics_scale = scale;
        }
        if self.rack && self.rails.as_ref().is_none_or(|r| r.scale != scale) {
            self.rails = Some(Rails::new(&self.opt, scale));
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
        // each device's pictures: the faces and shadows asked of the painter's threads
        let c = DevCtx { opt: &self.opt, rack_w: self.rack_w, accents: self.accents, scale, ms };
        let mut t = Times::default();
        for (di, d) in self.devs.iter_mut().enumerate() {
            prepare_dev(d, links.get(&di).copied().unwrap_or(0), &c, &mut self.inbox, &mut t);
        }
        // a device seen for the first time has nothing to show meanwhile: waited for (side by
        // side on the threads), whatever else comes back first
        let waited = Instant::now();
        let until = Instant::now() + Duration::from_secs(2);
        while self.inbox.busy() && self.devs.iter().any(|d| d.face.is_none() || d.shade.is_none()) {
            let Some(left) = until.checked_duration_since(Instant::now()) else { break };
            let Some(done) = self.inbox.wait(left) else { break };
            self.install(done);
        }
        self.prep_spent = vec![("faces.svg", t.svg), ("faces.knobs", t.knobs), ("faces.lcd", t.lcd), ("faces.meters", t.meters), ("faces.wait", waited.elapsed())];
        self.inbox.busy()
    }

    /// For pictures without a window: waits until every face and shadow asked for is drawn.
    pub fn finish(&mut self) {
        while self.inbox.busy() {
            let Some(done) = self.inbox.wait(Duration::from_secs(5)) else { break };
            self.install(done);
        }
    }

    /// A face or shadow the painter has drawn, in its place if it is still the one wanted.
    fn install(&mut self, done: painter::Done) {
        if let Some(i) = (0..self.jack_pics.len()).find(|i| done.key == painter::key("jack", i) && done.sig == painter::sig(i, self.pics_scale)) {
            self.jack_pics[i] = Some(pic_layer(i, done.pm));
            return;
        }
        for d in &mut self.devs {
            if let Some((want, sig, scale, w, h)) = d.face_want
                && want == done.sig
                && done.key == painter::key("face", &d.dev.key)
            {
                let old = d.face.replace(Layer { id: next_id(), pm: done.pm, x: -FACE_PAD, y: -FACE_PAD, w, h });
                // another screen's face is kept for going back
                if d.face_sig != sig
                    && let Some(old) = old
                {
                    d.spare = Some((d.face_sig, d.face_scale, old));
                }
                (d.face_sig, d.face_scale, d.face_want) = (sig, scale, None);
                return;
            }
            if let Some((want, w, h)) = d.shade_want
                && want == done.sig
                && done.key == painter::key("shade", &d.dev.key)
            {
                d.shade = Some((want, Layer { id: next_id(), pm: done.pm, x: -SHADOW, y: -SHADOW, w, h }));
                d.shade_want = None;
                return;
            }
            if let Some((want, x, h)) = d.lcd_want
                && want == done.sig
                && done.key == painter::key("lcd", &d.dev.key)
            {
                d.lcd = Some((want, Layer { id: next_id(), pm: done.pm, x, y: LCD_Y, w: LCD_W, h }));
                d.lcd_want = None;
                return;
            }
            if let Some(gi) = d.mix_want.iter().find(|(gi, w)| w.0 == done.sig && done.key == painter::key("mix", (&d.dev.key, *gi))).map(|(gi, _)| *gi) {
                let (sig, x, y, w) = d.mix_want.remove(&gi).unwrap_or_default();
                d.mix_pics.retain(|(g, _, _)| *g != gi);
                d.mix_pics.push((gi, sig, Layer { id: next_id(), pm: done.pm, x, y, w, h: MIX_H }));
                return;
            }
        }
    }

    /// How opaque a cord is drawn: faded or hidden by the cords switch; a cord in the hand always
    /// shows, and so do the hovered thing's cords, while the others fade.
    pub fn cord_alpha(&self, c: &Cord) -> f32 {
        let fade = [1.0, 0.22, 0.0][self.cables.min(2) as usize];
        let a = if c.a.held || c.b.held || c.grab.is_some() { c.alpha } else { c.alpha * fade };
        if self.lit == 0.0 {
            a
        } else if self.lights(c) {
            a + (c.alpha - a) * self.lit
        } else {
            a * (1.0 - 0.75 * self.lit)
        }
    }

    /// The cords and their lights drawn on the CPU, in window pixels, for pictures without a GPU
    /// (on screen the GPU draws them: see `mesh`).
    pub fn cord_layer(&self, view: View, win: (u32, u32)) -> Option<(Pixmap, i32, i32)> {
        let (x0, y0, x1, y1) = cords_box(self.cords.iter())?;
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite() && v.abs() < 1e12) {
            return None;
        }
        // room for the cords' width and their shadows (12 down, 13 wide)
        let m = 40.0;
        let s = view.s;
        let px0 = ((x0 - m) * s + view.x).floor().max(0.0) as i32;
        let py0 = ((y0 - m) * s + view.y).floor().max(0.0) as i32;
        let px1 = ((x1 + m) * s + view.x).ceil().min(win.0 as f64) as i32;
        let py1 = ((y1 + m) * s + view.y).ceil().min(win.1 as f64) as i32;
        if px1 <= px0 || py1 <= py0 {
            return None;
        }
        let mut pm = Pixmap::new((px1 - px0) as u32, (py1 - py0) as u32)?;
        let s = s as f32;
        let ts = Transform::from_row(s, 0.0, 0.0, s, view.x as f32 - px0 as f32, view.y as f32 - py0 as f32);
        paint::cords(&mut pm, &self.cords, |c| self.cord_alpha(c), ts);
        if self.cords.iter().any(|c| c.flow > 0.02) {
            paint::flow(&mut pm, &self.cords, |c| self.cord_alpha(c), ts);
        }
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
            // every jack is ringed in its group's colour (K switches only the bands); the L and R
            // of a split stereo jack keep its double ring: still the same pair
            let stereo = d.dev.groups.get(j.group).is_some_and(|g| g.stereo);
            let kind = if stereo { JackKind::Stereo } else { j.kind };
            let pic = JACK_PICS + 3 * face::accent_index(&d.dev, j.group)
                + match kind {
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
                    v.push((0, x + BADGE.0, y + BADGE.1, badge));
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

    /// A frame's SPLIT in its row of keys (beside MUTE) under a world point: its device and group.
    pub fn row_split_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| {
            d.geo.mix.iter().find_map(|(gi, m)| m.split.filter(|[kx, ky, kw, kh]| x >= d.x + kx && x <= d.x + kx + kw && y >= d.y + ky && y <= d.y + ky + kh).map(|_| (di, *gi)))
        })
    }

    /// Any of an app stream's controls under a world point.
    pub fn mix_control_at(&self, x: f64, y: f64) -> bool {
        self.knob_at(x, y).or(self.bal_at(x, y)).or(self.mute_at(x, y)).is_some()
    }

    /// Sets an app stream's volume, balance and mute (PipeWire is told; the LCD says what
    /// changed, and which stream on a device with two: IN or OUT).
    fn set_mix(&mut self, (di, gi): (usize, usize), volume: f64, balance: f64, mute: bool, says: Says) {
        let Some(d) = self.devs.get_mut(di) else { return };
        let Some(g) = d.dev.groups.get(gi) else { return };
        let Some(m) = &g.mix else { return };
        let input = m.gain;
        // only a stereo pair has a balance
        let balance = if g.stereo { balance } else { 0.0 };
        let was = d.mix(gi);
        let volumes = m.volumes_for(volume, balance);
        // a card keeps it on its port in use (as pavucontrol sets it); an app or a virtual device
        // on its node
        self.outbox.push(match m.route {
            Some(r) => Cmd::SetRoute { device: r.card, index: r.index, route_device: r.device, volumes: Some(volumes), mute: Some(mute), latency_ns: None },
            None => Cmd::SetMix { node: m.node, volumes, mute },
        });
        d.mix_local.insert(gi, (Instant::now(), (volume, balance, mute)));
        // what the hand turns, the LCD says, all the while it turns (told by what changed it
        // flickered between the two as the balance, rounded, changed or didn't)
        let bal = match says {
            Says::Balance => true,
            Says::Volume => false,
            Says::Changed => was.is_some_and(|(_, b, _)| (b - balance).abs() > 0.001),
        };
        let mut note = if bal && !mute { bal_text(volume, balance) } else { vol_text(volume, mute, input) };
        if d.dev.groups.iter().filter(|g| g.mix.is_some()).count() > 1 {
            // which one, by its frame's name: INPUT, OUTPUT, SPEAKERS, MIC…
            note = format!("{} {note}", face::name(&d.dev, gi));
        }
        d.mix_note = Some((Instant::now() + MIX_NOTE, note));
        self.edited = true;
    }

    pub fn set_volume(&mut self, at: (usize, usize), volume: f64) {
        let Some((_, bal, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        self.set_mix(at, (volume.clamp(0.0, VOL_MAX) * 100.0).round() / 100.0, bal, mute, Says::Volume);
    }

    pub fn set_balance(&mut self, at: (usize, usize), balance: f64) {
        let Some((vol, _, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        self.set_mix(at, vol, (balance.clamp(-1.0, 1.0) * 100.0).round() / 100.0, mute, Says::Balance);
    }

    /// Sets a stereo pair's L and R (on the knob's scale, 1 is 100%) apart: the louder one is its
    /// volume, the quieter one its balance.
    pub fn set_channels(&mut self, at: (usize, usize), l: f64, r: f64) {
        let Some((_, _, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        // each channel to a whole per cent, and the balance exactly from them: a balance rounded
        // on its own moved the other channel a little whenever one fader moved
        let pct = |x: f64| (x.clamp(0.0, VOL_MAX) * 100.0).round() / 100.0;
        let (l, r) = (pct(l), pct(r));
        let v = l.max(r);
        let bal = if v <= 0.0 || l == r { 0.0 } else if l > r { r / l - 1.0 } else { 1.0 - l / r };
        self.set_mix(at, v, bal, mute, Says::Changed);
    }

    pub fn toggle_mute(&mut self, at: (usize, usize)) {
        let Some((vol, bal, mute)) = self.devs.get(at.0).and_then(|d| d.mix(at.1)) else { return };
        self.set_mix(at, vol, bal, !mute, Says::Volume);
    }

    /// A card's outputs and inputs with ports to choose from, for its PORT list: group, label,
    /// ports, the one in use and its latency offset (ns).
    pub fn port_sections(&self, di: usize) -> Vec<PortSection> {
        let Some(d) = self.devs.get(di) else { return Vec::new() };
        d.dev
            .groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.ports_to.len() > 1)
            .map(|(gi, g)| {
                let r = g.mix.as_ref().and_then(|m| m.route);
                PortSection { group: gi, name: face::name(&d.dev, gi), ports: g.ports_to.clone(), current: r.map(|r| r.index), latency_ns: r.map_or(0, |r| r.latency_ns) }
            })
            .collect()
    }

    /// Switches a card's output or input to another of its ports.
    pub fn set_port(&mut self, di: usize, gi: usize, index: u32) {
        let Some(d) = self.devs.get_mut(di) else { return };
        let Some(g) = d.dev.groups.get(gi) else { return };
        let Some(r) = g.mix.as_ref().and_then(|m| m.route) else { return };
        if let Some(p) = g.ports_to.iter().find(|p| p.index == index) {
            d.mix_note = Some((Instant::now() + MIX_NOTE, format!("{}{}", p.description.to_uppercase(), if p.unplugged { " · UNPLUGGED" } else { "" })));
        }
        self.outbox.push(Cmd::SetRoute { device: r.card, index, route_device: r.device, volumes: None, mute: None, latency_ns: None });
        self.edited = true;
    }

    /// Sets the latency offset of the port a card's output or input uses.
    pub fn set_latency(&mut self, di: usize, gi: usize, ns: i64) {
        let Some(d) = self.devs.get_mut(di) else { return };
        let Some(r) = d.dev.groups.get(gi).and_then(|g| g.mix.as_ref()).and_then(|m| m.route) else { return };
        d.mix_note = Some((Instant::now() + MIX_NOTE, format!("LATENCY {:+} ms", ns as f64 / 1e6)));
        self.outbox.push(Cmd::SetRoute { device: r.card, index: r.index, route_device: r.device, volumes: None, mute: None, latency_ns: Some(ns) });
        self.edited = true;
    }

    /// The default output's or input's node.name, as PipeWire's "default" metadata says.
    pub fn default_name(&self, output: bool) -> Option<String> {
        let v = self.settings.get(if output { "default.audio.sink" } else { "default.audio.source" })?;
        json_name(v)
    }

    /// The outputs (devices' sinks) or inputs (their sources) that can be the default: device,
    /// group, node.name, and how the list names it (with the group's when a device has several).
    /// The outputs (or inputs) that can be the default: (device, group, node name, shown name).
    /// A device index past `devs` is one hidden by hand, in `hidden_devs`.
    pub fn ends(&self, output: bool) -> Vec<(usize, usize, String, String)> {
        let mut v = Vec::new();
        // the devices shown, then those hidden by hand (still there in PipeWire, and still a
        // choice: numbered after the shown ones)
        let all = self.devs.iter().map(|d| &d.dev).chain(&self.hidden_devs);
        for (di, d) in all.enumerate() {
            let of = |g: &crate::graph::Group| g.mix.as_ref().is_some_and(|m| m.gain != output && (g.dir == Dir::In) == output);
            let n = d.groups.iter().filter(|g| of(g)).count();
            for (gi, g) in d.groups.iter().enumerate().filter(|(_, g)| of(g)) {
                let Some(name) = g.ports.first().and_then(|p| self.graph.node_name(p.node)) else { continue };
                let shown = if n > 1 { format!("{} · {}", d.name, face::name(d, gi)) } else { d.name.clone() };
                v.push((di, gi, name.to_owned(), shown));
            }
        }
        v
    }

    /// What listening to a jack (or, from ENGINE, a port: `port_key`) brings into the cue: (node,
    /// port, channel) of each output port heard: an output jack's own ports; for an input jack,
    /// what is linked into it. A port is one channel: FL left, FR right, anything else both.
    pub fn cue_sources(&self, key: &str) -> Vec<crate::pw::CueSource> {
        if let Some(id) = port_of_key(key) {
            let Some(p) = self.graph.props_of(id) else { return Vec::new() };
            let ch = ear(p.get("audio.channel").map_or("", String::as_str));
            if !self.listenable(id) {
                return Vec::new();
            }
            if p.get("port.direction").map(String::as_str) == Some("out") {
                return self.graph.port_node(id).map(|n| vec![(n, id, ch)]).unwrap_or_default();
            }
            return self.graph.links().iter().filter(|l| l.in_port == id).filter_map(|l| Some((self.graph.port_node(l.out_port)?, l.out_port, ch))).collect();
        }
        let Some((d, j)) = self.jack(key) else { return Vec::new() };
        let ch = |p: &crate::graph::Port| ear(&p.channel);
        let ports: Vec<&crate::graph::Port> = j.ports.iter().filter_map(|id| d.dev.groups.iter().flat_map(|g| &g.ports).find(|p| p.id == *id)).collect();
        if j.dir == Dir::Out {
            return ports.iter().map(|p| (p.node, p.id, ch(p))).collect();
        }
        // a device's output (PLAYBACK): what it plays, its monitor (all it gets, mixed)
        let node = ports.first().map(|p| p.node);
        if let Some(mon) = d.dev.groups.iter().find(|g| g.role == "MONITOR" && g.ports.first().map(|p| p.node) == node) {
            return mon.ports.iter().map(|p| (p.node, p.id, ch(p))).collect();
        }
        let links = self.graph.links();
        ports.iter().flat_map(|p| links.iter().filter(move |l| l.in_port == p.id).filter_map(|l| Some((self.graph.port_node(l.out_port)?, l.out_port, ch(p))))).collect()
    }

    /// What a set of heard jacks brings into the cue, each sound once: a source whose sound also
    /// reaches a heard device's monitor (an app playing into a device heard) is heard there, and
    /// one that plays in the output listened on is heard already (the cue would double it).
    pub fn cue_mix(&self, keys: &[String]) -> Vec<crate::pw::CueSource> {
        let all: Vec<crate::pw::CueSource> = keys.iter().flat_map(|k| self.cue_sources(k)).collect();
        // the devices whose monitor is heard, and the output listened on
        let mut monitored: Vec<u32> = self.devs.iter().flat_map(|d| d.dev.groups.iter().filter(|g| g.role == "MONITOR").flat_map(|g| &g.ports)).filter(|p| all.iter().any(|s| s.1 == p.id)).map(|p| p.node).collect();
        monitored.extend(self.default_output_node());
        let mut out: Vec<crate::pw::CueSource> = Vec::new();
        for s in all {
            // however far along the cords (through a device and out of its monitor, and on)
            let into_heard = self.graph.reaches(s.1).iter().any(|n| monitored.contains(n) && *n != s.0);
            if !into_heard && !out.iter().any(|o| o.1 == s.1 && o.2 == s.2) {
                out.push(s);
            }
        }
        out
    }

    /// The default output's node.
    fn default_output_node(&self) -> Option<u32> {
        let name = self.default_name(true)?;
        let (di, gi, _, _) = self.ends(true).into_iter().find(|e| e.2 == name)?;
        // (shown or hidden by hand)
        let dev = self.devs.get(di).map(|d| &d.dev).or_else(|| self.hidden_devs.get(di - self.devs.len()))?;
        dev.groups[gi].ports.first().map(|p| p.node)
    }

    /// A jack whose sound plays in the output listened on already: the cue adds nothing.
    pub fn cue_already_heard(&self, key: &str) -> bool {
        !self.cue_sources(key).is_empty() && self.cue_mix(&[key.to_owned()]).is_empty()
    }

    /// Listening to a jack of the default output, or of its monitor, would feed what it plays
    /// back into it.
    pub fn cue_blocked(&self, key: &str) -> bool {
        let port = port_of_key(key).or_else(|| self.jack(key).and_then(|(_, j)| j.ports.first().copied()));
        let node = port.and_then(|p| self.graph.port_node(p));
        self.default_name(true).is_some_and(|n| node.and_then(|node| self.graph.node_name(node)) == Some(n.as_str()))
    }

    /// How the status bar names a jack: "fooyin · OUTPUT", "Ryzen … · PLAYBACK L".
    pub fn jack_title(&self, key: &str) -> String {
        if let Some(id) = port_of_key(key) {
            let node = self.graph.port_node(id);
            let desc = node.and_then(|n| self.graph.props_of(n)).and_then(|p| p.get("node.description").or(p.get("node.name"))).map_or("?", String::as_str);
            let name = self.graph.props_of(id).and_then(|p| p.get("port.name")).map_or("?", String::as_str);
            return format!("{desc} · {name}");
        }
        let Some((d, j)) = self.jack(key) else { return "?".into() };
        let g = d.dev.groups.get(j.group);
        let split = d.defs.iter().filter(|x| x.group == j.group).count() > 1;
        format!("{} · {}{}", d.dev.name, g.map_or("", |g| g.label.as_str()), if split { format!(" {}", j.label) } else { String::new() })
    }

    /// Which half of a stereo jack a port of it is (false: the left, FL; true: the right, FR);
    /// none for a jack of one port.
    pub fn port_side(&self, jack: &str, port: u32) -> Option<bool> {
        let (_, j) = self.jack(jack)?;
        (j.ports.len() == 2).then(|| j.ports.iter().position(|p| *p == port) == Some(1))
    }

    /// The other jack of a split stereo frame (L's R, R's L), and whether this one is the L.
    pub fn split_pair(&self, jack: &str) -> Option<(String, bool)> {
        let (d, j) = self.jack(jack)?;
        if j.ports.len() != 1 || !d.dev.groups.get(j.group).is_some_and(|g| g.stereo) {
            return None;
        }
        let both: Vec<&face::JackDef> = d.defs.iter().filter(|x| x.group == j.group).collect();
        if both.len() != 2 {
            return None;
        }
        let first = both[0].key == jack;
        Some((if first { both[1] } else { both[0] }.key.clone(), first))
    }

    /// The jack a heard key shows on: a jack itself, or the jack a port heard from ENGINE is on
    /// (none: a port of a device not on the desk, or of no jack).
    pub fn cue_jack<'a>(&'a self, key: &'a str) -> Option<&'a str> {
        match port_of_key(key) {
            Some(id) => self.port_jack.get(&id).map(String::as_str),
            None => Some(key),
        }
    }

    /// Whether a port can be listened to: sound (no MIDI, no video), and not one of Patchcord's
    /// own (its meters, the cue).
    pub fn listenable(&self, port: u32) -> bool {
        let audio = self.graph.props_of(port).and_then(|p| p.get("format.dsp")).is_some_and(|f| f.contains("audio"));
        let node = self.graph.port_node(port).and_then(|n| self.graph.node_name(n));
        audio && node.is_some_and(|n| !n.starts_with(crate::pw::METER_PREFIX) && !n.starts_with(crate::pw::CUE_NODE))
    }

    /// The jack with a slot (a mult's spare too) nearest a world point, within `r`: in CUE mode
    /// its hole (`HOLE`) patches and the ring round it (`RING`) listens.
    pub fn jack_near(&self, x: f64, y: f64, r: f64) -> Option<String> {
        let mut best: Option<(f64, &String)> = None;
        for (key, &(di, ji)) in &self.jacks {
            let n = face::slots(self.devs[di].counts.get(ji).copied().unwrap_or(0));
            for s in 0..n {
                if let Some((sx, sy)) = self.slot_pos(key, s) {
                    let d = (sx as f64 - x).hypot(sy as f64 - y);
                    if d <= r && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, key));
                    }
                }
            }
        }
        best.map(|(_, k)| k.clone())
    }

    /// Where a jack is (its first jack's centre), in world units.
    pub fn jack_pos(&self, key: &str) -> Option<(f32, f32)> {
        self.slot_pos(key, 0)
    }

    /// How the panel names the default output or input: the device (and group), or NONE.
    pub fn default_shown(&self, output: bool) -> String {
        match self.default_name(output) {
            Some(n) => self.ends(output).into_iter().find(|e| e.2 == n).map_or(n, |e| e.3),
            None => "NONE".to_owned(),
        }
    }

    /// Makes a node the default output or input (PipeWire moves the apps that follow the default).
    pub fn set_default(&mut self, output: bool, node_name: &str) {
        let key = if output { "default.configured.audio.sink" } else { "default.configured.audio.source" };
        self.outbox.push(Cmd::SetDefault { key: key.into(), node_name: node_name.to_owned() });
        if let Some(d) = self.ends(output).into_iter().find(|e| e.2 == node_name).and_then(|e| self.devs.get_mut(e.0)) {
            d.mix_note = Some((Instant::now() + MIX_NOTE, if output { "DEFAULT OUTPUT" } else { "DEFAULT INPUT" }.into()));
        }
        self.edited = true;
    }

    /// The LCD still shows a volume just changed: it has to be drawn again when that ends.
    pub fn noting(&self) -> bool {
        self.devs.iter().any(|d| d.mix_note.as_ref().is_some_and(|(until, _)| Instant::now() < *until + Duration::from_millis(50)))
    }

    /// The device whose PORT key is under a world point.
    pub fn port_key_at(&self, x: f64, y: f64) -> Option<usize> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| d.geo.port.filter(|[kx, ky, kw, kh]| x >= d.x + kx && x <= d.x + kx + kw && y >= d.y + ky && y <= d.y + ky + kh).map(|_| di))
    }

    /// The device whose PROFILE key is under a world point.
    pub fn profile_key_at(&self, x: f64, y: f64) -> Option<usize> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| d.geo.profile.filter(|[kx, ky, kw, kh]| x >= d.x + kx && x <= d.x + kx + kw && y >= d.y + ky && y <= d.y + ky + kh).map(|_| di))
    }

    /// The SPLIT/JOIN switch under a world point: (device, group).
    pub fn switch_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        self.devs.iter().enumerate().rev().find_map(|(di, d)| {
            d.geo.switches.iter().find_map(|(g, [bx, by, bw, bh])| (x >= d.x + bx && x <= d.x + bx + bw && y >= d.y + by && y <= d.y + by + bh).then_some((di, *g)))
        })
    }

    /// One stereo jack ⇄ separate L and R jacks. The cords re-plug themselves to match.
    /// The stereo jacks its cords lead to follow, and the ones theirs lead to, and so on: else a
    /// stereo jack would show two cords that each carry only L or R and look like they carry both.
    pub fn toggle_split(&mut self, di: usize, g: usize) {
        // a frame's one SPLIT: its first stereo pair says which way they all go
        let dev = &self.devs[di].dev;
        let g = face::sections(dev, g).into_iter().find(|s| dev.groups[*s].stereo).unwrap_or(g);
        let Some(&was) = self.devs[di].split.get(g) else { return };
        let group_of = self.stereo_groups();
        // links to hidden streams (Patchcord's own meters on app outputs, say) don't count: their
        // ports aren't in `group_of`
        let links = self.graph.links();
        let mut flip = vec![(di, g)];
        let mut k = 0;
        while k < flip.len() {
            let here = flip[k];
            // a frame's one SPLIT splits every stereo pair in it, and the MONITOR inside it
            let dev = &self.devs[here.0].dev;
            let secs = face::sections(dev, here.1);
            for o in (0..dev.groups.len()).filter(|o| face::host(dev, *o) == Some(here.1) || face::host(dev, here.1) == Some(*o) || (secs.contains(o) && dev.groups[*o].stereo)) {
                if !flip.contains(&(here.0, o)) {
                    flip.push((here.0, o));
                }
            }
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
        // its own device's colour; plugged into an output, it takes that one's
        let color = d.dev.color;
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

    /// A group's ports, as its jacks have them (FL, FR).
    fn group_ports(&self, (di, gi): (usize, usize)) -> Vec<u32> {
        self.devs.get(di).and_then(|d| d.dev.groups.get(gi)).map(|g| g.ports.iter().map(|p| p.id).collect()).unwrap_or_default()
    }

    /// The PipeWire links from group `out`'s ports into group `inp`'s.
    pub fn group_links(&self, out: (usize, usize), inp: (usize, usize)) -> Vec<(u32, u32)> {
        let (a, b) = (self.group_ports(out), self.group_ports(inp));
        self.graph.links().into_iter().filter(|l| a.contains(&l.out_port) && b.contains(&l.in_port)).map(|l| (l.out_port, l.in_port)).collect()
    }

    /// Links whole groups (`add`, out → in, as a cord between their jacks would: L to L and R to R,
    /// mono to both) and takes away all links between others (`del`), as one step to undo: what
    /// MIXER's PLAY ON and RECORD FROM keys do.
    pub fn reroute(&mut self, add: &[Route], del: &[Route]) {
        let mut a = Vec::new();
        for &(o, i) in add {
            let (x, y) = (self.group_ports(o), self.group_ports(i));
            if x.len() == y.len() {
                a.extend(x.iter().copied().zip(y.iter().copied()));
            } else {
                a.extend(x.iter().flat_map(|&p| y.iter().map(move |&q| (p, q))));
            }
        }
        let links = self.graph.links();
        a.retain(|(o, i)| !links.iter().any(|l| l.out_port == *o && l.in_port == *i));
        let d: Vec<(u32, u32)> = del.iter().flat_map(|&(o, i)| self.group_links(o, i)).collect();
        self.link(&a, &d);
        self.record(a, d);
    }

    /// Links ports (`add`, output → input) and takes links away (`del`), as one step to undo:
    /// what ENGINE's links do. One already there (or not there) is left out.
    pub fn link_ports(&mut self, add: &[(u32, u32)], del: &[(u32, u32)]) {
        let links = self.graph.links();
        let there = |&(o, i): &(u32, u32)| links.iter().any(|l| l.out_port == o && l.in_port == i);
        let a: Vec<(u32, u32)> = add.iter().copied().filter(|p| !there(p)).collect();
        let d: Vec<(u32, u32)> = del.iter().copied().filter(there).collect();
        self.link(&a, &d);
        self.record(a, d);
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
        let dir_of = |g: &crate::graph::Group| if g.dir == Dir::In { "in" } else { "out" }.to_owned();
        let ports = self
            .devs
            .iter()
            .flat_map(|d| {
                d.dev.groups.iter().filter_map(move |g| {
                    let r = g.mix.as_ref()?.route?;
                    let p = g.ports_to.iter().find(|p| p.index == r.index)?;
                    Some(((d.dev.key.clone(), g.label.clone(), dir_of(g)), (p.description.clone(), r.latency_ns)))
                })
            })
            .collect();
        Preset { name: name.to_owned(), nodes: self.graph.node_names(), links, split, pos: self.positions().into_iter().collect(), mix, profiles, ports, default_out: self.default_name(true), default_in: self.default_name(false) }
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
        // cards' ports (by name) and their latency offsets, and the default output and input
        for d in &self.devs {
            for g in &d.dev.groups {
                let key = (d.dev.key.clone(), g.label.clone(), if g.dir == Dir::In { "in" } else { "out" }.to_owned());
                if let (Some((name, ns)), Some(r)) = (p.ports.get(&key), g.mix.as_ref().and_then(|m| m.route))
                    && let Some(want) = g.ports_to.iter().find(|x| x.description == *name)
                    && (want.index != r.index || *ns != r.latency_ns)
                {
                    self.outbox.push(Cmd::SetRoute { device: r.card, index: want.index, route_device: r.device, volumes: None, mute: None, latency_ns: Some(*ns) });
                }
            }
        }
        for (output, want) in [(true, &p.default_out), (false, &p.default_in)] {
            if let Some(n) = want
                && self.default_name(output).as_ref() != Some(n)
                && self.ends(output).iter().any(|e| e.2 == *n)
            {
                let key = if output { "default.configured.audio.sink" } else { "default.configured.audio.source" };
                self.outbox.push(Cmd::SetDefault { key: key.into(), node_name: n.clone() });
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
                    self.set_mix((di, gi), vol.clamp(0.0, VOL_MAX), bal.clamp(-1.0, 1.0), mute, Says::Changed);
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
            x >= d.x && x <= d.x + d.geo.w && y >= d.y && y <= d.y + d.h()
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
            let group = if j.group < d.dev.groups.len() { face::name(&d.dev, j.group) } else { String::new() };
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
        // a frame's name: its block in ENGINE, by the node's number
        if let Some(node) = self.graph_key_at(x, y) {
            let name = self.graph.node_name(node).unwrap_or("?");
            return format!("#{node} · {name}  ·  click: open it in ENGINE");
        }
        if let Some((di, gi)) = self.knob_at(x, y)
            && let Some((v, _, mute)) = self.devs[di].mix(gi)
        {
            let g = &self.devs[di].dev.groups[gi];
            let input = g.mix.as_ref().is_some_and(|m| m.gain);
            return format!("{} · {} · {}  ·  drag up/down: {}, Shift: fine · wheel: 5% (Shift: 1%) · double-click: 100%", self.devs[di].dev.name, face::name(&self.devs[di].dev, gi), vol_text(v, mute, input), if input { "gain" } else { "volume" });
        }
        if let Some((di, gi)) = self.bal_at(x, y)
            && let Some((v, b, _)) = self.devs[di].mix(gi)
        {
            let d = &self.devs[di];
            let label = &face::name(&d.dev, gi);
            return if d.bal(gi) == BalKnob::Out {
                format!("{} · {label} · {}  ·  drag up/down: balance (up is right), Shift: fine · wheel · click: push it in", d.dev.name, bal_text(v, b))
            } else {
                format!("{} · {label} · {}  ·  click: pop the knob out to turn it", d.dev.name, bal_text(v, b))
            };
        }
        if let Some((di, gi)) = self.mute_at(x, y)
            && let Some((_, _, mute)) = self.devs[di].mix(gi)
        {
            return format!("{} · {}  ·  click: {}", self.devs[di].dev.name, face::name(&self.devs[di].dev, gi), if mute { "unmute" } else { "mute" });
        }
        if let Some(di) = self.port_key_at(x, y) {
            let now: Vec<String> = self.port_sections(di).iter().filter_map(|s| Some(format!("{}: {}", s.name, s.ports.iter().find(|p| Some(p.index) == s.current)?.description))).collect();
            return format!("{}  ·  click: choose its ports and latency offset", now.join(" · "));
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

/// The name in a default's value: `{ "name": "bluez_output.02_00_00_00_00_01.1" }`.
fn json_name(v: &str) -> Option<String> {
    let rest = &v[v.find("\"name\"")? + 6..];
    let rest = &rest[rest.find('"')? + 1..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

/// The hover highlight one frame on, towards `to`: up quickly, down a little slower, and still
/// once there (it used to step down from 1 and back up every frame, and the cords blinked).
fn ease_lit(lit: f32, to: f32, dt: f32) -> f32 {
    if lit < to { (lit + dt * 8.0).min(to) } else { (lit - dt * 5.0).max(to) }
}

/// What `Scene::prepare` spent on drawing in the frame itself, by part (for the frame log).
#[derive(Default)]
struct Times {
    /// faces' SVG built (and their geometry), to be rasterized on the painter's threads
    svg: Duration,
    /// knobs rasterized here
    knobs: Duration,
    /// LCDs rasterized here
    lcd: Duration,
    /// lit meters rasterized here
    meters: Duration,
}

/// What `Scene::prepare` hands each device's drawing: the same for all of them.
struct DevCtx<'a> {
    opt: &'a usvg::Options<'static>,
    rack_w: Option<f64>,
    accents: bool,
    scale: f64,
    ms: u128,
}

/// A device's pictures, brought up to date (see `Scene::prepare`): its face and shadow asked of
/// the painter (the old ones show meanwhile), its knobs, LCD and meters drawn here (they are
/// small). `n`: how many cords it has.
fn prepare_dev(d: &mut DevView, n: usize, c: &DevCtx, inbox: &mut painter::Inbox, t: &mut Times) {
    let sig = d.sig(c.rack_w, c.accents);
    let want = painter::sig(sig, c.scale);
    if (d.face.is_none() || want != painter::sig(d.face_sig, d.face_scale)) && d.face_want.map(|w| w.0) != Some(want) {
        // its geometry (where its jacks and knobs are) changes at once; its picture follows
        let t0 = Instant::now();
        let (svg, geo, skin) = face::face(&d.dev, &d.defs, &d.counts, &d.split, c.rack_w, c.accents);
        t.svg += t0.elapsed();
        // (by the face shown: its geometry may have changed already, as the rack's width does). Only
        // a new height moves its frames (they go on to another unit): a face only wider or
        // narrower (the rack's width following the window's) has them where they were, from its
        // left edge, so the old picture shows, at its own size, until the new one comes
        let reshaped = d.face.as_ref().is_some_and(|f| f.h != geo.h + 2.0 * FACE_PAD);
        d.geo = geo;
        // back to the other screen: its face as it was shows (scaled) until this one comes
        if d.face_sig != sig
            && let Some((ss, sc, sl)) = d.spare.take_if(|(ss, ..)| *ss == sig)
        {
            if let Some(old) = d.face.replace(sl) {
                d.spare = Some((d.face_sig, d.face_scale, old));
            }
            (d.face_sig, d.face_scale) = (ss, sc);
        }
        let (w, h) = (d.geo.w + 2.0 * FACE_PAD, d.geo.h + 2.0 * FACE_PAD);
        if reshaped && d.face_sig != sig {
            // a new shape and none kept for it: waited for (as a new device is, side by side on
            // the painter's threads), an old face of another shape would show its jacks and knobs
            // elsewhere
            if let Some(old) = d.face.take() {
                d.spare = Some((d.face_sig, d.face_scale, old));
            }
            d.face_want = Some((want, sig, c.scale, w, h));
            inbox.ask_skinned(painter::key("face", &d.dev.key), want, svg, c.scale, Some(skin));
        } else if want != painter::sig(d.face_sig, d.face_scale) {
            d.face_want = Some((want, sig, c.scale, w, h));
            inbox.ask_skinned(painter::key("face", &d.dev.key), want, svg, c.scale, Some(skin));
        }
        d.gmeters.clear();
    }
    // each group's knobs and MUTE LED, drawn again when they move
    let mut pics = Vec::new();
    for (gi, m) in &d.geo.mix {
        let Some((vol, bal, mute)) = d.mix(*gi) else { continue };
        let mut h = DefaultHasher::new();
        // (its shape too: a stream gone from stereo to mono has a narrower section, without BAL)
        // its SPLIT, in the row with MUTE, and which of the two is held down
        let split_on = d.split.get(*gi).copied().unwrap_or(false);
        let held = d.pressed.filter(|(g, _)| g == gi).map(|(_, k)| k);
        (vol.to_bits(), bal.to_bits(), mute, d.bal(*gi), c.scale.to_bits(), m.x.to_bits(), m.y.to_bits(), m.w.to_bits(), m.bal.is_some(), m.split.is_some(), split_on, held).hash(&mut h);
        let sig = h.finish();
        match (d.mix_pics.iter().position(|(g, s, _)| g == gi && *s == sig), d.mix_pics.iter().position(|(g, _, _)| g == gi)) {
            (Some(i), _) => pics.push(d.mix_pics.swap_remove(i)),
            // the knobs as they were show until they are drawn as they are now, where they are now
            // (not when the section changed its width: the old one would stand out of its frame)
            (None, Some(i)) if d.mix_pics[i].2.w == m.w => {
                let mut p = d.mix_pics.swap_remove(i);
                (p.2.x, p.2.y) = (m.x, m.y + MIX_Y);
                pics.push(p);
                if d.mix_want.get(gi).map(|w| w.0) != Some(sig) {
                    d.mix_want.insert(*gi, (sig, m.x, m.y + MIX_Y, m.w));
                    inbox.ask(painter::key("mix", (&d.dev.key, gi)), sig, face::mix_pic(m, vol, bal, mute, d.bal(*gi), split_on, held), c.scale);
                }
            }
            (None, other) => {
                if let Some(i) = other {
                    d.mix_pics.swap_remove(i);
                }
                let t0 = Instant::now();
                let pm = rasterize(c.opt, &face::mix_pic(m, vol, bal, mute, d.bal(*gi), split_on, held), c.scale);
                t.knobs += t0.elapsed();
                pics.push((*gi, sig, Layer { id: next_id(), pm, x: m.x, y: m.y + MIX_Y, w: m.w, h: MIX_H }));
            }
        }
    }
    d.mix_pics = pics;
    let wsig = d.geo.w.to_bits() ^ d.geo.h.to_bits().rotate_left(17) ^ c.rack_w.is_some() as u64;
    if d.shade.as_ref().is_none_or(|(s, _)| *s != wsig) && d.shade_want.map(|w| w.0) != Some(wsig) {
        // a blur: drawn at a small fixed scale and stretched, so zooming never redraws it
        d.shade_want = Some((wsig, d.geo.w + 2.0 * SHADOW, d.geo.h + 2.0 * SHADOW));
        inbox.ask(painter::key("shade", &d.dev.key), wsig, face::shadow(d.geo.w, d.geo.h, c.rack_w.is_some()), SHADE_SCALE);
    }
    let note = d.mix_note.as_ref().filter(|(until, _)| Instant::now() < *until).map(|(_, s)| s.clone());
    let links = if let Some(note) = note {
        note
    } else if d.dev.groups.is_empty() && d.dev.card.is_some() {
        "NO INPUTS OR OUTPUTS".to_owned()
    } else {
        match n {
            0 => "NO LINKS".to_owned(),
            1 => "1 LINK".to_owned(),
            n => format!("{n} LINKS"),
        }
    };
    // what it takes of PipeWire
    let usage = match (d.dev.nodes, d.dev.ports) {
        (0, _) => "NO NODES".to_owned(),
        (n, p) => format!("{n} NODE{}  {p} PORT{}", if n == 1 { "" } else { "S" }, if p == 1 { "" } else { "S" }),
    };
    // a card's profile (and codec), scrolling when long; else what it carries: a stereo pair, one
    // channel, or as many channels as its widest frame has when they don't say they're L and R
    // (a JACK app's out1, out2)
    let channels = d.dev.groups.iter().map(|g| g.ports.iter().filter(|p| p.media == Media::Audio).count()).max().unwrap_or(0);
    let carries = match &d.dev.card {
        Some(card) => face::marquee(&card.summary(), c.ms),
        None if d.dev.groups.iter().any(|g| g.ports.iter().any(|p| p.media == Media::Midi)) => "MIDI".to_owned(),
        None if d.dev.groups.iter().any(|g| g.stereo) => "STEREO".to_owned(),
        None if channels > 1 => format!("{channels} CH"),
        None => "MONO".to_owned(),
    };
    // what it is, and its state against the right edge (the name is on the plate)
    let state = d.dev.state.to_uppercase();
    let kind = format!("{:<w$}{state}", d.dev.kind, w = face::LCD_CH.saturating_sub(state.chars().count()).max(d.dev.kind.chars().count() + 1));
    // in this order: what it is, what it carries (a card's profile), what it takes of PipeWire
    // and its cords on one line (two spaces between, one where the line is full; a volume note
    // instead while it changes)
    let third = if d.mix_note.as_ref().is_some_and(|(until, _)| Instant::now() < *until) {
        links
    } else {
        let wide = format!("{usage}  {links}");
        if wide.chars().count() <= face::LCD_CH { wide } else { format!("{usage} {links}") }
    };
    let text = format!("{kind}\n{carries}\n{third}");
    let x = d.geo.left + LCD_X;
    let lsig = painter::sig((&text, x.to_bits()), c.scale);
    // the screen as it was shows where it is now until it is drawn again
    if let Some((_, l)) = &mut d.lcd {
        l.x = x;
    }
    if d.lcd.as_ref().is_none_or(|(s, _)| *s != lsig) && d.lcd_want.map(|w| w.0) != Some(lsig) {
        let l: Vec<&str> = text.split('\n').collect();
        let svg = face::lcd([l[0], l[1], l[2]]);
        if d.lcd.is_none() {
            let t0 = Instant::now();
            let pm = rasterize(c.opt, &svg, c.scale);
            t.lcd += t0.elapsed();
            d.lcd = Some((lsig, Layer { id: next_id(), pm, x, y: LCD_Y, w: LCD_W, h: LCD_H }));
        } else {
            // the old one shows until this one is drawn
            d.lcd_want = Some((lsig, x, LCD_H));
            inbox.ask(painter::key("lcd", &d.dev.key), lsig, svg, c.scale);
        }
    }
    // a group's own meter
    let mut meters = Vec::new();
    for &(gi, x, oy, cols) in &d.geo.meters {
        let Some(lv) = d.glevel.get(&gi) else { continue };
        let gi = &gi;
        let lit = (lv[0].round() as u8, if cols > 1 { lv[1].round() as u8 } else { 0 });
        if lit == (0, 0) {
            continue;
        }
        match d.gmeters.iter().position(|(g, l, _)| g == gi && *l == lit) {
            Some(i) => meters.push(d.gmeters.swap_remove(i)),
            None => {
                let t0 = Instant::now();
                meters.push((*gi, lit, meter_layer(lit, Leds::strip(x, oy, cols, c.accents), c.scale)));
                t.meters += t0.elapsed();
            }
        }
    }
    d.gmeters = meters;
}

/// The lit LEDs of a signal meter (the slot and the dark ones are part of the face), as MIXER's: no glow.
fn meter_layer(lit: (u8, u8), leds: Leds, scale: f64) -> Layer {
    let (x0, y0) = (leds.x - 4.0, leds.y - 9.0 * leds.step - 4.0);
    let (w, h) = (leds.col + leds.w + 8.0, 9.0 * leds.step + leds.h + 8.0);
    let s = scale as f32;
    let mut pm = Pixmap::new((w as f32 * s).ceil() as u32, (h as f32 * s).ceil() as u32);
    if let Some(pm) = &mut pm {
        let ts = Transform::from_row(s, 0.0, 0.0, s, -(x0 as f32) * s, -(y0 as f32) * s);
        for (c, n) in [lit.0, lit.1].into_iter().enumerate() {
            for i in 0..n.min(10) {
                // lit as MIXER's: its colour, no glow
                let rgb = Leds::colour(i as usize);
                let (x, y) = ((leds.x + c as f64 * leds.col) as f32, (leds.y - i as f64 * leds.step) as f32);
                if let Some(r) = Rect::from_xywh(x, y, leds.w as f32, leds.h as f32) {
                    let mut p = Paint { anti_alias: true, ..Default::default() };
                    p.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
                    pm.fill_path(&PathBuilder::from_rect(r), &p, FillRule::Winding, ts, None);
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

/// Which ear a channel is heard in: FL the left (0), FR the right (1), anything else both.
fn ear(channel: &str) -> Option<usize> {
    match channel {
        "FL" => Some(0),
        "FR" => Some(1),
        _ => None,
    }
}

/// A jack's black hole, in world units: in CUE mode it patches.
pub const HOLE: f64 = 5.5;
/// The ring round a jack heard (the cue's mark), in world units: in CUE mode all of it listens.
pub const RING: f64 = 21.1;

/// A group of a device: (device, group).
pub type At = (usize, usize);
/// Groups linked whole: from an output group into an input group.
pub type Route = (At, At);

/// The key a port heard from ENGINE goes by among the jacks heard, and back.
pub fn port_key(port: u32) -> String {
    format!("port:{port}")
}

pub fn port_of_key(key: &str) -> Option<u32> {
    key.strip_prefix("port:")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene with devices on the desk at (x, y), `w` wide.
    fn desk(at: &[(f64, f64, f64)]) -> Scene {
        let mut s = Scene::new();
        s.rack = false;
        for (k, &(x, y, w)) in at.iter().enumerate() {
            let dev = Device { key: format!("d{k}"), name: format!("d{k}"), model: String::new(), kind: "", finish: crate::graph::Finish::Aluminium, color: [0; 3], groups: Vec::new(), state: "idle", chain: false, nodes: 0, ports: 0, card: None };
            let geo = Geometry { w, ..Default::default() };
            s.devs.push(DevView::new(dev, Vec::new(), geo, (x, y)));
        }
        s
    }

    fn overlap(s: &Scene) -> bool {
        let d = &s.devs;
        (0..d.len()).any(|i| (0..d.len()).any(|j| i != j && !apart((d[i].dx, d[i].dy, d[i].geo.w, d[i].h()), (d[j].dx, d[j].dy, d[j].geo.w, d[j].h()), 0.0)))
    }

    /// ARRANGE puts unused devices kind by kind, in reading order: sound cards, then apps, then
    /// MIDI, the MIDI devices next to each other; rows shaped to the window.
    #[test]
    fn arrange_puts_unused_devices_together_by_kind() {
        use crate::graph::Finish;
        let mut s = desk(&[(0.0, 0.0, 300.0), (0.0, 300.0, 300.0), (0.0, 600.0, 300.0), (0.0, 900.0, 300.0)]);
        for (d, f) in s.devs.iter_mut().zip([Finish::Olive, Finish::Aluminium, Finish::Olive, Finish::Black]) {
            d.dev.finish = f;
        }
        for (view, what) in [((1400.0, 820.0), "wide"), ((500.0, 1400.0), "tall")] {
            (s.view_w, s.view_h) = view;
            s.arrange();
            let mut order: Vec<usize> = (0..4).collect();
            order.sort_by(|a, b| s.devs[*a].ty.total_cmp(&s.devs[*b].ty).then(s.devs[*a].tx.total_cmp(&s.devs[*b].tx)));
            assert_eq!(order, [1, 3, 0, 2], "{what}: cards, apps, then MIDI");
            assert!(!overlap(&s), "{what}");
        }
        // a tall window gets them in a column, a wide one in a row
        assert!(s.devs[2].ty > s.devs[0].ty);
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
            client: None,
            nick: None,
            role: if label == "PLAYBACK" { "PLAYBACK" } else { "OUTPUT" },
            kind: if label == "PLAYBACK" { "SINK" } else { "PLAYBACK STREAM" },
            dir,
            ports: ids.iter().zip(["FL", "FR"]).map(|(&id, ch)| Port { id, node: id, name: format!("p{id}"), label: ch.into(), dir, media: Media::Audio, channel: ch.into() }).collect(),
            stereo: true,
            meter: true,
            mix: None,
            ports_to: Vec::new(),
            stream: 0,
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

    /// A frame's one SPLIT splits its input and the MONITOR inside it together, and JOIN joins both.
    #[test]
    fn one_split_splits_an_input_and_its_monitor() {
        use crate::graph::{Group, Mix, Port};
        let port = |id: u32, dir: Dir, ch: &str| Port { id, node: 1, name: format!("p{id}"), label: if ch == "FL" { "L" } else { "R" }.into(), dir, media: Media::Audio, channel: ch.into() };
        let group = |role: &'static str, dir: Dir, ids: [u32; 2], mix: bool| Group {
            label: role.into(),
            client: None,
            nick: None,
            role,
            kind: "SINK",
            dir,
            ports: vec![port(ids[0], dir, "FL"), port(ids[1], dir, "FR")],
            stereo: true,
            meter: true,
            mix: mix.then(|| Mix { node: 1, volumes: vec![1.0, 1.0], mute: false, gain: false, route: None }),
            ports_to: Vec::new(),
            stream: 0,
        };
        let mut s = desk(&[(0.0, 0.0, 300.0)]);
        s.devs[0].dev.groups = vec![group("PLAYBACK", Dir::In, [1, 2], true), group("MONITOR", Dir::Out, [3, 4], false)];
        s.devs[0].split = vec![false, false];
        s.index();
        s.toggle_split(0, 0);
        assert_eq!(s.devs[0].split, [true, true]);
        s.toggle_split(0, 0);
        assert_eq!(s.devs[0].split, [false, false]);
    }

    /// A plugged-in cord has its light running, sound or not, and it is drawn; off, it fades out.
    #[test]
    fn a_plugged_in_cord_glows() {
        use crate::graph::{Group, Port};
        let group = |label: &str, dir: Dir, ids: [u32; 2]| Group {
            label: label.into(),
            client: None,
            nick: None,
            role: if label == "PLAYBACK" { "PLAYBACK" } else { "OUTPUT" },
            kind: if label == "PLAYBACK" { "SINK" } else { "PLAYBACK STREAM" },
            dir,
            ports: ids.iter().zip(["FL", "FR"]).map(|(&id, ch)| Port { id, node: id, name: format!("p{id}"), label: ch.into(), dir, media: Media::Audio, channel: ch.into() }).collect(),
            stereo: true,
            meter: true,
            mix: None,
            ports_to: Vec::new(),
            stream: 0,
        };
        let mut s = desk(&[(0.0, 0.0, 300.0), (500.0, 0.0, 300.0)]);
        for (d, g) in s.devs.iter_mut().zip([group("OUTPUT", Dir::Out, [1, 2]), group("PLAYBACK", Dir::In, [3, 4])]) {
            d.dev.groups = vec![g];
            d.split = vec![false];
        }
        s.index();
        let key = |s: &Scene, di: usize| s.devs[di].defs[0].key.clone();
        let (out, inp) = (key(&s, 0), key(&s, 1));
        s.show_cord(&out, &inp);
        for _ in 0..60 {
            s.tick(1.0 / 60.0, None, None, false);
        }
        let c = &s.cords[0];
        assert!(c.flow > 0.5, "it lights up: {}", c.flow);
        assert!(c.flow_at > 0.0, "its light moves");
        let mut pm = Pixmap::new(900, 400).unwrap();
        paint::flow(&mut pm, &s.cords, |_| 1.0, Transform::identity());
        assert!(pm.pixels().iter().any(|p| p.alpha() > 0), "and it is drawn");
        s.flow = false;
        for _ in 0..90 {
            s.tick(1.0 / 60.0, None, None, false);
        }
        assert!(s.cords[0].flow < 0.02, "switched off, it fades out");
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
    fn a_default_is_read_by_its_node_name() {
        assert_eq!(json_name(r#"{ "name": "bluez_output.02_00_00_00_00_01.1" }"#).as_deref(), Some("bluez_output.02_00_00_00_00_01.1"));
        assert_eq!(json_name(r#"{"name":"a \"quoted\" sink"}"#).as_deref(), Some(r#"a "quoted" sink"#));
        assert_eq!(json_name("{}"), None);
    }

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

    /// A port listened to from ENGINE: an output port is heard itself, in its own ear (FL left,
    /// FR right, MONO both); an input port, what is linked into it, in the input's ear; MIDI and
    /// Patchcord's own ports not at all.
    #[test]
    fn ports_are_heard_in_their_own_ear() {
        use crate::pw::{Event, Kind};
        let mut s = Scene::new();
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<crate::pw::Props>();
        let mut ev = Vec::new();
        for (id, name, class) in [(10, "vivaldi", "Stream/Output/Audio"), (20, "recorder", "Stream/Input/Audio"), (30, "patchcord-meter-1", "Stream/Input/Audio"), (40, "midi", "Midi/Bridge")] {
            let p = props(&[("node.name", name), ("node.description", name), ("media.class", class)]);
            ev.push(Event::Added { id, kind: Kind::Node, props: p.clone() });
            ev.push(Event::Info { id, props: p, state: Some("running") });
        }
        for (id, node, dir, ch, fmt) in [(11, "10", "out", "FL", "32 bit float mono audio"), (12, "10", "out", "FR", "32 bit float mono audio"), (21, "20", "in", "FR", "32 bit float mono audio"), (31, "30", "in", "FL", "32 bit float mono audio"), (41, "40", "out", "", "8 bit raw midi")] {
            ev.push(Event::Added { id, kind: Kind::Port, props: props(&[("node.id", node), ("port.direction", dir), ("audio.channel", ch), ("port.name", "p"), ("format.dsp", fmt)]) });
        }
        let l = props(&[("link.output.node", "10"), ("link.output.port", "11"), ("link.input.node", "20"), ("link.input.port", "21")]);
        ev.push(Event::Added { id: 50, kind: Kind::Link, props: l });
        ev.push(Event::Synced);
        s.apply(ev);
        assert_eq!(s.cue_sources(&port_key(12)), vec![(10, 12, Some(1))], "an output port: itself, FR in the right ear");
        assert_eq!(s.cue_sources(&port_key(21)), vec![(10, 11, Some(1))], "an input port: what comes in, in the input's ear");
        assert!(s.cue_sources(&port_key(31)).is_empty() && !s.listenable(31), "a meter's port: Patchcord's own");
        assert!(s.cue_sources(&port_key(41)).is_empty() && !s.listenable(41), "MIDI: nothing to hear");
        assert_eq!(port_of_key(&port_key(12)), Some(12));
        assert_eq!(port_of_key("device:x/out/0"), None);
    }

    /// A made-up graph: two devices, a few apps (one on several streams), the music player
    /// playing into the speakers.
    fn demo_events() -> Vec<Event> {
        use crate::pw::{Kind, Props};
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
        let mut ev = Vec::new();
        let mut port = 1000;
        for (id, name, class, ports) in [
            (10u32, "Speakers", "Audio/Sink", &[("in", "playback_FL"), ("in", "playback_FR")][..]),
            (11, "Microphone", "Audio/Source", &[("out", "capture_FL"), ("out", "capture_FR")][..]),
            (12, "Music player", "Stream/Output/Audio", &[("out", "output_FL"), ("out", "output_FR")][..]),
            (13, "Browser", "Stream/Output/Audio", &[("out", "output_FL"), ("out", "output_FR")][..]),
            (14, "Recorder", "Stream/Input/Audio", &[("in", "input_FL"), ("in", "input_FR")][..]),
            // an app on several streams: a frame each, more than a narrow rack has room for
            (15, "Meeting", "Stream/Output/Audio", &[("out", "output_FL"), ("out", "output_FR")][..]),
            (16, "Meeting", "Stream/Output/Audio", &[("out", "output_FL"), ("out", "output_FR")][..]),
            (17, "Meeting", "Stream/Output/Audio", &[("out", "output_FL"), ("out", "output_FR")][..]),
            (18, "Meeting", "Stream/Input/Audio", &[("in", "input_FL"), ("in", "input_FR")][..]),
        ] {
            let pid = if name == "Meeting" { "4242".to_owned() } else { id.to_string() };
            // the sink and source are hardware's, without a program
            let app = if class.starts_with("Stream/") { name } else { "" };
            let p = props(&[("node.name", name), ("node.description", name), ("media.class", class), ("application.name", app), ("application.process.id", &pid)]);
            ev.push(Event::Added { id, kind: Kind::Node, props: p.clone() });
            ev.push(Event::Info { id, props: p, state: Some("running") });
            for (dir, pname) in ports {
                port += 1;
                let nid = id.to_string();
                ev.push(Event::Added { id: port, kind: Kind::Port, props: props(&[("node.id", &nid), ("port.direction", dir), ("port.name", pname), ("format.dsp", "32 bit float mono audio")]) });
            }
        }
        // the music player into the speakers
        for (lid, op, ip) in [(500u32, "1005", "1001"), (501, "1006", "1002")] {
            let p = props(&[("link.output.node", "12"), ("link.output.port", op), ("link.input.node", "10"), ("link.input.port", ip)]);
            ev.push(Event::Added { id: lid, kind: Kind::Link, props: p.clone() });
            ev.push(Event::Info { id: lid, props: p, state: Some("active") });
        }
        ev.push(Event::Synced);
        ev
    }

    /// Split headphones: the L and R of one app stand in one column of the mult, whatever order
    /// PipeWire made the links in (here A's L, B's L, B's R, A's R).
    #[test]
    fn split_rows_keep_a_sources_l_and_r_in_one_column() {
        use crate::pw::{Kind, Props};
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
        let mut ev = Vec::new();
        for (id, name, class, dir, pname) in [(10u32, "Phones", "Audio/Sink", "in", "playback"), (20, "A", "Stream/Output/Audio", "out", "output"), (30, "B", "Stream/Output/Audio", "out", "output")] {
            let p = props(&[("node.name", name), ("media.class", class), ("application.name", if class.starts_with("Stream") { name } else { "" }), ("application.process.id", &id.to_string())]);
            ev.push(Event::Added { id, kind: Kind::Node, props: p.clone() });
            ev.push(Event::Info { id, props: p, state: Some("running") });
            for (k, ch) in ["FL", "FR"].iter().enumerate() {
                ev.push(Event::Added { id: id + 1 + k as u32, kind: Kind::Port, props: props(&[("node.id", &id.to_string()), ("port.direction", dir), ("port.name", &format!("{pname}_{ch}")), ("audio.channel", ch), ("format.dsp", "32 bit float mono audio")]) });
            }
        }
        ev.push(Event::Synced);
        let mut s = Scene::new();
        s.apply(ev);
        // split first, then the links come, one by one
        let phones = s.devs.iter().position(|d| d.dev.name == "Phones").expect("the headphones");
        s.toggle_split(phones, 0);
        for (lid, o, i) in [(100u32, 21, 11), (101, 31, 11), (102, 32, 12), (103, 22, 12)] {
            let p = props(&[("link.output.port", &o.to_string()), ("link.input.port", &i.to_string())]);
            s.apply(vec![Event::Added { id: lid, kind: Kind::Link, props: p.clone() }, Event::Info { id: lid, props: p, state: Some("active") }]);
        }
        let rank = |s: &Scene, src: &str, ch: &str| {
            s.cords.iter().find_map(|c| {
                let (a, b) = (c.a.jack.as_deref()?, c.b.jack.as_deref()?);
                (a.contains(&format!("node:{src}|")) && b.ends_with(&format!("playback_{ch}"))).then_some(c.b.rank)
            })
        };
        assert_eq!((rank(&s, "A", "FL"), rank(&s, "A", "FR")), (Some(0), Some(0)), "A's L over its R");
        assert_eq!((rank(&s, "B", "FL"), rank(&s, "B", "FR")), (Some(1), Some(1)), "B's L over its R");
    }

    /// Every device its own cord colour while there are colours to go round.
    #[test]
    fn devices_get_colours_of_their_own() {
        let mut s = Scene::new();
        s.apply(demo_events());
        let colours: HashSet<[u8; 3]> = s.devs.iter().map(|d| d.dev.color).collect();
        assert_eq!(colours.len(), s.devs.len(), "{} devices", s.devs.len());
    }

    /// A picture of the rack, for looking at without PipeWire: a few devices from a made-up graph,
    /// the rails, at 100% in a window `PATCHCORD_PIC_SIZE` (WxH, 1200x900 by default),
    /// scrolled `PATCHCORD_PIC_DOWN` units down. Writes PATCHCORD_PIC. Run with --ignored.
    #[test]
    #[ignore]
    fn rack_picture() {
        let Ok(out) = std::env::var("PATCHCORD_PIC") else { return };
        let (ww, wh) = std::env::var("PATCHCORD_PIC_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))).unwrap_or((1200, 900));
        let down: f64 = std::env::var("PATCHCORD_PIC_DOWN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let ev = demo_events();
        let mut s = Scene::new();
        s.apply(ev);
        s.set_rack(true);
        s.set_rack_room(ww as f64, wh as f64 - 60.0 - 22.0);
        let (l, r, _) = s.rack_extent().expect("in the rack");
        let sc = ww as f64 / (r - l);
        let top = 60.0;
        let v = View { s: sc, x: -l * sc, y: top - down * RACK_U * sc };
        for _ in 0..400 {
            s.tick(1.0 / 60.0, None, None, false);
        }
        s.prepare(sc, 0);
        s.finish();
        s.rail_numbers((wh as f64 - v.y) / sc);
        let mut pm = Pixmap::new(ww, wh).unwrap();
        pm.fill(resvg::tiny_skia::Color::from_rgba8(0x17, 0x18, 0x1a, 255));
        let draw = |pm: &mut Pixmap, l: &Layer, (x, y, w, h): (f64, f64, f64, f64)| {
            let Some(p) = &l.pm else { return };
            let (px, py) = ((x * sc + v.x) as f32, (y * sc + v.y) as f32);
            let (sx, sy) = ((w * sc) as f32 / p.width() as f32, (h * sc) as f32 / p.height() as f32);
            let paint = resvg::tiny_skia::PixmapPaint { quality: resvg::tiny_skia::FilterQuality::Bilinear, ..Default::default() };
            pm.draw_pixmap(0, 0, p.as_ref(), &paint, Transform::from_row(sx, 0.0, 0.0, sy, px, py), None);
        };
        for (l, [x, y, w, h]) in s.rails((0.0 - v.y) / sc, (wh as f64 - v.y) / sc) {
            draw(&mut pm, l, (x, y, w, h));
        }
        // (in the rack every shadow under every face, as on screen)
        for d in s.devs.iter().filter(|_| s.rack) {
            if let Some((_, l)) = &d.shade {
                draw(&mut pm, l, (d.x + l.x, d.y + l.y, l.w, l.h));
            }
        }
        for d in &s.devs {
            for l in d.layers().skip(usize::from(s.rack && d.shade.is_some())) {
                draw(&mut pm, l, (d.x + l.x, d.y + l.y, l.w, l.h));
            }
        }
        // the top panel's place
        if let Some(rect) = Rect::from_xywh(0.0, 0.0, ww as f32, top as f32) {
            let mut p = Paint::default();
            p.set_color_rgba8(0x8a, 0x8d, 0x91, 255);
            pm.fill_rect(rect, &p, Transform::identity(), None);
        }
        pm.save_png(&out).unwrap();
        println!("{} devices, scale {sc:.3}, {} racks, {:.0} wide, {:.0} down", s.devs.len(), s.racks, r - l, s.rack_h);
    }

    /// How long a rack face takes to draw, and what in it costs: the whole, then without each of
    /// its textures and filters. Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn face_time() {
        let mut s = Scene::new();
        s.apply(demo_events());
        s.set_rack(true);
        s.set_rack_room(1450.0, 900.0);
        let opt = text::svg_options();
        for d in s.devs.iter().take(3) {
            let (svg, geo, skin) = face::face(&d.dev, &d.defs, &d.counts, &d.split, s.rack_w, s.accents);
            let time = |doc: &str| {
                let t = Instant::now();
                for _ in 0..5 {
                    face::raster(&opt, doc, &skin, 1.7);
                }
                t.elapsed() / 5
            };
            let without = |pat: &str| svg.replace(pat, "url(#none)");
            println!(
                "{} {:.0}x{:.0}: all {:?}; no sheen {:?}",
                d.dev.name,
                geo.w,
                geo.h,
                time(&svg),
                time(&without("url(#gSheen)"))
            );
        }
    }

    /// A face's picture leaves room round it: nothing of the face (its outline's edge reaches a
    /// pixel past it) touches the picture's edge, on the desk or in the rack.
    #[test]
    fn faces_keep_inside_their_pictures() {
        let opt = text::svg_options();
        for rack in [false, true] {
            let mut s = Scene::new();
            s.apply(demo_events());
            s.set_rack(rack);
            s.set_rack_room(1000.0, 900.0);
            for d in &s.devs {
                let (svg, _, skin) = face::face(&d.dev, &d.defs, &d.counts, &d.split, s.rack_w, s.accents);
                let pm = face::raster(&opt, &svg, &skin, 1.0).expect("drawn");
                let (w, h) = (pm.width() as usize, pm.height() as usize);
                let alpha = |x: usize, y: usize| pm.data()[(y * w + x) * 4 + 3];
                let edge = (0..w).any(|x| alpha(x, 0) > 0 || alpha(x, h - 1) > 0) || (0..h).any(|y| alpha(0, y) > 0 || alpha(w - 1, y) > 0);
                assert!(!edge, "rack {rack}, {}: drawn up to its picture's edge", d.dev.name);
            }
        }
    }

    /// The demo graph with volumes, as MIXER shows it.
    fn mixer_scene() -> Scene {
        let mut ev = demo_events();
        for id in [10u32, 11, 12, 13, 14] {
            ev.insert(ev.len() - 1, Event::Mix { id, volumes: vec![0.8, 0.8], mute: false });
        }
        // the speakers are the default output
        ev.insert(ev.len() - 1, Event::Setting { key: "default.audio.sink".into(), value: r#"{"name":"Speakers"}"#.into() });
        let mut s = Scene::new();
        s.apply(ev);
        s
    }

    /// MIXER: the groups in a console's order (inputs, apps, outputs), a module a strip;
    /// an app's PLAY ON keys are its links.
    #[test]
    fn the_mixer_is_a_console_of_strips_and_its_keys_are_the_links() {
        use super::super::mixer::{Role, Section};
        let s = mixer_scene();
        let mut m = super::super::mixer::Mixer::new(true);
        let mods = m.modules(&s);
        let names: Vec<(Section, &str)> = mods.iter().map(|m| (m.section, m.name.as_str())).collect();
        assert!(mods.windows(2).all(|w| w[0].section <= w[1].section), "{names:?}");
        assert!(names.contains(&(Section::Inputs, "MICROPHONE")) && names.contains(&(Section::Outputs, "SPEAKERS")) && names.contains(&(Section::Apps, "MUSIC PLAYER")), "{names:?}");
        // the default output among the outputs, MAIN lit on it
        assert!(mods.iter().find(|m| m.section == Section::Outputs && m.name == "SPEAKERS").is_some_and(|m| m.strips[0].main));
        assert!(mods.iter().all(|m| m.strips.len() <= 1));
        let strip = |dev: &str| mods.iter().find(|m| m.name == dev).and_then(|m| m.strips.first()).expect(dev);
        // the music player plays into the speakers: its key to them is lit; the browser's isn't
        let music = strip("MUSIC PLAYER");
        assert_eq!(music.targets.iter().map(|t| (t.label.as_str(), t.lit)).collect::<Vec<_>>(), vec![("SPEAKERS", true)]);
        assert!(strip("BROWSER").targets.iter().all(|t| !t.lit));
        // one place to play on: the one output
        assert_eq!(strip("BROWSER").targets.len(), 1);
        // a recorder's keys are the sources
        assert_eq!(strip("RECORDER").targets.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), vec!["MICROPHONE"]);
        // one app on several streams: a module each, side by side, what records first
        let meeting: Vec<Role> = mods.iter().filter(|m| m.name == "MEETING").map(|m| m.strips[0].role).collect();
        assert_eq!(meeting, vec![Role::Rec, Role::Play, Role::Play, Role::Play]);
    }

    /// A split fader moves its own channel only: the other stays where it is.
    #[test]
    fn one_channel_of_a_split_fader_leaves_the_other_where_it_is() {
        let mut s = mixer_scene();
        let at = (s.devs.iter().position(|d| d.dev.name == "Speakers").expect("speakers"), 0);
        // (the made-up graph's ports say no channel: a stereo pair here)
        s.devs[at.0].dev.groups[at.1].stereo = true;
        let lr = |s: &Scene| {
            let (v, b, _) = s.devs[at.0].mix(at.1).expect("a volume");
            [if b > 0.0 { v * (1.0 - b) } else { v }, if b < 0.0 { v * (1.0 + b) } else { v }]
        };
        s.set_channels(at, 0.8, 0.53);
        for l in [0.81, 0.97, 1.13, 0.6, 0.4, 0.77] {
            let r = lr(&s)[1];
            s.set_channels(at, l, r);
            let now = lr(&s);
            assert!((now[0] - l).abs() < 1e-9 && (now[1] - 0.53).abs() < 1e-9, "{l}: {now:?}");
        }
    }

    /// PLAY ON's links: L to L and R to R, and only there takes the old ones away, as one step.
    #[test]
    fn a_play_on_key_links_l_to_l_and_takes_the_old_links_away() {
        let mut s = mixer_scene();
        let at = |s: &Scene, name: &str| (s.devs.iter().position(|d| d.dev.name == name).expect(name), 0);
        let (browser, music, speakers) = (at(&s, "Browser"), at(&s, "Music player"), at(&s, "Speakers"));
        s.outbox.clear();
        s.reroute(&[(browser, speakers)], &[(music, speakers)]);
        let links: Vec<(u32, u32)> = s.outbox.iter().filter_map(|c| if let Cmd::Link { out_port, in_port, .. } = c { Some((*out_port, *in_port)) } else { None }).collect();
        let unlinks: Vec<u32> = s.outbox.iter().filter_map(|c| if let Cmd::Unlink(id) = c { Some(*id) } else { None }).collect();
        assert_eq!(links, vec![(1007, 1001), (1008, 1002)]);
        assert_eq!(unlinks, vec![500, 501]);
        assert!(s.undo());
    }

    /// MIXER without PipeWire: PATCHCORD_PIC=out.png [PATCHCORD_PIC_SIZE=WxH]. Run with --ignored.
    #[test]
    #[ignore]
    fn mixer_picture() {
        let Ok(out) = std::env::var("PATCHCORD_PIC") else { return };
        let (w, h) = std::env::var("PATCHCORD_PIC_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))).unwrap_or((1700, 640));
        let s = mixer_scene();
        let mut m = super::super::mixer::Mixer::new(true);
        let mut panel = super::super::panel::Panel::new(super::super::panel::State { rack: false, engine: false, mixer: true, locked: false, align: true, cables: 0, lcd: ["NO PRESET".into(), String::new()], defaults: ["Speakers".into(), "Microphone".into()], cue_led: false, cue_level: 70, hidden: 0, flow: true, pressed: None, more: false });
        let pm = crate::shot::compose_mixer(&mut m, &s, (w, h), 2.0, &mut panel);
        pm.save_png(out).expect("written");
    }
}
