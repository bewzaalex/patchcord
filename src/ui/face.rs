//! The front panel of a device as an SVG document (ported from docs/design/mockup).
//!
//! The name and LCD block on the left, then the port groups: inputs on the left, outputs on the
//! right (in a rack the outputs sit against the signal meter, with a grille between). Each port
//! is a mult: one jack per cord plugged in, plus one free jack.

use std::fmt::Write;
use std::sync::OnceLock;

use resvg::tiny_skia::{Color, FillRule, FilterQuality, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pattern, Pixmap, Point, SpreadMode, Transform};
use resvg::usvg;

use super::print::{SECTION, SMALL, SWITCH};
use super::text::{self, esc};
use crate::graph::{Device, Dir, Finish, Media, VOL_MAX};

pub const DEV_H: f64 = 128.0;
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
/// characters the LCD shows on one line
pub const LCD_CH: usize = 27;
/// where the LCD's text area is, relative to the face (without rack ears): right under the name,
/// down to the row of keys; the same on every face
pub const LCD_X: f64 = PAD + 1.5;
pub const LCD_Y: f64 = 43.0;
pub const LCD_W: f64 = IDW - 3.0;
pub const LCD_H: f64 = 57.0;
/// the row of keys under the LCD, on every face: HIDE at its left, then a card's PROFILE and PORT
/// sharing the rest evenly (the row a little in from the LCD's edges, clear of the desk's screws)
const KEY_Y: f64 = 107.0;
const KEY_H: f64 = 14.0;
const KEY_IN: f64 = 4.0;
const HIDE_W: f64 = 22.0;
const KEY_GAP: f64 = 6.0;

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
    /// a card's PROFILE key (x, y, w, h)
    pub profile: Option<[f64; 4]>,
    /// beside it, the PORT key of a card whose output or input has ports to choose from
    pub port: Option<[f64; 4]>,
    /// the HIDE key, right of the name (x, y, w, h)
    pub hide: [f64; 4],
    /// the controls of each app stream, by group
    pub mix: Vec<(usize, MixGeo)>,
    /// each audio frame's meter: its group, its left edge, how far down its unit starts, and
    /// its columns (two for stereo)
    pub meters: Vec<(usize, f64, f64, usize)>,
    /// each frame's GRAPH key, on its band before its name: its node, and (x, y, w, h)
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

/// The groups' colours (ADR 0014: a port's, its jack's ring here and its dot in ENGINE): MIDI,
/// MONITOR (what an output plays, to record it), PLAYBACK (a sink's inputs: speakers, headphones,
/// HDMI), CAPTURE (a source's outputs: a microphone), anything else's OUTPUT and INPUT, VIDEO.
/// (A frame's own colour is its block's kind: `kind_colour`.)
/// One lightness and one chroma (OKLCH .52 / .115), the hues spread round the circle (340, 290,
/// 195, 245, 140, 85°): none shouts over another, each is told apart. OUTPUT is a warm brown
/// (`#7e5a42`, less chroma): rust read as ENGINE's error red (docs/design/apps/palette.py, B).
/// The eighth, OTHER (a port of a format Patchcord doesn't know, or of none), is NO CLASS's sand:
/// nothing is said of it.
pub const ACCENTS: [&str; 8] = ["#934c7e", "#695ca6", "#007c7d", "#206ea6", "#7e5a42", "#427935", "#876200", "#cba553"];

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

/// A frame's colour: its block's kind (`graph::block_kind`, ADR 0014), the same as the block's in
/// ENGINE. A kind with a main group of ports has that group's colour (a sink's PLAYBACK, a
/// source's CAPTURE…); DUPLEX and NO CLASS have their own (docs/design/apps/kinds.py, variant 4).
pub fn kind_colour(kind: &str) -> &'static str {
    match kind {
        "SINK" => "#007c7d",
        "SOURCE" => "#206ea6",
        "PLAYBACK STREAM" => "#7e5a42",
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

/// GRAPH, on a frame's band before its name: the node's number (#180). Its width.
fn graph_key_w(dev: &Device, gi: usize) -> f64 {
    let id = dev.groups[gi].ports.first().map_or(0, |p| p.node);
    10.0 + SMALL.width(&format!("#{id}"))
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
}

fn fin(f: Finish) -> Fin {
    match f {
        // ADR 0014 (docs/design/apps/paints.py, variant D): one paint a kind of device, light,
        // dark and in between, so none passes for another
        Finish::Aluminium => Fin { base: "#c3c6ca", paint: true, noise: 0.0, print: "#1d1e20" },
        Finish::Black => Fin { base: "#232427", paint: false, noise: 0.9, print: "#d5d7d9" },
        Finish::Olive => Fin { base: "#66683f", paint: false, noise: 0.7, print: "#f1f1e6" },
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
/// In a split stereo group, the first jack of each row: right of its tag (IN L, OUT R).
const ROW_X: f64 = 62.0;

/// What a frame is called on the face. An app's (and an effect's) say which way the sound goes:
/// INPUT, OUTPUT. A sound card's say what it is, from the port in use, so they follow the PORT
/// key: SPEAKERS, HEADPHONES, HDMI, BUILT-IN MIC, MIC; a card that tells nothing, SOUND OUT and
/// SOUND IN. A monitor is MONITOR: a copy of what plays, to record or listen to it.
/// (PipeWire's own words, PLAYBACK, CAPTURE and MONITOR, mean little to someone with a headset.)
pub fn name(dev: &Device, gi: usize) -> String {
    let g = &dev.groups[gi];
    // the port in use; with none (HDMI with nothing plugged in), the one it has, or the word all
    // its ports share
    let active = g.mix.as_ref().and_then(|m| m.route).and_then(|r| g.ports_to.iter().find(|p| p.index == r.index));
    let words: Vec<&str> = g.ports_to.iter().filter_map(|p| port_word(&p.name, g.dir == Dir::Out)).collect();
    if let Some(w) = active.and_then(|p| port_word(&p.name, g.dir == Dir::Out)).or(words.first().copied().filter(|w| words.iter().all(|x| x == w))) {
        return w.to_owned();
    }
    // a name of its own: what its program named its node, a MIDI client's
    if g.label != g.role {
        return g.label.clone();
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
    dev.groups.iter().position(|o| o.dir == Dir::In && o.mix.is_some() && o.ports.first().map(|p| p.node) == g.ports.first().map(|p| p.node))
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

/// A split MONITOR's first jack in each row, from the rule before it: room for its tag (MONITOR L).
fn mon_row_x() -> f64 {
    9.0 + SMALL.width("MONITOR L") + 10.0 + 6.0 + 21.0
}

/// A stereo group whose switch is on SPLIT: its L and R stand one above the other.
fn stacked(dev: &Device, gi: usize, jd: &[JackDef]) -> bool {
    dev.groups[gi].stereo && jd.iter().filter(|j| j.group == gi).count() == 2
}

/// A group's whole width: its jacks and its controls.
fn group_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    jacks_w(dev, gi, jd, counts) + mix_w(dev, gi)
}

/// A port's tag under its jacks: IN or OUT, and a MIDI bridge's client in it (IN · HYDROGEN).
/// The tag under jack `i` of group `gi`: what the port is (IN, OUT, MONITOR in its input's
/// frame) and, inside it, a MIDI client's name (IN · HYDROGEN) or, in a group of several jacks
/// side by side, its channel (OUT · OUT5, MONITOR · FC).
fn jack_tag(dev: &Device, gi: usize, jd: &[JackDef], i: usize) -> String {
    let g = &dev.groups[gi];
    let word = if host(dev, gi).is_some() { "MONITOR" } else if g.dir == Dir::In { "IN" } else { "OUT" };
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
    // GRAPH and the name span the MONITOR too: room for them is found over both
    let rec = rec_w(dev, gi, jd, counts);
    // room on the band for GRAPH, SPLIT and the name (a frame of several sections has them once,
    // across them all)
    let n = sections(dev, gi).len().max(1) as f64;
    let keys = graph_key_w(dev, gi) + if splits(dev, gi) { split_key_w() + 4.0 } else { 0.0 };
    let named = ((label_w(&name(dev, gi)) + keys + 34.0) / n).ceil();
    (jacks + rec).max(named)
}

/// A frame's one SPLIT key, on its band right of GRAPH: as wide whether it says SPLIT or JOIN.
fn split_key_w() -> f64 {
    10.0 + SMALL.width("SPLIT").max(SMALL.width("JOIN"))
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
                Some(r) if r.left.len() + b.len() <= PER_ROW => r.left.extend(b),
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
            let first = |g: usize| (0..dev.groups.len()).position(|o| node_of(dev, o) == node_of(dev, g)).unwrap_or(g);
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
/// name over a row of frames, not frames stepping down. (All of a unit's frames are in `left`.)
fn rack_rows(dev: &Device, jd: &[JackDef], counts: &[usize], room: f64, more: f64) -> Vec<Row> {
    let (ins, outs) = sides(dev);
    let all: Vec<usize> = ins.into_iter().chain(outs).collect();
    // a node's frames stay on one unit, side by side
    let blocks = blocks(dev, &all);
    for n in 1..=all.len().max(1) {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut used = vec![0.0; n];
        let mut k = n - 1;
        let mut whole = true;
        for b in blocks.iter().rev() {
            let gw = b.iter().map(|g| group_w(dev, *g, jd, counts) + gap_after(dev, *g)).sum::<f64>() - gap_after(dev, b[b.len() - 1]);
            loop {
                let cap = if k == 0 { room } else { more };
                let need = if rows[k].is_empty() { gw } else { used[k] + 10.0 + gw };
                if need <= cap + 1e-6 {
                    rows[k].splice(0..0, b.iter().copied());
                    used[k] = need;
                    break;
                }
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
            return rows.into_iter().map(|left| Row { left, right: Vec::new() }).collect();
        }
    }
    // (nothing fits however many units: a frame to a unit, under the name and screen)
    std::iter::once(Vec::new()).chain(all.into_iter().map(|g| vec![g])).map(|left| Row { left, right: Vec::new() }).collect()
}

/// Groups in a row, in blocks: a node's groups together, one frame.
fn blocks(dev: &Device, gs: &[usize]) -> Vec<Vec<usize>> {
    let node = |g: usize| dev.groups[g].ports.first().map(|p| p.node);
    let mut out: Vec<Vec<usize>> = Vec::new();
    for &g in gs {
        match out.last_mut() {
            Some(b) if node(b[0]) == node(g) => b.push(g),
            _ => out.push(vec![g]),
        }
    }
    out
}

fn node_of(dev: &Device, gi: usize) -> Option<u32> {
    dev.groups[gi].ports.first().map(|p| p.node)
}

/// A frame is a node (ADR 0014): group `gi`'s frame's sections, its node's groups side by side
/// (its inputs, then its outputs; a MONITOR inside the input it copies, not a section).
pub fn sections(dev: &Device, gi: usize) -> Vec<usize> {
    let node = node_of(dev, gi);
    (0..dev.groups.len()).filter(|o| node_of(dev, *o) == node && host(dev, *o).is_none()).collect()
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
    let hide = [ear + PAD + KEY_IN, KEY_Y, HIDE_W, KEY_H];
    let (kx, room) = (hide[0] + HIDE_W + KEY_GAP, IDW - 2.0 * KEY_IN - HIDE_W - KEY_GAP);
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
    let mut geo = Geometry { w, h, jacks: vec![(0.0, 0.0); jd.len()], switches: Vec::new(), hide, left: ear, groups: vec![(0.0, 0.0, 0.0, 0.0); dev.groups.len()], grille: None, profile, port, mix: Vec::new(), meters: Vec::new(), vents: Vec::new(), graph: Vec::new() };
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
        if g.mix.is_some() {
            let (ox, stereo) = (x + jw, g.stereo);
            let w = if stereo { 96.0 } else { 58.0 };
            let mute = [ox + 7.0, oy + 89.0, w - 14.0, 14.0];
            geo.mix.push((gi, MixGeo {
                x: ox,
                y: oy,
                w,
                input: g.mix.as_ref().is_some_and(|m| m.gain),
                knob: (if stereo { ox + 33.0 } else { ox + w / 2.0 }, oy + 60.0),
                bal: stereo.then_some((ox + 75.0, oy + 56.0)),
                mute,
            }));
        }
        // its meter: after its knobs, or (without) right of its jacks
        if metered(g) && host(dev, gi).is_none() {
            let mx = if g.mix.is_some() { x + jw + if g.stereo { 96.0 } else { 58.0 } + 1.0 } else { x + jw - 2.0 };
            // a column for each of a stereo pair's channels, and for each of two channels that don't
            // say which they are (a JACK app's out1, out2)
            let two = g.stereo || (g.ports.len() == 2 && g.ports.iter().all(|p| p.media == Media::Audio));
            geo.meters.push((gi, mx, oy, if two { 2 } else { 1 }));
        }
        geo.groups[gi] = (x, gw, jw, oy);
        // a frame's first section: its GRAPH key on the band, and right of it the frame's one
        // SPLIT (every stereo pair in the frame, a MONITOR inside it too)
        if sections(dev, gi).first() == Some(&gi) {
            if let Some(node) = node_of(dev, gi) {
                geo.graph.push((node, [x + 6.0, oy + 21.0, graph_key_w(dev, gi), 12.0]));
            }
            if splits(dev, gi) {
                geo.switches.push((gi, [x + 6.0 + graph_key_w(dev, gi) + 4.0, oy + 21.0, split_key_w(), 12.0]));
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
            let oy = u as f64 * if rack { RACK_U } else { DEV_H };
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
        geo.vents.retain(|[a, b, _]| b - a > 50.0);
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
            let oy = u as f64 * DEV_H;
            for &(gi, c) in &at[u] {
                let x = if dev.groups[gi].dir == Dir::In { cx[c] } else { cx[c] + cw[c] - group_w(dev, gi, jd, counts) };
                place(gi, x, oy, &mut geo);
            }
            let (k, m) = (row.left.len(), row.right.len());
            let a = if k > 0 { cx[k - 1] + cw[k - 1] + 10.0 } else { x0 };
            let b = if m > 0 { cx[cw.len() - m] - 10.0 } else { right };
            if u > 0 && k == 0 {
                geo.vents.push([ear + PAD, b, oy]);
            } else {
                if u > 0 {
                    geo.vents.push([ear + PAD, ear + PAD + IDW, oy]);
                }
                geo.vents.push([a, b, oy]);
            }
        }
        geo.vents.retain(|[a, b, _]| b - a > 50.0);
        return geo;
    }
    // what takes sound in on the left, what sends it out on the right; in a rack against the
    // right edge
    let (ins, outs) = sides(dev);
    let span: f64 = outs.iter().map(|g| group_w(dev, *g, jd, counts) + 10.0).sum::<f64>();
    let mut x = ear + PAD + IDW + 14.0;
    for g in &ins {
        x = place(*g, x, 0.0, &mut geo);
    }
    let gx0 = x;
    // (in a rack, against the right edge)
    let mut ox = if rack { w - ear - PAD + 10.0 - span } else { x };
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

/// Half the size of a jack's picture: the nut and its shadow fit inside.
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

/// A jack with a ring in its group's colour: mono and MIDI one ring, stereo two (as the gold
/// ring marks a stereo jack without colours).
pub fn jack_pic_ring(kind: JackKind, color: &str) -> String {
    let s = 2.0 * PIC;
    let (base, rings) = match kind {
        JackKind::Mono => ("jack-mono", format!(r#"<circle cx="{PIC}" cy="{PIC}" r="10.1" fill="none" stroke="{color}" stroke-width="1.3"/>"#)),
        JackKind::Midi => ("jack-midi", format!(r#"<circle cx="{PIC}" cy="{PIC}" r="10.1" fill="none" stroke="{color}" stroke-width="1.3"/>"#)),
        JackKind::Stereo => ("jack-mono", format!(r#"<circle cx="{PIC}" cy="{PIC}" r="10.7" fill="none" stroke="{color}" stroke-width="1"/><circle cx="{PIC}" cy="{PIC}" r="9.3" fill="none" stroke="{color}" stroke-width="1"/>"#)),
    };
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">{}<use href="#{base}" x="{PIC}" y="{PIC}"/>{rings}</svg>"##, jack_defs())
}

/// Half the size of a cue mark's picture: the ring round a jack listened to, and the
/// headphones on it.
pub const CUE_PIC: f64 = 27.0;

/// A jack kept on (clicked in CUE mode): a cyan disc out to the ring, laid under the jack, so the
/// jack's nut lies on it with its shadow.
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
/// its paint's texture or its noise, its sheen. Drawn through the SVG these took most of a face's time
/// (a pattern sampled at every pixel of a face as wide as the rack).
#[derive(Clone, Copy, Debug)]
pub struct Skin {
    w: f64,
    h: f64,
    r: f64,
    base: [u8; 3],
    paint: bool,
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

/// The paint's texture and the noise, as pixels (as `defs` has them for the SVG).
fn textures() -> &'static (Option<Pixmap>, Option<Pixmap>) {
    static T: OnceLock<(Option<Pixmap>, Option<Pixmap>)> = OnceLock::new();
    T.get_or_init(|| (Pixmap::decode_png(include_bytes!("../../assets/paint.png")).ok(), Pixmap::decode_png(include_bytes!("../../assets/noise.png")).ok()))
}

/// A face's picture at `scale`: its `Skin` put straight onto the pixels (the base, then the
/// paint's texture in 80-unit tiles or the noise in 128-unit ones, as the SVG's patterns were,
/// and the sheen), then its SVG drawn over it.
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
        let (paint, noise) = textures();
        for (tex, tile, alpha) in [(paint.as_ref().filter(|_| skin.paint), 80.0, 1.0), (noise.as_ref().filter(|_| skin.noise > 0.0), 128.0, skin.noise)] {
            let Some(t) = tex else { continue };
            let k = tile / t.width() as f32;
            let shader = Pattern::new(t.as_ref(), SpreadMode::Repeat, FilterQuality::Bilinear, alpha, Transform::from_scale(k, k));
            pm.fill_path(&path, &Paint { shader, anti_alias: true, ..Default::default() }, FillRule::Winding, at, None);
        }
        // the sheen: light at the top, shade at the bottom (as `gSheen`)
        let stops = vec![GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 41)), GradientStop::new(0.35, Color::from_rgba8(255, 255, 255, 0)), GradientStop::new(1.0, Color::from_rgba8(0, 0, 0, 61))];
        if let Some(shader) = LinearGradient::new(Point::from_xy(0.0, 0.0), Point::from_xy(0.0, skin.h as f32), stops, SpreadMode::Pad, Transform::identity()) {
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
    // a unit more: a groove across the face parts it from the one above
    let step = if rack { RACK_U } else { DEV_H };
    let mut uy = step - if rack { RACK_GAP / 2.0 } else { 0.0 };
    while uy < h - 1.0 {
        let _ = write!(body, r#"<path d="M{a},{uy} H{b}" stroke="rgba(0,0,0,.55)" stroke-width="1.2"/><path d="M{a},{} H{b}" stroke="rgba(255,255,255,.08)"/>"#, uy + 1.2, a = ear + 8.0, b = w - ear - 8.0);
        uy += step;
    }
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
            let mut s = format!(r#"<path d="M{rx},{} V100" stroke="{}" stroke-opacity=".35" stroke-dasharray="2 3"/>"#, if accents { 44.0 } else { 34.0 }, f.print);
            for (i, _) in jd.iter().enumerate().filter(|(_, j)| j.group == gi) {
                let k = slots(counts[i]);
                let (jx, jy) = (geo.jacks[i].0, geo.jacks[i].1 - oy);
                let split = stacked(dev, gi, jd);
                let hm = if split { 32.0 } else { 42.0 };
                if k > 1 {
                    let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{hm}" rx="{}" fill="rgba(0,0,0,.08)" stroke="{}" stroke-width=".9" opacity=".5"/>"#, jx - hm / 2.0, jy - hm / 2.0, (k - 1) as f64 * JP + hm, hm / 2.0, f.print);
                }
                // split with its input: its rows' tags at their left, MONITOR L, MONITOR R
                if split {
                    let t = format!("MONITOR {}", jd[i].label);
                    s.push_str(&port_tag(jx - 21.0 - (SMALL.width(&t) + 10.0) / 2.0, jy, &t, &f));
                    continue;
                }
                // what it is, under it (ADR 0014); several (5.1): each its channel in the tag
                let cx = jx + (k - 1) as f64 * JP / 2.0;
                s.push_str(&port_tag(cx, KEY_ROW, &jack_tag(dev, gi, jd, i), &f));
            }
            if oy > 0.0 {
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
        // in its block's colour, GRAPH and the name once; the others a dotted rule before them
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
                r#"<rect x="{fx}" y="18" width="{fw}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="rgba(0,0,0,.3)"/><path d="M{},110.6 h{}" stroke="rgba(255,255,255,.14)"/>"#,
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
            // GRAPH: this frame's block in ENGINE; the node's number (#180), as ENGINE's block shows it
            let kw = graph_key_w(dev, gi);
            let (kx, ky) = (fx + 6.0, 21.0);
            let id = node_of(dev, gi).unwrap_or(0);
            let _ = write!(
                body,
                r##"<rect x="{kx}" y="{ky}" width="{kw}" height="12" rx="2" fill="#1b1c1e" stroke="rgba(0,0,0,.45)"/>{}"##,
                SMALL.text(kx + kw / 2.0, ky + 9.0, "#e6e7e8", "middle", &format!("#{id}"))
            );
            // the name at the left, after GRAPH and SPLIT
            let nx = kx + kw + 7.0 + if splits(dev, gi) { split_key_w() + 4.0 } else { 0.0 };
            body.push_str(&SECTION.text(nx, 31.0, ink, "start", &name(dev, gi)));
        } else {
            let _ = write!(body, r#"<path d="M{x},40 V104" stroke="{}" stroke-opacity=".35" stroke-dasharray="2 3"/>"#, f.print);
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
        if oy > 0.0 {
            body.insert_str(at, &format!(r#"<g transform="translate(0,{oy})">"#));
            body.push_str("</g>");
        }
    }
    // a frame's SPLIT: a key like GRAPH, on the band; JOIN while its pairs are split
    for (gi, [bx, by, bw, bh]) in &geo.switches {
        let on = sections(dev, *gi).iter().find(|s| dev.groups[**s].stereo).and_then(|s| split.get(*s)).copied().unwrap_or(false);
        let _ = write!(
            body,
            r##"<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="2" fill="#1b1c1e" stroke="rgba(0,0,0,.45)"/>{}"##,
            SMALL.text(bx + bw / 2.0, by + 9.0, "#e6e7e8", "middle", if on { "JOIN" } else { "SPLIT" })
        );
    }
    for (_, m) in &geo.mix {
        body.push_str(&mix_panel(m, f.print, accents));
    }
    // every audio frame's meter: SIG on its heading, the dark LEDs (lit ones are layers of their own)
    for &(gi, mx, oy, cols) in &geo.meters {
        let ink = if accents { band_ink(dev.groups[gi].kind) } else { f.print };
        // (a pair's band has its name across it: no SIG)
        let sig = if sections(dev, gi).len() > 1 { String::new() } else { SMALL.text(mx + 7.0, 31.0, ink, "middle", "SIG") };
        let _ = write!(body, r#"<g transform="translate(0,{oy})">{sig}{}</g>"#, Leds::strip(mx, 0.0, cols, accents).dark());
    }
    // the slots of a rack unit's vents: between a 1U device's inputs and outputs in a rack, and
    // wherever a multi-unit face has nothing to show
    let mut grille = String::new();
    for (gx0, gx1, oy) in geo.grille.map(|(a, b)| (a, b, 0.0)).into_iter().chain(geo.vents.iter().map(|v| (v[0], v[1], v[2]))) {
        let mut y = 30.0;
        while y <= 98.0 {
            let _ = write!(grille, r##"<rect x="{gx0}" y="{}" width="{}" height="4.2" rx="2.1" fill="#0b0c0d"/><path d="M{},{} h{}" stroke="rgba(255,255,255,.12)"/>"##, oy + y, gx1 - gx0, gx0 + 2.0, oy + y + 5.1, gx1 - gx0 - 4.0);
            y += 9.5;
        }
    }

    let r = if rack { 3.0 } else { 12.0 };
    let title = format!(r#"<text x="{x0}" y="33" font-size="{TITLE}">{}</text>"#, esc(&fit_title(&dev.name, IDW)));
    // HIDE: a small key at the left of the row under the LCD, an eye struck through; dimmed while
    // the device has cords (only an unused one can be hidden)
    let hide = {
        let [hx, hy, hw, hh] = geo.hide;
        let (ex, ey) = (hx + hw / 2.0, hy + hh / 2.0);
        format!(
            r##"<g opacity="{}"><rect x="{hx}" y="{hy}" width="{hw}" height="{hh}" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/><path d="M{},{ey} q6,-6 12,0 q-6,6 -12,0z" fill="none" stroke="#e6e7e8" stroke-width="1.1"/><circle cx="{ex}" cy="{ey}" r="1.7" fill="#e6e7e8"/><path d="M{},{} L{},{}" stroke="#1b1c1e" stroke-width="2.6"/><path d="M{},{} L{},{}" stroke="#e6e7e8" stroke-width="1.1"/></g>"##,
            if counts.iter().any(|c| *c > 0) { 0.35 } else { 1.0 },
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
    // in the rack the screws go through the face into the rails' holes, and are as big as those
    let (inset, sy, big) = if rack { (RACK_SCREW_X, RACK_SCREW_Y, RACK_SCREW_R / 5.2) } else { (11.0, 11.0, 1.0) };
    let screws: String = {
        [(inset, sy), (w - inset, sy), (inset, h - sy), (w - inset, h - sy)]
            .iter()
            .enumerate()
            .map(|(i, (sx, sy))| {
                format!(
                    r##"<use href="#screw" transform="translate({sx},{sy}) rotate({}) scale({big})"/>"##,
                    (i * 37 + dev.name.len() * 11) % 90
                )
            })
            .collect()
    };
    // paint: solid base, its texture and the sheen, put under the drawing (`Skin`)
    let under = Skin { w, h, r, base: hex(f.base), paint: f.paint, noise: f.noise as f32 };
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

/// The LCD's four lines of text, as a document the size of the LCD's text area: the top one bold.
pub fn lcd(lines: [&str; 4]) -> String {
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

/// The shared pieces of the faces' SVG: the paint, the noise, the sheen, the LCD's glow, the
/// screws, and the chrome and the dark hole (the jacks are pictures of their own: `jack_defs`).
fn defs() -> &'static str {
    static DEFS: OnceLock<String> = OnceLock::new();
    DEFS.get_or_init(|| {
        let paint = paint_uri();
        let noise = format!("data:image/png;base64,{}", base64(include_bytes!("../../assets/noise.png")));
        format!(
            r##"<defs>
<pattern id="pt" width="80" height="80" patternUnits="userSpaceOnUse"><image href="{paint}" width="80" height="80"/></pattern>
<pattern id="nz" width="128" height="128" patternUnits="userSpaceOnUse"><image href="{noise}" width="128" height="128"/></pattern>
<linearGradient id="gSheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".24"/></linearGradient>
<linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e38c2a"/></linearGradient>
<g id="screw"><circle r="5.2" fill="url(#gChrome)" stroke="#35363a" stroke-width=".7"/><path d="M-3.2,0 H3.2 M0,-3.2 V3.2" stroke="#36373b" stroke-width="1.4" stroke-linecap="round"/></g>
{metal}</defs>"##,
            metal = METAL
        )
    })
}

/// Only what a jack's picture needs (without the faces' textures, a hundred kilobytes and more
/// each picture would parse again).
fn jack_defs() -> &'static str {
    static DEFS: OnceLock<String> = OnceLock::new();
    DEFS.get_or_init(|| format!("<defs>\n{METAL}\n{}</defs>", jack_parts()))
}

/// The chrome (screws, jacks) and the dark of a hole: on the faces and in the jacks.
const METAL: &str = r##"<radialGradient id="gChrome" cx=".35" cy=".3" r=".85"><stop offset="0" stop-color="#fff"/><stop offset=".4" stop-color="#b9bcc1"/><stop offset=".75" stop-color="#65686d"/><stop offset="1" stop-color="#a6a9ad"/></radialGradient>
<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>"##;

/// The nut and the washers of a mono and a MIDI jack (with `METAL`).
fn jack_parts() -> String {
    let hex: String = (0..6)
        .map(|k| {
            let a = (k as f64 * 60.0 + 30.0).to_radians();
            format!("{:.2},{:.2} ", 15.0 * a.cos(), 15.0 * a.sin())
        })
        .collect();
    format!(
        r##"<linearGradient id="gNut" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f7f8"/><stop offset=".45" stop-color="#b6b9bd"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gWasher" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#5e6166"/><stop offset="1" stop-color="#141517"/></radialGradient>
<radialGradient id="gWasherMidi" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#c8483a"/><stop offset="1" stop-color="#4a120d"/></radialGradient>
<filter id="nutShadow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="1.2"/></filter>
<polygon id="hexShape" points="{hex}"/>
<g id="hex"><use href="#hexShape" fill="#000" opacity=".4" transform="translate(1.2,2.4)" filter="url(#nutShadow)"/><use href="#hexShape" fill="url(#gNut)" stroke="#43454a" stroke-width=".8"/></g>
<g id="jack-mono"><use href="#hex"/><circle r="11.4" fill="url(#gWasher)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
<g id="jack-midi"><use href="#hex"/><circle r="11.4" fill="url(#gWasherMidi)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
"##
    )
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
        for kind in [JackKind::Mono, JackKind::Stereo, JackKind::Midi] {
            let small = jack_pic_ring(kind, ACCENTS[0]);
            let full = small.replacen(jack_defs(), &format!("{}{}", defs(), jack_defs()), 1);
            assert_ne!(small, full);
            let (a, b) = (super::super::scene::rasterize(&opt, &small, 2.0).unwrap(), super::super::scene::rasterize(&opt, &full, 2.0).unwrap());
            assert!(a.data().iter().any(|&v| v > 0), "a {kind:?} jack draws nothing");
            assert_eq!(a.data(), b.data(), "a {kind:?} jack draws differently without the faces' defs");
        }
    }
}
