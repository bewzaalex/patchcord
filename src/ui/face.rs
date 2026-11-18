//! The front panel of a device as an SVG document (ported from docs/design/mockup).
//!
//! The name and LCD block on the left, then the port groups: inputs on the left, outputs on the
//! right (in a rack the outputs sit against the signal meter, with a grille between). Each port
//! is a mult: one jack per cord plugged in, plus one free jack.

use std::fmt::Write;
use std::sync::OnceLock;

use super::print::{JACK, SECTION, SMALL, SWITCH};
use super::text;
use crate::graph::{Device, Dir, Finish, Media, VOL_MAX};

pub const DEV_H: f64 = 128.0;
/// distance between jacks
pub const JP: f64 = 48.0;
/// a rack is at least this wide
pub const RACK_MIN: f64 = 1000.0;
/// room around a face for its shadow
pub const SHADOW: f64 = 30.0;
/// width of the name and LCD block
const IDW: f64 = 190.0;
const PAD: f64 = 16.0;
const EAR: f64 = 30.0;
const TITLE: f64 = 14.0;
/// characters the LCD shows on one line
pub const LCD_CH: usize = 27;
/// where the LCD's text area is, relative to the face (without rack ears): three lines, a little
/// shorter on a card, whose PROFILE key sits under it
pub const LCD_X: f64 = PAD + 1.5;
pub const LCD_Y: f64 = 54.0;
pub const LCD_W: f64 = IDW - 3.0;
pub fn lcd_h(card: bool) -> f64 {
    if card { 46.0 } else { 57.0 }
}
/// a card's PROFILE key, under its LCD (centred, clear of the corner screws); with a PORT key
/// beside it, the two share the width
const KEY_W: f64 = 92.0;
const KEY_W2: f64 = 84.0;
const KEY_Y: f64 = 107.0;
const KEY_H: f64 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum JackKind {
    Mono,
    Stereo,
    Midi,
}

/// What you plug into: a stereo jack carries the L and R ports, a mono jack one port.
#[derive(Clone, Debug)]
pub struct JackDef {
    /// stays the same while the device and its split switches do
    pub key: String,
    pub group: usize,
    pub ports: Vec<u32>,
    pub kind: JackKind,
    pub dir: Dir,
    pub media: Media,
    pub label: String,
}

pub fn jacks(dev: &Device, split: &[bool]) -> Vec<JackDef> {
    let mut v = Vec::new();
    for (gi, g) in dev.groups.iter().enumerate() {
        // one of an app's several streams: its jacks keyed by its number (their names repeat), a
        // mono one labelled with it
        let pre = if g.stream > 0 { format!("{}:", g.stream) } else { String::new() };
        if g.stereo && !split.get(gi).copied().unwrap_or(false) {
            let names = format!("{}+{}", g.ports[0].name, g.ports[1].name);
            v.push(JackDef {
                key: format!("{}|{:?}|{pre}{names}", dev.key, g.dir),
                group: gi,
                ports: vec![g.ports[0].id, g.ports[1].id],
                kind: JackKind::Stereo,
                dir: g.dir,
                media: Media::Audio,
                label: "STEREO".into(),
            });
        } else {
            for p in &g.ports {
                v.push(JackDef {
                    key: format!("{}|{:?}|{pre}{}", dev.key, g.dir, p.name),
                    group: gi,
                    ports: vec![p.id],
                    kind: if p.media == Media::Midi { JackKind::Midi } else { JackKind::Mono },
                    dir: g.dir,
                    media: p.media,
                    label: if g.stream > 0 && g.ports.len() == 1 { g.stream.to_string() } else { p.label.clone() },
                });
            }
        }
    }
    v
}

/// Where things are on a face, relative to its top-left corner.
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub w: f64,
    /// a unit (`DEV_H`) for every row of at most `PER_ROW` frames
    pub h: f64,
    /// the first jack of each mult, by `JackDef` index
    pub jacks: Vec<(f64, f64)>,
    /// split/join switches: group and rectangle (x, y, w, h)
    pub switches: Vec<(usize, [f64; 4])>,
    /// the groups whose switch is a key beside their MUTE (an app's numbered streams)
    pub keyed: Vec<usize>,
    /// left edge of the panel proper (after a rack ear)
    pub left: f64,
    /// each group's left edge, width, the width of its jacks part (the rest is its controls), and
    /// how far down its unit starts
    pub groups: Vec<(f64, f64, f64, f64)>,
    /// left edge of the signal meter
    pub meter: f64,
    /// the SIG meter at the right edge: not when each group with controls has its own
    pub sig: bool,
    /// the free space between inputs and outputs in a rack, for the grille
    pub grille: Option<(f64, f64)>,
    /// a card's PROFILE key (x, y, w, h)
    pub profile: Option<[f64; 4]>,
    /// beside it, the PORT key of a card whose output or input has ports to choose from
    pub port: Option<[f64; 4]>,
    /// the controls of each app stream, by group
    pub mix: Vec<(usize, MixGeo)>,
}

/// An app stream's controls, right of its jacks inside its group's frame: a VOLUME knob (GAIN
/// on an input), a BAL knob for stereo, a MUTE key under them, and on a device with two streams
/// a meter of its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixGeo {
    pub x: f64,
    /// how far down its unit starts (the points below are on the face, this included)
    pub y: f64,
    /// its own meter's columns: two for stereo, one for mono
    pub cols: usize,
    /// the knobs' part (its meter, when it has one, is right of it)
    pub w: f64,
    pub input: bool,
    /// its own meter's left edge
    pub meter: Option<f64>,
    /// the knobs' centres
    pub knob: (f64, f64),
    pub bal: Option<(f64, f64)>,
    /// the MUTE key (x, y, w, h)
    pub mute: [f64; 4],
}

impl MixGeo {
    /// The same within its unit, as if it were the top one.
    fn local(&self) -> MixGeo {
        let mut m = *self;
        m.knob.1 -= self.y;
        m.bal = m.bal.map(|(x, y)| (x, y - self.y));
        m.mute[1] -= self.y;
        m.y = 0.0;
        m
    }
}

/// How far from its centre a knob takes the pointer.
pub const KNOB_R: f64 = 21.0;
pub const BAL_R: f64 = 15.0;

/// The BAL knob is a push-push knob, as on an old car stereo: pushed in, it sits flush in its
/// well and can't be turned; held down, it sinks below flush; out, it stands up and turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BalKnob {
    #[default]
    In,
    Held,
    Out,
}

/// The knob's angle for a volume: from -135° (silence) to 135° (153%).
fn vol_angle(v: f64) -> f64 {
    -135.0 + 270.0 * v.clamp(0.0, VOL_MAX) / VOL_MAX
}

/// Each kind of group has its colour, on a band behind its name and a thin line around it, as
/// the sections of a hardware effect: muted, with light lettering on the band.
fn accent(dev: &Device, gi: usize) -> &'static str {
    ACCENTS[accent_index(dev, gi)]
}

/// The kinds' colours: MIDI, MONITOR (what an output plays, to record it), PLAYBACK (a device's
/// output: speakers, headphones, HDMI), CAPTURE (a device's input: a microphone), an app's
/// OUTPUT and its INPUT.
pub const ACCENTS: [&str; 6] = ["#8f3f63", "#5d5390", "#2e8580", "#3a6596", "#b45a2e", "#a8862c"];

/// Which of `ACCENTS` a group has.
pub fn accent_index(dev: &Device, gi: usize) -> usize {
    let g = &dev.groups[gi];
    match g.label.as_str() {
        _ if g.ports.first().is_some_and(|p| p.media == Media::Midi) => 0,
        "MONITOR" => 1,
        "PLAYBACK" => 2,
        "CAPTURE" => 3,
        _ if g.dir == Dir::Out => 4,
        _ => 5,
    }
}

/// The lettering on a coloured band.
const BAND_INK: &str = "#f3efe6";

/// The width of a group's controls, its meter included.
fn mix_w(dev: &Device, gi: usize) -> f64 {
    // a BAL knob only for a stereo pair of jacks (a stream may report two volumes for one port)
    let g = &dev.groups[gi];
    match &g.mix {
        Some(_) => (if g.stereo { 96.0 } else { 58.0 }) + if own_meters(dev) { STRIP_METER } else { 0.0 },
        None => 0.0,
    }
}

/// A device with two app streams (one plays, one records) shows a meter in each of their frames
/// instead of the SIG meter at its edge.
pub fn own_meters(dev: &Device) -> bool {
    dev.groups.iter().filter(|g| g.mix.is_some()).count() > 1
}

/// Room for a group's own meter.
const STRIP_METER: f64 = 22.0;

/// Where a meter's LEDs are: the left one's x, the bottom LED's y, one LED's size, the step up
/// and the step to the right column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub step: f64,
    pub col: f64,
    pub cols: usize,
}

impl Leds {
    /// The SIG meter at a face's right edge.
    pub fn sig(x: f64) -> Leds {
        Leds { x, y: 92.0, w: 8.0, h: 5.4, step: 7.6, col: 11.0, cols: 2 }
    }

    /// A group's own meter, right of its knobs, in the unit `y` down: a column for each of its
    /// channels (one, centred, for mono). Under a coloured band it is packed a little closer, so
    /// its top LED clears the band and the SIG on it.
    pub fn strip(x: f64, y: f64, cols: usize, band: bool) -> Leds {
        Leds { x: if cols == 1 { x + 4.0 } else { x }, y: 96.0 + y, w: 6.0, h: if band { 4.2 } else { 4.6 }, step: if band { 5.9 } else { 6.6 }, col: 8.0, cols }
    }

    /// The dark LEDs, printed on the face.
    fn dark(&self) -> String {
        let mut s = String::new();
        for c in 0..self.cols {
            for i in 0..10 {
                let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="1" fill="rgba(0,0,0,.5)"/>"#, self.x + c as f64 * self.col, self.y - i as f64 * self.step, self.w, self.h);
            }
        }
        s
    }
}

struct Fin {
    base: &'static str,
    paint: bool,
    noise: f64,
    print: &'static str,
    sub: &'static str,
    /// text on a filled OUT tag
    inv: &'static str,
}

fn fin(f: Finish) -> Fin {
    match f {
        Finish::Metal => Fin { base: "#8b8d90", paint: true, noise: 0.0, print: "#1d1e20", sub: "#2c2e31", inv: "#d9dbdd" },
        Finish::Black => Fin { base: "#26272a", paint: false, noise: 0.9, print: "#d5d7d9", sub: "#9a9da1", inv: "#222" },
        Finish::Cream => Fin { base: "#d9d3be", paint: false, noise: 0.6, print: "#2a2926", sub: "#5a574f", inv: "#d9dbdd" },
        Finish::Navy => Fin { base: "#2b3950", paint: false, noise: 0.8, print: "#dde5f0", sub: "#9aa8bd", inv: "#222" },
    }
}

/// How many jacks a mult shows for `count` cords: one each, and a free one.
pub fn slots(count: usize) -> usize {
    count + 1
}

fn label_w(t: &str) -> f64 {
    SECTION.width(t)
}

/// Rows of a split stereo group: L above R, each mult running to the right.
const ROWS: [f64; 2] = [50.0, 84.0];
/// With the coloured bands the rows sit lower, so the L jack clears the band.
const ROWS_BAND: [f64; 2] = [54.0, 88.0];
/// In a split stereo group, the first jack of each row: right of its L / R label.
const ROW_X: f64 = 38.0;

/// A stereo group whose switch is on SPLIT: its L and R stand one above the other.
fn stacked(dev: &Device, gi: usize, jd: &[JackDef]) -> bool {
    dev.groups[gi].stereo && jd.iter().filter(|j| j.group == gi).count() == 2
}

/// A group's whole width: its jacks and its controls.
fn group_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    jacks_w(dev, gi, jd, counts) + mix_w(dev, gi)
}

fn jacks_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    let g = &dev.groups[gi];
    let mine = || jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| slots(counts[i]));
    let jacks = if stacked(dev, gi, jd) {
        // the longest row, and room for the switch right of its lower jacks
        ROW_X + (mine().max().unwrap_or(1) - 1) as f64 * JP + 22.0 + 46.0
    } else {
        mine().sum::<usize>() as f64 * JP + 14.0
    };
    jacks.max(if g.stereo { 100.0 } else { 0.0 }).max((label_w(&g.label) + 48.0).ceil())
}

/// At most this many frames (inputs, outputs, monitors…) side by side: the next starts a new
/// unit under them, as a 2U or 3U device.
pub const PER_ROW: usize = 3;

/// The groups in their rows: inputs, then outputs (the order `Graph::devices` keeps), at most
/// `PER_ROW` in a row.
fn rows(dev: &Device) -> Vec<Vec<usize>> {
    let ins = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::In);
    let order: Vec<usize> = ins.chain((0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::Out)).collect();
    order.chunks(PER_ROW).map(<[usize]>::to_vec).collect()
}

/// In a device of more than one unit, the width of each column of frames: its widest.
fn columns(dev: &Device, jd: &[JackDef], counts: &[usize], rows: &[Vec<usize>]) -> Vec<f64> {
    (0..PER_ROW).map(|c| rows.iter().filter_map(|r| r.get(c)).map(|gi| group_w(dev, *gi, jd, counts)).fold(0.0, f64::max)).collect()
}

/// Width the face needs; a rack is as wide as its widest device.
pub fn natural_w(dev: &Device, jd: &[JackDef], counts: &[usize], rack: bool) -> f64 {
    let rows = rows(dev);
    let groups: f64 = if rows.len() > 1 {
        columns(dev, jd, counts, &rows).iter().filter(|w| **w > 0.0).map(|w| w + 10.0).sum()
    } else {
        (0..dev.groups.len()).map(|gi| group_w(dev, gi, jd, counts) + 10.0).sum()
    };
    // the SIG meter and the room before it
    let sig = if own_meters(dev) { 0.0 } else { 40.0 };
    2.0 * if rack { EAR } else { 0.0 } + 2.0 * PAD + IDW + 14.0 + groups - 10.0 + sig
}

/// Where everything goes, without drawing it.
pub fn face_geometry(dev: &Device, jd: &[JackDef], counts: &[usize], rack_w: Option<f64>, accents: bool) -> Geometry {
    let rack = rack_w.is_some();
    let ear = if rack { EAR } else { 0.0 };
    let natural = natural_w(dev, jd, counts, rack);
    let w = rack_w.map_or(natural, |r| r.max(natural));
    let mx = w - ear - PAD - 22.0;
    let ports = dev.card.is_some() && dev.groups.iter().any(|g| g.ports_to.len() > 1);
    let (profile, port) = if ports {
        let x = ear + PAD + (IDW - 2.0 * KEY_W2 - 6.0) / 2.0;
        (Some([x, KEY_Y, KEY_W2, KEY_H]), Some([x + KEY_W2 + 6.0, KEY_Y, KEY_W2, KEY_H]))
    } else {
        (dev.card.is_some().then(|| [ear + PAD + (IDW - KEY_W) / 2.0, KEY_Y, KEY_W, KEY_H]), None)
    };
    let rows_of = rows(dev);
    let h = DEV_H * rows_of.len().max(1) as f64;
    let mut geo = Geometry { w, h, jacks: vec![(0.0, 0.0); jd.len()], switches: Vec::new(), keyed: Vec::new(), left: ear, groups: vec![(0.0, 0.0, 0.0, 0.0); dev.groups.len()], meter: mx, sig: !own_meters(dev), grille: None, profile, port, mix: Vec::new() };
    // a group at `x`, in the unit `oy` down
    let place = |gi: usize, x: f64, oy: f64, geo: &mut Geometry| -> f64 {
        let (gw, jw) = (group_w(dev, gi, jd, counts), jacks_w(dev, gi, jd, counts));
        let n: usize = jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| slots(counts[i])).sum();
        let mut jx = x + (jw - n as f64 * JP) / 2.0 + JP / 2.0;
        let rows = stacked(dev, gi, jd);
        for (r, (i, _)) in jd.iter().enumerate().filter(|(_, j)| j.group == gi).enumerate() {
            if rows {
                geo.jacks[i] = (x + ROW_X, oy + if accents { ROWS_BAND } else { ROWS }[r]);
            } else {
                geo.jacks[i] = (jx, oy + 62.0);
                jx += slots(counts[i]) as f64 * JP;
            }
        }
        // an app's numbered stream with controls: its SPLIT is a key beside its MUTE; else a little
        // switch under the jacks
        let g = &dev.groups[gi];
        let keyed = g.stereo && g.stream > 0 && g.mix.is_some();
        if g.stereo && !keyed {
            geo.switches.push((gi, [x + jw - 42.0, oy + 95.0, 36.0, 14.0]));
        }
        // a stream's controls, right of its jacks
        if g.mix.is_some() {
            let (ox, stereo) = (x + jw, g.stereo);
            let w = if stereo { 96.0 } else { 58.0 };
            let mute = if keyed {
                geo.switches.push((gi, [ox + 7.0, oy + 89.0, 38.0, 14.0]));
                geo.keyed.push(gi);
                [ox + 49.0, oy + 89.0, w - 56.0, 14.0]
            } else {
                [ox + 7.0, oy + 89.0, w - 14.0, 14.0]
            };
            geo.mix.push((gi, MixGeo {
                x: ox,
                y: oy,
                cols: if stereo { 2 } else { 1 },
                w,
                input: g.mix.as_ref().is_some_and(|m| m.gain),
                meter: own_meters(dev).then_some(ox + w + 1.0),
                knob: (if stereo { ox + 33.0 } else { ox + w / 2.0 }, oy + 60.0),
                bal: stereo.then_some((ox + 75.0, oy + 56.0)),
                mute,
            }));
        }
        geo.groups[gi] = (x, gw, jw, oy);
        x + gw + 10.0
    };
    // more than one unit: the frames in columns, a unit for each row
    if rows_of.len() > 1 {
        let cols = columns(dev, jd, counts, &rows_of);
        for (u, row) in rows_of.iter().enumerate() {
            let mut x = ear + PAD + IDW + 14.0;
            for (c, gi) in row.iter().enumerate() {
                place(*gi, x, u as f64 * DEV_H, &mut geo);
                x += cols[c] + 10.0;
            }
        }
        return geo;
    }
    // inputs on the left, outputs on the right; in a rack the outputs sit against the meter
    let ins: Vec<usize> = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::In).collect();
    let outs: Vec<usize> = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::Out).collect();
    let span: f64 = outs.iter().map(|g| group_w(dev, *g, jd, counts) + 10.0).sum::<f64>();
    let mut x = ear + PAD + IDW + 14.0;
    for g in &ins {
        x = place(*g, x, 0.0, &mut geo);
    }
    let gx0 = x;
    // (against the right edge when there is no SIG meter)
    let mut ox = if !rack { x } else if geo.sig { mx - 4.0 - span } else { w - ear - PAD + 10.0 - span };
    if rack && ox - 10.0 - gx0 > 50.0 {
        geo.grille = Some((gx0, ox - 10.0));
    }
    for g in &outs {
        ox = place(*g, ox, 0.0, &mut geo);
    }
    geo
}

/// A point `r` from (x, y) at angle `a` (degrees, 0 straight up, clockwise).
fn polar(x: f64, y: f64, a: f64, r: f64) -> String {
    let a = a.to_radians();
    format!("{:.2},{:.2}", x + r * a.sin(), y - r * a.cos())
}

/// The printed part of a stream's controls, inside its group's frame after a groove: the scales,
/// the MUTE key and its own meter's dark LEDs (the knobs and the lit LEDs are pictures of their
/// own, see `mix_pic`). On an output the knob is the VOLUME it plays at; on an input, the GAIN it
/// records with, as on a mixer's input channel.
fn mix_panel(m: &MixGeo, print: &str, band: bool) -> String {
    let oy = m.y;
    let m = &m.local();
    let name = if m.input { "GAIN" } else { "VOLUME" };
    // on a coloured band the heading is in its light lettering, and the groove starts below it
    let (ink, gy) = if band { (BAND_INK, 40.0) } else { (print, 26.0) };
    let mut s = format!(
        r#"<path d="M{x},{gy} V102" stroke="rgba(0,0,0,.45)"/><path d="M{},{gy} V102" stroke="rgba(255,255,255,.1)"/>{}"#,
        m.x + 1.0,
        SECTION.text(m.x + m.w / 2.0, 31.0, ink, "middle", &if m.bal.is_some() { format!("{name} · BAL") } else { name.to_owned() }),
        x = m.x
    );
    if let Some(mx) = m.meter {
        s += &SMALL.text(mx + 7.0, 31.0, ink, "middle", "SIG");
        s += &Leds::strip(mx, 0.0, m.cols, band).dark();
    }
    // VOLUME: silence to 153%, a long tick at 100% and red past it (louder than the app plays it:
    // it can clip)
    let (cx, cy) = m.knob;
    let _ = write!(s, r##"<path d="M{} A19.5,19.5 0 0 1 {}" fill="none" stroke="#d2402f" stroke-width="2.2" opacity=".85"/>"##, polar(cx, cy, vol_angle(1.0), 19.5), polar(cx, cy, vol_angle(VOL_MAX), 19.5));
    for i in 0..=12 {
        let a = vol_angle(i as f64 / 12.0 * VOL_MAX);
        let _ = write!(s, r#"<path d="M{} L{}" stroke="{print}" stroke-width=".9" opacity=".6"/>"#, polar(cx, cy, a, 17.5), polar(cx, cy, a, if i % 4 == 0 { 21.0 } else { 19.5 }));
    }
    let _ = write!(s, r#"<path d="M{} L{}" stroke="{print}" stroke-width="1.6"/>"#, polar(cx, cy, vol_angle(1.0), 16.5), polar(cx, cy, vol_angle(1.0), 23.5));
    // BAL: centre straight up with a long tick, L and R at the ends, and the knob's well
    if let Some((bx, by)) = m.bal {
        for i in -4i32..=4 {
            let a = i as f64 * 33.75;
            let _ = write!(s, r#"<path d="M{} L{}" stroke="{print}" stroke-width="{}" opacity="{}"/>"#, polar(bx, by, a, 11.5), polar(bx, by, a, if i == 0 { 16.0 } else { 13.5 }), if i == 0 { 1.5 } else { 0.9 }, if i == 0 { 1.0 } else { 0.6 });
        }
        let _ = write!(s, r#"<circle cx="{bx}" cy="{by}" r="10.8" fill="url(#gHole)" stroke="rgba(255,255,255,.12)" stroke-width=".7"/>{}{}"#, SMALL.text(bx - 12.0, by + 17.0, print, "middle", "L"), SMALL.text(bx + 12.0, by + 17.0, print, "middle", "R"));
    }
    // MUTE: a dark key, its LED at the left (lit in `mix_pic`); the label centred on the key, or
    // beside the LED on a narrow one
    let [kx, ky, kw, kh] = m.mute;
    let _ = write!(
        s,
        r##"<rect x="{kx}" y="{ky}" width="{kw}" height="{kh}" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/><circle cx="{}" cy="{}" r="2.6" fill="#3a1411" stroke="rgba(0,0,0,.6)" stroke-width=".6"/>{}"##,
        kx + 8.0,
        ky + kh / 2.0,
        SWITCH.text(kx + kw / 2.0 + if kw >= 60.0 { 0.0 } else { 5.0 }, ky + 9.6, "#e6e7e8", "middle", "MUTE")
    );
    if oy > 0.0 { format!(r#"<g transform="translate(0,{oy})">{s}</g>"#) } else { s }
}

/// The knobs and the MUTE LED of a VOLUME section, as an SVG document covering the section
/// (`MIX_Y` down from the face's top, `MIX_H` high): drawn again whenever they move, without the
/// face.
pub fn mix_pic(m: &MixGeo, volume: f64, balance: f64, mute: bool, bal: BalKnob) -> String {
    let m = &m.local();
    let knob = |x: f64, y: f64, r: f64, a: f64| {
        format!(
            r##"<circle cx="{x}" cy="{}" r="{}" fill="#111" opacity=".5"/><circle cx="{x}" cy="{y}" r="{r}" fill="url(#gVol)" stroke="#0c0c0d"/><circle cx="{x}" cy="{y}" r="{}" fill="none" stroke="rgba(255,255,255,.08)"/><path d="M{} L{}" stroke="#ffb02e" stroke-width="{}" stroke-linecap="round"/>"##,
            y + 1.5,
            r + 0.5,
            r * 0.75,
            polar(x, y, a, r * 0.29),
            polar(x, y, a, r * 0.86),
            if r > 12.0 { 2.4 } else { 2.0 }
        )
    };
    let (cx, cy) = m.knob;
    let mut s = knob(cx, cy, 14.0, vol_angle(volume));
    if let Some((bx, by)) = m.bal {
        let a = balance.clamp(-1.0, 1.0) * 135.0;
        let pointer = |dy: f64, color: &str, w: f64, r0: f64, r1: f64| format!(r#"<path d="M{} L{}" stroke="{color}" stroke-width="{w}" stroke-linecap="round"/>"#, polar(bx, by + dy, a, r0), polar(bx, by + dy, a, r1));
        s += &match bal {
            // held down: below flush, deeper in its well
            BalKnob::Held => format!(r##"<circle cx="{bx}" cy="{}" r="8.3" fill="#151618" stroke="#050506"/><circle cx="{bx}" cy="{}" r="8.6" fill="none" stroke="rgba(0,0,0,.7)" stroke-width="1.6"/>{}"##, by + 0.6, by - 0.4, pointer(0.6, "#6a5028", 1.7, 2.4, 6.6)),
            // out: a taller cap with a shadow under it
            BalKnob::Out => format!(r##"<circle cx="{}" cy="{}" r="10.4" fill="#000" opacity=".55"/><circle cx="{bx}" cy="{}" r="9.6" fill="#0d0e0f"/>{}"##, bx + 1.6, by + 3.2, by + 2.0, knob(bx, by, 9.6, a)),
            // in: flush, a dim pointer
            BalKnob::In => format!(r##"<circle cx="{bx}" cy="{by}" r="9" fill="#202124" stroke="#0c0c0d"/>{}"##, pointer(0.0, "#8a6a32", 1.8, 2.6, 7.4)),
        };
    }
    if mute {
        let [kx, ky, _, kh] = m.mute;
        let _ = write!(s, r##"<circle cx="{}" cy="{}" r="5" fill="#ff3b2f" opacity=".35"/><circle cx="{}" cy="{}" r="2.6" fill="#ff3b2f" stroke="rgba(0,0,0,.6)" stroke-width=".6"/>"##, kx + 8.0, ky + kh / 2.0, kx + 8.0, ky + kh / 2.0);
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{MIX_H}" viewBox="{x} {MIX_Y} {w} {MIX_H}"><defs><radialGradient id="gVol" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient></defs>{s}</svg>"##,
        x = m.x,
        w = m.w
    )
}

/// Where the VOLUME section's picture starts, down from the top of its unit, and how high it is.
pub const MIX_Y: f64 = 18.0;
pub const MIX_H: f64 = 92.0;

/// The name in capitals at one size, on one line, ending in "…" when it doesn't fit (the LCD
/// scrolls the full name).
fn fit_title(name: &str, max_w: f64) -> String {
    let mut t = name.to_uppercase();
    let w = |s: &str| text::width(text::BLACK, s, TITLE, 0.0);
    if w(&t) <= max_w {
        return t;
    }
    while !t.is_empty() && w(&format!("{t}…")) > max_w {
        t.pop();
    }
    format!("{}…", t.trim_end())
}

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The shadow a face of width `w` casts on the desk, as an SVG document the size of the face's
/// picture. It is all blur, so it is drawn small and stretched, and only again when the face's
/// size changes: blurring at full size cost more than drawing the rest of the face.
pub fn shadow(w: f64, h: f64, rack: bool) -> String {
    let (dw, dh) = (w + 2.0 * SHADOW, h + 2.0 * SHADOW);
    let r = if rack { 3.0 } else { 12.0 };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}">
<filter id="sh" filterUnits="userSpaceOnUse" x="0" y="0" width="{dw}" height="{dh}"><feGaussianBlur stdDeviation="7"/></filter>
<rect x="{SHADOW}" y="{}" width="{w}" height="{h}" rx="{r}" fill="#000" opacity=".5" filter="url(#sh)"/>
</svg>"##,
        SHADOW + 10.0
    )
}

/// Half the size of a jack's picture: the nut and its shadow fit inside.
pub const PIC: f64 = 22.0;

/// A jack (`Some`) or the plus badge at the corner of a mult's free jack (`None`), as an SVG
/// document `2 PIC` square centred on it. The jacks are laid over the faces as pictures of their
/// own, so that while a plug is in the hand the ones it can't go into can fade without the faces
/// being drawn again.
pub fn jack_pic(kind: Option<JackKind>) -> String {
    let s = 2.0 * PIC;
    let what = match kind {
        Some(JackKind::Mono) => format!(r##"<use href="#jack-mono" x="{PIC}" y="{PIC}"/>"##),
        Some(JackKind::Stereo) => format!(r##"<use href="#jack-stereo" x="{PIC}" y="{PIC}"/>"##),
        Some(JackKind::Midi) => format!(r##"<use href="#jack-midi" x="{PIC}" y="{PIC}"/>"##),
        None => format!(
            r##"<circle cx="{PIC}" cy="{}" r="6.2" fill="#000" opacity=".35"/><circle cx="{PIC}" cy="{PIC}" r="5.8" fill="#1b1c1e" stroke="#f2f2f2" stroke-opacity=".35" stroke-width=".8"/><path d="M{},{PIC} h6 M{PIC},{} v6" stroke="#f2f2f2" stroke-width="1.6" stroke-linecap="round"/>"##,
            PIC + 0.8,
            PIC - 3.0,
            PIC - 3.0
        ),
    };
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">{}{what}</svg>"##, defs())
}

/// A jack with a ring in its group's colour: mono and MIDI one ring, stereo two (as the gold
/// ring marks a stereo jack without colours).
pub fn jack_pic_ring(kind: JackKind, color: &str) -> String {
    let s = 2.0 * PIC;
    let (base, rings) = match kind {
        JackKind::Mono => ("jack-mono", format!(r#"<circle cx="{PIC}" cy="{PIC}" r="10.1" fill="none" stroke="{color}" stroke-width="1.3"/>"#)),
        JackKind::Midi => ("jack-midi", format!(r#"<circle cx="{PIC}" cy="{PIC}" r="10.1" fill="none" stroke="{color}" stroke-width="1.3"/>"#)),
        JackKind::Stereo => ("jack-mono", format!(r#"<circle cx="{PIC}" cy="{PIC}" r="10.7" fill="none" stroke="{color}" stroke-width="1"/><circle cx="{PIC}" cy="{PIC}" r="9.3" fill="none" stroke="{color}" stroke-width="1"/>"#)),
    };
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">{}<use href="#{base}" x="{PIC}" y="{PIC}"/>{rings}</svg>"##, defs())
}

/// Where the badge sits from the centre of the free jack it marks.
pub const BADGE: (f64, f64) = (11.5, -11.5);

/// The face as an SVG document, `SHADOW` larger on every side than the face itself.
/// `rack_w`: in a rack, the rack's width (the face gets ears and fills it). `accents`: each group
/// on a band of its kind's colour.
pub fn face(dev: &Device, jd: &[JackDef], counts: &[usize], split: &[bool], rack_w: Option<f64>, accents: bool) -> (String, Geometry) {
    let f = fin(dev.finish);
    let rack = rack_w.is_some();
    let ear = if rack { EAR } else { 0.0 };
    let x0 = ear + PAD;
    let geo = face_geometry(dev, jd, counts, rack_w, accents);
    let (w, mx, h) = (geo.w, geo.meter, geo.h);
    let mut body = String::new();
    // a unit more: a groove across the face parts it from the one above
    let mut uy = DEV_H;
    while uy < h - 1.0 {
        let _ = write!(body, r#"<path d="M{a},{uy} H{b}" stroke="rgba(0,0,0,.55)" stroke-width="1.2"/><path d="M{a},{} H{b}" stroke="rgba(255,255,255,.08)"/>"#, uy + 1.2, a = ear + 8.0, b = w - ear - 8.0);
        uy += DEV_H;
    }
    for (gi, g) in dev.groups.iter().enumerate() {
        let (x, gw, jw, oy) = geo.groups[gi];
        // drawn as in the top unit, then moved down to its own
        let at = body.len();
        let acc = accents.then(|| accent(dev, gi));
        let ink = if acc.is_some() { BAND_INK } else { f.print };
        let _ = write!(
            body,
            r#"<rect x="{x}" y="18" width="{gw}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="rgba(0,0,0,.3)"/><path d="M{},110.6 h{}" stroke="rgba(255,255,255,.14)"/>"#,
            x + 6.0,
            gw - 12.0,
        );
        // the band behind its name, rounded at the top like the frame, and a line of its colour
        if let Some(c) = acc {
            let _ = write!(
                body,
                r#"<path d="M{x},36 V24 a6,6 0 0 1 6,-6 H{} a6,6 0 0 1 6,6 V36 Z" fill="{c}"/><path d="M{x},36.5 H{}" stroke="rgba(0,0,0,.35)"/><rect x="{}" y="18.6" width="{}" height="90.8" rx="5.6" fill="none" stroke="{c}" stroke-width="1.2" opacity=".9"/>"#,
                x + gw - 6.0,
                x + gw,
                x + 0.6,
                gw - 1.2
            );
        }
        body.push_str(&SECTION.text(x + 8.0, 31.0, ink, "start", &g.label));
        let tx = x + jw - 30.0;
        if g.dir == Dir::In {
            let _ = write!(
                body,
                r#"<rect x="{tx}" y="22.5" width="23" height="11" rx="2" fill="none" stroke="{ink}" stroke-width=".9"/>{}"#,
                SMALL.text(tx + 11.5, 31.0, ink, "middle", "IN")
            );
        } else {
            let _ = write!(
                body,
                r#"<rect x="{tx}" y="22.5" width="23" height="11" rx="2" fill="{ink}"/>{}"#,
                SMALL.text(tx + 11.5, 31.0, acc.unwrap_or(f.inv), "middle", "OUT")
            );
        }
        let rows = stacked(dev, gi, jd);
        for (i, j) in jd.iter().enumerate().filter(|(_, j)| j.group == gi) {
            let k = slots(counts[i]);
            let (jx, jy) = (geo.jacks[i].0, geo.jacks[i].1 - oy);
            // parallel jacks of one port get a printed outline, like a mult on a patch bay (lower
            // in a row of a split stereo group, which has less room)
            let h = if rows { 32.0 } else { 42.0 };
            if k > 1 {
                let _ = write!(body, r#"<rect x="{}" y="{}" width="{}" height="{h}" rx="{}" fill="rgba(0,0,0,.08)" stroke="{}" stroke-width=".9" opacity=".5"/>"#, jx - h / 2.0, jy - h / 2.0, (k - 1) as f64 * JP + h, h / 2.0, f.print);
            }
            // the jacks themselves are pictures of their own, laid over the face (see `jack_pic`);
            // in a split stereo group L and R are printed left of their rows
            if rows {
                body.push_str(&JACK.text(jx - 21.0, jy + 3.2, f.print, "end", &j.label));
            } else {
                body.push_str(&JACK.text(jx + (k - 1) as f64 * JP / 2.0, 91.0, f.print, "middle", &j.label));
            }
        }
        if oy > 0.0 {
            body.insert_str(at, &format!(r#"<g transform="translate(0,{oy})">"#));
            body.push_str("</g>");
        }
    }
    for (gi, [bx, by, bw, bh]) in &geo.switches {
        let on = split.get(*gi).copied().unwrap_or(false);
        // a numbered stream's: a key like its MUTE, beside it
        if geo.keyed.contains(gi) {
            let _ = write!(
                body,
                r##"<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>{}"##,
                SWITCH.text(bx + bw / 2.0, by + 9.6, "#e6e7e8", "middle", if on { "JOIN" } else { "SPLIT" })
            );
            continue;
        }
        // a little hardware switch: one stereo jack, or L and R apart
        let by = by + 2.0;
        let _ = write!(
            body,
            r##"<rect x="{bx}" y="{by}" width="34" height="10" rx="2" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>{}"##,
            SWITCH.text(bx + 17.0, by + 7.6, "#e6e7e8", "middle", if on { "JOIN" } else { "SPLIT" })
        );
    }
    for (_, m) in &geo.mix {
        body.push_str(&mix_panel(m, f.print, accents));
    }
    let mut grille = String::new();
    if let Some((gx0, gx1)) = geo.grille {
        let mut y = 30.0;
        while y <= 98.0 {
            let _ = write!(grille, r##"<rect x="{gx0}" y="{y}" width="{}" height="4.2" rx="2.1" fill="#0b0c0d"/><path d="M{},{} h{}" stroke="rgba(255,255,255,.12)"/>"##, gx1 - gx0, gx0 + 2.0, y + 5.1, gx1 - gx0 - 4.0);
            y += 9.5;
        }
    }
    let leds = if geo.sig { Leds::sig(mx).dark() + &SMALL.text(mx + 9.5, 112.0, f.print, "middle", "SIG") } else { String::new() };

    let r = if rack { 3.0 } else { 12.0 };
    let title = format!(r#"<text x="{x0}" y="33" font-size="{TITLE}">{}</text>"#, esc(&fit_title(&dev.name, IDW - 4.0)));
    // a card's PROFILE and PORT keys: dark keys with a small arrow, like the SPLIT switches
    let key_pic = |[kx, ky, kw, kh]: [f64; 4], name: &str| {
        let tw = SWITCH.width(name);
        let tx = kx + kw / 2.0 - 3.0;
        format!(
            r##"<rect x="{kx}" y="{ky}" width="{kw}" height="{kh}" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>{}<path d="M{},{} l3.2,0 l-1.6,2.2 z" fill="#e6e7e8"/>"##,
            SWITCH.text(tx, ky + 9.8, "#e6e7e8", "middle", name),
            tx + tw / 2.0 + 3.0,
            ky + 6.0
        )
    };
    let key = geo.profile.map_or(String::new(), |r| key_pic(r, "PROFILE")) + &geo.port.map_or(String::new(), |r| key_pic(r, "PORT"));
    let screws: String = if rack {
        String::new()
    } else {
        [(11.0, 11.0), (w - 11.0, 11.0), (11.0, h - 11.0), (w - 11.0, h - 11.0)]
            .iter()
            .enumerate()
            .map(|(i, (sx, sy))| {
                format!(
                    r##"<use href="#screw" transform="translate({sx},{sy}) rotate({})"/>"##,
                    (i * 37 + dev.name.len() * 11) % 90
                )
            })
            .collect()
    };
    // paint: solid base, then the texture on top
    let skin = |sx: f64, sw: f64, rr: f64| {
        let mut s = format!(r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="{}"/>"#, f.base);
        if f.paint {
            let _ = write!(s, r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="url(#pt)"/>"#);
        }
        if f.noise > 0.0 {
            let _ = write!(s, r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="url(#nz)" opacity="{}"/>"#, f.noise);
        }
        let _ = write!(s, r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="url(#gSheen)"/>"#);
        s
    };
    let ears = if rack {
        [0.0, w - EAR - 6.0]
            .iter()
            .map(|&ex| {
                let cx = if ex == 0.0 { EAR / 2.0 } else { w - EAR / 2.0 };
                let mut s = skin(ex, EAR + 6.0, 3.0);
                let _ = write!(s, r#"<rect x="{ex}" y="0" width="{}" height="{h}" rx="3" fill="none" stroke="rgba(0,0,0,.6)"/>"#, EAR + 6.0);
                for sy in [24.0, h - 24.0] {
                    let _ = write!(s, r##"<rect x="{}" y="{}" width="11" height="18" rx="5.5" fill="#0a0a0b"/><use href="#screw" transform="translate({cx},{sy}) rotate({sy})"/>"##, cx - 5.5, sy - 9.0);
                }
                s
            })
            .collect::<String>()
    } else {
        String::new()
    };

    let (dw, dh) = (w + 2.0 * SHADOW, h + 2.0 * SHADOW);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}" font-family="Noto Sans" font-weight="bold">
{defs}
<g transform="translate({SHADOW},{SHADOW})">
<g>
{ears}
{skin}
<rect x="{a}" y=".75" width="{b}" height="{c}" rx="{r}" fill="none" stroke="rgba(255,255,255,.2)"/>
<rect x="{ear}" y="0" width="{d}" height="{h}" rx="{r}" fill="none" stroke="rgba(0,0,0,.65)"/>
</g>
{screws}
<g font-weight="900" fill="{print}">{title}</g>
{model}
<circle cx="{led}" cy="43" r="5" fill="{glow}" opacity=".35"/><circle cx="{led}" cy="43" r="3.2" fill="{lamp}" stroke="rgba(0,0,0,.6)" stroke-width=".8"/>
<rect x="{lx}" y="{fy}" width="{lw}" height="{fh}" rx="4" fill="#111214"/>
<rect x="{ix}" y="{LCD_Y}" width="{iw}" height="{lh}" rx="2" fill="url(#gLcd)"/>
<rect x="{ix}" y="{LCD_Y}" width="{iw}" height="{lh}" rx="2" fill="none" stroke="rgba(110,45,0,.35)" stroke-width="3"/>
{key}
{body}{grille}{leds}
</g>
</svg>"##,
        defs = defs(),
        skin = skin(ear, w - 2.0 * ear, r),
        a = ear + 0.75,
        b = w - 2.0 * ear - 1.5,
        c = h - 1.5,
        d = w - 2.0 * ear,
        print = f.print,
        model = SMALL.text(x0, 46.0, f.sub, "start", &dev.model),
        lh = lcd_h(dev.card.is_some()),
        fy = LCD_Y - 3.0,
        fh = lcd_h(dev.card.is_some()) + 6.0,
        led = x0 + IDW - 5.0,
        lx = x0 - 1.5,
        lw = IDW + 3.0,
        ix = x0 + 1.5,
        iw = IDW - 3.0,
        // the status lamp: green while running, amber when idle, dark when suspended
        lamp = match dev.state {
            "running" => "#4cff6a",
            "idle" => "#c48a1c",
            _ => "#2a1d0a",
        },
        glow = if dev.state == "running" { "#4cff6a" } else { "none" },
    );
    (svg, geo)
}

/// The LCD's two lines of text, as a document the size of the LCD's text area.
pub fn lcd(lines: [&str; 3], card: bool) -> String {
    let h = lcd_h(card);
    let (y1, step) = if card { (13.0, 13.5) } else { (16.0, 16.0) };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{LCD_W}" height="{h}" viewBox="0 0 {LCD_W} {h}" font-family="Noto Sans Mono" fill="#3a2408" xml:space="preserve"><text x="7.5" y="{y1}" font-size="10.5" font-weight="bold">{}</text><text x="7.5" y="{}" font-size="10.5">{}</text><text x="7.5" y="{}" font-size="10.5">{}</text></svg>"##,
        esc(lines[0]),
        y1 + step,
        esc(lines[1]),
        y1 + 2.0 * step,
        esc(lines[2])
    )
}

/// The LCD's top line: the full name, scrolling when it is too long, pausing at the start.
pub fn marquee(name: &str, ms: u128) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= LCD_CH {
        return name.to_owned();
    }
    let looped: Vec<char> = format!("{name}   ·  {name}").chars().collect();
    let period = chars.len() + 6 + 8;
    let pos = ((ms / 280) as usize % period).saturating_sub(8);
    looped[pos..(pos + LCD_CH).min(looped.len())].iter().collect()
}

/// The painted-metal texture, as a data URI for SVG documents.
pub fn paint_uri() -> &'static str {
    static URI: OnceLock<String> = OnceLock::new();
    URI.get_or_init(|| format!("data:image/png;base64,{}", base64(include_bytes!("../../assets/paint.png"))))
}

fn defs() -> &'static str {
    static DEFS: OnceLock<String> = OnceLock::new();
    DEFS.get_or_init(|| {
        let paint = paint_uri();
        let noise = format!("data:image/png;base64,{}", base64(include_bytes!("../../assets/noise.png")));
        let hex: String = (0..6)
            .map(|k| {
                let a = (k as f64 * 60.0 + 30.0).to_radians();
                format!("{:.2},{:.2} ", 15.0 * a.cos(), 15.0 * a.sin())
            })
            .collect();
        format!(
            r##"<defs>
<pattern id="pt" width="80" height="80" patternUnits="userSpaceOnUse"><image href="{paint}" width="80" height="80"/></pattern>
<pattern id="nz" width="128" height="128" patternUnits="userSpaceOnUse"><image href="{noise}" width="128" height="128"/></pattern>
<linearGradient id="gSheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".24"/></linearGradient>
<linearGradient id="gNut" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f7f8"/><stop offset=".45" stop-color="#b6b9bd"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gWasher" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#5e6166"/><stop offset="1" stop-color="#141517"/></radialGradient>
<radialGradient id="gWasherMidi" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#c8483a"/><stop offset="1" stop-color="#4a120d"/></radialGradient>
<radialGradient id="gChrome" cx=".35" cy=".3" r=".85"><stop offset="0" stop-color="#fff"/><stop offset=".4" stop-color="#b9bcc1"/><stop offset=".75" stop-color="#65686d"/><stop offset="1" stop-color="#a6a9ad"/></radialGradient>
<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>
<linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e38c2a"/></linearGradient>
<filter id="nutShadow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="1.2"/></filter>
<polygon id="hexShape" points="{hex}"/>
<g id="hex"><use href="#hexShape" fill="#000" opacity=".4" transform="translate(1.2,2.4)" filter="url(#nutShadow)"/><use href="#hexShape" fill="url(#gNut)" stroke="#43454a" stroke-width=".8"/></g>
<g id="jack-mono"><use href="#hex"/><circle r="11.4" fill="url(#gWasher)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
<g id="jack-stereo"><use href="#jack-mono"/><circle r="10.1" fill="none" stroke="#c9a24a" stroke-width="1.3"/></g>
<g id="jack-midi"><use href="#hex"/><circle r="11.4" fill="url(#gWasherMidi)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
<g id="screw"><circle r="5.2" fill="url(#gChrome)" stroke="#35363a" stroke-width=".7"/><path d="M-3.2,0 H3.2 M0,-3.2 V3.2" stroke="#36373b" stroke-width="1.4" stroke-linecap="round"/></g>
</defs>"##
        )
    })
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}
