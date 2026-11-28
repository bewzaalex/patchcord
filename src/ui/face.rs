//! The front panel of a device as an SVG document (ported from docs/design/mockup).
//!
//! The name and LCD block on the left, then the port groups: inputs on the left, outputs on the
//! right (in a rack the outputs sit against the signal meter, with a grille between). Each port
//! is a mult: one jack per cord plugged in, plus one free jack.

use std::fmt::Write;
use std::sync::OnceLock;

use resvg::tiny_skia::{FillRule, FilterQuality, Paint, Path, PathBuilder, Pattern, Pixmap, SpreadMode, Transform};
use resvg::usvg;

use super::print::{SECTION, SMALL, SWITCH};
use super::text::{self, esc};
use crate::graph::{Device, Dir, Finish, Media, VOL_MAX};

/// A face's height (a unit's): its frames with a little room above and below them (2026-11-26: it
/// was 128, room for a four-line screen and its keys under the frames' bottom)
pub const DEV_H: f64 = 108.0;
/// How much higher than it used to a unit's content sits: its frames are drawn in their own
/// numbers (18 to 110 down a unit), the unit's top `LIFT` above where those start
pub(crate) const LIFT: f64 = 10.0;
/// distance between jacks
pub const JP: f64 = 48.0;
/// a rack is at least this wide
pub const RACK_MIN: f64 = 1000.0;
/// room around a face for its shadow
pub const SHADOW: f64 = 30.0;
/// The room round a face in its picture: its outline's edge reaches a pixel past it, no more
/// (its shadow is a picture of its own, `SHADOW` round).
pub const FACE_PAD: f64 = 2.0;
/// width of the name and LCD block
const IDW: f64 = 190.0;
const PAD: f64 = 16.0;
/// A unit of the rack: a 1U face (`DEV_H`, 1.719" in a real rack) and the gap to the next one,
/// as in a real rack (1.75"); the rails' holes and the screws are in proportion to it.
pub const RACK_U: f64 = DEV_H * 1.75 / 1.719;
/// the gap between two faces in the rack (0.031")
pub const RACK_GAP: f64 = RACK_U - DEV_H;
/// The rack's screws: their head, the room round them (to the face's edge and to its LCD alike),
/// and where they sit from the face's side edge.
pub const RACK_SCREW_R: f64 = 7.2;
const RACK_SCREW_PAD: f64 = 10.25;
pub const RACK_SCREW_X: f64 = RACK_SCREW_PAD + RACK_SCREW_R;
/// How far down from a face's top, and up from its bottom, its screws are: in the rack's
/// holes, the top one of its first unit and the bottom one of its last (0.25" into a unit).
pub const RACK_SCREW_Y: f64 = RACK_U * 0.25 / 1.75 - RACK_GAP / 2.0;
/// In the rack a face runs over the rails, with no ears of its own: what is on it starts this far
/// in, the screws beside its LCD.
const EAR: f64 = RACK_SCREW_X + RACK_SCREW_R + RACK_SCREW_PAD + 1.5 - PAD;
const TITLE: f64 = 14.0;
/// the name's baseline
const TITLE_Y: f64 = 33.0 - LIFT;
/// characters the LCD shows on one line
pub const LCD_CH: usize = 27;
/// where the LCD's text area is, relative to the face (without rack ears): right under the name,
/// down to the row of keys; the same on every face
pub const LCD_X: f64 = PAD + 1.5;
pub const LCD_Y: f64 = 33.0;
pub const LCD_W: f64 = IDW - 3.0;
/// three lines (it had four: its cords now share the third)
pub const LCD_H: f64 = 45.0;
/// the row of keys under the LCD, on every face: HIDE at its left, then a card's PROFILE and PORT
/// sharing the rest evenly (the row a little in from the LCD's edges, clear of the desk's screws);
/// its bottom level with the frames' bottom edge
const KEY_Y: f64 = 110.0 - LIFT - KEY_H;
const KEY_H: f64 = 14.0;
const KEY_IN: f64 = 4.0;
const HIDE_W: f64 = 22.0;
const KEY_GAP: f64 = 6.0;

/// What you plug into: a stereo jack carries the L and R ports, a mono jack one port.
#[derive(Clone, Debug)]
pub struct JackDef {
    /// stays the same while the device and its split switches do
    pub key: String,
    pub group: usize,
    pub ports: Vec<u32>,
    pub dir: Dir,
    pub media: Media,
    pub label: String,
}

pub fn jacks(dev: &Device, split: &[bool]) -> Vec<JackDef> {
    let mut v = Vec::new();
    // another group one way with a port of the same name (every effect of Easy Effects has its
    // input_FL): jacks keyed by their node too
    let repeats = |gi: usize| {
        let g = &dev.groups[gi];
        dev.groups.iter().enumerate().any(|(o, h)| o != gi && h.dir == g.dir && h.ports.iter().any(|p| g.ports.iter().any(|q| q.name == p.name)))
    };
    for (gi, g) in dev.groups.iter().enumerate() {
        // one of an app's several streams: its jacks keyed by its number (their names repeat); the
        // number is in the frame's name (OUTPUT 2), the jack says what it carries (MONO)
        let pre = if g.stream > 0 {
            format!("{}:", g.stream)
        } else if repeats(gi) {
            format!("n{}:", g.ports.first().map_or(0, |p| p.node))
        } else {
            String::new()
        };
        if g.stereo && !split.get(gi).copied().unwrap_or(false) {
            let names = format!("{}+{}", g.ports[0].name, g.ports[1].name);
            v.push(JackDef {
                key: format!("{}|{:?}|{pre}{names}", dev.key, g.dir),
                group: gi,
                ports: vec![g.ports[0].id, g.ports[1].id],
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
                    dir: g.dir,
                    media: p.media,
                    label: p.label.clone(),
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
    /// left edge of the panel proper (after a rack ear)
    pub left: f64,
    /// each group's left edge, width, the width of its jacks part (the rest is its controls), and
    /// how far down its unit starts
    pub groups: Vec<(f64, f64, f64, f64)>,
    /// the free space between inputs and outputs in a rack, for the grille
    pub grille: Option<(f64, f64)>,
    /// vents on a multi-unit face, where it has nothing to show: (x0, x1, how far down its unit)
    pub vents: Vec<[f64; 3]>,
    /// how wide the name may be: the screen's width, or in the rack, over a first unit with no
    /// frames, on to the face's right edge (docs/design/rack-top, variants C and D)
    pub title_w: f64,
    /// the name where it runs on past the screen's width (x, y, w, h): no vents under it
    pub title: Option<[f64; 4]>,
    /// a card's PROFILE key (x, y, w, h)
    pub profile: Option<[f64; 4]>,
    /// beside it, the PORT key of a card whose output or input has ports to choose from
    pub port: Option<[f64; 4]>,
    /// the HIDE key, at the left of the row under the LCD (x, y, w, h; no width while the device
    /// has cords: it isn't there)
    pub hide: [f64; 4],
    /// the controls of each app stream, by group
    pub mix: Vec<(usize, MixGeo)>,
    /// each audio frame's meter: its group, its left edge, how far down its unit starts, and
    /// its columns (two for stereo)
    pub meters: Vec<(usize, f64, f64, usize)>,
    /// each frame's name on its band, a click on which opens its block in ENGINE: its node, and
    /// (x, y, w, h)
    pub graph: Vec<(u32, [f64; 4])>,
}

/// An app stream's controls, right of its jacks inside its group's frame: a VOLUME knob (GAIN
/// on an input), a BAL knob for stereo, a MUTE key under them (its meter is in `meters`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixGeo {
    pub x: f64,
    /// how far down its unit starts (the points below are on the face, this included)
    pub y: f64,
    /// the knobs' part (its meter, when it has one, is right of it)
    pub w: f64,
    pub input: bool,
    /// the knobs' centres
    pub knob: (f64, f64),
    pub bal: Option<(f64, f64)>,
    /// the MUTE key (x, y, w, h)
    pub mute: [f64; 4],
    /// the frame's SPLIT, left of MUTE (x, y, w, h), when it is in the row (`split_below`)
    pub split: Option<[f64; 4]>,
    /// a surround node's MASTER (ADR 0025): no jacks before it, so no rule; and no VOLUME heading,
    /// its name says what it is (the name runs on over the knob)
    pub bare: bool,
}

/// A key in a frame's row (MUTE, SPLIT): which one is held down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowKey {
    Mute,
    Split,
}

impl MixGeo {
    /// The same within its unit, as if it were the top one.
    fn local(&self) -> MixGeo {
        let mut m = *self;
        m.knob.1 -= self.y;
        m.bal = m.bal.map(|(x, y)| (x, y - self.y));
        m.mute[1] -= self.y;
        m.split = m.split.map(|[x, y, w, h]| [x, y - self.y, w, h]);
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

/// The knob's angle for a volume: from -135° (silence) to 135° (150%).
pub(crate) fn vol_angle(v: f64) -> f64 {
    -135.0 + 270.0 * v.clamp(0.0, VOL_MAX) / VOL_MAX
}

/// The groups' colours (ADR 0014: a port's, its jack's ring here and its dot in ENGINE): MIDI,
/// MONITOR (what an output plays, to record it), PLAYBACK (a sink's inputs: speakers, headphones,
/// HDMI), CAPTURE (a source's outputs: a microphone), anything else's OUTPUT and INPUT, VIDEO.
/// (A frame's own colour is its block's kind: `kind_colour`.)
/// One lightness and one chroma (OKLCH .52 / .115), the hues spread round the circle (340, 290,
/// 195, 245, 140, 85°): none shouts over another, each is told apart. OUTPUT is an orange, as a
/// synth's painted sections (`#bb5a2a`, 2026-11-27, dark enough for its light lettering, 4:1; a warm brown `#7e5a42` before, chosen so rust
/// wouldn't read as ENGINE's error red: docs/design/apps/palette.py, B).
/// The eighth, OTHER (a port of a format Patchcord doesn't know, or of none), is NO CLASS's sand:
/// nothing is said of it.
pub const ACCENTS: [&str; 8] = ["#934c7e", "#695ca6", "#007c7d", "#206ea6", "#bb5a2a", "#427935", "#876200", "#cba553"];

/// A group's colour by its role (`graph::port_role`): its jacks' rings here, its ports in ENGINE.
pub fn role_colour(role: &str) -> &'static str {
    match role {
        "MIDI" => ACCENTS[0],
        "MONITOR" => ACCENTS[1],
        "PLAYBACK" => ACCENTS[2],
        "CAPTURE" => ACCENTS[3],
        "OUTPUT" => ACCENTS[4],
        "VIDEO" => ACCENTS[6],
        "OTHER" => ACCENTS[7],
        _ => ACCENTS[5],
    }
}

/// Which of `ACCENTS` a group has: its jacks' rings (what its ports are for).
pub fn accent_index(dev: &Device, gi: usize) -> usize {
    let g = &dev.groups[gi];
    match g.role {
        _ if g.ports.first().is_some_and(|p| p.media == Media::Midi) => 0,
        "MONITOR" => 1,
        "PLAYBACK" => 2,
        "CAPTURE" => 3,
        "VIDEO" => 6,
        "OTHER" => 7,
        _ if g.dir == Dir::Out => 4,
        _ => 5,
    }
}

/// The lettering on a coloured band.
const BAND_INK: &str = "#f3efe6";

/// A frame's printed rules (before its knobs, its MON, a section): from and to, as far below its
/// band (which ends at 36) as above its bottom (110), so they sit in the middle.
const RULE: (f64, f64) = (42.0, 104.0);

/// A frame's colour: its block's kind (`graph::block_kind`, ADR 0014), the same as the block's in
/// ENGINE. A kind with a main group of ports has that group's colour (a sink's PLAYBACK, a
/// source's CAPTURE…); DUPLEX and NO CLASS have their own (docs/design/apps/kinds.py, variant 4).
pub fn kind_colour(kind: &str) -> &'static str {
    match kind {
        "SINK" => "#007c7d",
        "SOURCE" => "#206ea6",
        "PLAYBACK STREAM" => "#bb5a2a",
        "RECORDING STREAM" => "#427935",
        "MIDI" => "#934c7e",
        "VIDEO" => "#876200",
        "DUPLEX" => "#73348d",
        _ => "#cba553",
    }
}

/// The lettering on a frame's band: light, but dark on NO CLASS's light sand.
pub fn band_ink(kind: &str) -> &'static str {
    if kind_colour(kind) == "#cba553" { "#1d1e20" } else { BAND_INK }
}

/// The width of a group's controls, its meter included.
fn mix_w(dev: &Device, gi: usize) -> f64 {
    // a BAL knob only for a stereo pair of jacks (a stream may report two volumes for one port)
    let g = &dev.groups[gi];
    let knobs = match &g.mix {
        Some(_) => if g.stereo { 96.0 } else { 58.0 },
        None => 0.0,
    };
    knobs + if metered(g) && host(dev, gi).is_none() { STRIP_METER } else { 0.0 }
}

/// Every audio frame has a meter of its own, inside it, at its right (MIDI none): a device
/// without frames (a card that is off) has none.
pub fn metered(g: &crate::graph::Group) -> bool {
    g.meter
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
    /// A group's own meter, right of its knobs, in the unit `y` down: a column for each of its
    /// channels (one, centred, for mono), thin LEDs as MIXER's (4 wide, 2 between). Under a
    /// coloured band it is packed a little closer, so its top LED clears the band and the SIG on it.
    pub fn strip(x: f64, y: f64, cols: usize, band: bool) -> Leds {
        Leds { x: if cols == 1 { x + 5.0 } else { x + 2.0 }, y: 96.0 + y, w: 4.0, h: if band { 4.2 } else { 4.6 }, step: if band { 5.9 } else { 6.6 }, col: 6.0, cols }
    }

    /// The colour a LED lights in: green, amber near the top, red at it (as MIXER's).
    pub fn colour(i: usize) -> [u8; 3] {
        if i >= 9 {
            [0xff, 0x3b, 0x2f]
        } else if i >= 7 {
            [0xff, 0xb0, 0x2e]
        } else {
            [0x4c, 0xff, 0x6a]
        }
    }

    /// The meter as printed on the face, as MIXER's: a black slot, and in it the LEDs off, each
    /// a dark of the colour it lights in.
    fn dark(&self) -> String {
        let top = self.y - 9.0 * self.step;
        let mut s = format!(
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="#0b0b0c"/>"##,
            self.x - 3.0,
            top - 3.0,
            (self.cols - 1) as f64 * self.col + self.w + 6.0,
            9.0 * self.step + self.h + 6.0
        );
        for c in 0..self.cols {
            for i in 0..10 {
                let dark = if i >= 9 { "#351612" } else if i >= 7 { "#2f2810" } else { "#1c2a1f" };
                let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="1" fill="{dark}"/>"#, self.x + c as f64 * self.col, self.y - i as f64 * self.step, self.w, self.h);
            }
        }
        s
    }
}

struct Fin {
    base: &'static str,
    noise: f64,
    print: &'static str,
}

fn fin(f: Finish) -> Fin {
    match f {
        // ADR 0014 (docs/design/apps/paints.py, variant D): one paint a kind of device, light,
        // dark and in between, so none passes for another
        // (hardware in the top panel's own painted metal, 2026-11-23; navy since 2026-11-26, the
        // first version's paint, as it was: the light grey stood out of the rack)
        Finish::Aluminium => Fin { base: "#2b3950", noise: 0.8, print: "#dde5f0" },
        Finish::Black => Fin { base: "#232427", noise: 0.9, print: "#d5d7d9" },
        // (a dark olive since 2026-11-27: the olive stood out beside the navy and the black, and a
        // graphite green after it passed for the black)
        Finish::Olive => Fin { base: "#45512f", noise: 0.8, print: "#ebedd9" },
    }
}

/// A device's paint as MIXER's module wears it: what goes under its drawing (`raster`), and the
/// colour printed on it.
pub fn paint(f: Finish, w: f64, h: f64, r: f64) -> (Skin, &'static str) {
    let f = fin(f);
    (Skin { w, h, r, base: hex(f.base), noise: f.noise as f32 }, f.print)
}

/// A vent's hole: its hexagon's radius.
const VENT_R: f64 = 5.2;

/// The free spans worth vents: wide ones, and a narrow one (a frame's end to the face's) that
/// runs on into a wide one in the unit over or under it, so the field goes on there; a narrow
/// one alone stays plain metal. `pitch`: a unit's.
fn keep_vents(vents: &mut Vec<[f64; 3]>, pitch: f64) {
    let wide: Vec<[f64; 3]> = vents.iter().filter(|v| v[1] - v[0] > 50.0).copied().collect();
    vents.retain(|v| v[1] - v[0] > 50.0 || wide.iter().any(|w| ((w[2] - v[2]).abs() - pitch).abs() < 1.0 && v[1].min(w[1]) - v[0].max(w[0]) > 2.0 * VENT_R));
}

/// A face's vents as a fine honeycomb (docs/design/vents, variant 1): small hexagons on the face's
/// own grid, not each unit's, so where the free space runs on from one unit into the next the
/// holes run on with it and a multi-unit device reads as one. In each unit the field is as high
/// as the frames, its first and last rows touching their top and bottom; between two units' fields
/// two rows more. Only whole holes: a hole an edge would cut is left out. `pitch`: a unit's.
fn honeycomb(geo: &Geometry, pitch: f64) -> String {
    const R: f64 = VENT_R;
    // a frame's top and bottom in its unit's own numbers, and how many rows apart they are
    const TOP: f64 = 18.0;
    const BOTTOM: f64 = 110.0;
    const ROWS: usize = 8;
    const BETWEEN: usize = 3;
    let spans: Vec<[f64; 3]> = geo.grille.map(|(a, b)| [a, b, -LIFT]).into_iter().chain(geo.vents.iter().copied()).collect();
    if spans.is_empty() {
        return String::new();
    }
    // each span as high as a frame, and where two spans one above the other overlap, the room
    // between them too
    let mut rects: Vec<[f64; 4]> = spans.iter().map(|v| [v[0], v[2] + TOP, v[1], v[2] + BOTTOM]).collect();
    for v in &spans {
        for w in spans.iter().filter(|w| (w[2] - v[2] - pitch).abs() < 1.0) {
            let (x0, x1) = (v[0].max(w[0]), v[1].min(w[1]));
            if x1 - x0 > 2.0 * R {
                rects.push([x0, v[2] + BOTTOM - 1.0, x1, w[2] + TOP + 1.0]);
            }
        }
    }
    // the rows, from the first unit's down to the last's
    let first = spans.iter().map(|v| v[2]).fold(f64::MAX, f64::min);
    let units = spans.iter().map(|v| ((v[2] - first) / pitch).round() as usize).max().unwrap_or(0) + 1;
    let step = (BOTTOM - TOP - 2.0 * R) / ROWS as f64;
    let gap = (pitch - (BOTTOM - TOP) + 2.0 * R) / BETWEEN as f64;
    let mut rows = Vec::new();
    for u in 0..units {
        let y = first + u as f64 * pitch + TOP + R;
        rows.extend((0..=ROWS).map(|j| y + j as f64 * step));
        if u + 1 < units {
            let last = y + ROWS as f64 * step;
            rows.extend((1..BETWEEN).map(|j| last + j as f64 * gap));
        }
    }
    let on = |x: f64, y: f64| rects.iter().any(|r| r[0] <= x && x <= r[2] && r[1] - 0.01 <= y && y <= r[3] + 0.01);
    // (a name run on over the first unit: no hole under it, nor within a frame's gap of it)
    let under_title = |x: f64, y: f64| geo.title.is_some_and(|[tx, ty, tw, th]| x + R > tx - 10.0 && x - R < tx + tw + 10.0 && y + R > ty - 6.0 && y - R < ty + th + 6.0);
    let dx = R * 3f64.sqrt() + 2.4;
    let (x_min, x_max) = rects.iter().fold((f64::MAX, f64::MIN), |(a, b), r| (a.min(r[0]), b.max(r[2])));
    // the hexagon's corners round its middle (a point up)
    let corners: Vec<(f64, f64)> = (0..6).map(|k| (30.0 + 60.0 * k as f64).to_radians()).map(|a| (R * a.cos(), R * a.sin())).collect();
    let mut d = String::new();
    for (j, &y) in rows.iter().enumerate() {
        let off = if j % 2 != 0 { dx / 2.0 } else { 0.0 };
        let mut i = ((x_min - off) / dx).floor() as i64;
        while i as f64 * dx + off <= x_max {
            let x = i as f64 * dx + off;
            let fits = on(x, y) && [(-R, -R), (R, -R), (-R, R), (R, R)].iter().all(|(a, b)| on(x + a, y + b)) && !under_title(x, y);
            if fits {
                for (k, (a, b)) in corners.iter().enumerate() {
                    let _ = write!(d, "{}{:.1},{:.1}", if k == 0 { "M" } else { "L" }, x + a, y + b);
                }
                d.push('Z');
            }
            i += 1;
        }
    }
    // each hole's light edge (the light from the top left catches its far rim), then the holes
    // over it so only the rim shows
    format!(r##"<path d="{d}" fill="none" stroke="rgba(255,255,255,.12)" transform="translate(.4,.9)"/><path d="{d}" fill="#0b0c0d"/>"##)
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
/// In a split stereo group, the first jack of each row: right of its tag (IN L, OUT R).
const ROW_X: f64 = 62.0;

/// A frame's name (ADR 0020). A card's: the port in use as a word (SPEAKERS, HEADPHONES, HDMI),
/// when no other frame of the device has that word; else PipeWire's short name for its node
/// (`node.nick`: HDMI 1, DIGITAL MICROPHONE), when no other node of the device shares it (a
/// desktop card's sink and source both say ALC287 ANALOG: the codec's) and it isn't the device's
/// own name; else the word, or what it is (SOUND OUT, SOUND IN). A program's or the system's:
/// what its node is called, or which way its sound goes. Two frames of a device still of one
/// name: the second is numbered (HDMI 2).
pub fn name(dev: &Device, gi: usize) -> String {
    // a surround node's part (ADR 0025): its node's name (its MASTER's) and the part
    if let Some(part) = &dev.groups[gi].part {
        let g = &dev.groups[gi];
        let master = dev.groups.iter().position(|o| o.node == g.node && o.part.as_deref() == Some(crate::graph::MASTER)).unwrap_or(gi);
        return format!("{} · {part}", base_name(dev, master));
    }
    let n = base_name(dev, gi);
    let counted = |i: usize| matches!(dev.groups[i].role, "PLAYBACK" | "CAPTURE");
    if !counted(gi) {
        return n;
    }
    let before = (0..gi).filter(|&i| counted(i) && base_name(dev, i) == n).count();
    if before > 0 { format!("{n} {}", before + 1) } else { n }
}

/// The port in use as a word; with none in use (HDMI with nothing plugged in), the word all its
/// ports share.
fn port_word_of(g: &crate::graph::Group) -> Option<&'static str> {
    let active = g.mix.as_ref().and_then(|m| m.route).and_then(|r| g.ports_to.iter().find(|p| p.index == r.index));
    let words: Vec<&str> = g.ports_to.iter().filter_map(|p| port_word(&p.name, g.dir == Dir::Out)).collect();
    active.and_then(|p| port_word(&p.name, g.dir == Dir::Out)).or(words.first().copied().filter(|w| words.iter().all(|x| x == w)))
}

/// A frame's name before two of a name are numbered (`name`).
fn base_name(dev: &Device, gi: usize) -> String {
    let g = &dev.groups[gi];
    // the other nodes' frames of this device
    let others = || dev.groups.iter().enumerate().filter(move |(i, o)| *i != gi && o.node != g.node).map(|(_, o)| o);
    let word = port_word_of(g);
    if let Some(w) = word
        && others().all(|o| port_word_of(o) != Some(w))
    {
        return w.to_owned();
    }
    // a name of its own: what its program named its node, a MIDI client's
    if g.label != g.role {
        return g.label.clone();
    }
    // hardware's: what PipeWire calls its node, when that tells it apart
    if dev.finish == Finish::Aluminium
        && matches!(g.role, "PLAYBACK" | "CAPTURE")
        && let Some(nick) = g.nick.as_deref().map(str::trim).filter(|n| !n.is_empty())
        && !nick.eq_ignore_ascii_case(dev.name.trim())
        && others().all(|o| !o.nick.as_deref().is_some_and(|x| x.trim().eq_ignore_ascii_case(nick)))
    {
        return nick.to_uppercase();
    }
    if let Some(w) = word {
        return w.to_owned();
    }
    let card = dev.card.is_some();
    // no port to go by (a Bluetooth headset's microphone in its hi-fi profile): what it is
    let headset = dev.model.contains("HEADSET") || dev.model.contains("HEADPHONE");
    match g.role {
        "PLAYBACK" if headset => "HEADPHONES",
        "CAPTURE" if headset => "MIC",
        "PLAYBACK" if card => "SOUND OUT",
        "PLAYBACK" => "INPUT",
        "CAPTURE" if card => "SOUND IN",
        "CAPTURE" => "OUTPUT",
        "MONITOR" => "MONITOR",
        _ => g.label.as_str(),
    }
    .to_owned()
}

/// A short word for a card's port, from PipeWire's name for it (the same in every language;
/// its description is translated): `analog-output-speaker` is SPEAKERS.
fn port_word(name: &str, input: bool) -> Option<&'static str> {
    let n = name.to_lowercase();
    let has = |s: &str| n.contains(s);
    Some(if input {
        if has("internal") {
            "BUILT-IN MIC"
        } else if has("line") {
            "LINE IN"
        } else if has("mic") || has("headset") || has("hands") {
            "MIC"
        } else {
            return None;
        }
    } else if has("speaker") {
        "SPEAKERS"
    } else if has("headphone") || has("headset") || has("hands") {
        "HEADPHONES"
    } else if has("hdmi") {
        "HDMI"
    } else if has("spdif") || has("iec958") {
        "S/PDIF"
    } else if has("line") {
        "LINE OUT"
    } else {
        return None;
    })
}

/// A MONITOR's host: the output whose copy it carries (on the same node). Its MONITOR jack sits
/// in that output's frame, after a thin rule, instead of a frame of its own.
pub fn host(dev: &Device, gi: usize) -> Option<usize> {
    let g = &dev.groups[gi];
    if g.role != "MONITOR" {
        return None;
    }
    // (a surround node's: its part's, ADR 0025)
    dev.groups.iter().position(|o| o.dir == Dir::In && o.mix.is_some() && o.node == g.node && o.part == g.part)
}

/// The MONITOR whose MONITOR sits in group `gi`'s frame.
fn rec_of(dev: &Device, gi: usize) -> Option<usize> {
    (0..dev.groups.len()).find(|m| host(dev, *m) == Some(gi))
}

/// Room for MONITOR in a frame: its jacks and the rule before them.
fn rec_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    rec_of(dev, gi).map_or(0.0, |m| {
        let mine = || jd.iter().enumerate().filter(|(_, j)| j.group == m).map(|(i, _)| slots(counts[i]));
        if stacked(dev, m, jd) {
            // split with its input: L over R, its tags at their left, the longest row
            mon_row_x() + (mine().max().unwrap_or(1) - 1) as f64 * JP + 26.0
        } else {
            jd.iter().enumerate().filter(|(_, j)| j.group == m).map(|(i, _)| cell_w(dev, m, jd, i, slots(counts[i]))).sum::<f64>() + 18.0
        }
    })
}

/// A split MONITOR's first jack in each row, from the rule before it: room for its tag (MON L).
fn mon_row_x() -> f64 {
    9.0 + SMALL.width("MON L") + 10.0 + 6.0 + 21.0
}

/// A stereo group whose switch is on SPLIT: its L and R stand one above the other.
fn stacked(dev: &Device, gi: usize, jd: &[JackDef]) -> bool {
    dev.groups[gi].stereo && jd.iter().filter(|j| j.group == gi).count() == 2
}

/// A group's whole width: its jacks and its controls.
fn group_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    jacks_w(dev, gi, jd, counts) + mix_w(dev, gi)
}

/// The tag under jack `i` of group `gi`: what the port is (IN, OUT, MON in its input's frame)
/// and, inside it, a MIDI client's name (IN · HYDROGEN) or, in a group of several jacks side by
/// side, its channel (OUT · OUT5, MON · FC).
fn jack_tag(dev: &Device, gi: usize, jd: &[JackDef], i: usize) -> String {
    let g = &dev.groups[gi];
    // (MON: the monitor's tag short, as IN and OUT are, 2026-11-26)
    let word = if host(dev, gi).is_some() { "MON" } else if g.dir == Dir::In { "IN" } else { "OUT" };
    if let Some(c) = &g.client {
        format!("{word} · {c}")
    } else if jd.iter().filter(|j| j.group == gi).count() > 1 && !stacked(dev, gi, jd) {
        format!("{word} · {}", jd[i].label)
    } else {
        word.to_owned()
    }
}

/// Jack `i`'s room in a row: its mult (a jack a cord, and a free one), or its tag, the wider.
fn cell_w(dev: &Device, gi: usize, jd: &[JackDef], i: usize, slots_n: usize) -> f64 {
    let jacks = slots_n as f64 * JP;
    let t = jack_tag(dev, gi, jd, i);
    if t.contains(" · ") { jacks.max(SMALL.width(&t) + 18.0) } else { jacks }
}

fn jacks_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    let mine = || jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| slots(counts[i]));
    let jacks = if stacked(dev, gi, jd) {
        // the longest row
        ROW_X + (mine().max().unwrap_or(1) - 1) as f64 * JP + 36.0
    } else {
        jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| cell_w(dev, gi, jd, i, slots(counts[i]))).sum::<f64>() + 14.0
    };
    // a surround node's MASTER (ADR 0025): no jacks, and its name runs on over its knob (it has no
    // heading) to SIG; only what it needs beyond that before the knob
    let g = &dev.groups[gi];
    if g.part.as_deref() == Some(crate::graph::MASTER) {
        let knobs = if g.stereo { 96.0 } else { 58.0 };
        return (label_w(&name(dev, gi)) + 21.0 - knobs).max(4.0).ceil();
    }
    // the name spans the MON too: room for it is found over both
    let rec = rec_w(dev, gi, jd, counts);
    // room on the band for SPLIT and the name (a frame of several sections has them once,
    // across them all)
    let n = sections(dev, gi).len().max(1) as f64;
    let keys = if splits(dev, gi) && !split_below(dev, gi) { split_key_w() + 4.0 } else { 0.0 };
    // (9 before the name; after it 12 to what is on the band at its right, the volume's heading or
    // SIG, or 25 to the frame's edge where nothing is)
    let g = &dev.groups[gi];
    let after = if g.mix.is_some() || metered(g) { 21.0 } else { 34.0 };
    let mut named = ((label_w(&name(dev, gi)) + keys + after) / n).ceil();
    // the name may run on over the knobs, up to their heading (VOLUME · BAL, centred over them):
    // only what it needs beyond that widens the jacks' part
    if n == 1.0
        && let Some(m) = &g.mix
    {
        let knobs = if g.stereo { 96.0 } else { 58.0 };
        let heading = format!("{}{}", if m.gain { "GAIN" } else { "VOLUME" }, if g.stereo { " · BAL" } else { "" });
        named -= ((knobs - label_w(&heading)) / 2.0 - 6.0).max(0.0);
    }
    (jacks + rec).max(named)
}

/// A frame's one SPLIT key (on its band before its name, or in its row of keys): as wide whether
/// it says SPLIT or JOIN.
fn split_key_w() -> f64 {
    10.0 + SMALL.width("SPLIT").max(SMALL.width("JOIN"))
}

/// Whether a frame's SPLIT is in its row of keys, left of MUTE (its first section is a stereo pair
/// with a volume: room for both under its knobs), rather than on its band.
fn split_below(dev: &Device, gi: usize) -> bool {
    splits(dev, gi) && sections(dev, gi).first().is_some_and(|f| dev.groups[*f].mix.is_some() && dev.groups[*f].stereo)
}

/// Whether a frame has a SPLIT key: one of its sections is a stereo pair.
fn splits(dev: &Device, gi: usize) -> bool {
    sections(dev, gi).iter().any(|s| dev.groups[*s].stereo)
}

/// At most this many frames (inputs, outputs, monitors…) side by side: the next starts a new
/// unit under them, as a 2U or 3U device.
pub const PER_ROW: usize = 4;

/// A unit of a multi-unit face: its inputs (at its left) and its outputs (at its right).
struct Row {
    /// what takes sound in (IN jacks: SPEAKERS, HEADPHONES, an app's INPUT), at its left
    left: Vec<usize>,
    /// what sends it out (OUT jacks: an app's OUTPUT, a MIC), at its right
    right: Vec<usize>,
}

/// The groups in their units, balanced: as few units as up to `PER_ROW` frames side by side allow,
/// the frames spread evenly over them (8: 4 + 4, 7: 4 + 3, 5: 3 + 2), so no unit is left half
/// empty. What takes sound in fills from the top-left, what sends it out the room left at the rows'
/// right ends, so sound flows from left to right as on a 1U face.
fn rows(dev: &Device) -> Vec<Row> {
    let (ins, outs) = sides(dev);
    if framed(dev) {
        // a node's frames stay on one unit: whole blocks, up to PER_ROW frames a unit (more when
        // one block is wider)
        let mut rows: Vec<Row> = Vec::new();
        for b in blocks(dev, &ins) {
            match rows.last_mut() {
                Some(r) if r.left.len() + b.len() <= PER_ROW && !(fresh(dev, &b) && !r.left.is_empty()) => r.left.extend(b),
                _ => rows.push(Row { left: b, right: Vec::new() }),
            }
        }
        if rows.is_empty() {
            rows.push(Row { left: Vec::new(), right: Vec::new() });
        }
        return rows;
    }
    let n = ins.len() + outs.len();
    let units = n.div_ceil(PER_ROW).max(1);
    let cap = n.div_ceil(units);
    // the first rows take one more when they don't divide evenly (10: 4 3 3)
    let full = n - (cap.saturating_sub(1)) * units;
    let (mut i, mut o) = (0, 0);
    (0..units)
        .map(|u| {
            let k = if u < full { cap } else { cap.saturating_sub(1) };
            let a = k.min(ins.len() - i);
            let b = (k - a).min(outs.len() - o);
            let row = Row { left: ins[i..i + a].to_vec(), right: outs[o..o + b].to_vec() };
            (i, o) = (i + a, o + b);
            row
        })
        .collect()
}

/// A face's two sides, so that sound flows through it from left to right: the groups that take
/// sound in (IN jacks: SPEAKERS, HEADPHONES with their MONITOR inside, an app's INPUT) for the
/// left, and those that send it out (OUT jacks: an app's OUTPUT, a MIC) for the right. A program
/// of several nodes keeps its groups in the order its sound goes through them, all from the left.
fn sides(dev: &Device) -> (Vec<usize>, Vec<usize>) {
    if framed(dev) {
        let mut all: Vec<usize> = (0..dev.groups.len()).filter(|g| host(dev, *g).is_none()).collect();
        if !dev.chain {
            // node by node: those that take sound in first, then those that only send it out
            let first = |g: usize| (0..dev.groups.len()).position(|o| dev.groups[o].node == dev.groups[g].node).unwrap_or(g);
            let only_out = |g: usize| sections(dev, g).iter().all(|o| dev.groups[*o].dir == Dir::Out);
            all.sort_by_key(|g| (only_out(*g), first(*g)));
        }
        return (all, Vec::new());
    }
    let take = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::In).collect();
    let make = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::Out && host(dev, *g).is_none()).collect();
    (take, make)
}

/// A multi-unit face is one grid: a row's inputs in its first columns, its outputs in its last
/// (a short row's gap falls between them). Each group's column, and each column's width (its
/// widest frame).
fn grid(dev: &Device, jd: &[JackDef], counts: &[usize], rows: &[Row]) -> (Vec<Vec<(usize, usize)>>, Vec<f64>) {
    let n = rows.iter().map(|r| r.left.len() + r.right.len()).max().unwrap_or(0);
    let at: Vec<Vec<(usize, usize)>> = rows.iter().map(|r| r.left.iter().enumerate().map(|(c, g)| (*g, c)).chain(r.right.iter().enumerate().map(|(j, g)| (*g, n - r.right.len() + j))).collect()).collect();
    let w = (0..n).map(|c| at.iter().flatten().filter(|(_, k)| *k == c).map(|(g, _)| group_w(dev, *g, jd, counts)).fold(0.0, f64::max)).collect();
    (at, w)
}

/// In the rack a face is as wide as the rack: its frames go in order (what takes sound in, then
/// what sends it out), each unit's from its left edge, on as few units as fit. The lower units
/// (the whole width, `more`, under the name and screen too) fill first, from the last frame back;
/// the unit beside the name and screen (`room`) takes only what is left over, so a 2U face is its
/// name over a row of frames, not frames stepping down. A face of three units or more is a grid
/// already, frames over frames: its first unit takes what fits beside the name and screen, the
/// rest follow on, a unit filled after another (docs/design/rack-top, variant B). (All of a
/// unit's frames are in `left`.)
fn rack_rows(dev: &Device, jd: &[JackDef], counts: &[usize], room: f64, more: f64) -> Vec<Row> {
    let (ins, outs) = sides(dev);
    let all: Vec<usize> = ins.into_iter().chain(outs).collect();
    // a frame's sections stay on one unit, side by side
    let blocks = blocks(dev, &all);
    let width = |b: &[usize]| b.iter().map(|g| group_w(dev, *g, jd, counts) + gap_after(dev, *g)).sum::<f64>() - gap_after(dev, b[b.len() - 1]);
    let fresh: Vec<bool> = blocks.iter().map(|b| fresh(dev, b)).collect();
    let rows = from_below(&blocks, &fresh, &width, room, more).unwrap_or_else(|| {
        // (nothing fits however many units: a frame to a unit, under the name and screen)
        std::iter::once(Vec::new()).chain(all.iter().map(|g| vec![*g])).collect()
    });
    let rows = if rows.len() >= 3 {
        let top = from_above(&blocks, &fresh, &width, room, more);
        if top.len() <= rows.len() { top } else { rows }
    } else {
        rows
    };
    rows.into_iter().map(|left| Row { left, right: Vec::new() }).collect()
}

/// The blocks on as few units as fit, the lower units filled first from the last block back, the
/// first unit (`room` wide) taking what is left over; none if a block fits no unit. A `fresh`
/// block starts its unit: nothing goes before it there.
fn from_below(blocks: &[Vec<usize>], fresh: &[bool], width: &dyn Fn(&[usize]) -> f64, room: f64, more: f64) -> Option<Vec<Vec<usize>>> {
    for n in 1..=blocks.len() + 1 {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut used = vec![0.0; n];
        let mut k = n - 1;
        let mut whole = true;
        // the unit `k` has a fresh block first: the next goes to the unit over it
        let mut closed = false;
        for (bi, b) in blocks.iter().enumerate().rev() {
            let gw = width(b);
            loop {
                let cap = if k == 0 { room } else { more };
                let need = if rows[k].is_empty() { gw } else { used[k] + 10.0 + gw };
                if need <= cap + 1e-6 && !closed {
                    rows[k].splice(0..0, b.iter().copied());
                    used[k] = need;
                    closed = fresh[bi];
                    break;
                }
                closed = false;
                if k == 0 {
                    whole = false;
                    break;
                }
                k -= 1;
            }
            if !whole {
                break;
            }
        }
        if whole {
            return Some(rows);
        }
    }
    None
}

/// The blocks a unit after another, from the first (`room` wide, beside the name and screen):
/// each unit takes what fits, the next block starts the next unit. A block wider than any unit
/// has one of its own. A `fresh` block starts a unit of its own (the first, if that is empty).
fn from_above(blocks: &[Vec<usize>], fresh: &[bool], width: &dyn Fn(&[usize]) -> f64, room: f64, more: f64) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (bi, b) in blocks.iter().enumerate() {
        let gw = width(b);
        let cap = if rows.len() == 1 { room } else { more };
        let empty = rows[rows.len() - 1].is_empty();
        let need = if empty { gw } else { used + 10.0 + gw };
        if (need <= cap + 1e-6 && (empty || !fresh[bi])) || (rows.len() > 1 && empty) {
            rows.last_mut().expect("a unit").extend(b.iter().copied());
            used = need;
        } else {
            rows.push(b.clone());
            used = gw;
        }
    }
    rows
}

/// A block that starts a unit of its own: a surround node's MASTER, so the node's frames start
/// a unit with it (ADR 0025); a node of one or two channels stays beside the frame before.
fn fresh(dev: &Device, b: &[usize]) -> bool {
    dev.groups[b[0]].part.as_deref() == Some(crate::graph::MASTER)
}

/// Groups in a row, in blocks: a frame's groups together (a node's, or a surround node's part's,
/// ADR 0025: its parts are frames of their own and wrap onto units as any frames do).
fn blocks(dev: &Device, gs: &[usize]) -> Vec<Vec<usize>> {
    let frame = |g: usize| (dev.groups[g].node, dev.groups[g].part.as_deref());
    let mut out: Vec<Vec<usize>> = Vec::new();
    for &g in gs {
        match out.last_mut() {
            Some(b) if frame(b[0]) == frame(g) => b.push(g),
            _ => out.push(vec![g]),
        }
    }
    out
}

/// A frame is a node (ADR 0014), or a surround node's part (ADR 0025): group `gi`'s frame's
/// sections, its node's (part's) groups side by side (its inputs, then its outputs; a MONITOR
/// inside the input it copies, not a section).
pub fn sections(dev: &Device, gi: usize) -> Vec<usize> {
    let g = &dev.groups[gi];
    (0..dev.groups.len()).filter(|o| dev.groups[*o].node == g.node && dev.groups[*o].part == g.part && host(dev, *o).is_none()).collect()
}

/// A face with a frame of several sections (a node both ways, a MIDI bridge's clients, a
/// program's chain): its frames go node by node from the left, each whole on one unit.
fn framed(dev: &Device) -> bool {
    dev.chain || (0..dev.groups.len()).any(|g| host(dev, g).is_none() && sections(dev, g).len() > 1)
}

/// The room after a group before the next: none between a frame's sections.
fn gap_after(dev: &Device, gi: usize) -> f64 {
    let s = sections(dev, gi);
    if s.len() > 1 && s.last() != Some(&gi) { 0.0 } else { 10.0 }
}

/// What a face beside its name and screen has in the rack, `rack_w` wide; and in its other
/// units, the whole width.
fn rack_room(rack_w: f64) -> (f64, f64) {
    let all = rack_w - 2.0 * (EAR + PAD);
    (all - IDW - 14.0, all)
}

/// The narrowest a rack can be for this face (a narrower window scrolls sideways): its name and
/// screen, or its widest frame (on a unit of its own, under them), whichever is wider.
pub fn rack_min_w(dev: &Device, jd: &[JackDef], counts: &[usize]) -> f64 {
    let widest = (0..dev.groups.len()).filter(|gi| host(dev, *gi).is_none()).map(|gi| group_w(dev, gi, jd, counts)).fold(0.0, f64::max);
    2.0 * (EAR + PAD) + widest.max(IDW)
}

/// How many units the face takes in a rack `rack_w` wide.
pub fn rack_units(dev: &Device, jd: &[JackDef], counts: &[usize], rack_w: f64) -> usize {
    let (room, more) = rack_room(rack_w.max(rack_min_w(dev, jd, counts)));
    rack_rows(dev, jd, counts, room, more).len().max(1)
}

/// Width the face needs; a rack is as wide as its widest device.
pub fn natural_w(dev: &Device, jd: &[JackDef], counts: &[usize], rack: bool) -> f64 {
    let rows = rows(dev);
    if framed(dev) {
        // each unit's frames from its left: beside the name and screen, then under them
        let row_w = |r: &Row| r.left.iter().map(|g| group_w(dev, *g, jd, counts) + gap_after(dev, *g)).sum::<f64>() - 10.0;
        let widest = rows.iter().enumerate().map(|(u, r)| row_w(r) + if u == 0 { IDW + 14.0 } else { 0.0 }).fold(IDW, f64::max);
        return 2.0 * if rack { EAR } else { 0.0 } + 2.0 * PAD + widest;
    }
    let groups: f64 = if rows.len() > 1 {
        grid(dev, jd, counts, &rows).1.iter().map(|w| w + 10.0).sum()
    } else {
        // (a monitor's MONITOR is inside its output's frame: no room of its own)
        (0..dev.groups.len()).filter(|gi| host(dev, *gi).is_none()).map(|gi| group_w(dev, gi, jd, counts) + gap_after(dev, gi)).sum()
    };
    2.0 * if rack { EAR } else { 0.0 } + 2.0 * PAD + IDW + 14.0 + groups - 10.0
}

/// Where everything goes, without drawing it.
pub fn face_geometry(dev: &Device, jd: &[JackDef], counts: &[usize], rack_w: Option<f64>, accents: bool) -> Geometry {
    let rack = rack_w.is_some();
    let ear = if rack { EAR } else { 0.0 };
    let natural = natural_w(dev, jd, counts, rack);
    // (in the rack as wide as it, or its widest frame if the rack is narrower still)
    let w = rack_w.map_or(natural, |r| r.max(rack_min_w(dev, jd, counts)));
    let ports = dev.card.is_some() && dev.groups.iter().any(|g| g.ports_to.len() > 1);
    // (no HIDE while it has cords: it can't be hidden then, the key isn't there, and PROFILE and
    // PORT take its room)
    let cords = counts.iter().any(|c| *c > 0);
    let hide = [ear + PAD + KEY_IN, KEY_Y, if cords { 0.0 } else { HIDE_W }, if cords { 0.0 } else { KEY_H }];
    let taken = if cords { 0.0 } else { HIDE_W + KEY_GAP };
    let (kx, room) = (hide[0] + taken, IDW - 2.0 * KEY_IN - taken);
    let (profile, port) = if ports {
        let kw = (room - KEY_GAP) / 2.0;
        (Some([kx, KEY_Y, kw, KEY_H]), Some([kx + kw + KEY_GAP, KEY_Y, kw, KEY_H]))
    } else {
        (dev.card.is_some().then_some([kx, KEY_Y, room, KEY_H]), None)
    };
    let rows_of = if rack {
        let (room, more) = rack_room(w);
        rack_rows(dev, jd, counts, room, more)
    } else {
        rows(dev)
    };
    // (in the rack a face fills its units but for the gap to the next one)
    let units = rows_of.len().max(1) as f64;
    let h = if rack { RACK_U * units - RACK_GAP } else { DEV_H * units };
    let mut geo = Geometry { w, h, jacks: vec![(0.0, 0.0); jd.len()], switches: Vec::new(), hide, left: ear, groups: vec![(0.0, 0.0, 0.0, 0.0); dev.groups.len()], grille: None, profile, port, mix: Vec::new(), meters: Vec::new(), vents: Vec::new(), title_w: IDW, title: None, graph: Vec::new() };
    // in the rack, a first unit with no frames: the name on over it to the right edge
    if rack && rows_of.first().is_none_or(|r| r.left.is_empty() && r.right.is_empty()) {
        geo.title_w = w - 2.0 * (ear + PAD);
        let tw = text::width(text::BLACK, &fit_title(&dev.name, geo.title_w), TITLE, 0.0);
        if tw > IDW {
            geo.title = Some([ear + PAD, TITLE_Y - TITLE, tw, TITLE + 3.0]);
        }
    }
    // a group at `x`, in the unit `oy` down
    let place = |gi: usize, x: f64, oy: f64, geo: &mut Geometry| -> f64 {
        let (gw, jw) = (group_w(dev, gi, jd, counts), jacks_w(dev, gi, jd, counts));
        let cells: f64 = jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| cell_w(dev, gi, jd, i, slots(counts[i]))).sum();
        let rw = rec_w(dev, gi, jd, counts);
        // the left edge of the next jack's room
        let mut jx = x + (jw - rw - cells) / 2.0;
        let rows = stacked(dev, gi, jd);
        // MONITOR: after a thin rule at the right of the jacks (between the rows when split)
        if let Some(m) = rec_of(dev, gi) {
            let r = if accents { ROWS_BAND } else { ROWS };
            let ry = oy + if rows { (r[0] + r[1]) / 2.0 } else { 62.0 };
            let mut mx = x + jw - rw + 9.0;
            let split = stacked(dev, m, jd);
            for (k, (i, _)) in jd.iter().enumerate().filter(|(_, j)| j.group == m).enumerate() {
                if split {
                    // split with its input (one SPLIT a frame): L over R, as the input's rows
                    geo.jacks[i] = (x + jw - rw + mon_row_x(), oy + r[k.min(1)]);
                } else {
                    // (its mult in the middle of its room, as the input's)
                    let (k, cw) = (slots(counts[i]), cell_w(dev, m, jd, i, slots(counts[i])));
                    geo.jacks[i] = (mx + (cw - k as f64 * JP) / 2.0 + JP / 2.0, ry);
                    mx += cw;
                }
            }
            geo.groups[m] = (x + jw - rw, rw, rw, oy);
        }
        for (r, (i, _)) in jd.iter().enumerate().filter(|(_, j)| j.group == gi).enumerate() {
            if rows {
                // (its rows in the middle of their room, as one row's jacks are)
                let longest = jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| slots(counts[i])).max().unwrap_or(1);
                let off = ((jw - rw - (ROW_X + (longest - 1) as f64 * JP + 36.0)) / 2.0).max(0.0);
                geo.jacks[i] = (x + off + ROW_X, oy + if accents { ROWS_BAND } else { ROWS }[r]);
            } else {
                // (its mult in the middle of its room)
                let (k, cw) = (slots(counts[i]), cell_w(dev, gi, jd, i, slots(counts[i])));
                geo.jacks[i] = (jx + (cw - k as f64 * JP) / 2.0 + JP / 2.0, oy + 62.0);
                jx += cw;
            }
        }
        let g = &dev.groups[gi];
        // a stream's controls, right of its jacks
        // a column for each of a stereo pair's channels, and for each of two channels that don't
        // say which they are (a JACK app's out1, out2)
        let two = g.stereo || (g.ports.len() == 2 && g.ports.iter().all(|p| p.media == Media::Audio));
        let cols = if two { 2 } else { 1 };
        let metered_here = metered(g) && host(dev, gi).is_none();
        if g.mix.is_some() {
            let (ox, stereo) = (x + jw, g.stereo);
            let w = if stereo { 96.0 } else { 58.0 };
            // MUTE under the knobs; the frame's SPLIT beside it at the left, the two halves of
            // the row (as MIXER's keys)
            let first = sections(dev, gi).first() == Some(&gi) && split_below(dev, gi);
            // a surround node's part has no MUTE (PipeWire mutes only the node, its MASTER does):
            // SPLIT has the row to itself, or there is none (ADR 0025)
            let muted = !g.channels_part();
            // (side by side in a block, as MIXER's: 2 of the hole's dark between them)
            let kw = if first && muted { (w - 14.0 - 2.0) / 2.0 } else { w - 14.0 };
            if first {
                geo.switches.push((gi, [ox + 7.0, oy + 89.0, kw, 14.0]));
            }
            let mx0 = ox + 7.0 + if first { kw + if muted { 2.0 } else { 0.0 } } else { 0.0 };
            let mute = if muted { [mx0, oy + 89.0, kw, 14.0] } else { [mx0, oy + 89.0, 0.0, 0.0] };
            geo.mix.push((gi, MixGeo {
                x: ox,
                y: oy,
                w,
                input: g.mix.as_ref().is_some_and(|m| m.gain),
                knob: (if stereo { ox + 33.0 } else { ox + w / 2.0 }, oy + 60.0),
                bal: stereo.then_some((ox + 75.0, oy + 56.0)),
                mute,
                split: first.then_some([ox + 7.0, oy + 89.0, kw, 14.0]),
                bare: g.part.as_deref() == Some(crate::graph::MASTER),
            }));
        }
        // its meter: after its knobs, or (without) right of its jacks
        if metered_here {
            let mx = if g.mix.is_some() { x + jw + if g.stereo { 96.0 } else { 58.0 } + 1.0 } else { x + jw - 2.0 };
            geo.meters.push((gi, mx, oy, cols));
        }
        geo.groups[gi] = (x, gw, jw, oy);
        // a frame's first section: its name on the band (`graph`), and the frame's one
        // SPLIT (every stereo pair in the frame, a MONITOR inside it too)
        if sections(dev, gi).first() == Some(&gi) {
            {
                // the name: a click on it opens ENGINE with this block (it was a key, #180)
                let nx = x + 9.0 + if splits(dev, gi) && !split_below(dev, gi) { split_key_w() + 4.0 } else { 0.0 };
                geo.graph.push((dev.groups[gi].node, [nx - 3.0, oy + 20.0, label_w(&name(dev, gi)) + 6.0, 14.0]));
            }
            if splits(dev, gi) && !split_below(dev, gi) {
                geo.switches.push((gi, [x + 6.0, oy + 21.0, split_key_w(), 12.0]));
            }
        }
        x + gw + gap_after(dev, gi)
    };
    // in the rack each unit on its own: its frames from the left (beside the name and screen in
    // the first unit, under them in the others), vents after them; a face with nothing to show,
    // vents all along
    if rack || framed(dev) {
        let right = w - ear - PAD;
        for (u, row) in rows_of.iter().enumerate() {
            let oy = u as f64 * if rack { RACK_U } else { DEV_H } - LIFT;
            let x0 = if u == 0 { ear + PAD + IDW + 14.0 } else { ear + PAD };
            let mut a = x0;
            for &gi in &row.left {
                a = place(gi, a, oy, &mut geo);
            }
            let span: f64 = row.right.iter().map(|g| group_w(dev, *g, jd, counts) + 10.0).sum();
            let mut b = right + 10.0 - span;
            let b0 = b;
            for &gi in &row.right {
                b = place(gi, b, oy, &mut geo);
            }
            geo.vents.push([a, b0 - 10.0, oy]);
        }
        keep_vents(&mut geo.vents, if rack { RACK_U } else { DEV_H });
        return geo;
    }
    // more than one unit: one grid, a unit for each row; inputs on their column's left edge,
    // outputs on its right one. Vents where a face has nothing to show: under the name and screen
    // from the second unit down, and between a short row's inputs and outputs
    if rows_of.len() > 1 {
        let (at, cw) = grid(dev, jd, counts, &rows_of);
        let x0 = ear + PAD + IDW + 14.0;
        let cx: Vec<f64> = cw.iter().scan(x0, |x, w| {
            let here = *x;
            *x += w + 10.0;
            Some(here)
        }).collect();
        let right = w - ear - PAD;
        for (u, row) in rows_of.iter().enumerate() {
            let oy = u as f64 * DEV_H - LIFT;
            for &(gi, c) in &at[u] {
                let x = if dev.groups[gi].dir == Dir::In { cx[c] } else { cx[c] + cw[c] - group_w(dev, gi, jd, counts) };
                place(gi, x, oy, &mut geo);
            }
            let (k, m) = (row.left.len(), row.right.len());
            let a = if k > 0 { cx[k - 1] + cw[k - 1] + 10.0 } else { x0 };
            // (to where its first output starts: it sits at its column's right edge, its column
            // may be wider)
            let b = if m > 0 { row.right.iter().map(|&g| geo.groups[g].0).fold(f64::MAX, f64::min) - 10.0 } else { right };
            if u > 0 && k == 0 {
                geo.vents.push([ear + PAD, b, oy]);
            } else {
                if u > 0 {
                    geo.vents.push([ear + PAD, ear + PAD + IDW, oy]);
                }
                geo.vents.push([a, b, oy]);
            }
        }
        keep_vents(&mut geo.vents, DEV_H);
        return geo;
    }
    // what takes sound in on the left, what sends it out on the right; in a rack against the
    // right edge
    let (ins, outs) = sides(dev);
    let span: f64 = outs.iter().map(|g| group_w(dev, *g, jd, counts) + 10.0).sum::<f64>();
    let mut x = ear + PAD + IDW + 14.0;
    for g in &ins {
        x = place(*g, x, -LIFT, &mut geo);
    }
    let gx0 = x;
    // (in a rack, against the right edge)
    let mut ox = if rack { w - ear - PAD + 10.0 - span } else { x };
    if rack && ox - 10.0 - gx0 > 50.0 {
        geo.grille = Some((gx0, ox - 10.0));
    }
    for g in &outs {
        ox = place(*g, ox, -LIFT, &mut geo);
    }
    geo
}

/// A point `r` from (x, y) at angle `a` (degrees, 0 straight up, clockwise).
/// A knob's shadow on the panel, the same for every knob (the rack's, MIXER's): lit from the top
/// left, down and a little right of it, as far as it stands out (`lift`).
pub(crate) fn knob_shadow(x: f64, y: f64, r: f64, lift: f64) -> String {
    format!(r##"<circle cx="{}" cy="{}" r="{}" fill="#000" opacity=".5"/>"##, x + 0.5 * lift, y + lift, r + 0.3 * lift)
}

/// A knob `r` round at (x, y), its pointer at `a` (degrees from straight up), the rack's and
/// MIXER's alike (its gradient `gVol`).
pub(crate) fn knob(x: f64, y: f64, r: f64, a: f64) -> String {
    format!(
        r##"<circle cx="{x}" cy="{y}" r="{r}" fill="url(#gVol)" stroke="#0c0c0d"/><circle cx="{x}" cy="{y}" r="{}" fill="none" stroke="rgba(255,255,255,.08)"/><path d="M{} L{}" stroke="#ffb02e" stroke-width="{}" stroke-linecap="round"/>"##,
        r * 0.75,
        polar(x, y, a, r * 0.29),
        polar(x, y, a, r * 0.86),
        if r > 12.0 { 2.4 } else { 2.0 }
    )
}

pub(crate) fn polar(x: f64, y: f64, a: f64, r: f64) -> String {
    let a = a.to_radians();
    format!("{:.2},{:.2}", x + r * a.sin(), y - r * a.cos())
}

/// The printed part of a stream's controls, inside its group's frame after a rule: the scales,
/// the MUTE key and its own meter's dark LEDs (the knobs and the lit LEDs are pictures of their
/// own, see `mix_pic`). On an output the knob is the VOLUME it plays at; on an input, the GAIN it
/// records with, as on a mixer's input channel.
fn mix_panel(m: &MixGeo, print: &str, band: bool, edge: &str) -> String {
    let oy = m.y;
    let m = &m.local();
    let name = if m.input { "GAIN" } else { "VOLUME" };
    // on a coloured band the heading is in its light lettering, and the rule starts below it: a
    // line in the frame's own colour, as its edge (`edge`; 2026-11-26: a groove pressed in)
    // (as far from the band, or the heading, as from the frame's bottom: RULE)
    let (ink, gy) = if band { (BAND_INK, RULE.0) } else { (print, 26.0) };
    let mut s = if m.bare {
        String::new()
    } else {
        format!(
            r#"<path d="M{x},{gy} V{}" stroke="{edge}" stroke-width="1.2" stroke-opacity=".9"/>{}"#,
            RULE.1,
            SECTION.text(m.x + m.w / 2.0, 31.0, ink, "middle", &if m.bal.is_some() { format!("{name} · BAL") } else { name.to_owned() }),
            x = m.x
        )
    };
    // VOLUME: silence to 150%, a long tick at 100% and red past it (louder than the app plays it:
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
    // the hole milled for the row of keys (SPLIT and MUTE, or MUTE): the keys themselves in
    // `mix_pic`, which is drawn again as they change or are pressed
    let [kx, ky, kw, kh] = m.mute;
    let row = m.split.map_or([kx, ky, kw, kh], |[sx, ..]| [sx, ky, kx + kw - sx, kh]);
    // (none: a surround part without SPLIT has no keys)
    if row[2] > 0.0 {
        super::mixer::key_hole(&mut s, row);
    }
    if oy != 0.0 { format!(r#"<g transform="translate(0,{oy})">{s}</g>"#) } else { s }
}

/// The knobs and the MUTE LED of a VOLUME section, as an SVG document covering the section
/// (`MIX_Y` down from the face's top, `MIX_H` high): drawn again whenever they move, without the
/// face.
pub fn mix_pic(m: &MixGeo, volume: f64, balance: f64, mute: bool, bal: BalKnob, split: bool, held: Option<RowKey>) -> String {
    let m = &m.local();
    let (cx, cy) = m.knob;
    let mut s = knob_shadow(cx, cy, 14.0, 2.5) + &knob(cx, cy, 14.0, vol_angle(volume));
    if let Some((bx, by)) = m.bal {
        let a = balance.clamp(-1.0, 1.0) * 135.0;
        let pointer = |dy: f64, color: &str, w: f64, r0: f64, r1: f64| format!(r#"<path d="M{} L{}" stroke="{color}" stroke-width="{w}" stroke-linecap="round"/>"#, polar(bx, by + dy, a, r0), polar(bx, by + dy, a, r1));
        s += &match bal {
            // held down: below flush, deeper in its well
            BalKnob::Held => format!(r##"<circle cx="{bx}" cy="{}" r="8.3" fill="#151618" stroke="#050506"/><circle cx="{bx}" cy="{}" r="8.6" fill="none" stroke="rgba(0,0,0,.7)" stroke-width="1.6"/>{}"##, by + 0.6, by - 0.4, pointer(0.6, "#6a5028", 1.7, 2.4, 6.6)),
            // out: a taller cap with a shadow under it
            BalKnob::Out => format!(r##"{}<circle cx="{bx}" cy="{}" r="9.6" fill="#0d0e0f"/>{}"##, knob_shadow(bx, by, 9.6, 2.5), by + 2.0, knob(bx, by, 9.6, a)),
            // in: flush, a dim pointer
            BalKnob::In => format!(r##"<circle cx="{bx}" cy="{by}" r="9" fill="#202124" stroke="#0c0c0d"/>{}"##, pointer(0.0, "#8a6a32", 1.8, 2.6, 7.4)),
        };
    }
    if m.mute[2] > 0.0 {
        super::mixer::lit_key(&mut s, m.mute, "MUTE", "#ff4a32", mute, held == Some(RowKey::Mute));
    }
    if let Some(r) = m.split {
        super::mixer::lit_key(&mut s, r, "SPLIT", "#ffb02e", split, held == Some(RowKey::Split));
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{MIX_H}" viewBox="{x} {MIX_Y} {w} {MIX_H}"><defs><radialGradient id="gVol" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient>{}</defs>{s}</svg>"##,
        super::mixer::glass_defs(),
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
    // a number at the end (the second of two programs of one name) stays: cut before it
    let tail = match t.rsplit_once(' ') {
        Some((head, n)) if !head.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => format!(" {n}"),
        _ => String::new(),
    };
    t.truncate(t.len() - tail.len());
    while !t.is_empty() && w(&format!("{t}…{tail}")) > max_w {
        t.pop();
    }
    format!("{}…{tail}", t.trim_end())
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

/// Half the size of a jack's picture: its bushing and edge fit inside.
pub const PIC: f64 = 22.0;

/// The plus badge at the corner of a mult's free jack, as an SVG document `2 PIC` square centred
/// on it. It and the jacks (`jack_pic_ring`) are laid over the faces as pictures of their own,
/// so that while a plug is in the hand the ones it can't go into can fade without the faces
/// being drawn again.
pub fn badge_pic() -> String {
    let s = 2.0 * PIC;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><circle cx="{PIC}" cy="{}" r="6.2" fill="#000" opacity=".35"/><circle cx="{PIC}" cy="{PIC}" r="5.8" fill="#1b1c1e" stroke="#f2f2f2" stroke-opacity=".35" stroke-width=".8"/><path d="M{},{PIC} h6 M{PIC},{} v6" stroke="#f2f2f2" stroke-width="1.6" stroke-linecap="round"/></svg>"##,
        PIC + 0.8,
        PIC - 3.0,
        PIC - 3.0
    )
}

/// A jack as a small amp's panel-mount one, flush with the face (ADR 0023; it had a hex nut): a
/// wide bushing in its group's colour, and inside it the jack: a black bezel, nickel round the deep
/// hole; the same for mono, stereo and MIDI (its colour tells MIDI; its tag, a stereo pair).
pub fn jack_pic_ring(color: &str) -> String {
    let s = 2.0 * PIC;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">{}<circle cx="{PIC}" cy="{PIC}" r="14.6" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="1.2"/><circle cx="{PIC}" cy="{PIC}" r="14" fill="{color}"/><use href="#jack" x="{PIC}" y="{PIC}"/></svg>"##,
        jack_defs()
    )
}

/// Half the size of a cue mark's picture: the ring round a jack listened to, and the
/// headphones on it.
pub const CUE_PIC: f64 = 27.0;

/// A jack kept on (clicked in CUE mode): a cyan disc out to the ring, laid under the jack, so the
/// jack's bushing lies on it.
pub fn cue_disc_pic() -> String {
    let s = 2.0 * CUE_PIC;
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><circle cx="{CUE_PIC}" cy="{CUE_PIC}" r="21.1" fill="#5fd4ff"/><circle cx="{CUE_PIC}" cy="{CUE_PIC}" r="20.7" fill="none" stroke="#000" stroke-opacity=".35" stroke-width=".8"/></svg>"##)
}

/// Half a jack kept on: the left (FL) or the right (FR) half of the disc under a stereo jack, when
/// only one of its two channels is kept on (from ENGINE).
pub fn cue_half_pic(right: bool) -> String {
    let s = 2.0 * CUE_PIC;
    let (c, r) = (CUE_PIC, 21.1);
    let sweep = right as u8;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><path d="M{c},{} A{r},{r} 0 0 {sweep} {c},{} Z" fill="#5fd4ff"/><path d="M{c},{} A{},{} 0 0 {sweep} {c},{}" fill="none" stroke="#000" stroke-opacity=".35" stroke-width=".8"/></svg>"##,
        c - r,
        c + r,
        c - r + 0.4,
        r - 0.4,
        r - 0.4,
        c + r - 0.4
    )
}

/// How far the R jack of a split stereo frame sits under its L jack.
pub const SPLIT_DY: f64 = ROWS[1] - ROWS[0];

/// A split pair (L over R) both kept on, or both listened to: one disc, or one ring with one
/// badge, round the two, so that their marks don't run into each other. Centred on the L jack,
/// `SPLIT_DY` longer down.
pub fn cue_pair_pic(ring: bool, red: bool) -> String {
    let (w, h) = (2.0 * CUE_PIC, 2.0 * CUE_PIC + SPLIT_DY);
    let c = if red { "#ff503c" } else { "#5fd4ff" };
    let body = if ring {
        let (bx, by) = (CUE_PIC + 17.0, CUE_PIC - 17.0);
        format!(
            r##"<rect x="{}" y="{}" width="40" height="{}" rx="20" fill="none" stroke="{c}" stroke-width="2.2"/>{}"##,
            CUE_PIC - 20.0,
            CUE_PIC - 20.0,
            40.0 + SPLIT_DY,
            phones_badge(c, bx, by)
        )
    } else {
        format!(
            r##"<rect x="{}" y="{}" width="42.2" height="{}" rx="21.1" fill="{c}"/><rect x="{}" y="{}" width="41.4" height="{}" rx="20.7" fill="none" stroke="#000" stroke-opacity=".35" stroke-width=".8"/>"##,
            CUE_PIC - 21.1,
            CUE_PIC - 21.1,
            42.2 + SPLIT_DY,
            CUE_PIC - 20.7,
            CUE_PIC - 20.7,
            41.4 + SPLIT_DY
        )
    };
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{body}</svg>"##)
}

/// The headphones of the CUE key on a dark badge at (bx, by), in colour `c`.
fn phones_badge(c: &str, bx: f64, by: f64) -> String {
    format!(r##"<circle cx="{bx}" cy="{by}" r="8.5" fill="#121315" stroke="{c}" stroke-width="1.4"/><g transform="translate({bx},{by}) scale(.4583) translate(-12,-12.5)"><path d="M4 15v-3a8 8 0 0 1 16 0v3" fill="none" stroke="{c}" stroke-width="2.4"/><rect x="3" y="14" width="4.5" height="7" fill="{c}"/><rect x="16.5" y="14" width="4.5" height="7" fill="{c}"/></g>"##)
}

/// A jack listened to: a ring round it and the headphones of the CUE key on a badge at its top
/// right; red for one that would feed back.
pub fn cue_ring_pic(red: bool) -> String {
    let s = 2.0 * CUE_PIC;
    let c = if red { "#ff503c" } else { "#5fd4ff" };
    let (bx, by) = (CUE_PIC + 17.0, CUE_PIC - 17.0);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><circle cx="{CUE_PIC}" cy="{CUE_PIC}" r="20" fill="none" stroke="{c}" stroke-width="2.2"/>{}</svg>"##,
        phones_badge(c, bx, by)
    )
}

/// Half the size of a target ring's picture: the ring and its halo.
pub const TARGET_PIC: f64 = 34.0;

/// While a cord's end is in the hand, a jack it may go into: a green ring in a soft halo (it
/// pulses through the picture's opacity); `hot`, the one it is pulled to: gold, brighter.
pub fn target_ring_pic(hot: bool) -> String {
    let s = 2.0 * TARGET_PIC;
    let (c, glow, r, w, ring) = if hot { ("#ffc85a", 0.75, 19.0, 3.5, 0.95) } else { ("#78eb96", 0.55, 17.5, 2.5, 0.9) };
    let halo = r + 14.0;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><defs><radialGradient id="h" cx="{TARGET_PIC}" cy="{TARGET_PIC}" r="{halo}" gradientUnits="userSpaceOnUse"><stop offset=".48" stop-color="{c}" stop-opacity="0"/><stop offset=".62" stop-color="{c}" stop-opacity="{glow}"/><stop offset="1" stop-color="{c}" stop-opacity="0"/></radialGradient></defs><circle cx="{TARGET_PIC}" cy="{TARGET_PIC}" r="{halo}" fill="url(#h)"/><circle cx="{TARGET_PIC}" cy="{TARGET_PIC}" r="{r}" fill="none" stroke="{c}" stroke-opacity="{ring}" stroke-width="{w}"/></svg>"##
    )
}

/// Where the badge sits from the centre of the free jack it marks.
pub const BADGE: (f64, f64) = (11.5, -11.5);

/// What lies under a face's drawing, put straight onto its pixels (see `raster`): its base paint,
/// and its noise. Drawn through the SVG these took most of a face's time
/// (a pattern sampled at every pixel of a face as wide as the rack).
#[derive(Clone, Copy, Debug)]
pub struct Skin {
    w: f64,
    h: f64,
    r: f64,
    base: [u8; 3],
    noise: f32,
}

fn hex(c: &str) -> [u8; 3] {
    let v = u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// A rectangle with round corners, `r` round.
fn rounded(w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    // (a quarter circle as a cubic)
    let k = r * (1.0 - 0.552_284_8);
    let mut p = PathBuilder::new();
    p.move_to(r, 0.0);
    p.line_to(w - r, 0.0);
    p.cubic_to(w - k, 0.0, w, k, w, r);
    p.line_to(w, h - r);
    p.cubic_to(w, h - k, w - k, h, w - r, h);
    p.line_to(r, h);
    p.cubic_to(k, h, 0.0, h - k, 0.0, h - r);
    p.line_to(0.0, r);
    p.cubic_to(0.0, k, k, 0.0, r, 0.0);
    p.close();
    p.finish()
}

/// The paint's noise, as pixels.
fn noise() -> Option<&'static Pixmap> {
    static T: OnceLock<Option<Pixmap>> = OnceLock::new();
    T.get_or_init(|| Pixmap::decode_png(include_bytes!("../../assets/noise.png")).ok()).as_ref()
}

/// A face's picture at `scale`: its `Skin` put straight onto the pixels (the base, then its noise
/// in 128-unit tiles), then its SVG drawn over it. (No light of its own: one light lies over all the faces, `scene::light_layer`.)
pub fn raster(opt: &usvg::Options, svg: &str, skin: &Skin, scale: f64) -> Option<Pixmap> {
    let tree = match usvg::Tree::from_str(svg, opt) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("patchcord: svg error: {e}");
            return None;
        }
    };
    let s = scale as f32;
    let size = tree.size();
    let mut pm = Pixmap::new((size.width() * s).ceil().max(1.0) as u32, (size.height() * s).ceil().max(1.0) as u32)?;
    let at = Transform::from_row(s, 0.0, 0.0, s, FACE_PAD as f32 * s, FACE_PAD as f32 * s);
    if let Some(path) = rounded(skin.w as f32, skin.h as f32, skin.r as f32) {
        let mut base = Paint { anti_alias: true, ..Default::default() };
        base.set_color_rgba8(skin.base[0], skin.base[1], skin.base[2], 255);
        pm.fill_path(&path, &base, FillRule::Winding, at, None);
        if let Some(t) = noise().filter(|_| skin.noise > 0.0) {
            let k = 128.0 / t.width() as f32;
            let shader = Pattern::new(t.as_ref(), SpreadMode::Repeat, FilterQuality::Bilinear, skin.noise, Transform::from_scale(k, k));
            pm.fill_path(&path, &Paint { shader, anti_alias: true, ..Default::default() }, FillRule::Winding, at, None);
        }
    }
    resvg::render(&tree, Transform::from_scale(s, s), &mut pm.as_mut());
    Some(pm)
}

/// The face as an SVG document, `FACE_PAD` larger on every side than the face itself, and what
/// goes under it (`Skin`: draw the two with `raster`). `rack_w`: in a rack, the rack's width (the
/// face fills it). `accents`: each group on a band of its kind's colour.
pub fn face(dev: &Device, jd: &[JackDef], counts: &[usize], split: &[bool], rack_w: Option<f64>, accents: bool) -> (String, Geometry, Skin) {
    let f = fin(dev.finish);
    let rack = rack_w.is_some();
    let ear = if rack { EAR } else { 0.0 };
    let x0 = ear + PAD;
    let geo = face_geometry(dev, jd, counts, rack_w, accents);
    let (w, h) = (geo.w, geo.h);
    let mut body = String::new();
    // (a face of several units is one panel: no groove between its units, 2026-11-27; it made it
    // look like two devices stacked)
    for (gi, g) in dev.groups.iter().enumerate() {
        let (x, gw, _, oy) = geo.groups[gi];
        // MONITOR, inside its host's frame: a dotted rule, and under its jacks their name
        if let Some(hg) = host(dev, gi) {
            // the rule halfway between the input's jacks (their mult's outline) and MONITOR's
            let mut ends = jd.iter().enumerate().filter(|(_, j)| j.group == hg).map(|(i, _)| geo.jacks[i].0 + (slots(counts[i]) - 1) as f64 * JP + 21.0);
            let end = ends.next().map_or(x, |e| ends.fold(e, f64::max));
            let start = if stacked(dev, gi, jd) {
                x + 15.0
            } else {
                jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| geo.jacks[i].0 - 21.0).fold(f64::MAX, f64::min)
            };
            let rx = if start < f64::MAX && start > end { (end + start) / 2.0 } else { x + 2.0 };
            // (printed, as the tags are, dashed: what it copies is the input's own sound)
            let mut s = format!(r#"<path d="M{rx},{} V{}" stroke="{}" stroke-width=".9" stroke-dasharray="4 3"/>"#, if accents { RULE.0 } else { 34.0 }, RULE.1, f.print);
            for (i, _) in jd.iter().enumerate().filter(|(_, j)| j.group == gi) {
                let k = slots(counts[i]);
                let (jx, jy) = (geo.jacks[i].0, geo.jacks[i].1 - oy);
                let split = stacked(dev, gi, jd);
                let hm = if split { 32.0 } else { 42.0 };
                if k > 1 {
                    let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{hm}" rx="{}" fill="rgba(0,0,0,.08)" stroke="{}" stroke-width=".9" opacity=".5"/>"#, jx - hm / 2.0, jy - hm / 2.0, (k - 1) as f64 * JP + hm, hm / 2.0, f.print);
                }
                // split with its input: its rows' tags at their left, MON L, MON R
                if split {
                    let t = format!("MON {}", jd[i].label);
                    s.push_str(&port_tag(jx - 21.0 - (SMALL.width(&t) + 10.0) / 2.0, jy, &t, &f));
                    continue;
                }
                // what it is, under it (ADR 0014); several (5.1): each its channel in the tag
                let cx = jx + (k - 1) as f64 * JP / 2.0;
                s.push_str(&port_tag(cx, KEY_ROW, &jack_tag(dev, gi, jd, i), &f));
            }
            if oy != 0.0 {
                s = format!(r#"<g transform="translate(0,{oy})">{s}</g>"#);
            }
            body.push_str(&s);
            continue;
        }
        // drawn as in the top unit, then moved down to its own
        let at = body.len();
        let acc = accents.then(|| kind_colour(g.kind));
        let ink = if acc.is_some() { band_ink(g.kind) } else { f.print };
        // a frame is a node (ADR 0014): its first section draws the frame round them all, the band
        // in its block's colour, the name once; the others a dotted rule before them
        let secs = sections(dev, gi);
        if secs.first() == Some(&gi) {
            let (fx, fw) = match secs.last() {
                Some(&l) if l != gi => {
                    let (lx, lw, _, _) = geo.groups[l];
                    (x, lx + lw - x)
                }
                _ => (x, gw),
            };
            let _ = write!(
                body,
                r#"<rect x="{fx}" y="18" width="{fw}" height="92" rx="6" fill="none" stroke="rgba(0,0,0,.3)"/><path d="M{},110.6 h{}" stroke="rgba(255,255,255,.14)"/>"#,
                fx + 6.0,
                fw - 12.0,
            );
            if let Some(c) = acc {
                let _ = write!(
                    body,
                    r#"<path d="M{fx},36 V24 a6,6 0 0 1 6,-6 H{} a6,6 0 0 1 6,6 V36 Z" fill="{c}"/><path d="M{fx},36.5 H{}" stroke="rgba(0,0,0,.35)"/><rect x="{}" y="18.6" width="{}" height="90.8" rx="5.6" fill="none" stroke="{c}" stroke-width="1.2" opacity=".9"/>"#,
                    fx + fw - 6.0,
                    fx + fw,
                    fx + 0.6,
                    fw - 1.2
                );
            }
            // the name at the left, after SPLIT when that is on the band: a click on it opens ENGINE
            // with this frame's block (`geo.graph`)
            let nx = fx + 9.0 + if splits(dev, gi) && !split_below(dev, gi) { split_key_w() + 4.0 } else { 0.0 };
            body.push_str(&SECTION.text(nx, 31.0, ink, "start", &name(dev, gi)));
        } else {
            // (between its sections, IN and OUT: in the frame's own colour, as its edge and the
            // rule before its knobs)
            let _ = write!(body, r#"<path d="M{x},{} V{}" stroke="{}" stroke-width="1.2" stroke-opacity=".9"/>"#, RULE.0, RULE.1, acc.unwrap_or(f.print));
        }
        // under its jacks, what each port is: IN, OUT, outlined (ADR 0014); a split group's
        // rows IN L, IN R at their left; a MIDI client's name, or a channel, under the tag
        let word = if g.dir == Dir::In { "IN" } else { "OUT" };
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
            // the jacks themselves are pictures of their own, laid over the face (see `jack_pic`)
            if rows {
                let t = format!("{word} {}", j.label);
                body.push_str(&port_tag(jx - 21.0 - (SMALL.width(&t) + 10.0) / 2.0, jy, &t, &f));
            } else {
                let cx = jx + (k - 1) as f64 * JP / 2.0;
                // in one row with the keys, below a mult's outline (21 under the jacks' centre); a
                // MIDI client's name, or a channel, in the tag (IN · HYDROGEN, OUT · OUT5)
                body.push_str(&port_tag(cx, KEY_ROW, &jack_tag(dev, gi, jd, i), &f));
            }
        }
        if oy != 0.0 {
            body.insert_str(at, &format!(r#"<g transform="translate(0,{oy})">"#));
            body.push_str("</g>");
        }
    }
    // a frame's SPLIT: a dark key on the band (or a lit one in the row: `mix_pic`); JOIN while its
    // pairs are split
    for (gi, [bx, by, bw, bh]) in &geo.switches {
        // in the row of keys: drawn with MUTE over the knobs (`mix_pic`), which is drawn again
        // as it is pressed
        if split_below(dev, *gi) {
            continue;
        }
        let on = sections(dev, *gi).iter().find(|s| dev.groups[**s].stereo).and_then(|s| split.get(*s)).copied().unwrap_or(false);
        let _ = write!(
            body,
            r##"<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="2" fill="#1b1c1e" stroke="rgba(0,0,0,.45)"/>{}"##,
            SMALL.text(bx + bw / 2.0, by + bh / 2.0 + 3.0, "#e6e7e8", "middle", if on { "JOIN" } else { "SPLIT" })
        );
    }
    for (gi, m) in &geo.mix {
        // the rule before its knobs in the frame's own colour, as its edge (the print without bands)
        let edge = if accents { kind_colour(dev.groups[*gi].kind) } else { f.print };
        body.push_str(&mix_panel(m, f.print, accents, edge));
    }
    // every audio frame's meter: SIG on its heading, the dark LEDs (lit ones are layers of their own)
    for &(gi, mx, oy, cols) in &geo.meters {
        let ink = if accents { band_ink(dev.groups[gi].kind) } else { f.print };
        // (a pair's band has its name across it: no SIG)
        let sig = if sections(dev, gi).len() > 1 { String::new() } else { SMALL.text(mx + 7.0, 31.0, ink, "middle", "SIG") };
        let _ = write!(body, r#"<g transform="translate(0,{oy})">{sig}{}</g>"#, Leds::strip(mx, 0.0, cols, accents).dark());
    }
    // the vents: between a 1U device's inputs and outputs in a rack, and wherever a multi-unit
    // face has nothing to show (docs/design/vents, variant 1)
    let grille = honeycomb(&geo, if rack { RACK_U } else { DEV_H });

    let r = if rack { 3.0 } else { 12.0 };
    let title = format!(r#"<text x="{x0}" y="{TITLE_Y}" font-size="{TITLE}">{}</text>"#, esc(&fit_title(&dev.name, geo.title_w)));
    // HIDE: a small key at the left of the row under the LCD, an eye struck through; not there
    // while the device has cords (only an unused one can be hidden: `face_geometry`)
    let hide = if geo.hide[2] == 0.0 {
        String::new()
    } else {
        let [hx, hy, hw, hh] = geo.hide;
        let (ex, ey) = (hx + hw / 2.0, hy + hh / 2.0);
        format!(
            r##"<rect x="{hx}" y="{hy}" width="{hw}" height="{hh}" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/><path d="M{},{ey} q6,-6 12,0 q-6,6 -12,0z" fill="none" stroke="#e6e7e8" stroke-width="1.1"/><circle cx="{ex}" cy="{ey}" r="1.7" fill="#e6e7e8"/><path d="M{},{} L{},{}" stroke="#1b1c1e" stroke-width="2.6"/><path d="M{},{} L{},{}" stroke="#e6e7e8" stroke-width="1.1"/>"##,
            ex - 6.0,
            ex - 5.5, ey + 4.5, ex + 5.5, ey - 4.5,
            ex - 5.5, ey + 4.5, ex + 5.5, ey - 4.5
        )
    };
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
    // in the rack the screws go through the face into the rails' holes, and are as big as those;
    // on the desk a face has none (nothing to screw it to; they sat over its name)
    let screws: String = if !rack {
        String::new()
    } else {
        let (inset, sy, big) = (RACK_SCREW_X, RACK_SCREW_Y, RACK_SCREW_R / 5.2);
        [(inset, sy), (w - inset, sy), (inset, h - sy), (w - inset, h - sy)]
            .iter()
            .enumerate()
            .map(|(i, (sx, sy))| {
                // the head lit from the top left whichever way it turned; its cross turned, its
                // recess lit on the side away from the light
                let a = (i * 37 + dev.name.len() * 11) % 90;
                format!(
                    r##"<g transform="translate({sx},{sy}) scale({big})"><use href="#screwHead"/><use href="#screwCross" fill="#fff" fill-opacity=".55" transform="translate(.45,.45) rotate({a})"/><use href="#screwCross" fill="#2a2b2f" transform="rotate({a})"/></g>"##
                )
            })
            .collect::<String>()
            // a line printed down each side from one screw to the other, a little room round
            // their heads (the idea of a border round the face, its sides only: docs/ROADMAP.md)
            + &{
                let (y0, y1) = (sy + RACK_SCREW_R + 3.0, h - sy - RACK_SCREW_R - 3.0);
                format!(r#"<path d="M{inset},{y0} V{y1} M{},{y0} V{y1}" stroke="{}" stroke-width=".9"/>"#, w - inset, f.print)
            }
    };
    // paint: solid base and its texture, put under the drawing (`Skin`)
    let under = Skin { w, h, r, base: hex(f.base), noise: f.noise as f32 };
    let (dw, dh) = (w + 2.0 * FACE_PAD, h + 2.0 * FACE_PAD);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}" font-family="Noto Sans" font-weight="bold">
{defs}
<g transform="translate({FACE_PAD},{FACE_PAD})">
<g>
<rect x="{a}" y=".75" width="{b}" height="{c}" rx="{r}" fill="none" stroke="rgba(255,255,255,.2)"/>
<rect x="0" y="0" width="{w}" height="{h}" rx="{r}" fill="none" stroke="rgba(0,0,0,.65)"/>
</g>
{screws}
<g font-weight="900" fill="{print}">{title}</g>
<rect x="{lx}" y="{fy}" width="{lw}" height="{fh}" rx="4" fill="#111214"/>
<rect x="{ix}" y="{LCD_Y}" width="{iw}" height="{lh}" rx="2" fill="url(#gLcd)"/>
<rect x="{ix}" y="{LCD_Y}" width="{iw}" height="{lh}" rx="2" fill="none" stroke="rgba(110,45,0,.35)" stroke-width="3"/>
{key}{hide}
{body}{grille}
</g>
</svg>"##,
        defs = defs(),
        a = 0.75,
        b = w - 1.5,
        c = h - 1.5,
        print = f.print,
        lh = LCD_H,
        fy = LCD_Y - 3.0,
        fh = LCD_H + 6.0,
        lx = x0 - 1.5,
        lw = IDW + 3.0,
        ix = x0 + 1.5,
        iw = IDW - 3.0,
    );
    (svg, geo, under)
}

/// A port's tag under its jacks, centred on (cx, cy): IN, OUT, MONITOR alike, outlined in the
/// paint's print (a filled one passed for a key, as SPLIT and MUTE).
fn port_tag(cx: f64, cy: f64, t: &str, f: &Fin) -> String {
    let w = SMALL.width(t) + 10.0;
    // as high as the keys (SPLIT, MUTE), so under the jacks they make one row with them
    let (x, y) = (cx - w / 2.0, cy - KEY_H / 2.0);
    format!(r#"<rect x="{x}" y="{y}" width="{w}" height="{KEY_H}" rx="2.5" fill="none" stroke="{}" stroke-width=".9"/>{}"#, f.print, SMALL.text(cx, cy + 2.8, f.print, "middle", t))
}

/// The row of a frame's keys (SPLIT, MUTE: `oy + 89`, `KEY_H` high): its middle, where the tags
/// under the jacks go too.
const KEY_ROW: f64 = 89.0 + KEY_H / 2.0;

/// The LCD's three lines of text, as a document the size of the LCD's text area: the top one bold.
pub fn lcd(lines: [&str; 3]) -> String {
    let (h, y1, step) = (LCD_H, 13.0, 13.5);
    let mut out = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{LCD_W}" height="{h}" viewBox="0 0 {LCD_W} {h}" font-family="Noto Sans Mono" fill="#3a2408" xml:space="preserve">"##);
    for (i, l) in lines.iter().enumerate() {
        let bold = if i == 0 { r#" font-weight="bold""# } else { "" };
        out += &format!(r#"<text x="7.5" y="{}" font-size="10.5"{bold}>{}</text>"#, y1 + i as f64 * step, esc(l));
    }
    out + "</svg>"
}

/// A line too long for the LCD, scrolling, pausing at the start.
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

/// The shared pieces of the faces' SVG: the LCD's glow, the screws, and the dark of a knob's well
/// (the paint and its noise go under the drawing, `raster`; the jacks are pictures of their own,
/// `jack_defs`; the keys lit in `mix_pic`, with their own).
fn defs() -> &'static str {
    static DEFS: OnceLock<String> = OnceLock::new();
    DEFS.get_or_init(|| {
        format!(
            r##"<defs>
<linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e38c2a"/></linearGradient>
<radialGradient id="gScrew" cx=".38" cy=".32" r=".75"><stop offset="0" stop-color="#f4f5f6"/><stop offset=".45" stop-color="#b7babe"/><stop offset=".85" stop-color="#6d7075"/><stop offset="1" stop-color="#55585d"/></radialGradient>
<g id="screwHead"><circle r="5.5" fill="#000" opacity=".45" transform="translate(.7,1.2)"/><circle r="5.2" fill="url(#gScrew)" stroke="#2c2d30" stroke-width=".6"/><circle r="4.3" fill="none" stroke="#fff" stroke-opacity=".28" stroke-width=".5"/></g>
<path id="screwCross" d="M-3,-.75 H-.75 V-3 H.75 V-.75 H3 V.75 H.75 V3 H-.75 V.75 H-3 Z"/>
{HOLE}</defs>"##
        )
    })
}

/// Only what a jack's picture needs (without the faces' textures, a hundred kilobytes and more
/// each picture would parse again).
fn jack_defs() -> &'static str {
    static DEFS: OnceLock<String> = OnceLock::new();
    DEFS.get_or_init(|| format!("<defs>\n{}</defs>", jack_parts()))
}

/// The dark of a hole (BAL's well).
const HOLE: &str = r##"<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>"##;

/// What is inside a jack's coloured bushing (`jack_pic_ring`): a black bezel, the jack's nickel and
/// the deep hole.
fn jack_parts() -> String {
    r##"<linearGradient id="gNickel" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff"/><stop offset=".45" stop-color="#b9bcc1"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gBezel" cx=".45" cy=".35" r=".8"><stop offset="0" stop-color="#2c2d30"/><stop offset="1" stop-color="#0c0c0d"/></radialGradient>
<radialGradient id="gDeep" cx=".5" cy=".55" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".8" stop-color="#050506"/><stop offset="1" stop-color="#262729"/></radialGradient>
<g id="jack"><circle r="10.4" fill="url(#gBezel)"/><circle r="8.2" fill="url(#gNickel)"/><circle r="6.2" fill="url(#gDeep)"/></g>
"##
    .to_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A surround node's MASTER starts a unit of its own, from the bottom up and from the top
    /// down; the frames after it stay beside it.
    #[test]
    fn a_master_starts_a_unit() {
        let blocks = vec![vec![0], vec![1], vec![2]];
        let fresh = [false, true, false];
        let width = |_: &[usize]| 100.0;
        assert_eq!(from_below(&blocks, &fresh, &width, 1000.0, 1000.0), Some(vec![vec![0], vec![1, 2]]));
        assert_eq!(from_above(&blocks, &fresh, &width, 1000.0, 1000.0), vec![vec![0], vec![1, 2]]);
        // first, it stays beside the name and screen
        assert_eq!(from_above(&blocks, &[true, false, false], &width, 1000.0, 1000.0), vec![vec![0, 1, 2]]);
    }

    /// A narrow free span keeps its vents when a wide one runs on from it in the unit over or
    /// under it; alone, it is plain metal.
    #[test]
    fn a_narrow_span_keeps_its_vents_beside_a_field() {
        let mut v = vec![[900.0, 940.0, 0.0], [700.0, 940.0, RACK_U], [100.0, 140.0, 2.0 * RACK_U]];
        keep_vents(&mut v, RACK_U);
        assert_eq!(v, vec![[900.0, 940.0, 0.0], [700.0, 940.0, RACK_U]]);
    }
    use crate::pw::{Avail, Event, Kind, Profile, Props, Route};

    /// A card's nodes (id, class, node.nick, card.profile.device), and its routes in use
    /// (device, port name) when `routes`: its frames' names.
    fn card_frames(nodes: &[(u32, &str, &str, i32)], routes: Option<&[(i32, &str)]>) -> Vec<String> {
        let mut g = crate::graph::Graph::default();
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
        g.apply(&Event::Added { id: 1, kind: Kind::Device, props: props(&[("device.name", "alsa_card.x"), ("device.description", "Some HD Audio Controller"), ("media.class", "Audio/Device"), ("device.api", "alsa")]) });
        if let Some(rs) = routes {
            g.apply(&Event::Profiles { id: 1, list: vec![Profile { index: 1, name: "hifi".into(), description: "HiFi".into(), available: Avail::Yes, priority: 1 }], current: Some(1) });
            let route = |i: usize, (dev, name): (i32, &str)| Route { index: i as u32, output: name.contains("Out") || name.contains("output"), device: Some(dev), name: name.into(), description: name.into(), available: Avail::Yes, devices: vec![dev], profiles: vec![1], volumes: Some(vec![1.0, 1.0]), mute: false, latency_ns: 0 };
            let all: Vec<Route> = rs.iter().enumerate().map(|(i, r)| route(i, *r)).collect();
            g.apply(&Event::Routes { id: 1, all: all.clone(), active: all });
        }
        for &(id, class, nick, dev) in nodes {
            let dev = dev.to_string();
            let p = props(&[("media.class", class), ("node.name", &format!("alsa.{id}")), ("node.nick", nick), ("device.id", "1"), ("card.profile.device", &dev), ("factory.name", "api.alsa.pcm.sink")]);
            g.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
            g.apply(&Event::Info { id, props: p, state: Some("suspended") });
            let (dir, name) = if class == "Audio/Sink" { ("in", "playback") } else { ("out", "capture") };
            for (j, ch) in ["FL", "FR"].iter().enumerate() {
                let pp = props(&[("node.id", &id.to_string()), ("port.direction", dir), ("port.name", &format!("{name}_{ch}")), ("format.dsp", "32 bit float mono audio"), ("audio.channel", ch)]);
                g.apply(&Event::Added { id: id * 100 + j as u32, kind: Kind::Port, props: pp });
            }
        }
        let devs = g.devices();
        let d = devs.iter().find(|d| d.finish == Finish::Aluminium).expect("the card");
        (0..d.groups.len()).filter(|&i| matches!(d.groups[i].role, "PLAYBACK" | "CAPTURE")).map(|i| name(d, i)).collect()
    }

    /// A card's frame: the port in use as a word while no other frame has it; else PipeWire's
    /// short name for the node, when it tells the frames apart; else SOUND OUT / SOUND IN (ADR 0020).
    #[test]
    fn a_cards_frames_are_named_by_port_then_nick() {
        // a laptop's card in its HiFi profile (Fedora's Meteor Lake): a node a port
        let hifi = [(62, "Audio/Sink", "Headphones", 0), (63, "Audio/Sink", "HDMI 3", 5), (82, "Audio/Sink", "HDMI 1", 3), (83, "Audio/Sink", "HDMI 2", 4), (80, "Audio/Source", "Digital Microphone", 6), (81, "Audio/Source", "Headphones Mic", 7)];
        let routes = [(0, "[Out] Headphones"), (5, "[Out] HDMI3"), (3, "[Out] HDMI1"), (4, "[Out] HDMI2"), (6, "[In] Mic1"), (7, "[In] Mic2")];
        assert_eq!(card_frames(&hifi, Some(&routes)), ["HEADPHONES", "HDMI 3", "HDMI 1", "HDMI 2", "DIGITAL MICROPHONE", "HEADPHONES MIC"]);
        // the same with no routes known (a report saved before STATE)
        assert_eq!(card_frames(&hifi, None), ["HEADPHONES", "HDMI 3", "HDMI 1", "HDMI 2", "DIGITAL MICROPHONE", "HEADPHONES MIC"]);
        // a desktop card: its sink and source share the codec's name, which tells nothing
        let acp = [(51, "Audio/Sink", "ALC287 Analog", 1), (45, "Audio/Source", "ALC287 Analog", 2)];
        assert_eq!(card_frames(&acp, Some(&[(1, "analog-output-speaker"), (2, "analog-input-internal-mic")])), ["SPEAKERS", "BUILT-IN MIC"]);
        assert_eq!(card_frames(&acp, Some(&[])), ["SOUND OUT", "SOUND IN"]);
        // two HDMI outputs with no nick: numbered
        let two = [(70, "Audio/Sink", "", 1), (71, "Audio/Sink", "", 2)];
        assert_eq!(card_frames(&two, Some(&[(1, "hdmi-output-0"), (2, "hdmi-output-1")])), ["HDMI", "HDMI 2"]);
    }

    /// A long name is cut to fit, its number (the second of two programs of one name) kept.
    #[test]
    fn a_cut_name_keeps_its_number() {
        let t = fit_title("VLC media player (LibVLC 4.0.0-dev) 2", IDW);
        assert!(t.starts_with("VLC MEDIA") && t.ends_with("… 2"), "{t}");
        assert_eq!(fit_title("BLE MIDI 1", IDW), "BLE MIDI 1");
    }

    /// A jack's picture, drawn with only the jacks' defs, is the same as with the faces' too.
    #[test]
    fn jack_pictures_need_only_the_jacks_defs() {
        let opt = text::svg_options();
        for c in ACCENTS {
            let small = jack_pic_ring(c);
            let full = small.replacen(jack_defs(), &format!("{}{}", defs(), jack_defs()), 1);
            assert_ne!(small, full);
            let (a, b) = (super::super::scene::rasterize(&opt, &small, 2.0).unwrap(), super::super::scene::rasterize(&opt, &full, 2.0).unwrap());
            assert!(a.data().iter().any(|&v| v > 0), "a {c} jack draws nothing");
            assert_eq!(a.data(), b.data(), "a {c} jack draws differently without the faces' defs");
        }
    }
}
