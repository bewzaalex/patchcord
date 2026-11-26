//! ENGINE: the PipeWire graph as it really is (see docs/design/mockup, the ENGINE key).
//!
//! The desk and the rack show devices as Patchcord builds them: a card's sink, source and
//! monitor are one face, an app's streams are frames of one device, Patchcord's meters and
//! WirePlumber's plumbing are left out. ENGINE shows what PipeWire has: every node, port and link,
//! nodes grouped by the driver that clocks them (from PipeWire's profiler, as pw-top), the
//! timings of the last cycle, every object and metadata, what happened, and what looks wrong.
//! It only looks: patching is done on the desk.
//!
//! The graph is drawn in world units into one picture (redrawn when it changes or after zooming),
//! moved and scaled by the view; the strip, the inspector and the tabs below are drawn in window
//! units, like the panel.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write;
use std::time::{Duration, Instant, SystemTime};

use super::{face, painter, panel};
use super::scene::{Layer, View, next_id, rasterize};
use super::text::{self, esc};
use crate::graph::{Device, Dir, Graph, Media, block_kind, port_role};
use crate::pw::{CUE_NODE, Cmd, DriverTiming, Event, Kind, METER_PREFIX, NodeTiming, Props};

/// What can be selected and inspected.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sel {
    Node(u32),
    Port(u32),
    Link(u32),
    /// any other object (a client, a device, a module, a metadata…)
    Obj(u32),
}

/// The tabs below the graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    /// PipeWire's own state: its clock, quantum, load, drivers, version (it was ENGINE's strip)
    Server,
    Top,
    Issues,
    Objects,
    Metadata,
    Events,
}

/// The inspector's tabs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ITab {
    Info,
    /// a port's LINK TO (LINK FROM): every port it may go to
    Conn,
    Props,
    Params,
    Timing,
}

/// Something clickable in ENGINE's strip, inspector or tabs.
#[derive(Clone, PartialEq, Debug)]
enum Hit {
    Tab(Tab),
    ITab(ITab),
    Show(usize),
    Sel(Sel),
    /// an object type in OBJECTS
    OType(String),
    Close,
    /// back to the desk, to this device
    Desk(String),
    /// save a report for a bug ticket
    Report,
    /// hide or show the tabs below the graph
    Bottom,
    /// open or close the legend
    Legend,
    /// the legend window itself (or another card): a click there does nothing
    LegendWin,
    /// FIND · SHOW: opens or closes its card
    Find,
    /// the card's ▲ ▼: the previous or the next block that matches
    FindStep(i32),
    /// the scrollbar of the details (0) or the tables (1)
    Bar(usize),
    /// a port's LINK TO (LINK FROM) row: the link (output, input) it makes or takes away
    LinkTo(u32, u32),
}

/// A picture to draw: (layer, x, y, w, h) in window pixels, where it may draw (if not
/// everywhere), its opacity.
pub type Pic<'a> = (&'a Layer, f64, f64, f64, f64, Option<[f64; 4]>, f32);

/// How long the blocks take to glide to where a new layout puts them.
const GLIDE: Duration = Duration::from_millis(300);

/// The graph going from one layout to the next: the old picture, cut into its blocks, each
/// gliding from where it was to where it is now, and the new one fading in under them.
struct Glide {
    start: Instant,
    old: Layer,
    /// each node's place (x, y) and height in the old picture, where it starts gliding from
    /// (there, or where a glide before this one had taken it), and where it goes
    src: HashMap<u32, (f64, f64, f64)>,
    from: HashMap<u32, (f64, f64)>,
    to: HashMap<u32, (f64, f64)>,
}

impl Glide {
    /// How far along it is (0…1, eased) and the place of node `id` now.
    fn at(&self, id: u32) -> Option<(f64, f64)> {
        let t = (self.start.elapsed().as_secs_f64() / GLIDE.as_secs_f64()).min(1.0);
        let e = t * t * (3.0 - 2.0 * t);
        let (fx, fy) = *self.from.get(&id)?;
        let (tx, ty) = self.to.get(&id).copied().unwrap_or((fx, fy));
        Some((fx + (tx - fx) * e, fy + (ty - fy) * e))
    }
}

/// A scrolled body, the details' or the tables': drawn apart from its frame, a slice of it about
/// two views tall round what shows, so scrolling only moves the picture and clips it; a new
/// slice is drawn on the painter's threads near its edge, or when what it shows changed.
#[derive(Default)]
struct Body {
    /// (what it shows, where the slice starts in the body, the picture)
    layer: Option<(u64, f64, Layer)>,
    /// being drawn: (its painter sig, what it shows, start, width, height)
    want: Option<(u64, u64, f64, f64, f64)>,
}

/// A body as its panel lays it out, in the panel's units: its SVG, how wide, where the view on it
/// starts and how tall it is, where the body's top is now (scrolled) and how tall it all is.
#[derive(Default)]
struct Scrolled {
    body: String,
    w: f64,
    top: f64,
    view_h: f64,
    by: f64,
    content: f64,
}

/// A scrollbar, in logical pixels: its track, its thumb, and how far the body scrolls.
#[derive(Clone, Copy)]
struct Bar {
    track: [f64; 4],
    thumb: [f64; 4],
    max: f64,
}

/// What the app has to do after a click.
pub enum Action {
    /// leave ENGINE and find this device on the desk
    Desk(String),
    /// save a report (`Engine::report`) for a bug ticket
    Report,
    /// links to make and to take away (output port, input port), as one step to undo
    Link { add: Vec<(u32, u32)>, del: Vec<(u32, u32)> },
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
enum NKind {
    Device,
    Midi,
    Video,
    Stream,
    Meter,
    /// the cue: Patchcord's two streams, what is listened to in (`CUE_NODE`), out into the default
    /// output (after the meters: under them in a column)
    Cue,
    Other,
    Driver,
}

#[derive(Clone, PartialEq, Hash, Debug)]
struct EPort {
    id: u32,
    name: String,
    channel: String,
    monitor: bool,
    midi: bool,
    video: bool,
    audio: bool,
    /// what it is for (`graph::port_role`): its colour
    role: &'static str,
}

#[derive(Clone, PartialEq, Debug)]
struct ENode {
    id: u32,
    name: String,
    desc: String,
    class: String,
    kind: NKind,
    state: &'static str,
    ins: Vec<EPort>,
    outs: Vec<EPort>,
    color: [u8; 3],
    /// on the desk: the device's key and name, the frames it is, and whether it is hidden there
    desk: Option<(String, String, Vec<String>, bool)>,
    /// WirePlumber's plumbing behind a headset's MIC, or another program's level meter
    system: bool,
}

#[derive(Clone, PartialEq, Hash, Debug)]
struct ELink {
    id: u32,
    out_node: u32,
    out_port: u32,
    in_node: u32,
    in_port: u32,
    state: &'static str,
    passive: bool,
}

/// Nodes clocked together by one driver.
#[derive(Clone, PartialEq, Debug)]
struct Group {
    driver: u32,
    members: Vec<u32>,
    quantum: u64,
    rate: u32,
    load: f32,
    /// when the whole graph had finished, from the start of the cycle: on average, at worst
    end_ns: i64,
    end_max_ns: i64,
}

impl Group {
    fn period_ns(&self) -> f64 {
        self.quantum as f64 * 1e9 / self.rate.max(1) as f64
    }
}

#[derive(Default)]
struct Model {
    nodes: Vec<ENode>,
    links: Vec<ELink>,
    groups: Vec<Group>,
    at: HashMap<u32, usize>,
    /// each port's node
    port_node: HashMap<u32, u32>,
    /// each node's group
    group_of: HashMap<u32, usize>,
    /// each node's timing in the last cycle
    timing: HashMap<u32, NodeTiming>,
}

impl Model {
    fn node(&self, id: u32) -> Option<&ENode> {
        self.at.get(&id).map(|&i| &self.nodes[i])
    }
    fn port(&self, id: u32) -> Option<(&ENode, &EPort, bool)> {
        let n = self.node(*self.port_node.get(&id)?)?;
        n.ins.iter().find(|p| p.id == id).map(|p| (n, p, true)).or_else(|| n.outs.iter().find(|p| p.id == id).map(|p| (n, p, false)))
    }
    fn group(&self, node: u32) -> Option<&Group> {
        self.group_of.get(&node).map(|&g| &self.groups[g])
    }

    /// Whether ports `a` and `b` may be linked: one gives out, the other takes in, both of one
    /// kind (audio, MIDI, video), on two blocks, neither Patchcord's own (its meters, the cue).
    fn fits(&self, a: u32, b: u32) -> bool {
        let (Some((na, pa, ia)), Some((nb, pb, ib))) = (self.port(a), self.port(b)) else { return false };
        let kind = |p: &EPort| if p.midi { 1 } else if p.video { 2 } else { 0 };
        let own = |n: &ENode| matches!(n.kind, NKind::Meter | NKind::Cue);
        ia != ib && kind(pa) == kind(pb) && na.id != nb.id && !own(na) && !own(nb)
    }

    /// Ports `a` and `b` as a link: (output, input).
    fn pair(&self, a: u32, b: u32) -> (u32, u32) {
        if self.port(a).is_some_and(|(_, _, input)| input) { (b, a) } else { (a, b) }
    }

    fn linked(&self, (o, i): (u32, u32)) -> bool {
        self.links.iter().any(|l| l.out_port == o && l.in_port == i)
    }

    /// Whether a link carried from port `a` can go to port `b`: it fits and isn't there yet.
    fn takes(&self, a: u32, b: u32) -> bool {
        self.fits(a, b) && !self.linked(self.pair(a, b))
    }

    /// The links from block `from`'s head to block `to`, those not there yet: its outputs (an
    /// input-only block, its inputs) to the other's ports the other way, channel to channel (FL
    /// to FL, FR to FR), mono to all and all to mono; with no channel in common, in order.
    fn pairs(&self, from: u32, to: u32) -> Vec<(u32, u32)> {
        let (Some(f), Some(t)) = (self.node(from), self.node(to)) else { return Vec::new() };
        let src = if f.outs.is_empty() { &f.ins } else { &f.outs };
        let dst: Vec<&EPort> = (if f.outs.is_empty() { &t.outs } else { &t.ins }).iter().filter(|d| src.first().is_some_and(|s| self.fits(s.id, d.id))).collect();
        let ch = |p: &EPort| if p.channel.is_empty() { p.name.clone() } else { p.channel.clone() };
        let mut out = Vec::new();
        for s in src {
            for d in &dst {
                let (a, b) = (ch(s), ch(d));
                if a == b || a == "MONO" || b == "MONO" || src.len() == 1 || dst.len() == 1 {
                    out.push(self.pair(s.id, d.id));
                }
            }
        }
        if out.is_empty() {
            out = src.iter().zip(&dst).map(|(s, d)| self.pair(s.id, d.id)).collect();
        }
        out.retain(|&p| self.fits(p.0, p.1) && !self.linked(p));
        out
    }
}

/// One line of ISSUES: how bad (2 error, 1 warning, 0 info), its tag, what, and what to do.
#[derive(Clone, PartialEq, Hash)]
struct Issue {
    level: u8,
    tag: &'static str,
    node: Option<u32>,
    title: String,
    hint: String,
}

// sizes in the graph's world units
const NW: f64 = 244.0;
const NH0: f64 = 40.0;
const PR: f64 = 15.0;
const COLW: f64 = NW + 120.0;
const NGAP: f64 = 14.0;
const BPAD: f64 = 26.0;
const BHEAD: f64 = 36.0;
/// A band is laid out to fit a view this many times wider than high.
const ASPECT: f64 = 2.2;

// sizes of the window parts, in logical pixels
const BOTTOM_H: f64 = 250.0;
const INSP_W: f64 = 380.0;
const TABS_H: f64 = 30.0;
const ROW_H: f64 = 19.0;
/// a window narrower than this has the details over the graph
const NARROW: f64 = 700.0;
// A block's colour is its kind's, as its frame's on the rack (ADR 0014: `face::kind_colour`); a
// port's is its group's (`face::role_colour`), and a link takes the colour of the port it leaves.
// Patchcord's own meters and cue too (their PATCHCORD tag says whose). (Dummy-Driver and
// Freewheel-Driver have no class: NO CLASS.)

/// What ENGINE says of a node without `media.class`.
const NO_CLASS: &str = "no media.class";

/// the cue's marks on a port kept on, as on the desk
const CUE: &str = "#5fd4ff";
/// text on a port filled in the cue's colour
const CUE_TEXT: &str = "#062533";
/// What is chosen: the LCD's amber, and text on it as on the LCD.
const SEL: &str = "#f7bb62";
const SEL_TEXT: &str = "#3a2408";
/// The legend card, made by docs/design/engine/legend2.py.
const LEGEND_SVG: &str = include_str!("legend.svg");

const SHOW: [(&str, &str); 4] = [("PATCHCORD", "Patchcord's own signal meters"), ("MIDI", "MIDI nodes"), ("NOT RUNNING", "nodes no driver clocks now: suspended devices, idle streams"), ("INTERNAL", "Dummy-Driver and Freewheel-Driver: PipeWire's own clocks, idle")];
const OBJECT_TYPES: [&str; 12] = ["Core", "Module", "Factory", "Client", "Device", "Node", "Port", "Link", "Metadata", "Profiler", "SecurityContext", "Other"];

fn cut(t: &str, n: usize) -> String {
    if t.chars().count() > n { format!("{}…", t.chars().take(n.saturating_sub(1)).collect::<String>()) } else { t.to_owned() }
}

/// "#rrggbb" as bytes.
fn rgb(c: &str) -> [u8; 3] {
    let b = |i: usize| u8::from_str_radix(c.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    [b(1), b(3), b(5)]
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn ms(q: u64, r: u32) -> String {
    if r == 0 { "—".into() } else { format!("{:.1} ms", q as f64 * 1000.0 / r as f64) }
}

fn led_color(state: &str, err: bool) -> &'static str {
    if err {
        "#ff4a32"
    } else {
        match state {
            "running" | "active" => "#5cff7a",
            "idle" | "paused" => "#ffb02e",
            "error" => "#ff4a32",
            _ => "#4a4c50",
        }
    }
}

/// A link from output `a` to input `b`: one curve, level at both ports, over whatever lies between.
fn link_curve(a: (f64, f64), b: (f64, f64)) -> [(f64, f64); 4] {
    let d = ((b.0 - a.0).abs() / 2.0).max(50.0);
    [a, (a.0 + d, a.1), (b.0 - d, b.1), b]
}

/// The point at `t` (0…1) along a cubic.
fn bezier(c: &[(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * c[0].0 + b * c[1].0 + cc * c[2].0 + d * c[3].0, a * c[0].1 + b * c[1].1 + cc * c[2].1 + d * c[3].1)
}

/// How far point `p` is from the segment `a`–`b`.
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}

/// How far down its node a port's row is (its middle), by its place among the node's inputs
/// or outputs.
fn port_y(i: usize) -> f64 {
    NH0 + i as f64 * PR + PR / 2.0 + 2.0
}

fn node_h(n: &ENode) -> f64 {
    NH0 + n.ins.len().max(n.outs.len()).max(1) as f64 * PR + 8.0
}

/// Lines of at most `n` characters, broken between words where it can (a long word, a path,
/// is cut where it must).
fn wrap(t: &str, n: usize) -> Vec<String> {
    let n = n.max(4);
    let mut out = Vec::new();
    for line in t.split('\n') {
        let mut cur = String::new();
        for word in line.split_inclusive(' ') {
            if cur.chars().count() + word.trim_end().chars().count() > n && !cur.is_empty() {
                out.push(cur.trim_end().to_owned());
                cur.clear();
            }
            let mut w: Vec<char> = word.chars().collect();
            while w.len() > n {
                let rest = w.split_off(n);
                out.push(w.iter().collect());
                w = rest;
            }
            cur.extend(w);
        }
        out.push(cur.trim_end().to_owned());
    }
    out
}

pub struct Engine {
    pub on: bool,
    /// commands for PipeWire (the profiler, a node's params)
    pub outbox: Vec<Cmd>,
    /// the view of the graph: window pixels of world (0, 0), and pixels per world unit
    pub view: View,
    fitted: bool,
    sel: Option<Sel>,
    tab: Tab,
    itab: ITab,
    otype: String,
    show: [bool; 4],
    /// how far the inspector's body and the tabs' body are scrolled, in logical pixels
    scroll: [f64; 2],
    // what PipeWire told
    meta: BTreeMap<(u32, u32, String), (Option<String>, String)>,
    timings: Vec<DriverTiming>,
    params: HashMap<u32, BTreeMap<(String, u32), String>>,
    asked: HashSet<u32>,
    /// each node's B/Q (average, peak), and each driver's graph end over its period (average,
    /// worst), half a second apart
    hist: HashMap<u32, VecDeque<(f32, f32)>>,
    ends: HashMap<u32, VecDeque<(f32, f32)>>,
    load: VecDeque<f32>,
    xruns: HashMap<u32, (i32, Instant)>,
    events: VecDeque<(SystemTime, &'static str, String, Option<u32>)>,
    prev: Option<Seen>,
    // built from the above
    model: Model,
    issues: Vec<Issue>,
    objects: Vec<(String, u32, String, String)>,
    /// every object's properties
    props: HashMap<u32, Props>,
    pub dirty: bool,
    built: Option<Instant>,
    // what is drawn
    lay: Layout,
    graph_layer: Option<(u64, f64, Layer)>,
    /// each node's place (x, y) and height in the graph's picture, and in the one asked for
    graph_pos: HashMap<u32, (f64, f64, f64)>,
    want_pos: HashMap<u32, (f64, f64, f64)>,
    /// the blocks gliding to a new layout
    glide: Option<Glide>,
    /// what the layout shown was made for, and the one for the picture asked for (theirs)
    lay_sig: Option<u64>,
    want_lay: Option<(u64, Layout)>,
    /// the graph being drawn on the painter's threads: its sig, what it is drawn from, the
    /// view's scale then, its size
    graph_want: Option<(u64, u64, f64, f64, f64)>,
    inbox: painter::Inbox,
    /// PipeWire's version, for SERVER
    version: String,
    insp: Option<(u64, Layer)>,
    bottom: Option<(u64, Layer)>,
    /// the graph's overlay: its keys along its top (FIND · SHOW, LEGEND, DIAGNOSTICS), a strip,
    /// not a picture as big as the graph mostly empty
    overlay: Option<(u64, Layer)>,
    /// the legend window, while it is open
    legend: Option<(u64, Layer)>,
    /// what changes every moment on the graph (see `numbers`): by what it is, its picture
    nums: HashMap<String, (u64, Layer)>,
    /// how long `sprites` spent drawing each part, the last time (for the log of slow frames)
    pub spent: Vec<(&'static str, Duration)>,
    hits: Vec<(Hit, [f64; 4])>,
    /// a press in the graph: where, the view then, what was under it, whether it moved
    press: Option<Press>,
    /// a link in the hand, from the port or block head the press was on (carried once it moves)
    carry: Option<Carried>,

    zoomed: Option<Instant>,
    /// where FIT takes the view, gliding there as the desk's does; and when it last stepped
    view_to: Option<(View, Instant)>,
    /// the selection came from a list: bring it into view
    reveal: bool,
    /// the tabs below are put away (the inspector shows only with a selection)
    bottom_shut: bool,
    /// the legend is open over the graph
    legend_open: bool,
    /// FIND · SHOW's card is open, what is typed in its field, and its picture
    find_open: bool,
    find: String,
    find_card: Option<(u64, Layer)>,
    /// MAC addresses masked in names, for pictures others see
    pub private: bool,
    /// the ports of the jacks kept on in the cue, filled in its colour, with those linked into
    /// it: also the ones it leaves out (heard in the output listened on already, or asleep)
    pub cue_kept: Vec<u32>,
    /// in CUE mode, the port under the pointer, and whether it would feed back: framed (cyan,
    /// or red) where a click would listen, a small picture over the graph
    pub port_hover: Option<(u32, bool)>,
    hover_pic: Option<(u64, Layer)>,
    /// the details and the tables being drawn on the painter's threads: what for, where
    insp_want: Option<(u64, [f64; 4])>,
    bottom_want: Option<(u64, [f64; 4])>,
    /// the bodies of the details and the tables, their scrollbars, a thumb in the hand (which,
    /// where the pointer took it, the scroll then), and the bars' pixels (track, thumb)
    bodies: [Body; 2],
    bars: [Option<Bar>; 2],
    bar_drag: Option<(usize, f64, f64)>,
    bar_px: [Layer; 2],
    /// each body's view top and scrolled top in its panel, as last laid out
    body_at: [(f64, f64); 2],
}

/// What the last look saw: each link's ends by name, each node's name and state.
type Seen = (HashMap<u32, String>, HashMap<u32, (String, &'static str)>);
/// A press in the graph: where, the view then, what was under it, whether it moved since.
type Press = ((f64, f64), View, Option<Sel>, bool);

/// A link in the hand: from a port, or from a block's head (all its channels at once).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Carry {
    Port(u32),
    Node(u32),
}

/// A link being carried: what from, where the pointer is (graph units), and what it would go to
/// if let go now: a port (from a port), a block (from a head).
#[derive(Clone, Copy, Debug)]
struct Carried {
    from: Carry,
    at: (f64, f64),
    snap: Option<u32>,
}

/// A block's head: the band of its kind's colour, as a frame's on the rack, its name on it.
const HEAD_H: f64 = 24.0;
/// Where a node is on the desk: its device's key and name, its frames, and whether it is hidden.
type DeskPlace = (String, String, Vec<String>, bool);

#[derive(Default, Clone)]
struct Layout {
    pos: HashMap<u32, (f64, f64)>,
    bands: Vec<(f64, f64, Option<usize>, &'static str)>,
    w: f64,
    h: f64,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            on: false,
            outbox: Vec::new(),
            view: View { x: 40.0, y: 140.0, s: 0.8 },
            fitted: false,
            sel: None,
            tab: Tab::Server,
            itab: ITab::Info,
            otype: "Node".into(),
            show: [true, true, true, false],
            scroll: [0.0, 0.0],
            meta: BTreeMap::new(),
            timings: Vec::new(),
            params: HashMap::new(),
            asked: HashSet::new(),
            hist: HashMap::new(),
            ends: HashMap::new(),
            load: VecDeque::new(),
            xruns: HashMap::new(),
            events: VecDeque::new(),
            prev: None,
            model: Model::default(),
            issues: Vec::new(),
            objects: Vec::new(),
            props: HashMap::new(),
            dirty: true,
            built: None,
            lay: Layout::default(),
            graph_layer: None,
            graph_pos: HashMap::new(),
            want_pos: HashMap::new(),
            glide: None,
            lay_sig: None,
            want_lay: None,
            graph_want: None,
            inbox: painter::Inbox::default(),
            version: String::new(),
            insp: None,
            bottom: None,
            overlay: None,
            legend: None,
            spent: Vec::new(),
            nums: HashMap::new(),
            hits: Vec::new(),
            press: None,
            carry: None,
            zoomed: None,
            view_to: None,
            reveal: false,
            bottom_shut: true,
            legend_open: false,
            find_open: false,
            find: String::new(),
            find_card: None,
            private: false,
            cue_kept: Vec::new(),
            port_hover: None,
            hover_pic: None,
            insp_want: None,
            bottom_want: None,
            bodies: Default::default(),
            bars: [None, None],
            bar_drag: None,
            bar_px: [pixel([0x24, 0x25, 0x28, 0xff]), pixel([0x5a, 0x5d, 0x62, 0xff])],
            body_at: [(0.0, 0.0); 2],
        }
    }

    /// Turns ENGINE on or off; the profiler runs only while it is on.
    pub fn set_on(&mut self, on: bool) {
        if self.on != on {
            self.on = on;
            self.outbox.push(Cmd::Profile(on));
            self.dirty = true;
            // drawn again: while the desk was shown the GPU let their textures go
            self.graph_layer = None;
            self.glide = None;
            self.view_to = None;
            (self.insp, self.insp_want) = (None, None);
            (self.bottom, self.bottom_want) = (None, None);
            self.bodies = Default::default();
            (self.overlay, self.find_card) = (None, None);
            self.legend = None;
            self.nums.clear();
            // the graph first: the diagnostics below open on a click
            if on {
                self.bottom_shut = true;
            }
        }
    }

    /// Takes in what PipeWire reported for ENGINE; true when something it shows changed.
    pub fn apply(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Meta { id, subject, key, ty, value } => {
                match (key, value) {
                    (None, _) => self.meta.retain(|(m, s, _), _| !(*m == *id && *s == *subject)),
                    (Some(k), None) => {
                        self.meta.remove(&(*id, *subject, k.clone()));
                    }
                    (Some(k), Some(v)) => {
                        let v = if self.private { mask_macs(v) } else { v.clone() };
                        self.meta.insert((*id, *subject, k.clone()), (ty.clone(), v));
                    }
                }
                true
            }
            Event::Profile(t) => {
                for d in t {
                    let period = d.quantum as f64 * 1e9 / d.rate.max(1) as f64;
                    let e = self.ends.entry(d.node.id).or_default();
                    e.push_back(((d.end_ns as f64 / period) as f32, (d.end_max_ns as f64 / period) as f32));
                    if e.len() > 120 {
                        e.pop_front();
                    }
                    for n in std::iter::once(&d.node).chain(&d.followers) {
                        let h = self.hist.entry(n.id).or_default();
                        h.push_back(((n.busy_ns as f64 / period) as f32, (n.busy_max_ns as f64 / period) as f32));
                        if h.len() > 120 {
                            h.pop_front();
                        }
                        // xruns are counted since the node started: a new one is an event
                        match self.xruns.get(&n.id).copied() {
                            Some((before, _)) if n.xruns > before => {
                                let k = n.xruns - before;
                                self.log("xrun", format!("node {} {}: {k} xrun{} (now {})", n.id, n.name, if k == 1 { "" } else { "s" }, n.xruns), Some(n.id));
                                self.xruns.insert(n.id, (n.xruns, Instant::now()));
                            }
                            Some((before, at)) if n.xruns < before => {
                                self.xruns.insert(n.id, (n.xruns, at));
                            }
                            Some(_) => {}
                            None => {
                                self.xruns.insert(n.id, (n.xruns, Instant::now()));
                            }
                        }
                    }
                }
                self.load.push_back(t.iter().map(|d| d.load).fold(0.0, f32::max));
                if self.load.len() > 90 {
                    self.load.pop_front();
                }
                self.timings = t.clone();
                self.dirty = true;
                self.on
            }
            Event::Param { id, kind, index, text } => {
                let p = self.params.entry(*id).or_default();
                if *index == 0 {
                    p.retain(|(k, _), _| k != kind);
                }
                p.insert((kind.clone(), *index), text.clone());
                self.on
            }
            _ => false,
        }
    }

    fn log(&mut self, kind: &'static str, text: String, node: Option<u32>) {
        let text = if self.private { mask_macs(&text) } else { text };
        self.events.push_front((SystemTime::now(), kind, text, node));
        if self.events.len() > 300 {
            self.events.pop_back();
        }
    }

    /// Whether `build` has something to do: something changed, or a second went by.
    pub fn due(&self) -> bool {
        self.dirty || self.built.is_none_or(|t| t.elapsed() >= Duration::from_millis(1000))
    }

    /// Builds what ENGINE shows from the graph and the desk's devices.
    pub fn build(&mut self, graph: &Graph, devices: &[&Device], hidden: &HashSet<String>, settings: &HashMap<String, String>) {
        let now = Instant::now();
        if !self.due() {
            return;
        }
        self.dirty = false;
        self.built = Some(now);
        // where each node is on the desk
        let mut desk: HashMap<u32, DeskPlace> = HashMap::new();
        for d in devices {
            for g in &d.groups {
                for p in &g.ports {
                    let e = desk.entry(p.node).or_insert_with(|| (d.key.clone(), d.name.clone(), Vec::new(), hidden.contains(&d.key)));
                    if !e.2.contains(&g.label) {
                        e.2.push(g.label.clone());
                    }
                }
            }
        }
        let mut nodes = Vec::new();
        let mut ports: Vec<(u32, u32, bool, EPort)> = Vec::new();
        let mut links = Vec::new();
        let mut objects = Vec::new();
        let get = |p: &Props, k: &str| p.get(k).cloned().unwrap_or_default();
        for (id, kind, p) in graph.objects() {
            match kind {
                Kind::Node => {
                    let name = get(p, "node.name");
                    // a node's kind; not every node says it (Easy Effects' effects don't, so that
                    // WirePlumber leaves them alone): said so, nothing made up in its place
                    let class = p.get("media.class").filter(|c| !c.trim().is_empty()).cloned().unwrap_or_else(|| NO_CLASS.to_owned());
                    let desc = p.get("node.description").or(p.get("node.nick")).cloned().unwrap_or_else(|| name.clone());
                    let nkind = if name == "Dummy-Driver" || name == "Freewheel-Driver" || p.get("factory.name").is_some_and(|f| f == "support.node.driver") {
                        NKind::Driver
                    } else if name.starts_with(METER_PREFIX) {
                        NKind::Meter
                    } else if name.starts_with(CUE_NODE) {
                        NKind::Cue
                    } else if class.contains("Midi") {
                        NKind::Midi
                    } else if class.contains("Video") {
                        NKind::Video
                    } else if class.starts_with("Stream") {
                        NKind::Stream
                    } else if class.starts_with("Audio") {
                        NKind::Device
                    } else {
                        NKind::Other
                    };
                    let d = desk.get(&id);
                    let system = class.ends_with("/Internal") || (!matches!(nkind, NKind::Meter | NKind::Cue) && p.get("stream.monitor").is_some_and(|v| v == "true"));
                    // a block's colour says what it is (the legend's COLOURS)
                    let color = rgb(face::kind_colour(block_kind(p.get("media.class").map_or("", |s| s.as_str()))));
                    nodes.push(ENode {
                        id,
                        name,
                        desc,
                        class,
                        kind: nkind,
                        state: graph.state_of(id).unwrap_or("—"),
                        ins: Vec::new(),
                        outs: Vec::new(),
                        color,
                        desk: d.map(|d| (d.0.clone(), d.1.clone(), d.2.clone(), d.3)),
                        system,
                    });
                    objects.push(("Node".into(), id, get(p, "node.name"), get(p, "media.class")));
                }
                Kind::Port => {
                    let Some(node) = p.get("node.id").and_then(|v| v.parse::<u32>().ok()) else { continue };
                    let index = p.get("port.id").and_then(|v| v.parse::<u32>().ok()).unwrap_or(id);
                    let name = get(p, "port.name");
                    ports.push((
                        node,
                        index,
                        p.get("port.direction").is_some_and(|d| d == "in"),
                        EPort {
                            id,
                            channel: get(p, "audio.channel"),
                            monitor: p.get("port.monitor").is_some_and(|v| v == "true"),
                            midi: p.get("format.dsp").is_some_and(|f| f.contains("midi")) || name.contains("midi") && !p.contains_key("audio.channel"),
                            video: p.get("format.dsp").is_some_and(|f| f.contains("video")),
                            audio: p.get("format.dsp").is_some_and(|f| f.contains("audio")),
                            role: "",
                            name,
                        },
                    ));
                    objects.push(("Port".into(), id, format!("{}:{}", node, get(p, "port.name")), get(p, "port.direction")));
                }
                Kind::Link => {
                    let num = |k: &str| p.get(k).and_then(|v| v.parse::<u32>().ok());
                    if let (Some(on), Some(op), Some(inn), Some(ip)) = (num("link.output.node"), num("link.output.port"), num("link.input.node"), num("link.input.port")) {
                        let state = graph.state_of(id).unwrap_or("—");
                        // PipeWire 1.6 keeps no link.passive on a link out of a stream; a passive
                        // node's side makes the link passive all the same
                        let side = |n: u32, dir: &str| graph.props_of(n).and_then(|q| q.get("node.passive")).is_some_and(|v| v == "true" || v == dir);
                        let passive = p.get("link.passive").is_some_and(|v| v == "true") || side(inn, "in") || side(on, "out");
                        links.push(ELink { id, out_node: on, out_port: op, in_node: inn, in_port: ip, state, passive });
                        objects.push(("Link".into(), id, format!("{op} → {ip}"), format!("{state}{}", if passive { " · passive" } else { "" })));
                    }
                }
                Kind::Client => objects.push(("Client".into(), id, p.get("application.name").cloned().unwrap_or_else(|| get(p, "application.process.binary")), get(p, "client.api"))),
                Kind::Device => objects.push(("Device".into(), id, get(p, "device.name"), get(p, "device.description"))),
                Kind::Other => {
                    let ty = get(p, "object.type");
                    let short = ty.rsplit(':').next().unwrap_or("").to_owned();
                    let name = ["metadata.name", "module.name", "factory.name", "core.name"].iter().find_map(|k| p.get(*k).cloned()).unwrap_or_default();
                    objects.push((if OBJECT_TYPES.contains(&short.as_str()) { short } else { "Other".into() }, id, name, String::new()));
                }
            }
        }
        nodes.sort_by_key(|n| n.id);
        let at: HashMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        ports.sort_by_key(|p| (p.0, p.1));
        let mut port_node = HashMap::new();
        for (node, _, input, port) in ports {
            if let Some(&i) = at.get(&node) {
                port_node.insert(port.id, node);
                if input { nodes[i].ins.push(port) } else { nodes[i].outs.push(port) }
            }
        }
        for n in &mut nodes {
            if n.kind == NKind::Midi || n.ins.iter().chain(&n.outs).all(|p| p.midi) && !n.ins.is_empty() {
                for p in n.ins.iter_mut().chain(n.outs.iter_mut()) {
                    p.midi = true;
                }
            }
            // each port's group, as on the rack
            let class = if n.class == NO_CLASS { String::new() } else { n.class.clone() };
            for (dir, ps) in [(Dir::In, &mut n.ins), (Dir::Out, &mut n.outs)] {
                for p in ps.iter_mut() {
                    let media = if p.midi {
                        Media::Midi
                    } else if p.video {
                        Media::Video
                    } else if p.audio {
                        Media::Audio
                    } else {
                        Media::Other
                    };
                    p.role = port_role(&class, dir, p.monitor, media);
                }
            }
        }
        links.retain(|l| at.contains_key(&l.out_node) && at.contains_key(&l.in_node));
        links.sort_by_key(|l| l.id);
        // groups: from the profiler; until it has reported, nodes joined by links that run
        let mut groups = Vec::new();
        let mut timing = HashMap::new();
        if !self.timings.is_empty() {
            for d in &self.timings {
                let mut members = vec![d.node.id];
                timing.insert(d.node.id, d.node.clone());
                for f in &d.followers {
                    if f.id != d.node.id && !members.contains(&f.id) {
                        members.push(f.id);
                    }
                    timing.insert(f.id, f.clone());
                }
                members.retain(|m| at.contains_key(m));
                if !members.is_empty() {
                    groups.push(Group { driver: d.node.id, members, quantum: d.quantum, rate: d.rate, load: d.load, end_ns: d.end_ns, end_max_ns: d.end_max_ns });
                }
            }
        } else {
            let mut root: HashMap<u32, u32> = nodes.iter().map(|n| (n.id, n.id)).collect();
            fn find(r: &mut HashMap<u32, u32>, x: u32) -> u32 {
                let p = r[&x];
                if p == x {
                    return x;
                }
                let q = find(r, p);
                r.insert(x, q);
                q
            }
            for l in &links {
                let (a, b) = (find(&mut root, l.out_node), find(&mut root, l.in_node));
                root.insert(a, b);
            }
            let mut comps: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for n in &nodes {
                let r = find(&mut root, n.id);
                comps.entry(r).or_default().push(n.id);
            }
            let num = |k: &str| settings.get(k).and_then(|v| v.parse::<u64>().ok()).filter(|v| *v > 0);
            for members in comps.into_values() {
                if !members.iter().any(|m| nodes[at[m]].state == "running") {
                    continue;
                }
                let prio = |id: &u32| graph.props_of(*id).and_then(|p| p.get("priority.driver")).and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
                let driver = *members.iter().max_by_key(|m| prio(m)).unwrap_or(&members[0]);
                groups.push(Group { driver, members, quantum: num("clock.quantum").unwrap_or(1024), rate: num("clock.rate").unwrap_or(48000) as u32, load: 0.0, end_ns: 0, end_max_ns: 0 });
            }
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.members.len()));
        let mut group_of = HashMap::new();
        for (gi, g) in groups.iter().enumerate() {
            for m in &g.members {
                group_of.entry(*m).or_insert(gi);
            }
        }
        // what changed since the last look: links made and removed, nodes coming, going, starting
        let lnames: HashMap<u32, String> = links
            .iter()
            .map(|l| (l.id, format!("{}:{} → {}:{}", nodes[at[&l.out_node]].name, port_name(&nodes[at[&l.out_node]], l.out_port), nodes[at[&l.in_node]].name, port_name(&nodes[at[&l.in_node]], l.in_port))))
            .collect();
        let nstates: HashMap<u32, (String, &'static str)> = nodes.iter().map(|n| (n.id, (n.name.clone(), n.state))).collect();
        if let Some((pl, pn)) = self.prev.take() {
            let mut log = Vec::new();
            for (id, t) in &lnames {
                if !pl.contains_key(id) {
                    let passive = links.iter().any(|l| l.id == *id && l.passive);
                    log.push(("link", format!("link {id} created: {t}{}", if passive { " (passive)" } else { "" }), links.iter().find(|l| l.id == *id).map(|l| l.out_node)));
                }
            }
            for (id, t) in &pl {
                if !lnames.contains_key(id) {
                    log.push(("link", format!("link {id} destroyed: {t}"), None));
                }
            }
            for (id, (name, st)) in &nstates {
                match pn.get(id) {
                    None => log.push(("node", format!("node {id} {name} added"), Some(*id))),
                    Some((_, was)) if was != st && *was != "—" => log.push(("state", format!("node {id} {name}: {was} → {st}"), Some(*id))),
                    _ => {}
                }
            }
            for (id, (name, _)) in &pn {
                if !nstates.contains_key(id) {
                    log.push(("node", format!("node {id} {name} removed"), None));
                }
            }
            for (k, t, n) in log {
                self.log(k, t, n);
            }
        } else {
            self.log("node", format!("watching PipeWire: {} nodes, {} links", nodes.len(), links.len()), None);
        }
        self.prev = Some((lnames, nstates));
        self.props = graph.objects().map(|(id, _, p)| (id, if self.private { p.iter().map(|(k, v)| (k.clone(), mask_macs(v))).collect() } else { p.clone() })).collect();
        // the names the graph shows first; a device with its nodes (the graph's ids)
        for o in objects.iter_mut() {
            match o.0.as_str() {
                "Node" => {
                    if let Some(&i) = at.get(&o.1) {
                        let n = &nodes[i];
                        (o.2, o.3) = (n.desc.clone(), format!("{} · {}", n.name, n.class));
                    }
                }
                "Port" => {
                    if let Some(n) = port_node.get(&o.1).and_then(|n| at.get(n)).map(|&i| &nodes[i]) {
                        o.2 = format!("{}: {}", n.desc, port_name(n, o.1));
                        o.3 = format!("{} · node {}", o.3, n.id);
                    }
                }
                "Link" => {
                    if let Some(l) = links.iter().find(|l| l.id == o.1) {
                        let (a, b) = (&nodes[at[&l.out_node]], &nodes[at[&l.in_node]]);
                        o.2 = format!("{}:{} → {}:{}", a.desc, port_name(a, l.out_port), b.desc, port_name(b, l.in_port));
                    }
                }
                "Device" => {
                    let id = o.1.to_string();
                    let ns: Vec<String> = nodes.iter().filter(|n| graph.props_of(n.id).and_then(|p| p.get("device.id")) == Some(&id)).map(|n| n.id.to_string()).collect();
                    let name = std::mem::take(&mut o.2);
                    o.2 = std::mem::take(&mut o.3);
                    o.3 = format!("{name}{}", if ns.is_empty() { " · no nodes".to_owned() } else { format!(" · nodes {}", ns.join(", ")) });
                }
                _ => {}
            }
        }
        objects.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        self.objects = objects;
        self.model = Model { nodes, links, groups, at, port_node, group_of, timing };
        // what was kept for nodes that are gone (streams come and go all day)
        let alive: HashSet<u32> = self.model.nodes.iter().map(|n| n.id).collect();
        self.hist.retain(|id, _| alive.contains(id));
        self.ends.retain(|id, _| alive.contains(id));
        self.xruns.retain(|id, _| alive.contains(id));
        self.params.retain(|id, _| alive.contains(id));
        self.asked.retain(|id| alive.contains(id));
        self.issues = self.find_issues(graph);
        // pictures for others to see: no MAC addresses (Bluetooth devices are named by them)
        if self.private {
            for n in &mut self.model.nodes {
                (n.name, n.desc) = (mask_macs(&n.name), mask_macs(&n.desc));
            }
            for o in &mut self.objects {
                (o.2, o.3) = (mask_macs(&o.2), mask_macs(&o.3));
            }
            for i in &mut self.issues {
                (i.title, i.hint) = (mask_macs(&i.title), mask_macs(&i.hint));
            }
        }
        if let Some(s) = self.sel
            && !self.exists(s)
        {
            self.sel = None;
        }
    }

    fn exists(&self, s: Sel) -> bool {
        match s {
            Sel::Node(id) => self.model.node(id).is_some(),
            Sel::Port(id) => self.model.port(id).is_some(),
            Sel::Link(id) => self.model.links.iter().any(|l| l.id == id),
            Sel::Obj(id) => self.objects.iter().any(|o| o.1 == id),
        }
    }

    /// What looks wrong, or is worth knowing, worst first.
    fn find_issues(&self, graph: &Graph) -> Vec<Issue> {
        let m = &self.model;
        let mut v = Vec::new();
        for n in &m.nodes {
            if n.state == "error" {
                // PipeWire's own words for why, when it gives them
                let why = graph.failed(n.id).filter(|w| !w.trim().is_empty()).map_or_else(|| "PipeWire gives no reason: its device may be busy or gone.".to_owned(), |w| format!("PipeWire says: {w}"));
                v.push(Issue { level: 2, tag: "ERROR", node: Some(n.id), title: format!("{} is in error", n.desc), hint: why });
            }
            if let Some((x, at)) = self.xruns.get(&n.id)
                && *x > 0
            {
                let q = m.group(n.id).map_or(0, |g| g.quantum);
                v.push(Issue {
                    level: 2,
                    tag: "XRUNS",
                    node: Some(n.id),
                    title: format!("{}: {x} xrun{}, the last {} s ago", n.desc, if *x == 1 { "" } else { "s" }, at.elapsed().as_secs()),
                    hint: format!("A cycle missed its deadline: a click. Its group runs at quantum {q}: a larger one (clock.force-quantum) gives more room; or look for a node with a high B/Q in TOP."),
                });
            }
        }
        for l in &m.links {
            if l.state == "error" {
                let why = graph.failed(l.id).filter(|w| !w.trim().is_empty()).map_or_else(|| "PipeWire gives no reason: often its two ends could not agree on a format.".to_owned(), |w| format!("PipeWire says: {w}"));
                v.push(Issue { level: 2, tag: "LINK", node: Some(l.out_node), title: format!("link {} is in error", l.id), hint: why });
            }
        }
        for g in &m.groups {
            if g.load > 0.7 {
                let d = m.node(g.driver).map_or("?".into(), |n| n.desc.clone());
                v.push(Issue { level: 1, tag: "DSP LOAD", node: Some(g.driver), title: format!("{d}'s cycle is {:.0}% busy", g.load * 100.0), hint: "Close to its deadline: xruns come next. A larger quantum or fewer effects help.".into() });
            }
            for id in &g.members {
                let (Some(n), Some(t)) = (m.node(*id), m.timing.get(id)) else { continue };
                if *id != g.driver && matches!(n.kind, NKind::Stream | NKind::Device) && t.latency.1 > 0 && t.latency.1 != g.rate {
                    v.push(Issue {
                        level: 1,
                        tag: "RESAMPLING",
                        node: Some(*id),
                        title: format!("{} runs at {} Hz, its driver at {} Hz", n.desc, t.latency.1, g.rate),
                        hint: format!("PipeWire converts it on the fly (a little CPU, not bit-perfect). Adding {} to clock.allowed-rates lets the clock follow it.", t.latency.1),
                    });
                }
            }
        }
        v.sort_by_key(|i| std::cmp::Reverse(i.level));
        v
    }

    // ---------------- layout of the graph ----------------

    fn visible(&self, n: &ENode) -> bool {
        (0..SHOW.len()).all(|i| self.show[i] || !self.shown_by(i, n))
    }

    /// Whether ALSO SHOW's switch `i` is what shows node `n` (PATCHCORD, MIDI, NOT RUNNING,
    /// INTERNAL).
    fn shown_by(&self, i: usize, n: &ENode) -> bool {
        let running = self.model.group_of.contains_key(&n.id);
        match i {
            0 => matches!(n.kind, NKind::Meter | NKind::Cue),
            1 => n.kind == NKind::Midi,
            2 => !running,
            _ => n.kind == NKind::Driver && !running,
        }
    }

    // ---------------- FIND ----------------

    /// Whether node `n` matches what is typed in FIND: its name, description, id, class, or a
    /// port's name, whatever the case.
    fn matches(&self, n: &ENode) -> bool {
        let t = self.find.trim().to_lowercase();
        !t.is_empty()
            && (n.desc.to_lowercase().contains(&t) || n.name.to_lowercase().contains(&t) || n.id.to_string() == t || n.class.to_lowercase().contains(&t) || n.ins.iter().chain(&n.outs).any(|p| p.name.to_lowercase().contains(&t)))
    }

    /// The blocks FIND picks out, as they stand in the graph (top to bottom, left to right):
    /// while its card is open, of the ones shown.
    fn found(&self) -> Vec<u32> {
        if !self.find_open {
            return Vec::new();
        }
        let mut v: Vec<(f64, f64, u32)> = self.model.nodes.iter().filter(|n| self.visible(n) && self.matches(n)).filter_map(|n| self.lay.pos.get(&n.id).map(|&(x, y)| (y, x, n.id))).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().map(|(_, _, id)| id).collect()
    }

    /// Whether FIND's card is open: what is typed goes to its field.
    pub fn finding(&self) -> bool {
        self.on && self.find_open
    }

    /// Opens or closes FIND · SHOW's card (one card at a time: the legend closes).
    pub fn set_find(&mut self, on: bool) {
        self.find_open = on;
        if on {
            self.legend_open = false;
        }
        self.dirty = true;
    }

    /// What FIND's field has; changed, nothing is gone to yet.
    pub fn find_text(&mut self) -> &mut String {
        self.dirty = true;
        &mut self.find
    }

    /// Goes to the next block FIND picks out (or the previous one, `d` -1): chosen, its details
    /// open, the graph moved to show it.
    pub fn find_step(&mut self, d: i32) {
        let found = self.found();
        if found.is_empty() {
            return;
        }
        let at = match self.sel {
            Some(Sel::Node(id)) => found.iter().position(|&f| f == id),
            _ => None,
        };
        let n = found.len() as i32;
        let next = match at {
            Some(i) => (i as i32 + d).rem_euclid(n),
            None if d < 0 => n - 1,
            None => 0,
        };
        self.select(Some(Sel::Node(found[next as usize])));
        self.reveal = true;
    }

    /// FIND · SHOW's card: the field (what is typed, a caret), how many match and which one
    /// is chosen, ▲ ▼; what a switch hides that matches, with a key to show it; ALSO SHOW's
    /// switches, each its light, name, what it shows and how many. Its SVG, size and keys (in it).
    fn find_svg(&self) -> (String, f64, f64, Vec<(Hit, [f64; 4])>) {
        const W: f64 = 380.0;
        let mut o = String::new();
        let mut hits = Vec::new();
        let head = |o: &mut String, y: f64, t: &str, r: &str| {
            let _ = write!(o, r##"<text x="14" y="{y}" font-size="9" font-weight="bold" letter-spacing="1.2" fill="#7d8085">{t}</text><text x="{}" y="{y}" font-size="9" fill="#7d8085" text-anchor="end">{r}</text>"##, W - 14.0);
        };
        head(&mut o, 22.0, "FIND", "Ctrl+F · Esc");
        // the field, ready to type, a caret after what is typed
        let fx = 14.0;
        let fw = W - 14.0 - fx - 54.0 - 2.0 * 30.0;
        let typed = cut(&self.find, ((fw - 24.0) / 7.2) as usize);
        let _ = write!(
            o,
            r##"<rect x="{fx}" y="30" width="{fw}" height="24" rx="3" fill="#0e0f10" stroke="#e58f2c"/><text x="{}" y="46" font-size="12" fill="#e6e7e8" xml:space="preserve">{}<tspan fill="#e58f2c">▏</tspan></text>"##,
            fx + 8.0,
            esc(&typed)
        );
        if self.find.is_empty() {
            let _ = write!(o, r##"<text x="{}" y="46" font-size="12" fill="#55585b">a name, an id, a class, a port</text>"##, fx + 14.0);
        }
        let found = self.found();
        let at = match self.sel {
            Some(Sel::Node(id)) => found.iter().position(|&f| f == id),
            _ => None,
        };
        let cnt = if self.find.trim().is_empty() {
            String::new()
        } else if found.is_empty() {
            "none".into()
        } else {
            format!("{} of {}", at.map_or("–".into(), |i| (i + 1).to_string()), found.len())
        };
        let _ = write!(o, r##"<text x="{}" y="46" font-size="11" fill="#a8abae" text-anchor="end">{cnt}</text>"##, fx + fw + 52.0);
        for (k, (d, arrow)) in [(-1, "M-4,2 l4,-4 l4,4"), (1, "M-4,-2 l4,4 l4,-4")].into_iter().enumerate() {
            let bx = W - 14.0 - 54.0 + 30.0 * k as f64 - 6.0 + 6.0;
            let _ = write!(
                o,
                r##"<rect x="{bx}" y="30" width="24" height="24" rx="3" fill="#2a2b2e" stroke="#0b0b0c"/><path transform="translate({},42)" d="{arrow}" fill="none" stroke="#e6e7e8" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"##,
                bx + 12.0
            );
            hits.push((Hit::FindStep(d), [bx, 30.0, 24.0, 24.0]));
        }
        // what matches but a switch hides: said, and a key to show it
        let off: Vec<&ENode> = self.model.nodes.iter().filter(|n| !self.visible(n) && self.matches(n)).collect();
        let mut y = 72.0;
        if off.is_empty() {
            let t = if !self.find.trim().is_empty() && found.is_empty() { "nothing by that: names, ids, classes, ports" } else { "in names, ids, classes and ports' names" };
            let _ = write!(o, r##"<text x="14" y="{y}" font-size="10.5" fill="#7d8085">{t}</text>"##);
        } else {
            let why: Vec<usize> = (0..SHOW.len()).filter(|&i| !self.show[i] && off.iter().any(|n| self.shown_by(i, n))).collect();
            let t = format!("{} more hidden by {}", off.len(), why.iter().map(|&i| SHOW[i].0).collect::<Vec<_>>().join(", "));
            let _ = write!(o, r##"<text x="14" y="{y}" font-size="10.5" fill="#7d8085">{}</text>"##, esc(&t));
            let mut bx = 14.0;
            y += 8.0;
            for &i in &why {
                let label = format!("SHOW {}", SHOW[i].0);
                let bw = text::width(text::BOLD, &label, 9.0, 0.4) + 14.0;
                let _ = write!(o, r##"<rect x="{bx}" y="{y}" width="{bw}" height="18" rx="3" fill="#2a2b2e" stroke="#0b0b0c"/><text x="{}" y="{}" font-family="Noto Sans" font-weight="bold" font-size="9" letter-spacing=".4" fill="#e6e7e8">{}</text>"##, bx + 7.0, y + 12.5, esc(&label));
                hits.push((Hit::Show(i), [bx, y, bw, 18.0]));
                bx += bw + 6.0;
            }
            y += 18.0;
        }
        // ALSO SHOW: a row a switch
        y += 22.0;
        head(&mut o, y, "ALSO SHOW", "");
        y += 6.0;
        let what = ["Patchcord's own signal meters", "MIDI nodes", "suspended devices, idle streams", "Dummy-Driver, Freewheel-Driver"];
        for (i, (name, _)) in SHOW.iter().enumerate() {
            let n = self.model.nodes.iter().filter(|n| self.shown_by(i, n)).count();
            let _ = write!(
                o,
                r##"<path d="M14,{y} H{}" stroke="#26272a"/><circle cx="19" cy="{}" r="3.5" fill="{}"/><text x="30" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".5" fill="#e6e7e8">{name}</text><text x="134" y="{}" font-size="10.5" fill="#7d8085">{}</text><text x="{}" y="{}" font-size="11" fill="#a8abae" text-anchor="end">{n}</text>"##,
                W - 14.0,
                y + 13.0,
                if self.show[i] { "#ff4a32" } else { "#3a1512" },
                y + 17.0,
                y + 17.0,
                what[i],
                W - 14.0,
                y + 17.0
            );
            hits.push((Hit::Show(i), [14.0, y, W - 28.0, 26.0]));
            y += 26.0;
        }
        let h = y + 10.0;
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{h}" viewBox="0 0 {W} {h}" font-family="Noto Sans Mono"><rect x=".5" y=".5" width="{}" height="{}" rx="6" fill="#1d1e21" stroke="#0b0b0c"/>{o}</svg>"##,
            W - 1.0,
            h - 1.0
        );
        (svg, W, h, hits)
    }

    fn layout(&self) -> Layout {
        let m = &self.model;
        let mut bands: Vec<(Option<usize>, &'static str, Vec<u32>)> = Vec::new();
        for (gi, g) in m.groups.iter().enumerate() {
            let ns: Vec<u32> = g.members.iter().copied().filter(|id| m.group_of.get(id) == Some(&gi) && m.node(*id).is_some_and(|n| self.visible(n))).collect();
            bands.push((Some(gi), "", ns));
        }
        bands.push((None, "idle", m.nodes.iter().filter(|n| !m.group_of.contains_key(&n.id) && n.kind != NKind::Driver && self.visible(n)).map(|n| n.id).collect()));
        bands.push((None, "drivers", m.nodes.iter().filter(|n| !m.group_of.contains_key(&n.id) && n.kind == NKind::Driver && self.visible(n)).map(|n| n.id).collect()));
        // the links shown: both ends shown (in any band)
        let shown: HashSet<u32> = bands.iter().flat_map(|b| b.2.iter().copied()).collect();
        let all: Vec<&ELink> = m.links.iter().filter(|l| shown.contains(&l.out_node) && shown.contains(&l.in_node) && l.out_node != l.in_node).collect();
        let mut lay = Layout::default();
        let mut y = 0.0;
        for (gi, what, ids) in bands {
            if ids.is_empty() {
                continue;
            }
            let set: HashSet<u32> = ids.iter().copied().collect();
            let ls: Vec<&ELink> = all.iter().copied().filter(|l| set.contains(&l.out_node) && set.contains(&l.in_node)).collect();
            let (place, w, h) = self.band(&ids, &ls, &all);
            for (id, (dx, dy)) in place {
                lay.pos.insert(id, (BPAD + dx, y + BHEAD + dy));
            }
            lay.w = lay.w.max(BPAD + w + BPAD);
            let bh = BHEAD + h + 14.0;
            lay.bands.push((y, bh, gi, what));
            y += bh + 26.0;
        }
        // room for a driver's header line
        if !m.groups.is_empty() {
            lay.w = lay.w.max(980.0);
        }
        lay.h = y;
        lay
    }

    /// A band's nodes by their links, as they are now, in four groups left to right: no links;
    /// only links out; links in and out, a column further than the longest way to it through
    /// them; only links in, Patchcord's listeners (its meters, the cue's in side) after the rest.
    /// Each group in as many columns (none to several) as fit the band best into a wide view; then
    /// no link goes through another's port (see `unblock`). `ls`: the band's links, `all`: every
    /// link shown (which group a node is in). Each node's place from the band's top left, the
    /// band's width and height.
    fn band(&self, ids: &[u32], ls: &[&ELink], all: &[&ELink]) -> (HashMap<u32, (f64, f64)>, f64, f64) {
        let m = &self.model;
        let kind = |id: u32| m.node(id).map_or(NKind::Other, |n| n.kind);
        let desc = |id: u32| m.node(id).map_or("", |n| n.desc.as_str());
        let tall = |id: u32| m.node(id).map_or(60.0, node_h);
        let listener = |id: u32| m.node(id).is_some_and(|n| n.kind == NKind::Meter || (n.kind == NKind::Cue && n.name == CUE_NODE));
        let group = |id: u32| match (listener(id), all.iter().any(|l| l.in_node == id), all.iter().any(|l| l.out_node == id)) {
            (true, ..) | (false, true, false) => 3,
            (false, false, false) => 0,
            (false, false, true) => 1,
            (false, true, true) => 2,
        };
        let g: HashMap<u32, u8> = ids.iter().map(|&i| (i, group(i))).collect();
        let of = |k: u8| ids.iter().copied().filter(|i| g[i] == k).collect::<Vec<u32>>();
        // the middle: a column further than the longest way to it through the middle
        let mut depth: BTreeMap<u32, usize> = of(2).into_iter().map(|i| (i, 0)).collect();
        // (a loop back, feedback, doesn't push the loop on and on to the right: the links that
        // close a loop, found walking the middle in order of ids, don't count)
        let mut next: BTreeMap<u32, std::collections::BTreeSet<u32>> = BTreeMap::new();
        for l in ls.iter().filter(|l| depth.contains_key(&l.out_node) && depth.contains_key(&l.in_node)) {
            next.entry(l.out_node).or_default().insert(l.in_node);
        }
        let mut back: HashSet<(u32, u32)> = HashSet::new();
        let mut state: HashMap<u32, u8> = HashMap::new();
        for &root in depth.keys() {
            if state.contains_key(&root) {
                continue;
            }
            // (iterative: the node, and where it is in its links out)
            let mut stack: Vec<(u32, Vec<u32>)> = vec![(root, next.get(&root).map(|n| n.iter().copied().collect()).unwrap_or_default())];
            state.insert(root, 1);
            while let Some((at, todo)) = stack.last_mut() {
                let at = *at;
                match todo.pop() {
                    Some(to) => match state.get(&to) {
                        Some(1) => {
                            back.insert((at, to));
                        }
                        Some(_) => {}
                        None => {
                            state.insert(to, 1);
                            let mut n: Vec<u32> = next.get(&to).map(|n| n.iter().copied().collect()).unwrap_or_default();
                            n.reverse();
                            stack.push((to, n));
                        }
                    },
                    None => {
                        state.insert(at, 2);
                        stack.pop();
                    }
                }
            }
        }
        let mid: Vec<(u32, u32)> = next.iter().flat_map(|(a, bs)| bs.iter().map(move |b| (*a, *b))).filter(|e| !back.contains(e)).collect();
        for _ in 0..depth.len() {
            for &(a, b) in &mid {
                let d = depth[&a] + 1;
                if depth[&b] < d {
                    depth.insert(b, d);
                }
            }
        }
        let mids: Vec<Vec<u32>> = (0..depth.values().max().map_or(0, |d| d + 1)).map(|d| depth.iter().filter(|(_, v)| **v == d).map(|(k, _)| *k).collect()).collect();
        let stack = |c: &[u32]| c.iter().map(|&i| tall(i)).sum::<f64>() + NGAP * c.len().saturating_sub(1) as f64;
        let by_kind = |a: &u32, b: &u32| kind(*a).cmp(&kind(*b)).then(desc(*a).cmp(desc(*b))).then(a.cmp(b));
        let out_y = |l: &ELink| m.node(l.out_node)?.outs.iter().position(|p| p.id == l.out_port).map(port_y);
        // the height its links come in at (from the columns already placed): fewer crossings
        let bary = |id: u32, at: &HashMap<u32, f64>| {
            let ys: Vec<f64> = ls.iter().filter(|l| l.in_node == id).filter_map(|l| Some(at.get(&l.out_node)? + out_y(l)?)).collect();
            if ys.is_empty() { 1e6 + kind(id) as u8 as f64 } else { ys.iter().sum::<f64>() / ys.len() as f64 }
        };
        // the columns, each group going on in the next column past `cap` tall
        let columns = |cap: f64| -> Vec<Vec<u32>> {
            let mut at: HashMap<u32, f64> = HashMap::new();
            let mut cols = Vec::new();
            let lists = [(of(0), 0u8), (of(1), 0)].into_iter().chain(mids.iter().map(|c| (c.clone(), 1))).chain([(of(3), 2)]);
            for (mut c, how) in lists {
                if c.is_empty() {
                    continue;
                }
                match how {
                    0 => c.sort_by(by_kind),
                    // (each one's height worked out once, not at every comparison)
                    _ => {
                        let mut keyed: Vec<(bool, f64, u32)> = c.iter().map(|&i| (how == 2 && listener(i), bary(i, &at), i)).collect();
                        keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(by_kind(&a.2, &b.2)));
                        c = keyed.into_iter().map(|k| k.2).collect();
                    }
                }
                let (mut col, mut y) = (Vec::new(), 0.0);
                for i in c {
                    if !col.is_empty() && y + tall(i) > cap {
                        cols.push(std::mem::take(&mut col));
                        y = 0.0;
                    }
                    col.push(i);
                    at.insert(i, y);
                    y += tall(i) + NGAP;
                }
                cols.push(col);
            }
            cols
        };
        let size = |cols: &[Vec<u32>]| (cols.len().saturating_sub(1) as f64 * COLW + NW, cols.iter().map(|c| stack(c)).fold(0.0, f64::max));
        // as tall as needs the least zoom to fit a wide view; never lower than the middle's
        // tallest column (its depth is its own) or the tallest node
        let low = mids.iter().map(|c| stack(c)).chain(ids.iter().map(|&i| tall(i))).fold(0.0, f64::max);
        let top = stack(ids).max(low);
        let step = ((top - low) / 100.0).max(10.0);
        let mut best: Option<(f64, Vec<Vec<u32>>)> = None;
        let mut cap = low;
        while cap <= top + 1.0 {
            let cols = columns(cap);
            let (w, h) = size(&cols);
            let score = (w / ASPECT).max(h);
            if best.as_ref().is_none_or(|b| score < b.0 - 1e-6) {
                best = Some((score, cols));
            }
            cap += step;
        }
        let cols = best.map(|b| b.1).unwrap_or_default();
        let w = size(&cols).0;
        let pos = self.unblock(cols, ls);
        let h = pos.iter().map(|(i, (_, y))| y + tall(*i)).fold(0.0, f64::max);
        (pos, w, h)
    }

    /// Places `cols` (node ids, top down) so that no link goes through a port that isn't one of
    /// its two ends (it would look like it went in there): neighbours in a column change places
    /// while that helps; what is left, of the blocks that can set a line free (the one whose port
    /// it goes through, the line's two ends) the one that sets most free with the least step
    /// down goes down, by half rows (the ports of the columns stay in step). Always possible
    /// (far enough down, a block is clear of any line); the work is bounded all the same.
    fn unblock(&self, mut cols: Vec<Vec<u32>>, ls: &[&ELink]) -> HashMap<u32, (f64, f64)> {
        let m = &self.model;
        let tall = |id: u32| m.node(id).map_or(60.0, node_h);
        let mut down: HashMap<u32, f64> = HashMap::new();
        let place = |cols: &[Vec<u32>], down: &HashMap<u32, f64>| {
            let mut pos = HashMap::new();
            for (ci, c) in cols.iter().enumerate() {
                let mut y = 0.0;
                for &i in c {
                    y += down.get(&i).copied().unwrap_or(0.0);
                    pos.insert(i, (ci as f64 * COLW, y));
                    y += tall(i) + NGAP;
                }
            }
            pos
        };
        // (each look at the lines costs one of these)
        let mut budget = 800usize;
        let mut n = self.through(&place(&cols, &down), ls).len();
        while n > 0 && budget > 0 {
            let mut better: Option<(usize, usize, usize)> = None;
            for ci in 0..cols.len() {
                for k in 0..cols[ci].len().saturating_sub(1) {
                    if budget == 0 {
                        break;
                    }
                    budget -= 1;
                    cols[ci].swap(k, k + 1);
                    let got = self.through(&place(&cols, &down), ls).len();
                    cols[ci].swap(k, k + 1);
                    if got < n && better.is_none_or(|b| got < b.0) {
                        better = Some((got, ci, k));
                    }
                }
            }
            let Some((got, ci, k)) = better else { break };
            cols[ci].swap(k, k + 1);
            n = got;
        }
        let step = PR / 2.0;
        while budget > 0 {
            let hits = self.through(&place(&cols, &down), ls);
            if hits.is_empty() {
                break;
            }
            let free: std::collections::BTreeSet<u32> = hits.iter().flat_map(|&(l, owner)| [owner, l.out_node, l.in_node]).collect();
            let mut best: Option<(usize, usize, u32)> = None;
            'blocks: for i in free {
                for k in 1..=12 {
                    if budget == 0 {
                        break 'blocks;
                    }
                    budget -= 1;
                    *down.entry(i).or_default() += step * k as f64;
                    let got = self.through(&place(&cols, &down), ls).len();
                    *down.entry(i).or_default() -= step * k as f64;
                    if got < hits.len() {
                        if best.is_none_or(|b| (got, k) < (b.0, b.1)) {
                            best = Some((got, k, i));
                        }
                        break;
                    }
                }
            }
            let Some((_, k, i)) = best else { break };
            *down.entry(i).or_default() += step * k as f64;
        }
        place(&cols, &down)
    }

    /// The links that go through a port not theirs (a line this close to its dot), and whose
    /// port it is, with the nodes at `pos`.
    fn through<'a>(&self, pos: &HashMap<u32, (f64, f64)>, ls: &[&'a ELink]) -> Vec<(&'a ELink, u32)> {
        const DOT: f64 = 6.0;
        let m = &self.model;
        let mut dots = Vec::new();
        for (&id, &(x, y)) in pos {
            let Some(n) = m.node(id) else { continue };
            dots.extend(n.ins.iter().enumerate().map(|(i, p)| ((x, y + port_y(i)), id, p.id)));
            dots.extend(n.outs.iter().enumerate().map(|(i, p)| ((x + NW, y + port_y(i)), id, p.id)));
        }
        let at = |node: u32, port: u32| {
            let (x, y) = *pos.get(&node)?;
            let n = m.node(node)?;
            n.ins.iter().position(|p| p.id == port).map(|i| (x, y + port_y(i))).or_else(|| n.outs.iter().position(|p| p.id == port).map(|i| (x + NW, y + port_y(i))))
        };
        let mut found = Vec::new();
        for &l in ls {
            let (Some(a), Some(b)) = (at(l.out_node, l.out_port), at(l.in_node, l.in_port)) else { continue };
            let c = link_curve(a, b);
            let pts: Vec<(f64, f64)> = (0..=32).map(|k| bezier(&c, k as f64 / 32.0)).collect();
            let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
            let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
            for &((dx, dy), owner, port) in &dots {
                if port == l.out_port || port == l.in_port || dx < x0 - DOT || dx > x1 + DOT || dy < y0 - DOT || dy > y1 + DOT {
                    continue;
                }
                if pts.windows(2).any(|w| seg_dist((dx, dy), w[0], w[1]) < DOT) {
                    found.push((l, owner));
                }
            }
        }
        found
    }

    fn port_xy(&self, lay: &Layout, node: u32, port: u32) -> Option<(f64, f64)> {
        let (x, y) = *lay.pos.get(&node)?;
        let n = self.model.node(node)?;
        if let Some(i) = n.ins.iter().position(|p| p.id == port) {
            return Some((x, y + port_y(i)));
        }
        let i = n.outs.iter().position(|p| p.id == port)?;
        Some((x + NW, y + port_y(i)))
    }

    // ---------------- drawing ----------------

    /// The selected nodes (a node; a port's node; a device's or a client's nodes), and the nodes
    /// they are linked to (a link's two ends).
    fn focus(&self) -> (HashSet<u32>, HashSet<u32>) {
        let m = &self.model;
        let prim: HashSet<u32> = match self.sel {
            Some(Sel::Node(id)) => [id].into(),
            Some(Sel::Port(id)) => m.port_node.get(&id).copied().into_iter().collect(),
            Some(Sel::Obj(id)) => self.nodes_of(id).into_iter().collect(),
            _ => HashSet::new(),
        };
        let mut near = HashSet::new();
        for l in &m.links {
            if prim.contains(&l.out_node) || prim.contains(&l.in_node) || self.sel == Some(Sel::Link(l.id)) {
                near.insert(l.out_node);
                near.insert(l.in_node);
            }
        }
        (prim, near)
    }

    /// Where the selection is in the graph (world units): its nodes, or a link's two ends.
    fn sel_box(&self) -> Option<(f64, f64, f64, f64)> {
        let lay = &self.lay;
        let mut pts: Vec<(f64, f64)> = Vec::new();
        if let Some(Sel::Link(id)) = self.sel {
            let l = self.model.links.iter().find(|l| l.id == id)?;
            pts.extend(self.port_xy(lay, l.out_node, l.out_port));
            pts.extend(self.port_xy(lay, l.in_node, l.in_port));
        } else {
            for id in self.focus().0 {
                if let (Some(&(x, y)), Some(n)) = (lay.pos.get(&id), self.model.node(id)) {
                    pts.push((x, y));
                    pts.push((x + NW, y + node_h(n)));
                }
            }
        }
        let f = |a: fn(f64, f64) -> f64, i: usize| pts.iter().map(|p| if i == 0 { p.0 } else { p.1 }).reduce(a);
        Some((f(f64::min, 0)?, f(f64::min, 1)?, f(f64::max, 0)?, f(f64::max, 1)?))
    }

    /// A device's nodes, or a client's (by their `device.id` or `client.id`).
    fn nodes_of(&self, obj: u32) -> Vec<u32> {
        let id = obj.to_string();
        self.model.nodes.iter().filter(|n| self.props.get(&n.id).is_some_and(|p| p.get("device.id") == Some(&id) || p.get("client.id") == Some(&id))).map(|n| n.id).collect()
    }

    /// The graph; with `numbers`, also what changes every moment (each node's time, each group's
    /// load), which on screen are their own small pictures (see `numbers`).
    fn graph_svg(&self, numbers: bool) -> String {
        let m = &self.model;
        let lay = &self.lay;
        let (w, h) = (lay.w.max(10.0), lay.h.max(10.0));
        let mut o = String::new();
        for (y, bh, gi, what) in &lay.bands {
            // (the layout may be a moment older than the groups: one gone is drawn as no group)
            let (col, head) = match gi.and_then(|gi| m.groups.get(gi)) {
                Some(g) => {
                    let d = m.node(g.driver);
                    (
                        // (a driver not known yet: nothing said of it, NO CLASS)
                        d.map_or_else(|| rgb(face::kind_colour("NO CLASS")), |n| n.color),
                        format!(
                            r##"<tspan fill="#e58f2c" letter-spacing="1">DRIVER</tspan>  {}  <tspan fill="#7d8085" font-weight="normal">{}{}</tspan>"##,
                            esc(&d.map_or("?".into(), |n| n.desc.clone())),
                            esc(&band_rest(g, d)),
                            if numbers { band_load(g) } else { String::new() }
                        ),
                    )
                }
                None if *what == "idle" => ([0x3a, 0x3c, 0x40], r##"<tspan fill="#e58f2c" letter-spacing="1">NOT RUNNING</tspan>  <tspan fill="#7d8085" font-weight="normal">no driver clocks these now: suspended devices, idle streams. They use no CPU</tspan>"##.to_owned()),
                None => ([0x3a, 0x3c, 0x40], r##"<tspan fill="#e58f2c" letter-spacing="1">INTERNAL DRIVERS</tspan>  <tspan fill="#7d8085" font-weight="normal">clocks PipeWire keeps for nodes without a device, and for freewheeling</tspan>"##.to_owned()),
            };
            let c = hex(col);
            let _ = write!(
                o,
                r##"<rect x="0" y="{y}" width="{w}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="{c}" stroke-opacity="{}" stroke-width="1.2"{}/><text x="{BPAD}" y="{}" font-family="Noto Sans Mono" font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve">{head}</text>"##,
                if gi.is_some() { 0.35 } else { 0.5 },
                if gi.is_some() { "" } else { r#" stroke-dasharray="5 5""# },
                y + 22.0
            );
        }
        let (focus, near) = self.focus();
        // a link in the hand: the ports (blocks) that take it stand out, the rest fade; the port
        // it comes from and the one it is on are lit, as chosen ones
        let carry = self.carry_hint();
        let (cands, cand_nodes) = self.carry_sets();
        let carry_lit: HashSet<u32> = match carry {
            Some((Carry::Port(a), snap)) => std::iter::once(a).chain(snap).collect(),
            _ => HashSet::new(),
        };
        let carry_target = match carry {
            Some((Carry::Node(_), snap)) => snap,
            _ => None,
        };
        // what FIND picks out: amber, as chosen; the rest of the graph fades
        let found: HashSet<u32> = self.found().into_iter().collect();
        let any = carry.is_none() && self.sel.is_some() && (!focus.is_empty() || matches!(self.sel, Some(Sel::Link(_))));
        // what is chosen turns the LCD's amber: a node where it shows its colour (frame, header,
        // ports, the links out of it), a port as a whole, a link and the ports at its ends
        let chosen_node = |id: u32| focus.contains(&id) && matches!(self.sel, Some(Sel::Node(_) | Sel::Obj(_)));
        let lit_ports: HashSet<u32> = match self.sel {
            Some(Sel::Port(p)) => [p].into(),
            Some(Sel::Link(id)) => m.links.iter().filter(|l| l.id == id).flat_map(|l| [l.out_port, l.in_port]).collect(),
            _ => HashSet::new(),
        };
        // what the cue hears: the ports linked into its in side
        let cue_in = m.nodes.iter().find(|n| n.kind == NKind::Cue && n.name == CUE_NODE).map(|n| n.id);
        let heard: HashSet<u32> = m.links.iter().filter(|l| Some(l.in_node) == cue_in).map(|l| l.out_port).collect();
        for l in &m.links {
            let (Some(a), Some(b)) = (self.port_xy(lay, l.out_node, l.out_port), self.port_xy(lay, l.in_node, l.in_port)) else { continue };
            let c = link_curve(a, b);
            let d = format!("M{},{} C{},{} {},{} {},{}", c[0].0, c[0].1, c[1].0, c[1].1, c[2].0, c[2].1, c[3].0, c[3].1);
            let hl = self.sel == Some(Sel::Link(l.id)) || focus.contains(&l.out_node) || focus.contains(&l.in_node) || self.sel.is_some_and(|s| s == Sel::Port(l.out_port) || s == Sel::Port(l.in_port));
            // the colour of the block the sound comes out of; the state in its width, dashes and fading
            let from = m.node(l.out_node).and_then(|n| n.outs.iter().find(|p| p.id == l.out_port)).map_or_else(|| face::kind_colour("NO CLASS").to_owned(), |p| face::role_colour(p.role).to_owned());
            let (stroke, width, extra) = if l.state == "error" {
                ("#ff4a32".to_owned(), 2.0, "")
            } else if l.passive {
                (from, 1.2, r#" stroke-dasharray="3 4""#)
            } else if l.state == "active" {
                (from, 2.0, "")
            } else {
                (from, 1.6, r#" stroke-opacity=".45""#)
            };
            let op = if carry.is_some() { 0.35 } else if !found.is_empty() { if found.contains(&l.out_node) || found.contains(&l.in_node) { 1.0 } else { 0.2 } } else if any && !hl { 0.2 } else { 1.0 };
            let (stroke, width) = if self.sel == Some(Sel::Link(l.id)) {
                (SEL.to_owned(), 2.6)
            } else if chosen_node(l.out_node) && l.state != "error" {
                (SEL.to_owned(), width)
            } else {
                (stroke, width)
            };
            let _ = write!(o, r#"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{width}" opacity="{op}"{extra}/>"#);
        }
        for n in &m.nodes {
            let Some(&(x, y)) = lay.pos.get(&n.id) else { continue };
            let nh = node_h(n);
            let on = focus.contains(&n.id);
            let dim = if !found.is_empty() && carry.is_none() { !found.contains(&n.id) } else { (any && !on && !near.contains(&n.id)) || (carry.is_some() && !cand_nodes.contains(&n.id)) };
            let running = m.group_of.contains_key(&n.id);
            let op = if dim { if carry.is_some() { 0.3 } else { 0.35 } } else if running { 1.0 } else { 0.72 };
            let xr = self.xruns.get(&n.id).is_some_and(|x| x.0 > 0);
            let g = m.group(n.id);
            let t = m.timing.get(&n.id);
            let mut tags = Vec::new();
            if n.desk.as_ref().is_some_and(|d| d.3) {
                tags.push("HIDDEN ON DESK".to_owned());
            }
            if n.system {
                tags.push("SYSTEM".into());
            }
            if matches!(n.kind, NKind::Meter | NKind::Cue) {
                tags.push("PATCHCORD".into());
            }
            if g.is_some_and(|g| g.driver == n.id) {
                tags.push("DRIVER".into());
            }
            if let (Some(g), Some(t)) = (g, t)
                && g.driver != n.id
                && t.latency.1 > 0
                && t.latency.1 != g.rate
                && matches!(n.kind, NKind::Stream | NKind::Device)
            {
                tags.push(format!("{}→{}", t.latency.1, g.rate));
            }
            let tag_s = cut(&tags.join(" · "), 26);
            let tag_w = if tags.is_empty() { 0.0 } else { text::width(text::BOLD, &tag_s, 8.5, 0.5) + 8.0 };
            let class_n = (((NW - 22.0 - tag_w) / 6.05) as usize).saturating_sub(n.id.to_string().len() + 3).max(4);
            let picked = chosen_node(n.id) || carry_target == Some(n.id) || found.contains(&n.id);
            let c = if picked { SEL.to_owned() } else { hex(n.color) };
            // the head as a frame's band on the rack and in MIXER: the kind's colour across the
            // block's top, the name on it in the band's lettering, the light at its left; under
            // it, on the block, the id and class and the tags. Then the frame (how long it works
            // a cycle is in TOP, not here)
            let ink = if picked { SEL_TEXT } else if hex(n.color) == face::kind_colour("NO CLASS") { "#1d1e20" } else { "#f3efe6" };
            let room = NW - 32.0;
            let name = cut(&n.desc.to_uppercase(), (room / 8.4) as usize);
            let _ = write!(
                o,
                r##"<g transform="translate({x},{y})" opacity="{op}"><rect x=".75" y=".75" width="{}" height="{}" rx="5.25" fill="#1d1e21"/><path d="M0,{HEAD_H} V6 A6,6 0 0 1 6,0 H{} A6,6 0 0 1 {NW},6 V{HEAD_H} Z" fill="{c}"/><path d="M0,{} H{NW}" stroke="#000" stroke-opacity=".35"/><rect width="{NW}" height="{nh}" rx="6" fill="none" stroke="{c}" stroke-width="{}"/><circle cx="12" cy="12" r="4" fill="{}" stroke="#0f1011" stroke-width="1.2"/><text x="22" y="16" font-size="11" font-weight="bold" letter-spacing="1" fill="{ink}">{}</text><text x="12" y="36" font-family="Noto Sans Mono" font-size="10" fill="#7d8085">{} · {}</text>"##,
                NW - 1.5,
                nh - 1.5,
                NW - 6.0,
                HEAD_H + 0.5,
                if picked { 2.5 } else { 1.5 },
                led_color(if running { "running" } else { n.state }, xr),
                esc(&name),
                n.id,
                esc(&cut(&n.class, class_n)),
            );
            if !tags.is_empty() {
                let _ = write!(o, r##"<text x="{}" y="36" text-anchor="end" font-size="8.5" font-weight="bold" letter-spacing=".5" fill="{}">{}</text>"##, NW - 10.0, if xr { "#ff6a52" } else { "#e58f2c" }, esc(&tag_s));
            }
            // ports: ears from the edge in the block's colour (square there, round inside), the
            // whole ear the port; inputs on the left, outputs on the right
            for (out, ports) in [(false, &n.ins), (true, &n.outs)] {
                for (i, p) in ports.iter().enumerate() {
                    let py = port_y(i);
                    let label = port_label(n, p);
                    let w = ear_w(&label);
                    let (px, tx, anchor, d) = if out {
                        (NW, NW - 10.0, r#" text-anchor="end""#, format!("M{},{} H{} A6.5,6.5 0 0 0 {},{} H{} Z", NW - 0.75, py - 6.5, NW - w + 6.5, NW - w + 6.5, py + 6.5, NW - 0.75))
                    } else {
                        (0.0, 10.0, "", format!("M0.75,{} H{} A6.5,6.5 0 0 1 {},{} H0.75 Z", py - 6.5, w - 6.5, w - 6.5, py + 6.5))
                    };
                    let _ = if lit_ports.contains(&p.id) || carry_lit.contains(&p.id) {
                        write!(o, r##"<path d="{d}" fill="{SEL}"/><circle cx="{px}" cy="{py}" r="4" fill="{SEL}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{}"{anchor} font-family="Noto Sans Mono" font-size="10" font-weight="bold" fill="{SEL_TEXT}">{}</text>"##, py + 3.5, esc(&label))
                    } else if heard.contains(&p.id) || self.cue_kept.contains(&p.id) {
                        // kept on in the cue: the whole port in its colour, and the headphones of
                        // the CUE key just inside the block from its ear
                        let ix = if out { NW - w - 9.0 } else { w + 9.0 };
                        write!(
                            o,
                            r##"<path d="{d}" fill="{CUE}"/><circle cx="{px}" cy="{py}" r="4" fill="{CUE}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{}"{anchor} font-family="Noto Sans Mono" font-size="10" font-weight="bold" fill="{CUE_TEXT}">{}</text><g transform="translate({},{}) scale(.46)" fill="none" stroke="{CUE}" stroke-width="2.6" stroke-linecap="round"><path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4.5" height="7" rx="1.5" fill="{CUE}" stroke="none"/><rect x="16.5" y="14" width="4.5" height="7" rx="1.5" fill="{CUE}" stroke="none"/></g>"##,
                            py + 3.5,
                            esc(&label),
                            ix - 5.5,
                            py - 6.3
                        )
                    } else if cands.contains(&p.id) {
                        // one that takes the link in the hand: its ear stronger, a ring round its dot
                        let c = face::role_colour(p.role);
                        write!(o, r##"<path d="{d}" fill="{c}" fill-opacity=".35"/><circle cx="{px}" cy="{py}" r="7.5" fill="none" stroke="{c}" stroke-width="1.6"/><circle cx="{px}" cy="{py}" r="4" fill="{c}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{}"{anchor} font-family="Noto Sans Mono" font-size="10" fill="#e6e7e8">{}</text>"##, py + 3.5, esc(&label))
                    } else {
                        // a port in its group's colour (the block's own when it is chosen)
                        let c = if picked { c.as_str() } else { face::role_colour(p.role) };
                        write!(o, r##"<path d="{d}" fill="{c}" fill-opacity=".1"/><circle cx="{px}" cy="{py}" r="4" fill="{c}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{}"{anchor} font-family="Noto Sans Mono" font-size="10" fill="#a8abae">{}</text>"##, py + 3.5, esc(&label))
                    };
                }
            }
            o += "</g>";
        }
        format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans">{o}</svg>"##)
    }

    /// What changes every moment on the graph, drawn apart so the graph isn't drawn again for it:
    /// each running node's time, each group's load. Each as (what, where its text starts or, with
    /// `end`, ends, in world units, its text, size, colour, opacity).
    fn numbers(&self) -> Vec<Num> {
        let (m, lay) = (&self.model, &self.lay);
        let mut out = Vec::new();
        for (y, _, gi, _) in &lay.bands {
            // (the layout shown may be a moment older than the groups, until its new picture
            // comes: a group gone has no numbers)
            let Some(g) = gi.and_then(|gi| m.groups.get(gi)) else { continue };
            let d = m.node(g.driver);
            // after the header drawn in the graph (all in a monospaced font)
            let x = BPAD + text::width(text::MONO_BOLD, "DRIVER", 12.0, 1.0) + text::width(text::MONO_BOLD, &format!("  {}  ", d.map_or("?", |n| n.desc.as_str())), 12.0, 0.0) + text::width(text::MONO, &band_rest(g, d), 12.0, 0.0);
            out.push(Num { key: format!("g{}", g.driver), x, y: y + 22.0, end: false, text: band_load(g), size: 12.0, fill: "#7d8085", alpha: 1.0 });
        }
        out
    }

    /// The window's parts, in logical pixels: (graph, inspector, tabs) as [x, y, w, h].
    fn areas(&self, w: f64, h: f64) -> [[f64; 4]; 3] {
        let top = panel::H;
        let bottom_h = if self.bottom_shut { 0.0 } else { BOTTOM_H.min(((h - top - panel::STATUS_H) * 0.4).max(120.0)) };
        let by = h - panel::STATUS_H - bottom_h;
        // the details: at most half the window; in a narrow one they open over the graph, which
        // keeps the whole width
        let iw = if self.sel.is_none() { 0.0 } else { INSP_W.min(w * 0.5) };
        let gw = if w < NARROW { w } else { w - iw };
        let gy = top;
        [[0.0, gy, gw, (by - gy).max(0.0)], [w - iw, gy, iw, (by - gy).max(0.0)], [0.0, by, w, bottom_h]]
    }

    /// Whether a window point (in pixels) is over the graph.
    fn in_graph(&self, p: (f64, f64), size: (f64, f64), dpi: f64) -> bool {
        let [g, _, _] = self.areas(size.0 / dpi, size.1 / dpi);
        let (x, y) = (p.0 / dpi, p.1 / dpi);
        x >= g[0] && x < g[0] + g[2] && y >= g[1] && y < g[1] + g[3]
    }

    /// Brings every part up to date and returns the pictures to draw, in order (see `Pic`: a
    /// body cut to its view, the gliding blocks cut out of the old graph and fading), and how
    /// many of them are the graph's (the link in the hand goes over those, under the panels).
    pub fn sprites(&mut self, size: (u32, u32), dpi: f64, version: &str) -> (Vec<Pic<'_>>, usize) {
        let (fw, fh) = (size.0 as f64 / dpi, size.1 as f64 / dpi);
        let [graph_a, insp_a, bottom_a] = self.areas(fw, fh);
        if self.version != version {
            version.clone_into(&mut self.version);
        }
        self.spent.clear();
        self.glide_view();
        let mut t = Instant::now();
        let mut lap = |spent: &mut Vec<(&'static str, Duration)>, what: &'static str| {
            spent.push((what, t.elapsed()));
            t = Instant::now();
        };
        // the graph: drawn again when what it shows changes, or sharp again after zooming
        let mut hsh = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(&self.model.links, self.sel, self.show, &self.cue_kept, self.carry_hint(), self.found()), &mut hsh);
        for n in &self.model.nodes {
            // (all that graph_svg draws of a node: its tags too)
            std::hash::Hash::hash(&(n.id, n.state, &n.ins, &n.outs, &n.desc, self.model.group_of.get(&n.id), self.xruns.get(&n.id).map(|x| x.0)), &mut hsh);
            std::hash::Hash::hash(&(&n.class, n.system, n.color, n.desk.as_ref().map(|d| d.3), self.model.timing.get(&n.id).map(|t| t.latency)), &mut hsh);
        }
        for g in &self.model.groups {
            // (the members in any order: the profiler lists them as they came)
            let mut members = g.members.clone();
            members.sort_unstable();
            std::hash::Hash::hash(&(g.driver, g.quantum, g.rate, members), &mut hsh);
        }
        let sig = std::hash::Hasher::finish(&hsh);
        // the graph is drawn on the painter's threads: the old one shows until the new one comes
        for done in self.inbox.take() {
            self.done(done);
        }
        let settled = self.zoomed.is_none_or(|t| t.elapsed() > Duration::from_millis(250));
        let stale = self.graph_layer.as_ref().is_none_or(|(s, r, _)| *s != sig || (settled && (*r - self.view.s).abs() > 1e-3));
        let asked = self.graph_want.is_some_and(|(_, s, vs, _, _)| s == sig && (!settled || (vs - self.view.s).abs() <= 1e-3));
        if stale && !asked {
            // the layout for it: made again only when the graph changed (not for a zoom); it is
            // the one the pointer and the numbers go by once its picture comes (until then the
            // old picture shows, and the old layout with it)
            let fresh = match &self.want_lay {
                Some((s, l)) if *s == sig => l.clone(),
                _ if self.lay_sig == Some(sig) => self.lay.clone(),
                _ => self.layout(),
            };
            let shown = std::mem::replace(&mut self.lay, fresh);
            if !self.fitted && !self.lay.pos.is_empty() {
                self.fitted = true;
                self.fit_to(graph_a, dpi);
            }
            // as sharp as the view, within what one texture may hold
            let cap = 8000.0 / self.lay.w.max(self.lay.h).max(1.0);
            let r = self.view.s.min(cap);
            let want = painter::sig(sig, r);
            self.graph_want = Some((want, sig, self.view.s, self.lay.w.max(10.0), self.lay.h.max(10.0)));
            self.want_pos = self.lay.pos.iter().map(|(id, &(x, y))| (*id, (x, y, self.model.node(*id).map_or(60.0, node_h)))).collect();
            self.inbox.ask(painter::key("graph", 0u8), want, self.graph_svg(false), r);
            let fresh = std::mem::replace(&mut self.lay, shown);
            if self.graph_layer.is_none() {
                (self.lay, self.lay_sig, self.want_lay) = (fresh, Some(sig), None);
            } else {
                self.want_lay = Some((sig, fresh));
            }
        }
        if self.glide.as_ref().is_some_and(|g| g.start.elapsed() >= GLIDE) {
            self.glide = None;
        }
        lap(&mut self.spent, "engine.graph");
        // the numbers over it, each a small picture drawn again when it changes
        let cap = 8000.0 / self.lay.w.max(self.lay.h).max(1.0);
        let r = self.graph_layer.as_ref().map_or(self.view.s, |(_, s, _)| *s).min(cap);
        let nums = self.numbers();
        self.nums.retain(|k, _| nums.iter().any(|n| n.key == *k));
        for n in nums {
            let sig = hash_str(&format!("{}{}{}{}{}", n.text, n.alpha, n.x, n.y, n.end), r);
            if self.nums.get(&n.key).is_some_and(|(s, _)| *s == sig) {
                continue;
            }
            let (w, h) = (text::width(text::MONO, &n.text, n.size, 0.0) + 4.0, n.size * 1.5);
            let base = n.size * 1.15;
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><text x="{}" y="{base}" font-family="Noto Sans Mono" font-size="{}" fill="{}" opacity="{}"{} xml:space="preserve">{}</text></svg>"##,
                if n.end { w - 2.0 } else { 2.0 },
                n.size,
                n.fill,
                n.alpha,
                if n.end { r#" text-anchor="end""# } else { "" },
                esc(&n.text)
            );
            let x = if n.end { n.x - w + 2.0 } else { n.x - 2.0 };
            self.nums.insert(n.key, (sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, r), x, y: n.y - base, w, h }));
        }
        lap(&mut self.spent, "engine.numbers");
        // chosen in a list: the graph moves to show it, in the middle, when it is out of sight
        if self.reveal {
            self.reveal = false;
            if let Some((x0, y0, x1, y1)) = self.sel_box() {
                let v = self.view;
                let (gx0, gy0, gx1, gy1) = (graph_a[0] * dpi, graph_a[1] * dpi, (graph_a[0] + graph_a[2]) * dpi, (graph_a[1] + graph_a[3]) * dpi);
                let (sx0, sy0, sx1, sy1) = (x0 * v.s + v.x, y0 * v.s + v.y, x1 * v.s + v.x, y1 * v.s + v.y);
                let m = 30.0 * dpi;
                if sx0 < gx0 + m || sy0 < gy0 + m || sx1 > gx1 - m || sy1 > gy1 - m {
                    self.view.x = (gx0 + gx1) / 2.0 - (x0 + x1) / 2.0 * v.s;
                    self.view.y = (gy0 + gy1) / 2.0 - (y0 + y1) / 2.0 * v.s;
                }
            }
        }
        // the inspector, the tabs and the graph's overlay: in window units
        lap(&mut self.spent, "engine.reveal");
        self.hits.clear();

        let (ov, ov_hits, keys_h) = self.overlay_svg(graph_a);
        {
            let gw = graph_a[2].max(1.0);
            let sh = keys_h.min(graph_a[3].max(1.0));
            let sig = hash_str(&ov, dpi);
            if self.overlay.as_ref().is_none_or(|(s, _)| *s != sig) {
                self.overlay = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &ov, dpi), x: graph_a[0], y: graph_a[1], w: gw, h: sh }));
            }
            if let Some((_, l)) = &mut self.overlay {
                (l.x, l.y) = (graph_a[0], graph_a[1]);
            }
        }
        self.hits.extend(ov_hits.iter().cloned());
        // the legend under its key, made smaller to fit a small window; FIND · SHOW's card under
        // its key (one of them at a time)
        let key_x = |k: &Hit| ov_hits.iter().find(|(h, _)| h == k).map_or(graph_a[0] + 12.0, |(_, r)| r[0]);
        let (legend_x, find_x) = (key_x(&Hit::Legend), key_x(&Hit::Find));
        let below = graph_a[1] + keys_h;
        if self.legend_open {
            let (lw, lh) = legend_size();
            let s = ((graph_a[0] + graph_a[2] - legend_x - 6.0) / lw).min((graph_a[3] - keys_h - 12.0) / lh).clamp(0.2, 1.0);
            let (w, h) = (lw * s, lh * s);
            let (x, y) = (legend_x, below);
            let sig = hash_str(LEGEND_SVG, dpi * s);
            if self.legend.as_ref().is_none_or(|(sg, _)| *sg != sig) {
                self.legend = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), LEGEND_SVG, dpi * s), x, y, w, h }));
            }
            if let Some((_, l)) = &mut self.legend {
                (l.x, l.y) = (x, y);
            }
            // its cross, then the rest of it
            self.hits.push((Hit::Legend, [x + w - 32.0 * s, y + 2.0 * s, 28.0 * s, 28.0 * s]));
            self.hits.push((Hit::LegendWin, [x, y, w, h]));
        } else {
            self.legend = None;
        }
        let mut card_hits = Vec::new();
        if self.find_open {
            let (svg, w, h, hs) = self.find_svg();
            let (x, y) = (find_x, below);
            let sig = hash_str(&svg, dpi);
            if self.find_card.as_ref().is_none_or(|(sg, _)| *sg != sig) {
                self.find_card = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x, y, w, h }));
            }
            if let Some((_, l)) = &mut self.find_card {
                (l.x, l.y) = (x, y);
            }
            card_hits.extend(hs.into_iter().map(|(k, [hx, hy, hw, hh])| (k, [x + hx, y + hy, hw, hh])));
            card_hits.push((Hit::LegendWin, [x, y, w, h]));
        } else {
            self.find_card = None;
        }

        lap(&mut self.spent, "engine.overlay");
        if self.sel.is_none() {
            (self.insp, self.insp_want) = (None, None);
            (self.bodies[0], self.bars[0]) = (Body::default(), None);
        } else {
            let (isvg, mut ihits, ibody) = self.insp_svg(insp_a);
            self.body_slice(0, &ibody, dpi);
            self.bar(0, &ibody, insp_a, &mut ihits);
            let sig = hash_str(&isvg, dpi);
            // the first at once, then on the painter's threads (the old one shows meanwhile)
            if self.insp.is_none() {
                self.insp = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &isvg, dpi), x: insp_a[0], y: insp_a[1], w: insp_a[2], h: insp_a[3] }));
            } else if self.insp.as_ref().is_some_and(|(s, _)| *s != sig) && self.insp_want.map(|w| w.0) != Some(sig) {
                self.insp_want = Some((sig, insp_a));
                self.inbox.ask(painter::key("details", 0u8), sig, isvg, dpi);
            }
            // before the graph's own: in a narrow window the details lie over it
            self.hits.splice(0..0, ihits.into_iter().chain([(Hit::LegendWin, insp_a)]));
        }
        self.hits.splice(0..0, card_hits);
        lap(&mut self.spent, "engine.details");
        if self.bottom_shut {
            (self.bottom, self.bottom_want) = (None, None);
            (self.bodies[1], self.bars[1]) = (Body::default(), None);
        } else {
            let (bsvg, mut bhits, bbody) = self.bottom_svg(bottom_a);
            self.body_slice(1, &bbody, dpi);
            self.bar(1, &bbody, bottom_a, &mut bhits);
            let sig = hash_str(&bsvg, dpi);
            if self.bottom.is_none() {
                self.bottom = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &bsvg, dpi), x: bottom_a[0], y: bottom_a[1], w: bottom_a[2], h: bottom_a[3] }));
            } else if self.bottom.as_ref().is_some_and(|(s, _)| *s != sig) && self.bottom_want.map(|w| w.0) != Some(sig) {
                self.bottom_want = Some((sig, bottom_a));
                self.inbox.ask(painter::key("tables", 0u8), sig, bsvg, dpi);
            }
            self.hits.extend(bhits);
        }
        lap(&mut self.spent, "engine.tables");
        // the same picture may sit elsewhere after the window or a neighbour changed size
        for (l, at) in [(&mut self.insp, insp_a), (&mut self.bottom, bottom_a)] {
            if let Some((_, l)) = l {
                (l.x, l.y) = (at[0], at[1]);
            }
        }

        // the frame round the port a click would listen to
        let vs = self.view.s;
        // (drawn again with the graph: its ports may have changed)
        let shown_sig = self.graph_layer.as_ref().map(|(s, _, _)| *s);
        let hover = self.port_hover.map(|h| (h, hash_str(&format!("{h:?}{shown_sig:?}{:?}", self.lay.pos.get(&self.model.port_node.get(&h.0).copied().unwrap_or(0))), vs)));
        self.hover_pic = match (hover, self.hover_pic.take()) {
            (Some((_, sig)), Some(p)) if p.0 == sig => Some(p),
            (Some((h, sig)), _) => self.port_frame(h.0, h.1, vs).map(|l| (sig, l)),
            (None, _) => None,
        };
        let v = self.view;
        let mut out = Vec::new();
        // the graph; going to a new layout, the old one's blocks glide over it as it fades in
        let glide = self.glide.as_ref().map(|g| (g, (g.start.elapsed().as_secs_f64() / GLIDE.as_secs_f64()).min(1.0)));
        if let Some((_, _, l)) = &self.graph_layer {
            let a = glide.map_or(1.0, |(_, t)| ((t - 0.5) / 0.5).clamp(0.0, 1.0));
            out.push((l, v.x, v.y, l.w * v.s, l.h * v.s, None, (a * a * (3.0 - 2.0 * a)) as f32));
        }
        if let Some((g, t)) = glide {
            let e = t * t * (3.0 - 2.0 * t);
            // (a little round each block: the dots of its ports stand out of it)
            const PAD: f64 = 6.0;
            for (id, &(ox, oy, h)) in &g.src {
                // a block that went fades where it was
                let Some((cx, cy)) = g.at(*id) else { continue };
                let alpha = if g.to.contains_key(id) { 1.0 - (t - 0.85).max(0.0) / 0.15 } else { 1.0 - e };
                let clip = [v.x + (cx - PAD) * v.s, v.y + (cy - PAD) * v.s, (NW + 2.0 * PAD) * v.s, (h + 2.0 * PAD) * v.s];
                out.push((&g.old, v.x + (cx - ox) * v.s, v.y + (cy - oy) * v.s, g.old.w * v.s, g.old.h * v.s, Some(clip), alpha.clamp(0.0, 1.0) as f32));
            }
        }
        for (_, l) in self.nums.values() {
            out.push((l, l.x * v.s + v.x, l.y * v.s + v.y, l.w * v.s, l.h * v.s, None, 1.0));
        }
        if let Some((_, l)) = &self.hover_pic {
            out.push((l, l.x * v.s + v.x, l.y * v.s + v.y, l.w * v.s, l.h * v.s, None, 1.0));
        }
        let graph_end = out.len();
        for l in [&self.overlay, &self.legend, &self.find_card].into_iter().flatten().map(|(_, l)| l) {
            out.push((l, l.x * dpi, l.y * dpi, l.w * dpi, l.h * dpi, None, 1.0));
        }
        // the details and the tables: each its frame, then its body slid to where it is
        // scrolled and cut to its view, then its scrollbar
        for (i, frame, a) in [(0, &self.insp, insp_a), (1, &self.bottom, bottom_a)] {
            let Some((_, l)) = frame else { continue };
            out.push((l, l.x * dpi, l.y * dpi, l.w * dpi, l.h * dpi, None, 1.0));
            if let Some((_, s0, b)) = &self.bodies[i].layer {
                let (top, by) = self.body_at[i];
                let view = [a[0] * dpi, (a[1] + top) * dpi, a[2] * dpi, (a[3] - top).max(0.0) * dpi];
                out.push((b, a[0] * dpi, ((a[1] + by + s0) * dpi).round(), b.w * dpi, b.h * dpi, Some(view), 1.0));
            }
            if let Some(bar) = self.bars[i] {
                for (px, r) in [(&self.bar_px[0], bar.track), (&self.bar_px[1], bar.thumb)] {
                    out.push((px, r[0] * dpi, r[1] * dpi, r[2] * dpi, r[3] * dpi, None, 1.0));
                }
            }
        }
        (out, graph_end)
    }

    /// Brings body `i`'s slice up to date: kept while it shows what the body has and covers the
    /// view; else a new one round the view, at once the first time, then on the painter's
    /// threads (the old one shows meanwhile).
    fn body_slice(&mut self, i: usize, b: &Scrolled, dpi: f64) {
        self.body_at[i] = (b.top, b.by);
        let what = hash_str(&b.body, dpi + b.w * 1e-6);
        let (v0, v1) = ((b.top - b.by).max(0.0), (b.top - b.by + b.view_h).min(b.content).max(0.0));
        let covers = |s0: f64, sh: f64| s0 <= v0 + 0.5 && s0 + sh >= v1 - 0.5;
        let body = &mut self.bodies[i];
        if body.layer.as_ref().is_some_and(|(w, s0, l)| *w == what && covers(*s0, l.h)) {
            body.want = None;
            return;
        }
        if body.want.is_some_and(|(_, w, s0, _, sh)| w == what && covers(s0, sh)) {
            return;
        }
        let s0 = (v0 - b.view_h * 0.5).max(0.0).floor();
        let sh = (b.view_h * 2.0).min(b.content - s0).max(1.0).ceil();
        let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{sh}" viewBox="0 {s0} {w} {sh}" font-family="Noto Sans Mono">{}</svg>"##, b.body, w = b.w);
        let sig = hash_str(&svg, dpi);
        if body.layer.is_none() {
            body.layer = Some((what, s0, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w: b.w, h: sh }));
            body.want = None;
        } else {
            body.want = Some((sig, what, s0, b.w, sh));
            self.inbox.ask(painter::key("body", i as u8), sig, svg, dpi);
        }
    }

    /// Body `i`'s scrollbar, when it is taller than its view: a track at the panel's right
    /// edge, a thumb as long as the view's share of it; its hit first among the panel's.
    fn bar(&mut self, i: usize, b: &Scrolled, a: [f64; 4], hits: &mut Vec<(Hit, [f64; 4])>) {
        let max = (b.content - b.view_h).max(0.0);
        if max < 1.0 {
            self.bars[i] = None;
            return;
        }
        let track = [a[0] + a[2] - 8.0, a[1] + b.top + 3.0, 5.0, (b.view_h - 6.0).max(10.0)];
        let th = (track[3] * b.view_h / b.content).max(24.0).min(track[3]);
        let at = self.scroll[i].clamp(0.0, max) / max;
        let thumb = [track[0], track[1] + (track[3] - th) * at, track[2], th];
        self.bars[i] = Some(Bar { track, thumb, max });
        hits.insert(0, (Hit::Bar(i), [track[0] - 5.0, track[1], track[2] + 8.0, track[3]]));
    }

    /// The GPU has these pictures' pixels now: they are let go. Returns how many bytes.
    pub fn uploaded(&mut self, has: impl Fn(u64) -> bool) -> usize {
        let mut freed = 0;
        let parts = [self.graph_layer.as_mut().map(|(_, _, l)| l), self.insp.as_mut().map(|(_, l)| l), self.bottom.as_mut().map(|(_, l)| l), self.overlay.as_mut().map(|(_, l)| l), self.find_card.as_mut().map(|(_, l)| l), self.legend.as_mut().map(|(_, l)| l), self.hover_pic.as_mut().map(|(_, l)| l), self.glide.as_mut().map(|g| &mut g.old)];
        let bodies = self.bodies.iter_mut().filter_map(|b| b.layer.as_mut().map(|(_, _, l)| l));
        for l in self.nums.values_mut().map(|(_, l)| l).chain(parts.into_iter().flatten()).chain(bodies) {
            if has(l.id) {
                freed += l.pm.take().map_or(0, |pm| pm.data().len());
            }
        }
        freed
    }

    /// SERVER's rows: what PipeWire itself is doing (it was ENGINE's strip): each a name, its
    /// value now and what it means.
    fn server_rows(&self) -> Vec<(&'static str, String, &'static str)> {
        let m = &self.model;
        let ports: usize = m.nodes.iter().map(|n| n.ins.len() + n.outs.len()).sum();
        let get = |name: &str, key: &str| self.meta.iter().find(|((id, s, k), _)| *s == 0 && k == key && self.meta_name(*id) == name).map(|(_, (_, v))| v.clone());
        let rate = get("settings", "clock.rate").unwrap_or_else(|| "?".into());
        let allowed = get("settings", "clock.allowed-rates").unwrap_or_default();
        let q = get("settings", "clock.quantum").unwrap_or_else(|| "?".into());
        let qmin = get("settings", "clock.min-quantum").unwrap_or_else(|| "?".into());
        let qmax = get("settings", "clock.max-quantum").unwrap_or_else(|| "?".into());
        let forced = [get("settings", "clock.force-rate"), get("settings", "clock.force-quantum")].map(|v| v.filter(|v| v != "0"));
        let xr: i32 = self.xruns.values().map(|x| x.0).sum();
        let load = self.load.back().copied().unwrap_or(0.0);
        let session = self.objects.iter().find(|o| o.0 == "Client" && o.2.to_lowercase().contains("wireplumber")).map_or("—".into(), |_| "WirePlumber".to_owned());
        let cnt = |lv: u8| self.issues.iter().filter(|i| i.level == lv).count();
        vec![
            ("CLOCK", format!("{rate} Hz{}", forced[0].as_ref().map_or(String::new(), |f| format!(" · forced {f}"))), "the sample rate the graph runs at (clock.rate)"),
            ("QUANTUM", format!("{q} · {qmin}–{qmax}{}", forced[1].as_ref().map_or(String::new(), |f| format!(" · forced {f}"))), "samples a cycle, and the least and most it may be: larger is safer, smaller is quicker"),
            ("DSP LOAD", format!("{:.1}%", load * 100.0), "how much of its cycle the busiest driver spends working; the last 45 seconds"),
            ("XRUNS", xr.to_string(), "cycles missed since ENGINE opened: a click or a gap in the sound"),
            ("DRIVERS", format!("{} running", m.groups.len()), "devices (or the dummy driver) that clock a group of nodes: see TOP"),
            ("SERVER", format!("PipeWire {}", self.version), "the PipeWire this runs on"),
            ("SESSION", session, "the session manager: what links new streams and keeps settings"),
            ("ALLOWED", cut(allowed.trim_matches(|c| c == '[' || c == ']').trim(), 60), "rates the clock may switch to (clock.allowed-rates)"),
            ("OBJECTS", format!("{} · {} nodes · {ports} ports · {} links", self.objects.len(), m.nodes.len(), m.links.len()), "everything PipeWire lists: see OBJECTS"),
            ("HEALTH", format!("{} · {}", cnt(2), cnt(1)), "errors and warnings found: see ISSUES"),
        ]
    }

    fn meta_name(&self, id: u32) -> String {
        self.objects.iter().find(|o| o.1 == id && o.0 == "Metadata").map_or_else(|| id.to_string(), |o| o.2.clone())
    }

    /// The keys over the graph's top left corner: FIND · SHOW (and how many blocks its switches
    /// hide), LEGEND, DIAGNOSTICS (how many issues, its arrow the way a click moves the tables).
    /// Its SVG, where its keys are, and how deep it is.
    fn overlay_svg(&self, a: [f64; 4]) -> (String, Vec<(Hit, [f64; 4])>, f64) {
        const KH: f64 = 24.0;
        let w = a[2].max(1.0);
        let mut o = String::new();
        let mut hits = Vec::new();
        let key = |o: &mut String, x: f64, kw: f64, on: bool| {
            let _ = write!(o, r##"<rect x="{x}" y="10" width="{kw}" height="{KH}" rx="4" fill="{}" stroke="#0b0b0c"/>"##, if on { "#34363a" } else { "#2a2b2e" });
            if on {
                let _ = write!(o, r##"<rect x="{}" y="10.75" width="{}" height="{}" rx="3.5" fill="none" stroke="#e58f2c" stroke-width="1.2"/>"##, x + 0.75, kw - 1.5, KH - 1.5);
            }
        };
        let label = |o: &mut String, x: f64, t: &str, fill: &str| {
            let _ = write!(o, r##"<text x="{x}" y="26" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".5" fill="{fill}">{}</text>"##, esc(t));
        };
        let tw = |t: &str| text::width(text::BOLD, t, 10.0, 0.5);
        let mut x = 12.0;
        // FIND · SHOW: and what its switches hide
        let hidden = self.model.nodes.iter().filter(|n| !self.visible(n)).count();
        let more = if hidden > 0 { format!(" · {hidden} HIDDEN") } else { String::new() };
        let kw = KEY_TEXT + tw("FIND · SHOW") + tw(&more) + 12.0;
        key(&mut o, x, kw, self.find_open);
        o.push_str(&key_icon(x, 10.0, ICON_FIND));
        label(&mut o, x + KEY_TEXT, "FIND · SHOW", "#e6e7e8");
        label(&mut o, x + KEY_TEXT + tw("FIND · SHOW"), &more, "#e58f2c");
        hits.push((Hit::Find, [a[0] + x, a[1] + 10.0, kw, KH]));
        x += kw + 8.0;
        // LEGEND
        let kw = KEY_TEXT + tw("LEGEND") + 12.0;
        key(&mut o, x, kw, self.legend_open);
        o.push_str(&key_icon(x, 10.0, ICON_LEGEND));
        label(&mut o, x + KEY_TEXT, "LEGEND", "#e6e7e8");
        hits.push((Hit::Legend, [a[0] + x, a[1] + 10.0, kw, KH]));
        x += kw + 8.0;
        // DIAGNOSTICS: how much is wrong, and an arrow: down while the tables show (a click puts
        // them away), up while they are away (a click brings them up)
        let issues = match self.issues.len() {
            0 => String::new(),
            1 => " · 1 ISSUE".to_owned(),
            n => format!(" · {n} ISSUES"),
        };
        let kw = KEY_TEXT + tw("DIAGNOSTICS") + tw(&issues) + 26.0;
        key(&mut o, x, kw, false);
        o.push_str(&key_icon(x, 10.0, ICON_DIAGNOSTICS));
        label(&mut o, x + KEY_TEXT, "DIAGNOSTICS", "#e6e7e8");
        label(&mut o, x + KEY_TEXT + tw("DIAGNOSTICS"), &issues, "#e58f2c");
        let cx = x + kw - 16.0;
        let d = if self.bottom_shut { format!("M{},{} l3.5,-3.5 l3.5,3.5", cx - 3.5, 24.0) } else { format!("M{},{} l3.5,3.5 l3.5,-3.5", cx - 3.5, 20.0) };
        let _ = write!(o, r##"<path d="{d}" fill="none" stroke="#e6e7e8" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"##);
        hits.push((Hit::Bottom, [a[0] + x, a[1] + 10.0, kw, KH]));
        let h = 10.0 + KH + 6.0;
        (format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono">{o}</svg>"#), hits, h)
    }

    /// The inspector: what is selected, with its tabs.
    fn insp_svg(&mut self, a: [f64; 4]) -> (String, Vec<(Hit, [f64; 4])>, Scrolled) {
        let (w, h) = (a[2].max(1.0), a[3].max(1.0));
        let mut hits = Vec::new();
        let mut head = String::new();
        // the body: lines of (y, svg), measured as they go
        let mut body = String::new();
        let mut y = 0.0;
        let chars = ((w - 24.0) / 6.6) as usize;
        let line = |body: &mut String, y: &mut f64, t: &str, fill: &str, x: f64| {
            for l in wrap(t, chars.saturating_sub((x / 6.6) as usize)) {
                let _ = write!(body, r##"<text x="{}" y="{}" font-size="11" fill="{fill}" xml:space="preserve">{}</text>"##, 12.0 + x, *y + 13.0, esc(&l));
                *y += 16.0;
            }
        };
        let kv = |body: &mut String, y: &mut f64, rows: &[(&str, String)]| {
            // the keys' column as wide as the longest key needs, up to half the width; longer
            // keys and values wrap
            let longest = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0) as f64 * 6.6 + 24.0;
            let kcol = longest.clamp(90.0, w * 0.5);
            let kchars = ((kcol - 22.0) / 6.6) as usize;
            let vchars = ((w - kcol - 16.0) / 6.6) as usize;
            for (k, v) in rows {
                let lines = wrap(v, vchars);
                let klines = wrap(k, kchars);
                for (i, l) in klines.iter().enumerate() {
                    let _ = write!(body, r##"<text x="12" y="{}" font-size="11" fill="#7d8085" xml:space="preserve">{}</text>"##, *y + 13.0 + i as f64 * 15.0, esc(l));
                }
                for (i, l) in lines.iter().enumerate() {
                    let _ = write!(body, r##"<text x="{kcol}" y="{}" font-size="11" fill="#c9cbcd" xml:space="preserve">{}</text>"##, *y + 13.0 + i as f64 * 15.0, esc(l));
                }
                *y += lines.len().max(klines.len()).max(1) as f64 * 15.0 + 4.0;
                let _ = write!(body, r##"<path d="M12,{} H{}" stroke="#222326"/>"##, *y - 1.5, w - 12.0);
            }
        };
        let heading = |body: &mut String, y: &mut f64, t: &str| {
            *y += 8.0;
            let _ = write!(body, r##"<text x="12" y="{}" font-size="10" font-family="Noto Sans" font-weight="bold" letter-spacing="1" fill="#e58f2c">{}</text>"##, *y + 12.0, esc(t));
            *y += 20.0;
        };
        // rows that select something: (y, h) in body units, made hits once scrolled
        let mut rows: Vec<(Sel, f64)> = Vec::new();
        // a port's LINK TO (LINK FROM) rows: the link each makes or takes away, its y
        let mut link_rows: Vec<((u32, u32), f64)> = Vec::new();
        let mut desk_button: Option<(String, f64)> = None;
        let tabs: Vec<ITab>;
        let m = &self.model;
        let props_of = |id: u32| -> Vec<(String, String)> { self.props.get(&id).map(|p| p.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default() };
        let (title, sub, led) = match self.sel {
            // not drawn: the inspector shows only with a selection
            None => {
                tabs = Vec::new();
                (String::new(), String::new(), "#4a4c50")
            }
            Some(Sel::Node(id)) => {
                let n = m.node(id).cloned().unwrap_or_else(|| ENode { id, name: "?".into(), desc: "?".into(), class: String::new(), kind: NKind::Other, state: "—", ins: vec![], outs: vec![], color: [0; 3], desk: None, system: false });
                tabs = vec![ITab::Info, ITab::Props, ITab::Params, ITab::Timing];
                if !tabs.contains(&self.itab) {
                    self.itab = ITab::Info;
                }
                let g = m.group(id);
                let t = m.timing.get(&id);
                let xr = self.xruns.get(&id).map_or(0, |x| x.0);
                let running = g.is_some();
                match self.itab {
                    ITab::Info | ITab::Conn => {
                        let p = self.props.get(&id);
                        let pget = |k: &str| p.and_then(|p| p.get(k)).cloned();
                        let client = pget("client.id").and_then(|c| c.parse::<u32>().ok());
                        let client_name = client.and_then(|c| self.objects.iter().find(|o| o.0 == "Client" && o.1 == c)).map(|o| o.2.clone());
                        let mut r: Vec<(&str, String)> = vec![
                            ("STATE", format!("{}{}", if running { "running" } else { n.state }, if xr > 0 { format!(" · {xr} xruns") } else { String::new() })),
                            ("NAME", n.name.clone()),
                            ("CLASS", n.class.clone()),
                            ("CLIENT", client.map_or("—".into(), |c| format!("{} ({c})", client_name.unwrap_or_default()))),
                            (
                                "DRIVER",
                                match g {
                                    Some(g) if g.driver == id => format!("this node: it clocks {} other{}", g.members.len() - 1, if g.members.len() == 2 { "" } else { "s" }),
                                    Some(g) => format!("{} ({})", m.node(g.driver).map_or("?".into(), |d| d.desc.clone()), g.driver),
                                    None => "— (not running)".into(),
                                },
                            ),
                        ];
                        if let Some(g) = g {
                            r.push(("CLOCK", format!("{} Hz · quantum {} · {}", g.rate, g.quantum, ms(g.quantum, g.rate))));
                        }
                        if let (Some(g), Some(t)) = (g, t)
                            && t.latency.1 > 0
                        {
                            r.push(("OWN CLOCK", format!("{}/{}{}", t.latency.0, t.latency.1, if t.latency.1 != g.rate && g.driver != id { " · resampled" } else { "" })));
                        }
                        if let Some(l) = pget("node.latency") {
                            r.push(("ASKS FOR", format!("{l} (node.latency)")));
                        }
                        if let Some(t) = t.filter(|_| running) {
                            r.push(("TIMING", format!("wait {:.1} µs · busy {:.1} µs", t.wait_ns as f64 / 1000.0, t.busy_ns as f64 / 1000.0)));
                        }
                        kv(&mut body, &mut y, &r);
                        // its card: a device of its own, with an id of its own
                        if let Some(d) = pget("device.id").and_then(|d| d.parse::<u32>().ok())
                            && let Some(o) = self.objects.iter().find(|o| o.0 == "Device" && o.1 == d)
                        {
                            rows.push((Sel::Obj(d), y));
                            kv(&mut body, &mut y, &[("DEVICE", format!("{} (device {d}) ›", o.2))]);
                        }
                        heading(&mut body, &mut y, "ON THE DESK");
                        let desk = match (&n.desk, n.kind) {
                            (_, NKind::Meter) => "Patchcord's own: a meter (the LEDs in a frame). Passive: it follows what runs and wakes nothing. Not on the desk.".to_owned(),
                            (_, NKind::Cue) if n.name == CUE_NODE => "Patchcord's cue, its in side: what you listen to (CUE, a click on a jack or a port) comes in here. Its links wake what they come from while it is listened to (a mic asleep), and it sleeps again after. Not on the desk.".to_owned(),
                            (_, NKind::Cue) => "Patchcord's cue, its out side: what you listen to plays from here into the default output, at the CUE LEVEL. Not on the desk.".to_owned(),
                            (_, NKind::Driver) => "A clock of PipeWire itself, for nodes without a device. Not on the desk.".to_owned(),
                            (Some((_, name, labels, hidden)), _) => format!("{name} · {}{}", labels.join(" + "), if *hidden { " · hidden on the desk" } else { "" }),
                            (None, _) if n.system => "Plumbing Patchcord leaves out: WirePlumber's behind a headset's MIC, or another program's level meter. HIDDEN lists it under SYSTEM.".to_owned(),
                            (None, _) => "Not on the desk.".to_owned(),
                        };
                        line(&mut body, &mut y, &desk, "#c9cbcd", 0.0);
                        if let Some((key, _, _, false)) = &n.desk {
                            desk_button = Some((key.clone(), y + 4.0));
                            y += 32.0;
                        }
                        heading(&mut body, &mut y, &format!("PORTS {} IN · {} OUT", n.ins.len(), n.outs.len()));
                        for (input, p) in n.ins.iter().map(|p| (true, p)).chain(n.outs.iter().map(|p| (false, p))) {
                            let k = m.links.iter().filter(|l| l.out_port == p.id || l.in_port == p.id).count();
                            rows.push((Sel::Port(p.id), y));
                            let _ = write!(
                                body,
                                r##"<circle cx="16" cy="{}" r="3.5" fill="{}"/><text x="26" y="{}" font-size="11" fill="#c9cbcd" xml:space="preserve">{} {}</text><text x="{}" y="{}" font-size="11" fill="#7d8085" text-anchor="end">{k} link{}</text>"##,
                                y + 9.0,
                                hex(n.color),
                                y + 13.0,
                                if input { "IN " } else { "OUT" },
                                esc(&cut(&p.name, chars.saturating_sub(16))),
                                w - 12.0,
                                y + 13.0,
                                if k == 1 { "" } else { "s" }
                            );
                            y += 19.0;
                        }
                        let ls: Vec<&ELink> = m.links.iter().filter(|l| l.out_node == id || l.in_node == id).collect();
                        heading(&mut body, &mut y, &format!("LINKS {}", ls.len()));
                        for l in ls {
                            let (arrow, other, port) = if l.out_node == id { ("→", l.in_node, l.in_port) } else { ("←", l.out_node, l.out_port) };
                            let on = m.node(other);
                            let what = format!("{arrow} {}:{}", on.map_or("?".into(), |n| n.desc.clone()), on.map_or(String::new(), |n| port_name(n, port)));
                            rows.push((Sel::Link(l.id), y));
                            let _ = write!(
                                body,
                                r##"<circle cx="16" cy="{}" r="3.5" fill="{}"/><text x="26" y="{}" font-size="11" fill="#c9cbcd" xml:space="preserve">{}</text><text x="{}" y="{}" font-size="11" fill="#7d8085" text-anchor="end">{}</text>"##,
                                y + 9.0,
                                led_color(l.state, false),
                                y + 13.0,
                                esc(&cut(&what, chars.saturating_sub(12))),
                                w - 12.0,
                                y + 13.0,
                                if l.passive { "passive" } else { l.state }
                            );
                            y += 19.0;
                        }
                    }
                    ITab::Props => kv(&mut body, &mut y, &props_of(id).iter().map(|(k, v)| (k.as_str(), v.clone())).collect::<Vec<_>>()),
                    ITab::Params => {
                        if !self.asked.contains(&id) {
                            self.asked.insert(id);
                            self.outbox.push(Cmd::Params(id));
                        }
                        match self.params.get(&id) {
                            Some(p) if !p.is_empty() => {
                                let mut last = String::new();
                                for ((kind, _), text) in p {
                                    if *kind != last {
                                        heading(&mut body, &mut y, &kind.to_uppercase());
                                        last = kind.clone();
                                    }
                                    line(&mut body, &mut y, text, "#a8abae", 0.0);
                                }
                            }
                            _ => line(&mut body, &mut y, "asking PipeWire…", "#7d8085", 0.0),
                        }
                    }
                    ITab::Timing => {
                        let gw = w - 24.0;
                        // a chart of (average, peak) half a second apart, the last minute
                        let chart = |body: &mut String, y: &mut f64, hv: Option<&VecDeque<(f32, f32)>>, title: &str, line1: bool| {
                            let _ = write!(body, r##"<rect x="12" y="{y}" width="{gw}" height="90" rx="3" fill="#0e0f10"/>"##, y = *y);
                            match hv.filter(|h| h.len() > 1) {
                                Some(hv) => {
                                    let top = if line1 { 1.15 } else { hv.iter().map(|v| v.1).fold(0.02, f32::max) as f64 * 1.2 };
                                    let pts = |f: &dyn Fn(&(f32, f32)) -> f32| hv.iter().enumerate().map(|(i, v)| format!("{:.1},{:.1}", 12.0 + i as f64 / 119.0 * gw, *y + 88.0 - (f(v) as f64 / top).min(1.0) * 80.0)).collect::<Vec<_>>().join(" ");
                                    if line1 {
                                        // the deadline
                                        let ly = *y + 88.0 - 80.0 / top;
                                        let _ = write!(body, r##"<path d="M12,{ly} H{}" stroke="#ff4a32" stroke-opacity=".6" stroke-dasharray="4 3"/><text x="{}" y="{}" font-size="9" fill="#ff6a52" text-anchor="end">DEADLINE</text>"##, 12.0 + gw, 8.0 + gw, ly - 3.0);
                                    }
                                    let _ = write!(
                                        body,
                                        r##"<polyline points="{}" fill="none" stroke="#ffb02e" stroke-width="1.2" stroke-opacity=".8"/><polyline points="{}" fill="none" stroke="#5cff7a" stroke-width="1.6"/><text x="18" y="{}" font-size="10" fill="#7d8085">{title} · <tspan fill="#5cff7a">average</tspan> · <tspan fill="#ffb02e">peak</tspan></text>"##,
                                        pts(&|v| v.1),
                                        pts(&|v| v.0),
                                        *y + 14.0
                                    );
                                }
                                None => {
                                    let _ = write!(body, r##"<text x="18" y="{}" font-size="10" fill="#7d8085">no cycles: it is not running</text>"##, *y + 14.0);
                                }
                            }
                            *y += 100.0;
                        };
                        // a driver: how close its graph comes to the deadline
                        if let Some(g) = g.filter(|g| g.driver == id) {
                            heading(&mut body, &mut y, "TIME LEFT IN THE CYCLE");
                            chart(&mut body, &mut y, self.ends.get(&id), "graph done, of the period", true);
                            let per = g.period_ns();
                            let left = 100.0 * (1.0 - g.end_max_ns as f64 / per);
                            kv(
                                &mut body,
                                &mut y,
                                &[
                                    ("DONE IN", format!("{:.0} µs on average, {:.0} µs at worst", g.end_ns as f64 / 1000.0, g.end_max_ns as f64 / 1000.0)),
                                    ("OF", format!("{:.0} µs (quantum {} at {} Hz)", per / 1000.0, g.quantum, g.rate)),
                                    ("LEFT", format!("{left:.0}% at worst")),
                                ],
                            );
                            line(&mut body, &mut y, "From the start of a cycle to when the last node of the group has finished. Over the deadline, the sound card runs out of sound: an xrun, a click.", "#7d8085", 0.0);
                            heading(&mut body, &mut y, "THIS NODE");
                        }
                        chart(&mut body, &mut y, self.hist.get(&id), "B/Q", false);
                        let per = g.map_or(0.0, |g| g.period_ns());
                        let r: Vec<(&str, String)> = match (t.filter(|_| running), g) {
                            (Some(t), Some(_)) => vec![
                                ("WAIT", format!("{:.1} µs · peak {:.1}", t.wait_ns as f64 / 1000.0, t.wait_max_ns as f64 / 1000.0)),
                                ("BUSY", format!("{:.1} µs · peak {:.1}", t.busy_ns as f64 / 1000.0, t.busy_max_ns as f64 / 1000.0)),
                                ("W/Q", format!("{:.3}", t.wait_ns as f64 / per)),
                                ("B/Q", format!("{:.3} · peak {:.3}", t.busy_ns as f64 / per, t.busy_max_ns as f64 / per)),
                                ("CYCLES", format!("{} in the last half second", t.cycles)),
                                ("XRUNS", xr.to_string()),
                            ],
                            _ => vec![("XRUNS", xr.to_string())],
                        };
                        kv(&mut body, &mut y, &r);
                        y += 6.0;
                        line(&mut body, &mut y, "Every cycle of the last half second counts: the average, and the peak (one slow cycle is enough for a click). BUSY: how long the node took; B/Q: of the time the cycle had.", "#7d8085", 0.0);
                    }
                }
                (n.desc.clone(), format!("node {id} · {}", n.class), led_color(if running { "running" } else { n.state }, xr > 0))
            }
            Some(Sel::Port(id)) => {
                tabs = vec![ITab::Info, ITab::Conn, ITab::Props];
                if !tabs.contains(&self.itab) {
                    self.itab = ITab::Info;
                }
                let (n, p, input) = match m.port(id) {
                    Some((n, p, i)) => (n.clone(), p.clone(), i),
                    None => return (String::new(), hits, Scrolled { w, ..Default::default() }),
                };
                if self.itab == ITab::Props {
                    kv(&mut body, &mut y, &props_of(id).iter().map(|(k, v)| (k.as_str(), v.clone())).collect::<Vec<_>>());
                } else if self.itab == ITab::Conn {
                    // where it can go: every port it fits, block by block, a row each; one linked
                    // in the amber of what is chosen (a click links, or unlinks)
                    let fit: Vec<(&ENode, Vec<&EPort>)> = m.nodes.iter().map(|o| (o, o.ins.iter().chain(&o.outs).filter(|q| m.fits(id, q.id)).collect::<Vec<_>>())).filter(|(_, ps)| !ps.is_empty()).collect();
                    heading(&mut body, &mut y, if input { "LINK FROM" } else { "LINK TO" });
                    if fit.is_empty() {
                        line(&mut body, &mut y, "No port takes it: a link goes from an output to an input of one kind (audio, MIDI, video), on another block, not Patchcord's own.", "#7d8085", 0.0);
                    } else {
                        for (o, ps) in fit {
                            let _ = write!(body, r##"<text x="12" y="{}" font-family="Noto Sans" font-size="11" font-weight="bold" fill="#c9cbcd" xml:space="preserve">{} <tspan font-family="Noto Sans Mono" font-weight="normal" font-size="10" fill="#7d8085">#{}</tspan></text>"##, y + 12.0, esc(&cut(&o.desc, chars.saturating_sub(8))), o.id);
                            y += 18.0;
                            for q in ps {
                                let pair = m.pair(id, q.id);
                                let on = m.linked(pair);
                                let (kx, kw) = (24.0, w - 36.0);
                                let _ = write!(
                                    body,
                                    r##"<rect x="{kx}" y="{y}" width="{kw}" height="20" rx="4" fill="{}" stroke="#0b0b0c"/><circle cx="{}" cy="{}" r="4" fill="{}" stroke="#0f1011" stroke-width="1.5"/><text x="{}" y="{}" font-size="10"{} fill="{}" xml:space="preserve">{}</text>"##,
                                    if on { SEL } else { "#1d1e21" },
                                    kx + 12.0,
                                    y + 10.0,
                                    face::role_colour(q.role),
                                    kx + 22.0,
                                    y + 14.0,
                                    if on { r#" font-weight="bold""# } else { "" },
                                    if on { SEL_TEXT } else { "#a8abae" },
                                    esc(&cut(&q.name, chars.saturating_sub(16)))
                                );
                                if on {
                                    let _ = write!(body, r##"<text x="{}" y="{}" text-anchor="end" font-size="9" font-weight="bold" fill="{SEL_TEXT}">LINKED</text>"##, kx + kw - 10.0, y + 14.0);
                                }
                                link_rows.push((pair, y));
                                y += 24.0;
                            }
                            y += 4.0;
                        }
                        y += 2.0;
                        line(&mut body, &mut y, "Click: link it · again: unlink it · or drag from the port on the graph.", "#7d8085", 0.0);
                    }
                } else {
                    rows.push((Sel::Node(n.id), y));
                    kv(
                        &mut body,
                        &mut y,
                        &[
                            ("NODE", format!("{} ({})", n.desc, n.id)),
                            ("DIRECTION", if input { "input: takes sound".into() } else { "output: makes sound".into() }),
                            ("CHANNEL", if p.channel.is_empty() { "—".into() } else { p.channel.clone() }),
                            ("FORMAT", if p.midi { "8 bit raw midi".into() } else { "32 bit float mono audio".into() }),
                            ("MONITOR", if p.monitor { "yes: a copy of what the sink plays".into() } else { "no".into() }),
                        ],
                    );
                    let ls: Vec<&ELink> = m.links.iter().filter(|l| l.out_port == id || l.in_port == id).collect();
                    heading(&mut body, &mut y, &format!("LINKS {}", ls.len()));
                    for l in ls {
                        let (arrow, other, port) = if l.out_port == id { ("→", l.in_node, l.in_port) } else { ("←", l.out_node, l.out_port) };
                        let on = m.node(other);
                        let what = format!("{arrow} {}:{}", on.map_or("?".into(), |n| n.desc.clone()), on.map_or(String::new(), |n| port_name(n, port)));
                        rows.push((Sel::Link(l.id), y));
                        let _ = write!(body, r##"<circle cx="16" cy="{}" r="3.5" fill="{}"/><text x="26" y="{}" font-size="11" fill="#c9cbcd" xml:space="preserve">{}</text>"##, y + 9.0, led_color(l.state, false), y + 13.0, esc(&cut(&what, chars.saturating_sub(4))));
                        y += 19.0;
                    }
                }
                (format!("{}: {}", n.desc, p.name), format!("port {id} · {}", if input { "input" } else { "output" }), led_color(if m.group_of.contains_key(&n.id) { "running" } else { n.state }, false))
            }
            Some(Sel::Link(id)) => {
                tabs = vec![ITab::Info, ITab::Props];
                if !tabs.contains(&self.itab) {
                    self.itab = ITab::Info;
                }
                let Some(l) = m.links.iter().find(|l| l.id == id).cloned() else { return (String::new(), hits, Scrolled { w, ..Default::default() }) };
                let (on, inn) = (m.node(l.out_node), m.node(l.in_node));
                let end = |n: Option<&ENode>, port: u32| n.map_or("?".into(), |n| format!("{}:{}", n.desc, port_name(n, port)));
                if self.itab == ITab::Props {
                    kv(&mut body, &mut y, &props_of(id).iter().map(|(k, v)| (k.as_str(), v.clone())).collect::<Vec<_>>());
                } else {
                    rows.push((Sel::Port(l.out_port), y));
                    kv(&mut body, &mut y, &[("FROM", end(on, l.out_port))]);
                    rows.push((Sel::Port(l.in_port), y));
                    kv(
                        &mut body,
                        &mut y,
                        &[
                            ("TO", end(inn, l.in_port)),
                            ("STATE", l.state.to_owned()),
                            ("PASSIVE", if l.passive { "yes: it does not keep the nodes running".into() } else { "no: it wakes and runs both nodes".into() }),
                        ],
                    );
                    y += 6.0;
                    line(&mut body, &mut y, if l.passive { "Patchcord's meters and cue link passively: when nothing else plays, the device can suspend." } else { "One link carries one channel: a stereo cord on the desk is two links here." }, "#7d8085", 0.0);
                }
                (format!("link {id}"), format!("{} → {}", on.map_or("?".into(), |n| n.desc.clone()), inn.map_or("?".into(), |n| n.desc.clone())), led_color(l.state, false))
            }
            Some(Sel::Obj(id)) => {
                tabs = vec![ITab::Props];
                self.itab = ITab::Props;
                let o = self.objects.iter().find(|o| o.1 == id).cloned().unwrap_or_default();
                // a device's or a client's nodes: the ids the graph shows
                if o.0 == "Device" || o.0 == "Client" {
                    let ns = self.nodes_of(id);
                    heading(&mut body, &mut y, &format!("NODES {}", ns.len()));
                    for nid in ns {
                        let Some(n) = m.node(nid) else { continue };
                        rows.push((Sel::Node(nid), y));
                        let _ = write!(
                            body,
                            r##"<circle cx="16" cy="{}" r="3.5" fill="{}"/><text x="26" y="{}" font-size="11" fill="#c9cbcd" xml:space="preserve">{} · {}</text><text x="{}" y="{}" font-size="11" fill="#7d8085" text-anchor="end">node {nid}</text>"##,
                            y + 9.0,
                            led_color(if m.group_of.contains_key(&nid) { "running" } else { n.state }, false),
                            y + 13.0,
                            esc(&cut(&n.desc, chars.saturating_sub(30))),
                            esc(&n.class),
                            w - 12.0,
                            y + 13.0
                        );
                        y += 19.0;
                    }
                    heading(&mut body, &mut y, "PROPERTIES");
                }
                kv(&mut body, &mut y, &props_of(id).iter().map(|(k, v)| (k.as_str(), v.clone())).collect::<Vec<_>>());
                // a metadata's keys
                if o.0 == "Metadata" {
                    heading(&mut body, &mut y, "KEYS");
                    let keys: Vec<(&str, String)> = self.meta.iter().filter(|((m, _, _), _)| *m == id).map(|((_, s, k), (_, v))| (k.as_str(), format!("{v}{}", if *s != 0 { format!(" (subject {s})") } else { String::new() }))).collect();
                    kv(&mut body, &mut y, &keys);
                }
                (o.2.clone(), format!("{} {id}", o.0.to_lowercase()), "#5cff7a")
            }
        };
        // the head and the tabs
        let _ = write!(
            head,
            r##"<rect width="{w}" height="50" fill="#1f2023"/><path d="M0,50.5 H{w}" stroke="#000"/><circle cx="16" cy="25" r="4.5" fill="{led}"/><text x="28" y="23" font-family="Noto Sans" font-weight="bold" font-size="13" fill="#e6e7e8">{}</text><text x="28" y="39" font-size="10.5" fill="#7d8085">{}</text>"##,
            esc(&cut(&title, ((w - 70.0) / 7.6) as usize)),
            esc(&cut(&sub, ((w - 50.0) / 6.3) as usize))
        );
        // closes it (and drops the selection); choosing something opens it again
        let _ = write!(head, r##"<path d="M{},{} l8,8 M{},{} l-8,8" stroke="#7d8085" stroke-width="1.8" stroke-linecap="round"/>"##, w - 24.0, 21.0, w - 16.0, 21.0);
        hits.push((Hit::Close, [a[0] + w - 34.0, a[1] + 8.0, 30.0, 30.0]));
        let mut top = 51.0;
        if !tabs.is_empty() {
            let name = |t: &ITab| match t {
                ITab::Info => "INFO",
                ITab::Conn => "CONN",
                ITab::Props => "PROPS",
                ITab::Params => "PARAMS",
                ITab::Timing => "TIMING",
            };
            let names: Vec<(&str, bool)> = tabs.iter().map(|t| (name(t), *t == self.itab)).collect();
            // the chosen tab opens into the details under it
            let at = tab_row(&mut head, 51.0, 80.0, w, &names, "#17181a");
            for (t, (x, tw)) in tabs.iter().zip(at) {
                hits.push((Hit::ITab(*t), [a[0] + x, a[1] + 57.0, tw, 23.0]));
            }
            top = 80.0;
        }
        // the body, scrolled
        let body_h = h - top;
        let sc = self.scroll[0].clamp(0.0, (y + 16.0 - body_h).max(0.0));
        self.scroll[0] = sc;
        let by = top + 6.0 - sc;
        for (s, ry) in rows {
            let (yy, hh) = (by + ry, 19.0);
            if yy + hh > top && yy < h {
                hits.push((Hit::Sel(s), [a[0], a[1] + yy, w, hh]));
            }
        }
        for ((o, i), ry) in link_rows {
            let yy = by + ry;
            if yy + 20.0 > top && yy < h {
                hits.push((Hit::LinkTo(o, i), [a[0] + 24.0, a[1] + yy, w - 36.0, 20.0]));
            }
        }
        if let Some((key, ry)) = &desk_button {
            let _ = write!(body, r##"<rect x="12" y="{ry}" width="150" height="24" rx="4" fill="#38393d"/><text x="87" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" fill="#e6e7e8" text-anchor="middle">SHOW ON DESK</text>"##, ry + 16.0);
            let yy = by + ry;
            if yy + 24.0 > top && yy < h {
                hits.push((Hit::Desk(key.clone()), [a[0] + 12.0, a[1] + yy, 150.0, 24.0]));
            }
        }
        // the frame; the body is a picture of its own (`Scrolled`)
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono"><rect width="{w}" height="{h}" fill="#17181a"/><path d="M.5,0 V{h}" stroke="#000"/>{head}</svg>"##
        );
        (svg, hits, Scrolled { body, w, top, view_h: body_h, by, content: y + 16.0 })
    }

    /// The tabs below the graph: TOP, ISSUES, OBJECTS, METADATA, EVENTS.
    fn bottom_svg(&mut self, a: [f64; 4]) -> (String, Vec<(Hit, [f64; 4])>, Scrolled) {
        let (w, h) = (a[2].max(1.0), a[3].max(1.0));
        let m = &self.model;
        let mut hits = Vec::new();
        // (the colour right under the chosen tab: the tables' head band; OBJECTS has none)
        let under = if self.tab == Tab::Objects { "#131416" } else { "#1a1b1d" };
        let mut head = String::new();
        let tabs = [(Tab::Server, "SERVER".to_owned()), (Tab::Top, "TOP".to_owned()), (Tab::Issues, format!("ISSUES {}", self.issues.len())), (Tab::Objects, "OBJECTS".into()), (Tab::Metadata, "METADATA".into()), (Tab::Events, "EVENTS".into())];
        let names: Vec<(&str, bool)> = tabs.iter().map(|(t, n)| (n.as_str(), *t == self.tab)).collect();
        let at = tab_row(&mut head, 0.0, TABS_H, w, &names, under);
        let _ = write!(head, r##"<path d="M0,.5 H{w}" stroke="#000"/>"##);
        for ((t, _), (x, tw)) in tabs.iter().zip(&at) {
            hits.push((Hit::Tab(*t), [a[0] + x, a[1] + 6.0, *tw, TABS_H - 6.0]));
        }
        let x = at.last().map_or(10.0, |(x, tw)| x + tw + 4.0);
        // (DIAGNOSTICS over the graph puts the tabs away and brings them back)
        let tx = w - 8.0;
        // a report for a bug ticket: a key as DIAGNOSTICS' over the graph (and FIND's, LEGEND's),
        // with the panel's SAVE disk before its name
        let rw = KEY_TEXT + text::width(text::BOLD, "SAVE REPORT", 10.0, 0.5) + 12.0;
        let rx = (tx - rw).max(x + 6.0);
        let _ = write!(
            head,
            r##"<rect x="{rx}" y="4" width="{rw}" height="24" rx="4" fill="#2a2b2e" stroke="#0b0b0c"/>{}<text x="{}" y="20" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".5" fill="#e6e7e8">SAVE REPORT</text>"##,
            key_icon(rx, 4.0, super::panel::icon(super::panel::Key::Save)),
            rx + KEY_TEXT
        );
        hits.push((Hit::Report, [a[0] + rx, a[1] + 4.0, rw, 24.0]));
        // the body: a table head (fixed) and rows (scrolled); rows that select carry a Sel
        let mut thead = String::new();
        let mut body = String::new();
        let mut rows: Vec<(Option<Sel>, f64, f64)> = Vec::new();
        let mut y = 0.0;
        let sel_row = |s: Option<Sel>| s.is_some() && s == self.sel;
        match self.tab {
            Tab::Server => {
                let rows = self.server_rows();
                // three columns, each as wide as its widest text, one gap between them; every row
                // as high as the others, DSP LOAD's curve inside its own
                const GAP: f64 = 28.0;
                const RH: f64 = 26.0;
                let name_w = rows.iter().map(|(k, ..)| text::width(text::MONO_BOLD, k, 10.0, 1.1)).fold(0.0, f64::max);
                // (HEALTH's value is drawn as "N ERRORS   N WARNINGS")
                let shown = |k: &str, v: &str| if k == "HEALTH" { v.split_once(" · ").map_or_else(|| v.to_owned(), |(e, w)| format!("{e} ERRORS   {w} WARNINGS")) } else { v.to_owned() };
                let value_w = |k: &str, v: &str| text::width(text::MONO_BOLD, &shown(k, v), 12.0, 0.0);
                let widest = rows.iter().map(|(k, v, _)| value_w(k, v)).fold(0.0, f64::max);
                let (nx, vx) = (14.0, 14.0 + name_w + GAP);
                let wx = vx + widest + GAP;
                for (c, cx) in [("NAME", nx), ("NOW", vx), ("WHAT IT IS", wx)] {
                    let _ = write!(thead, r##"<text x="{cx}" y="14" font-size="11" fill="#7d8085">{c}</text>"##);
                }
                for (k, v, what) in rows {
                    let ty = y + RH / 2.0 + 4.0;
                    let _ = write!(
                        body,
                        r##"<text x="{nx}" y="{ty}" font-size="10" font-weight="bold" letter-spacing="1.1" fill="#e58f2c">{k}</text><text x="{wx}" y="{ty}" font-size="11" fill="#7d8085">{}</text>"##,
                        esc(&cut(what, ((w - wx - 10.0) / 6.6).max(10.0) as usize))
                    );
                    if k == "HEALTH" {
                        // errors and warnings, each counted in its word's colour, as ISSUES has them
                        let mut x = vx;
                        for ((c, word), n) in [("#ff5a42", "ERRORS"), ("#ffb02e", "WARNINGS")].iter().zip(v.split(" · ")) {
                            let t = format!("{n} {word}");
                            let _ = write!(body, r##"<text x="{x}" y="{ty}" font-size="12" font-weight="bold" fill="{c}">{t}</text>"##);
                            x += text::width(text::MONO_BOLD, &t, 12.0, 0.0) + text::width(text::MONO_BOLD, "   ", 12.0, 0.0);
                        }
                    } else {
                        let _ = write!(body, r##"<text x="{vx}" y="{ty}" font-size="12" font-weight="bold" fill="#e6e7e8" xml:space="preserve">{}</text>"##, esc(&v));
                    }
                    // DSP LOAD's curve: the busiest driver's load over 45 seconds, from just after
                    // its value to where the widest value ends
                    if k == "DSP LOAD" && self.load.len() > 1 {
                        let px = vx + text::width(text::MONO_BOLD, &v, 12.0, 0.0) + 14.0;
                        let (pw, ph) = ((vx + widest - px).max(120.0), RH - 8.0);
                        let top = self.load.iter().copied().fold(0.01, f32::max) as f64 * 1.2;
                        let step = pw / 89.0;
                        let pts: Vec<String> = self.load.iter().enumerate().map(|(j, l)| format!("{:.1},{:.1}", px + j as f64 * step, y + 4.0 + ph - *l as f64 / top * ph)).collect();
                        let _ = write!(body, r##"<rect x="{px}" y="{}" width="{pw}" height="{ph}" fill="#1f2023"/><polyline points="{}" fill="none" stroke="#e58f2c" stroke-width="1.4"/>"##, y + 4.0, pts.join(" "));
                    }
                    let _ = write!(body, r##"<path d="M0,{} H{w}" stroke="#1c1d20"/>"##, y + RH);
                    y += RH;
                }
            }
            Tab::Top => {
                let cols = top_columns(w);
                for &(c, cx, right) in &cols {
                    let _ = write!(thead, r##"<text x="{cx}" y="14" font-size="11" fill="#7d8085" text-anchor="{}">{c}</text>"##, if right { "end" } else { "start" });
                }
                let mut order: Vec<(u32, bool, Option<&Group>)> = Vec::new();
                for g in &m.groups {
                    order.push((g.driver, true, Some(g)));
                    for id in &g.members {
                        if *id != g.driver {
                            order.push((*id, false, Some(g)));
                        }
                    }
                }
                for n in &m.nodes {
                    if !m.group_of.contains_key(&n.id) {
                        order.push((n.id, true, None));
                    }
                }
                for (id, lead, g) in order {
                    let Some(n) = m.node(id) else { continue };
                    let t = m.timing.get(&id).filter(|_| g.is_some());
                    let per = g.map_or(1.0, |g| g.quantum as f64 * 1e9 / g.rate.max(1) as f64);
                    let st = if g.is_some() { "R" } else if n.state == "idle" { "I" } else if n.state == "error" { "E" } else { "S" };
                    let (q, r) = match (t, g) {
                        (Some(_), Some(g)) if lead => (g.quantum.to_string(), g.rate.to_string()),
                        (Some(t), _) => (t.latency.0.to_string(), t.latency.1.to_string()),
                        _ => ("0".into(), "0".into()),
                    };
                    let bq = t.map_or(0.0, |t| t.busy_ns as f64 / per);
                    let err = self.xruns.get(&id).map_or(0, |x| x.0);
                    let fmt = self.format_of(id);
                    let sel = Some(Sel::Node(id));
                    if sel_row(sel) {
                        let _ = write!(body, r##"<rect y="{y}" width="{w}" height="{ROW_H}" fill="#2a2618"/>"##);
                    }
                    if lead && g.is_some() {
                        let _ = write!(body, r##"<path d="M0,{y} H{w}" stroke="#2a2c30"/>"##);
                    }
                    let fill = if lead && g.is_some() { "#e6e7e8" } else { "#a8abae" };
                    let weight = if lead && g.is_some() { "bold" } else { "normal" };
                    let cells: [String; 12] = [
                        st.into(),
                        id.to_string(),
                        q,
                        r,
                        t.map_or("---".into(), |t| format!("{:.1}µs", t.wait_ns as f64 / 1000.0)),
                        t.map_or("---".into(), |t| format!("{:.1}µs", t.busy_ns as f64 / 1000.0)),
                        t.map_or("---".into(), |t| format!("{:.1}µs", t.busy_max_ns as f64 / 1000.0)),
                        t.map_or("---".into(), |t| format!("{:.2}", t.wait_ns as f64 / per)),
                        t.map_or("---".into(), |_| format!("{bq:.2}")),
                        err.to_string(),
                        fmt,
                        // the name the graph shows first, then PipeWire's
                        format!("{}{}{}", if lead { "" } else { " + " }, n.desc, if n.desc != n.name { format!("  ({})", n.name) } else { String::new() }),
                    ];
                    for (i, (c, cx, right)) in cols.iter().enumerate() {
                        if c.is_empty() {
                            continue;
                        }
                        let txt = if i == 11 { cut(&cells[i], ((w - cx - 10.0) / 6.6).max(4.0) as usize) } else { cells[i].clone() };
                        let color = if i == 9 && err > 0 { "#ff6a52" } else { fill };
                        let _ = write!(body, r##"<text x="{cx}" y="{}" font-size="11" font-weight="{weight}" fill="{color}" text-anchor="{}" xml:space="preserve">{}</text>"##, y + 14.0, if *right { "end" } else { "start" }, esc(&txt));
                    }
                    if t.is_some() && !cols[8].0.is_empty() {
                        let bx = cols[8].1 - 110.0;
                        let _ = write!(body, r##"<rect x="{bx}" y="{}" width="50" height="6" rx="2" fill="#24262a"/><rect x="{bx}" y="{}" width="{:.1}" height="6" rx="2" fill="#5cff7a"/>"##, y + 7.0, y + 7.0, (bq * 200.0).min(50.0));
                    }
                    rows.push((sel, y, ROW_H));
                    y += ROW_H;
                }
            }
            Tab::Issues => {
                if self.issues.is_empty() {
                    let _ = write!(body, r##"<text x="14" y="18" font-size="11" fill="#7d8085">Nothing to worry about.</text>"##);
                    y = 24.0;
                }
                for i in &self.issues {
                    // how bad, in a word of its colour; then what it is about
                    let (word, c) = if i.level >= 2 { ("ERROR", "#ff5a42") } else { ("WARNING", "#ffb02e") };
                    let sel = i.node.map(Sel::Node);
                    if sel_row(sel) {
                        let _ = write!(body, r##"<rect y="{y}" width="{w}" height="40" fill="#2a2618"/>"##);
                    }
                    let _ = write!(
                        body,
                        r##"<text x="14" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".8" fill="{c}">{word}</text><text x="92" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".6" fill="#a8abae">{}</text><text x="200" y="{}" font-size="11.5" fill="#e6e7e8">{}</text><text x="200" y="{}" font-size="10.5" fill="#7d8085">{}</text><path d="M0,{} H{w}" stroke="#1c1d20"/>"##,
                        y + 18.0,
                        y + 18.0,
                        i.tag,
                        y + 17.0,
                        esc(&cut(&i.title, ((w - 210.0) / 7.0) as usize)),
                        y + 32.0,
                        esc(&cut(&i.hint, ((w - 210.0) / 6.4) as usize)),
                        y + 40.0
                    );
                    rows.push((sel, y, 40.0));
                    y += 40.0;
                }
            }
            Tab::Objects => {
                // the types on the left, the chosen type's objects on the right, under their heads
                let mut ty = 20.0;
                let _ = write!(thead, r##"<text x="14" y="14" font-size="11" fill="#7d8085">TYPE</text>"##);
                for t in OBJECT_TYPES {
                    let k = self.objects.iter().filter(|o| o.0 == t).count();
                    if k == 0 {
                        continue;
                    }
                    let on = self.otype == t;
                    let _ = write!(
                        thead,
                        r##"<rect x="6" y="{}" width="158" height="18" rx="3" fill="{}"/><text x="14" y="{}" font-size="11" fill="{}">{t}</text><text x="156" y="{}" font-size="11" fill="#7d8085" text-anchor="end">{k}</text>"##,
                        ty + 2.0,
                        if on { "#2e3034" } else { "none" },
                        ty + 15.0,
                        if on { "#e6e7e8" } else { "#a8abae" },
                        ty + 15.0
                    );
                    hits.push((Hit::OType(t.to_owned()), [a[0] + 6.0, a[1] + TABS_H + 4.0 + ty, 158.0, 19.0]));
                    ty += 19.0;
                }
                // three columns as wide as their widest text, one gap between them
                const GAP: f64 = 28.0;
                let mine: Vec<&(String, u32, String, String)> = self.objects.iter().filter(|o| o.0 == self.otype).collect();
                let tw = |t: &str| text::width(text::MONO, t, 11.0, 0.0);
                let id_w = mine.iter().map(|o| tw(&o.1.to_string())).fold(tw("ID"), f64::max);
                let room = w - 172.0 - 14.0 - id_w - 2.0 * GAP;
                let name_w = mine.iter().map(|o| tw(&o.2)).fold(tw("NAME"), f64::max).min(room * 0.6);
                let (ix, nx) = (172.0 + 14.0 + id_w, 172.0 + 14.0 + id_w + GAP);
                let dx = nx + name_w + GAP;
                let (name_head, more_head) = match self.otype.as_str() {
                    "Node" => ("NAME", "CLASS"),
                    "Port" => ("NODE:PORT", "DIRECTION"),
                    "Link" => ("OUTPUT → INPUT", "STATE"),
                    "Client" => ("APPLICATION", "API"),
                    "Device" => ("NAME", "DESCRIPTION"),
                    _ => ("NAME", ""),
                };
                let _ = write!(
                    thead,
                    r##"<text x="{ix}" y="14" font-size="11" fill="#7d8085" text-anchor="end">ID</text><text x="{nx}" y="14" font-size="11" fill="#7d8085">{}</text><text x="{dx}" y="14" font-size="11" fill="#7d8085">{more_head}</text>"##,
                    esc(name_head)
                );
                for o in mine {
                    let sel = Some(match o.0.as_str() {
                        "Node" => Sel::Node(o.1),
                        "Port" => Sel::Port(o.1),
                        "Link" => Sel::Link(o.1),
                        _ => Sel::Obj(o.1),
                    });
                    if sel_row(sel) {
                        let _ = write!(body, r##"<rect x="172" y="{y}" width="{}" height="{ROW_H}" fill="#2a2618"/>"##, w - 172.0);
                    }
                    // (a client that gives no name: a dash, not an empty row)
                    let (name, fill) = if o.2.trim().is_empty() { ("—".to_owned(), "#55585b") } else { (cut(&o.2, (name_w / 6.6).max(10.0) as usize), "#e6e7e8") };
                    let _ = write!(
                        body,
                        r##"<text x="{ix}" y="{}" font-size="11" fill="#a8abae" text-anchor="end">{}</text><text x="{nx}" y="{}" font-size="11" fill="{fill}" xml:space="preserve">{}</text><text x="{dx}" y="{}" font-size="11" fill="#7d8085" xml:space="preserve">{}</text>"##,
                        y + 14.0,
                        o.1,
                        y + 14.0,
                        esc(&name),
                        y + 14.0,
                        esc(&cut(&o.3, ((w - dx - 10.0) / 6.6).max(6.0) as usize))
                    );
                    rows.push((sel, y, ROW_H));
                    y += ROW_H;
                }
                let _ = write!(thead, r##"<path d="M170.5,0 V{h}" stroke="#000"/>"##);
            }
            Tab::Metadata => {
                for (c, cx) in [("METADATA", 14.0), ("SUBJECT", 170.0), ("KEY", 240.0), ("TYPE", 560.0), ("VALUE", 720.0)] {
                    let _ = write!(thead, r##"<text x="{cx}" y="14" font-size="11" fill="#7d8085">{c}</text>"##);
                }
                for ((id, subject, key), (ty, value)) in &self.meta {
                    let vchars = ((w - 730.0) / 6.6).max(10.0) as usize;
                    let lines = wrap(value, vchars);
                    let _ = write!(
                        body,
                        r##"<text x="14" y="{}" font-size="11" fill="#a8abae">{}</text><text x="170" y="{}" font-size="11" fill="#a8abae">{subject}</text><text x="240" y="{}" font-size="11" fill="#e6e7e8">{}</text><text x="560" y="{}" font-size="11" fill="#7d8085">{}</text>"##,
                        y + 14.0,
                        esc(&self.meta_name(*id)),
                        y + 14.0,
                        y + 14.0,
                        esc(&cut(key, 50)),
                        y + 14.0,
                        esc(&cut(ty.as_deref().unwrap_or(""), 24))
                    );
                    for (i, l) in lines.iter().enumerate() {
                        let _ = write!(body, r##"<text x="720" y="{}" font-size="11" fill="#e6e7e8" xml:space="preserve">{}</text>"##, y + 14.0 + i as f64 * 15.0, esc(l));
                    }
                    let rh = ROW_H.max(lines.len() as f64 * 15.0 + 5.0);
                    let _ = write!(body, r##"<path d="M0,{} H{w}" stroke="#1c1d20"/>"##, y + rh);
                    y += rh;
                }
            }
            Tab::Events => {
                for (t, kind, text, node) in &self.events {
                    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                    let clock = local_clock(secs);
                    let sel = node.map(Sel::Node);
                    if sel_row(sel) {
                        let _ = write!(body, r##"<rect y="{y}" width="{w}" height="{ROW_H}" fill="#2a2618"/>"##);
                    }
                    let fill = if *kind == "xrun" { "#ff8a72" } else { "#c9cbcd" };
                    let _ = write!(
                        body,
                        r##"<text x="14" y="{}" font-size="11" fill="#7d8085">{clock}</text><text x="90" y="{}" font-size="9.5" fill="#7d8085">{}</text><text x="150" y="{}" font-size="11" fill="{fill}" xml:space="preserve">{}</text>"##,
                        y + 14.0,
                        y + 14.0,
                        kind.to_uppercase(),
                        y + 14.0,
                        esc(&cut(text, ((w - 160.0) / 6.6) as usize))
                    );
                    rows.push((sel, y, ROW_H));
                    y += ROW_H;
                }
            }
        }
        let thead_h = if matches!(self.tab, Tab::Server | Tab::Top | Tab::Objects | Tab::Metadata) { 20.0 } else { 0.0 };
        let top = TABS_H + 4.0 + thead_h;
        let body_h = h - top;
        let sc = self.scroll[1].clamp(0.0, (y + 8.0 - body_h).max(0.0));
        self.scroll[1] = sc;
        let by = top - sc;
        for (s, ry, rh) in rows {
            let Some(s) = s else { continue };
            let yy = by + ry;
            if yy + rh > top && yy < h {
                let x0 = if self.tab == Tab::Objects { 172.0 } else { 0.0 };
                hits.push((Hit::Sel(s), [a[0] + x0, a[1] + yy, w - x0, rh]));
            }
        }
        let thead_svg = if self.tab == Tab::Objects {
            format!(r##"<g transform="translate(0,{})">{thead}</g>"##, TABS_H + 4.0)
        } else {
            format!(r##"<rect y="{TABS_H}" width="{w}" height="{}" fill="#1a1b1d"/><g transform="translate(0,{})">{thead}</g>"##, thead_h + 4.0, TABS_H + 2.0)
        };
        // the frame; the body is a picture of its own (`Scrolled`)
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono"><rect width="{w}" height="{h}" fill="#131416"/>{thead_svg}{head}</svg>"##
        );
        (svg, hits, Scrolled { body, w, top, view_h: body_h, by, content: y + 8.0 })
    }

    /// The format a node runs in, as pw-top shows it: its sample format when it says, its
    /// channels and its rate.
    fn format_of(&self, id: u32) -> String {
        let (Some(p), Some(n)) = (self.props.get(&id), self.model.node(id)) else { return String::new() };
        let Some(g) = self.model.group(id) else { return String::new() };
        if n.kind == NKind::Midi {
            return "midi".into();
        }
        let ch = p.get("audio.channels").cloned().unwrap_or_else(|| {
            let k = n.ins.iter().chain(&n.outs).filter(|p| !p.monitor).count().max(n.ins.len().max(n.outs.len()));
            k.to_string()
        });
        let rate = if g.driver == id { g.rate } else { self.model.timing.get(&id).map(|t| t.latency.1).filter(|r| *r > 0).unwrap_or(g.rate) };
        let fmt = p.get("audio.format").cloned().unwrap_or_default();
        format!("{fmt} {ch}ch {rate}").trim().to_owned()
    }

    // ---------------- input ----------------

    /// A press at window pixel `p`: a click on the strip, inspector or tabs does its thing; in the
    /// graph it may become a pan.
    pub fn press(&mut self, p: (f64, f64), size: (u32, u32), dpi: f64) -> Option<Action> {
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        if let Some((hit, _)) = self.hits.iter().find(|(_, [x, y, w, h])| lx >= *x && lx <= x + w && ly >= *y && ly <= y + h).cloned() {
            match hit {
                Hit::Tab(t) => {
                    self.tab = t;
                    self.bottom_shut = false;
                    self.scroll[1] = 0.0;
                }
                Hit::ITab(t) => {
                    self.itab = t;
                    self.scroll[0] = 0.0;
                }
                Hit::Show(i) => {
                    self.show[i] = !self.show[i];
                    self.fitted = false;
                }
                Hit::Sel(s) => {
                    self.select(Some(s));
                    self.reveal = true;
                }
                Hit::OType(t) => {
                    self.otype = t;
                    self.scroll[1] = 0.0;
                }
                Hit::Close => self.select(None),
                Hit::Desk(key) => return Some(Action::Desk(key)),
                Hit::Report => return Some(Action::Report),
                Hit::Bottom => self.bottom_shut = !self.bottom_shut,
                Hit::Legend => {
                    self.legend_open = !self.legend_open;
                    if self.legend_open {
                        self.find_open = false;
                    }
                }
                Hit::Find => self.set_find(!self.find_open),
                Hit::FindStep(d) => self.find_step(d),
                Hit::LegendWin => {}
                // a port's LINK TO row: links it there, or takes the link away
                Hit::LinkTo(o, i) => {
                    let l = vec![(o, i)];
                    return Some(if self.model.linked((o, i)) { Action::Link { add: Vec::new(), del: l } } else { Action::Link { add: l, del: Vec::new() } });
                }
                Hit::Bar(i) => {
                    if let Some(bar) = self.bars[i] {
                        let [_, ty, _, th] = bar.thumb;
                        if ly >= ty && ly <= ty + th {
                            // the thumb in the hand
                            self.bar_drag = Some((i, ly, self.scroll[i].min(bar.max)));
                        } else {
                            // a page towards where the track was clicked
                            let page = (bar.track[3] - 20.0).max(40.0);
                            self.scroll[i] = (self.scroll[i].min(bar.max) + if ly < ty { -page } else { page }).clamp(0.0, bar.max);
                        }
                    }
                }
            }
            return None;
        }
        if self.in_graph(p, (size.0 as f64, size.1 as f64), dpi) {
            self.view_to = None;
            let w = ((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s);
            let hit = self.graph_hit(w);
            // on a port, or a block's head: a link may be carried from it (once it moves)
            let own = |id: u32| self.model.node(id).is_some_and(|n| matches!(n.kind, NKind::Meter | NKind::Cue));
            let from = match hit {
                Some(Sel::Port(id)) if !self.model.port_node.get(&id).is_some_and(|&n| own(n)) => Some(Carry::Port(id)),
                Some(Sel::Node(id)) if !own(id) && self.lay.pos.get(&id).is_some_and(|&(_, ny)| w.1 - ny <= HEAD_H) => Some(Carry::Node(id)),
                _ => None,
            };
            self.carry = from.map(|from| Carried { from, at: w, snap: None });
            self.press = Some((p, self.view, hit, false));
        }
        None
    }

    /// The pointer moved with the button held (`dpi`: window pixels per logical one): a thumb
    /// scrolls its body, the graph pans.
    pub fn drag(&mut self, p: (f64, f64), dpi: f64) -> bool {
        if let Some((i, y0, s0)) = self.bar_drag {
            let Some(bar) = self.bars[i] else { return false };
            let room = (bar.track[3] - bar.thumb[3]).max(1.0);
            let s = (s0 + (p.1 / dpi - y0) * bar.max / room).clamp(0.0, bar.max);
            let moved = s != self.scroll[i];
            self.scroll[i] = s;
            return moved;
        }
        let Some((from, v, _, moved)) = &mut self.press else { return false };
        if !*moved && ((p.0 - from.0).abs() > 4.0 || (p.1 - from.1).abs() > 4.0) {
            *moved = true;
        }
        if !*moved {
            return false;
        }
        let (from, v) = (*from, *v);
        // a link in the hand follows the pointer; else the graph pans
        if self.carry.is_some() {
            self.carry_to(((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s));
        } else {
            self.view.x = v.x + p.0 - from.0;
            self.view.y = v.y + p.1 - from.1;
        }
        true
    }

    /// The link in the hand at graph point `w`: what it would go to there, the nearest port that
    /// takes it near enough (from a port), or the block under it that takes some (from a head).
    fn carry_to(&mut self, w: (f64, f64)) {
        let Some(c) = self.carry else { return };
        let m = &self.model;
        let snap = match c.from {
            Carry::Port(a) => match self.graph_hit(w) {
                Some(Sel::Port(b)) if m.takes(a, b) => Some(b),
                _ => {
                    let near = |b: u32| m.port_node.get(&b).and_then(|&n| self.port_xy(&self.lay, n, b)).map(|(x, y)| (x - w.0).hypot(y - w.1));
                    m.nodes.iter().flat_map(|n| n.ins.iter().chain(&n.outs)).filter(|b| m.takes(a, b.id)).filter_map(|b| near(b.id).map(|d| (d, b.id))).filter(|(d, _)| *d < 22.0).min_by(|x, y| x.0.total_cmp(&y.0)).map(|(_, b)| b)
                }
            },
            Carry::Node(a) => m.nodes.iter().find(|n| self.lay.pos.get(&n.id).is_some_and(|&(x, y)| w.0 >= x - 8.0 && w.0 <= x + NW + 8.0 && w.1 >= y && w.1 <= y + node_h(n))).map(|n| n.id).filter(|&b| b != a && !m.pairs(a, b).is_empty()),
        };
        self.carry = Some(Carried { from: c.from, at: w, snap });
    }

    /// The links letting go now would make: to the port (block) the link in the hand is on.
    fn carry_pairs(&self) -> Vec<(u32, u32)> {
        let Some(Carried { from, snap: Some(b), .. }) = self.carry else { return Vec::new() };
        match from {
            Carry::Port(a) if self.model.takes(a, b) => vec![self.model.pair(a, b)],
            Carry::Port(_) => Vec::new(),
            Carry::Node(a) => self.model.pairs(a, b),
        }
    }

    /// While a link is carried: the ports that take it, and the blocks that take some (with the
    /// one it comes from).
    fn carry_sets(&self) -> (HashSet<u32>, HashSet<u32>) {
        let m = &self.model;
        match self.carry_hint() {
            Some((Carry::Port(a), _)) => {
                let c: HashSet<u32> = m.nodes.iter().flat_map(|n| n.ins.iter().chain(&n.outs)).filter(|b| m.takes(a, b.id)).map(|b| b.id).collect();
                let ns = c.iter().filter_map(|b| m.port_node.get(b).copied()).chain(m.port_node.get(&a).copied()).collect();
                (c, ns)
            }
            Some((Carry::Node(a), _)) => (HashSet::new(), m.nodes.iter().filter(|n| n.id == a || !m.pairs(a, n.id).is_empty()).map(|n| n.id).collect()),
            None => (HashSet::new(), HashSet::new()),
        }
    }

    /// A link is in the hand and moving (not just pressed).
    pub fn carrying(&self) -> bool {
        self.carry.is_some() && self.press.is_some_and(|p| p.3)
    }

    /// The link in the hand as the graph shows it: what from, and what it is on now.
    fn carry_hint(&self) -> Option<(Carry, Option<u32>)> {
        self.carrying().then(|| self.carry.map(|c| (c.from, c.snap))).flatten()
    }

    /// While a link is carried near the graph's edge (window pixel `p`), the graph scrolls that
    /// way, as the desk does under a plug: the other end may be out of sight. Whether it moved.
    pub fn edge_scroll(&mut self, p: (f64, f64), size: (u32, u32), dpi: f64) -> bool {
        if !self.carrying() {
            return false;
        }
        let [g, _, _] = self.areas(size.0 as f64 / dpi, size.1 as f64 / dpi);
        let (x0, y0, x1, y1) = (g[0] * dpi, g[1] * dpi, (g[0] + g[2]) * dpi, (g[1] + g[3]) * dpi);
        let edge = 48.0 * dpi;
        let push = |v: f64, lo: f64, hi: f64| if v < lo + edge { ((lo + edge - v) / edge).min(1.0) } else if v > hi - edge { -((v - (hi - edge)) / edge).min(1.0) } else { 0.0 };
        let (dx, dy) = (push(p.0, x0, x1) * 14.0 * dpi, push(p.1, y0, y1) * 14.0 * dpi);
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        self.view.x += dx;
        self.view.y += dy;
        self.view_to = None;
        self.carry_to(((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s));
        true
    }

    /// The link in the hand, for the GPU (window pixels): from its port to the pointer, dashed,
    /// or, on a port (block) that takes it, the links letting go would make.
    pub fn carried(&self, dpi: f64) -> Option<super::mesh::Mesh> {
        let c = self.carry.filter(|_| self.carrying())?;
        let v = self.view;
        let px = |(x, y): (f64, f64)| ((x * v.s + v.x) as f32, (y * v.s + v.y) as f32);
        let sample = |a: (f64, f64), b: (f64, f64)| -> Vec<(f32, f32)> { let cv = link_curve(a, b); (0..=48).map(|k| px(bezier(&cv, k as f64 / 48.0))).collect() };
        let colour = |port: u32| rgb(self.model.port(port).map_or(face::kind_colour("NO CLASS"), |(_, p, _)| face::role_colour(p.role)));
        let mut lines: Vec<super::mesh::Line> = Vec::new();
        let mut dot = None;
        let pairs = self.carry_pairs();
        if !pairs.is_empty() {
            for (o, i) in pairs {
                let (Some(a), Some(b)) = (self.model.port_node.get(&o).and_then(|&n| self.port_xy(&self.lay, n, o)), self.model.port_node.get(&i).and_then(|&n| self.port_xy(&self.lay, n, i))) else { continue };
                lines.push((sample(a, b), colour(o), 0.9));
            }
        } else {
            // from its port (a block's head: its side the links go from) to the pointer, dashed
            let (start, out, col) = match c.from {
                Carry::Port(a) => {
                    let n = *self.model.port_node.get(&a)?;
                    let input = self.model.port(a).is_some_and(|(_, _, i)| i);
                    (self.port_xy(&self.lay, n, a)?, !input, colour(a))
                }
                Carry::Node(a) => {
                    let n = self.model.node(a)?;
                    let &(x, y) = self.lay.pos.get(&a)?;
                    let out = !n.outs.is_empty();
                    ((if out { x + NW } else { x }, y + HEAD_H / 2.0), out, rgb(&hex(n.color)))
                }
            };
            let pts = if out { sample(start, c.at) } else { sample(c.at, start) };
            // dashes 6 long, 4 apart (in window pixels)
            let (dash, gap) = (6.0 * dpi as f32, 4.0 * dpi as f32);
            let mut seg = vec![pts[0]];
            let (mut run, mut on) = (0.0f32, true);
            for w in pts.windows(2) {
                let ((x0, y0), (x1, y1)) = (w[0], w[1]);
                let (mut fx, mut fy) = (x0, y0);
                let mut left = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                while left > 0.0 {
                    let room = (if on { dash } else { gap }) - run;
                    let step = room.min(left);
                    let t = step / left;
                    let (nx, ny) = (fx + (x1 - fx) * t, fy + (y1 - fy) * t);
                    if on {
                        seg.push((nx, ny));
                    }
                    run += step;
                    left -= step;
                    (fx, fy) = (nx, ny);
                    if run >= (if on { dash } else { gap }) - 1e-3 {
                        if on && seg.len() > 1 {
                            lines.push((std::mem::take(&mut seg), col, 1.0));
                        }
                        on = !on;
                        run = 0.0;
                        seg = vec![(fx, fy)];
                    }
                }
            }
            if on && seg.len() > 1 {
                lines.push((seg, col, 1.0));
            }
            dot = Some((px(c.at), col));
        }
        let mut m = super::mesh::lines(&lines, dpi as f32);
        // the end in the hand: a dot where the pointer holds it
        if let Some((e, col)) = dot {
            m.normal.extend(super::mesh::lines(&[(vec![e, (e.0 + 0.01, e.1)], col, 1.0)], 3.0 * dpi as f32).normal);
        }
        Some(m)
    }

    /// The chosen link, as (output port, input port): what Delete takes away.
    /// Whether a link (output port, input port) is Patchcord's own: into one of its meters, or
    /// into or out of its cue. Those aren't taken away from ENGINE.
    pub fn patchcords(&self, (o, i): (u32, u32)) -> bool {
        [o, i].iter().any(|p| self.model.port_node.get(p).and_then(|n| self.model.node(*n)).is_some_and(|n| matches!(n.kind, NKind::Meter | NKind::Cue)))
    }

    pub fn chosen_link(&self) -> Option<(u32, u32)> {
        let Some(Sel::Link(id)) = self.sel else { return None };
        self.model.links.iter().find(|l| l.id == id).map(|l| (l.out_port, l.in_port))
    }

    /// What a press in the graph does, letting go: a click chooses what was under it; a link
    /// carried onto what takes it makes those links (to send, as one step to undo).
    pub fn release(&mut self) -> Option<Action> {
        self.bar_drag = None;
        let add = self.carry_pairs();
        let carried = self.carrying();
        self.carry = None;
        let (_, _, hit, moved) = self.press.take()?;
        if !moved {
            self.select(hit);
            return None;
        }
        (carried && !add.is_empty()).then_some(Action::Link { add, del: Vec::new() })
    }

    fn select(&mut self, s: Option<Sel>) {
        if self.sel != s {
            self.sel = s;
            self.scroll[0] = 0.0;
            if matches!(s, Some(Sel::Obj(_))) {
                self.itab = ITab::Props;
            } else if self.itab == ITab::Props && s.is_none() {
                self.itab = ITab::Info;
            }
            if self.itab == ITab::Params
                && let Some(Sel::Node(id)) = s
                && self.asked.insert(id)
            {
                self.outbox.push(Cmd::Params(id));
            }
        }
    }

    /// A frame's GRAPH key on the rack: its node chosen, and the graph moved to show it when it is
    /// out of sight (as when it is chosen in a list).
    pub fn show_node(&mut self, id: u32) {
        self.select(Some(Sel::Node(id)));
        self.reveal = true;
    }

    /// Escape: closes the legend if it is open, else drops the selection (which closes the
    /// inspector).
    pub fn deselect(&mut self) {
        if self.legend_open {
            self.legend_open = false;
        } else {
            self.select(None);
        }
    }

    /// Where in the window (pixels) node `name` (its description or name, or how it starts) has
    /// its header, once laid out: for the README's animation.
    pub fn node_point(&self, name: &str) -> Option<(f64, f64)> {
        let n = self.model.nodes.iter().find(|n| n.desc.starts_with(name) || n.name.starts_with(name))?;
        let (x, y) = self.lay.pos.get(&n.id)?;
        Some(((x + 60.0) * self.view.s + self.view.x, (y + 12.0) * self.view.s + self.view.y))
    }

    /// The ? key: opens or closes the legend.
    pub fn toggle_legend(&mut self) {
        self.legend_open = !self.legend_open;
        if self.legend_open {
            self.find_open = false;
        }
    }

    /// D: shows or puts away the diagnostics below the graph.
    pub fn toggle_bottom(&mut self) {
        self.bottom_shut = !self.bottom_shut;
    }

    /// The frame round port `id` where a click would listen to it (red: it would feed back), in
    /// the port's own shape and round its dot: a small picture at the port, in world units.
    fn port_frame(&self, id: u32, bad: bool, scale: f64) -> Option<Layer> {
        let n = self.model.node(*self.model.port_node.get(&id)?)?;
        let &(nx, ny) = self.lay.pos.get(&n.id)?;
        let (out, i, p) = n.ins.iter().enumerate().map(|(i, p)| (false, i, p)).chain(n.outs.iter().enumerate().map(|(i, p)| (true, i, p))).find(|(_, _, p)| p.id == id)?;
        let w = ear_w(&port_label(n, p));
        let py = ny + port_y(i);
        // the picture: from a little outside the dot to a little past the ear
        let (x0, y0, pw, ph) = if out { (nx + NW - w - 4.0, py - 9.0, w + 12.0, 18.0) } else { (nx - 8.0, py - 9.0, w + 12.0, 18.0) };
        let (px, ly) = ((if out { nx + NW } else { nx }) - x0, py - y0);
        let d = if out {
            format!("M{},{} H{} A6.5,6.5 0 0 0 {},{} H{} Z", px - 0.75, ly - 6.5, px - w + 6.5, px - w + 6.5, ly + 6.5, px - 0.75)
        } else {
            format!("M{},{} H{} A6.5,6.5 0 0 1 {},{} H{} Z", px + 0.75, ly - 6.5, px + w - 6.5, px + w - 6.5, ly + 6.5, px + 0.75)
        };
        let f = if bad { "#ff503c" } else { CUE };
        let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="0 0 {pw} {ph}"><path d="{d}" fill="none" stroke="{f}" stroke-width="1.6"/><circle cx="{px}" cy="{ly}" r="5.6" fill="none" stroke="{f}" stroke-width="1.4"/></svg>"##);
        Some(Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, scale), x: x0, y: y0, w: pw, h: ph })
    }

    /// Whether window pixel `p` is over the graph itself (not over a panel or chip on it).
    pub fn over_graph(&self, p: (f64, f64), size: (u32, u32), dpi: f64) -> bool {
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        self.in_graph(p, (size.0 as f64, size.1 as f64), dpi) && !self.hits.iter().any(|(_, [x, y, w, h])| lx >= *x && lx <= x + w && ly >= *y && ly <= y + h)
    }

    /// The port under window pixel `p`, on the graph (not under a panel over it).
    pub fn port_at(&self, p: (f64, f64), size: (u32, u32), dpi: f64) -> Option<u32> {
        if !self.over_graph(p, size, dpi) {
            return None;
        }
        match self.graph_hit(((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s)) {
            Some(Sel::Port(id)) => Some(id),
            _ => None,
        }
    }

    /// What is under a point of the graph (world units): a port, a node, or a link.
    fn graph_hit(&self, (x, y): (f64, f64)) -> Option<Sel> {
        let lay = &self.lay;
        for n in &self.model.nodes {
            let Some(&(nx, ny)) = lay.pos.get(&n.id) else { continue };
            // a port: its jack, or anywhere on its ear
            for (i, p) in n.ins.iter().enumerate() {
                let py = ny + port_y(i);
                let w = ear_w(&port_label(n, p));
                if (x - nx).abs() < 8.0 && (y - py).abs() < 7.0 || (x >= nx && x <= nx + w && (y - py).abs() <= PR / 2.0) {
                    return Some(Sel::Port(p.id));
                }
            }
            for (i, p) in n.outs.iter().enumerate() {
                let py = ny + port_y(i);
                let w = ear_w(&port_label(n, p));
                if (x - nx - NW).abs() < 8.0 && (y - py).abs() < 7.0 || (x >= nx + NW - w && x <= nx + NW && (y - py).abs() <= PR / 2.0) {
                    return Some(Sel::Port(p.id));
                }
            }
            if x >= nx && x <= nx + NW && y >= ny && y <= ny + node_h(n) {
                return Some(Sel::Node(n.id));
            }
        }
        // a link: near its curve
        let mut best: Option<(f64, u32)> = None;
        for l in &self.model.links {
            let (Some(a), Some(b)) = (self.port_xy(lay, l.out_node, l.out_port), self.port_xy(lay, l.in_node, l.in_port)) else { continue };
            let c = link_curve(a, b);
            for k in 0..=60 {
                let (px, py) = bezier(&c, k as f64 / 60.0);
                let d = (px - x).hypot(py - y);
                if d < 7.0 && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, l.id));
                }
            }
        }
        best.map(|(_, id)| Sel::Link(id))
    }

    /// The wheel at window pixel `p`: zooms the graph, or scrolls the inspector or the tabs.
    pub fn wheel(&mut self, p: (f64, f64), f: f64, size: (u32, u32), dpi: f64) {
        let [_, insp, bottom] = self.areas(size.0 as f64 / dpi, size.1 as f64 / dpi);
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        let inside = |a: [f64; 4]| lx >= a[0] && lx < a[0] + a[2] && ly >= a[1] && ly < a[1] + a[3];
        let step = if f > 1.0 { -60.0 } else if f < 1.0 { 60.0 } else { 0.0 };
        let max = |i: usize| self.bars[i].map_or(0.0, |b| b.max);
        if inside(insp) {
            self.scroll[0] = (self.scroll[0].min(max(0)) + step).clamp(0.0, max(0));
        } else if inside(bottom) {
            self.scroll[1] = (self.scroll[1].min(max(1)) + step).clamp(0.0, max(1));
        } else if self.in_graph(p, (size.0 as f64, size.1 as f64), dpi) {
            self.view_to = None;
            let w = ((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s);
            let s = (self.view.s * f).clamp(0.15 * dpi, 2.5 * dpi);
            self.view = View { s, x: p.0 - w.0 * s, y: p.1 - w.1 * s };
            self.zoomed = Some(Instant::now());
        }
    }

    /// For pictures without a window: brings every part up to date, the graph drawn too (it is
    /// drawn on the painter's threads, see `sprites`).
    pub fn finish(&mut self, size: (u32, u32), dpi: f64, version: &str) {
        let _ = self.sprites(size, dpi, version);
        while self.inbox.busy() {
            let Some(done) = self.inbox.wait(Duration::from_secs(5)) else { break };
            self.done(done);
        }
    }

    /// A picture back from the painter's threads: the graph, the details or the tables.
    fn done(&mut self, done: painter::Done) {
        if let Some((want, sig, vs, w, h)) = self.graph_want
            && want == done.sig
            && done.key == painter::key("graph", 0u8)
        {
            let pos = std::mem::take(&mut self.want_pos);
            if let Some((s, l)) = self.want_lay.take() {
                (self.lay, self.lay_sig) = (l, Some(s));
            }
            // blocks that moved (the layout changed, not only the zoom): they glide there
            let moved = pos.iter().any(|(id, p)| self.graph_pos.get(id).is_some_and(|q| (p.0 - q.0).abs() + (p.1 - q.1).abs() > 1.0));
            let shown = self.graph_layer.take();
            let to: HashMap<u32, (f64, f64)> = pos.iter().map(|(id, p)| (*id, (p.0, p.1))).collect();
            if moved {
                // gliding already: on from where the blocks are now, cut from the same picture
                self.glide = match (self.glide.take(), shown) {
                    (Some(g), _) => Some(Glide { start: Instant::now(), from: g.src.keys().filter_map(|id| Some((*id, g.at(*id)?))).collect(), old: g.old, src: g.src, to }),
                    (None, Some((_, _, old))) => Some(Glide { start: Instant::now(), old, from: self.graph_pos.iter().map(|(id, p)| (*id, (p.0, p.1))).collect(), src: std::mem::take(&mut self.graph_pos), to }),
                    (None, None) => None,
                };
            }
            self.graph_pos = pos;
            self.graph_layer = Some((sig, vs, Layer { id: next_id(), pm: done.pm, x: 0.0, y: 0.0, w, h }));
            self.graph_want = None;
            return;
        }
        for (i, body) in self.bodies.iter_mut().enumerate() {
            if let Some((sig, what, s0, w, sh)) = body.want
                && sig == done.sig
                && done.key == painter::key("body", i as u8)
            {
                body.layer = Some((what, s0, Layer { id: next_id(), pm: done.pm, x: 0.0, y: 0.0, w, h: sh }));
                body.want = None;
                return;
            }
        }
        for (key, want, layer) in [("details", &mut self.insp_want, &mut self.insp), ("tables", &mut self.bottom_want, &mut self.bottom)] {
            if let Some((sig, [x, y, w, h])) = *want
                && sig == done.sig
                && done.key == painter::key(key, 0u8)
            {
                *layer = Some((sig, Layer { id: next_id(), pm: done.pm, x, y, w, h }));
                *want = None;
                return;
            }
        }
    }

    /// Whether a re-raster after zooming is still to come (the app keeps drawing until then).
    pub fn settling(&self) -> bool {
        self.zoomed.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) || self.inbox.busy() || self.glide.is_some() || self.view_to.is_some()
    }

    /// Fits the graph into its part of the window.
    pub fn fit(&mut self, size: (u32, u32), dpi: f64) {
        let [g, _, _] = self.areas(size.0 as f64 / dpi, size.1 as f64 / dpi);
        self.lay = self.layout();
        // there as the desk goes: gliding, the graph drawn sharp again once it is there
        self.view_to = Some((self.fitted_view(g, dpi), Instant::now()));
        self.zoomed = Some(Instant::now());
    }

    fn fit_to(&mut self, g: [f64; 4], dpi: f64) {
        self.view = self.fitted_view(g, dpi);
        self.view_to = None;
    }

    /// The view that shows the whole graph in `g` (window units).
    fn fitted_view(&self, g: [f64; 4], dpi: f64) -> View {
        let (w, h) = (self.lay.w.max(1.0), self.lay.h.max(1.0));
        let s = ((g[2] - 40.0) / w).min((g[3] - 90.0) / h).clamp(0.2, 1.1) * dpi;
        View { s, x: (g[0] + (g[2] - w * s / dpi) / 2.0) * dpi, y: (g[1] + 44.0) * dpi }
    }

    /// A step of FIT's glide, at the desk's pace; the graph's picture waits for its end to be
    /// drawn again at the new scale (stretched until then, as a wheel's zoom is).
    fn glide_view(&mut self) {
        let Some((t, last)) = self.view_to else { return };
        let dt = last.elapsed().as_secs_f32().min(0.1);
        let k = super::ease(0.15, dt) as f64;
        let v = &mut self.view;
        v.x += (t.x - v.x) * k;
        v.y += (t.y - v.y) * k;
        v.s += (t.s - v.s) * k;
        if (t.s - v.s).abs() < 0.001 && (t.x - v.x).abs() < 0.5 && (t.y - v.y).abs() < 0.5 {
            self.view = t;
            self.view_to = None;
        } else {
            self.view_to = Some((t, Instant::now()));
        }
        self.zoomed = Some(Instant::now());
    }

    /// For the design (`--render … engine specimen`): the graph four times, with nothing, a node,
    /// a port and a link selected, under the overlay's chips and legend, as one SVG.
    pub fn specimen(&mut self, node: u32, port: u32, link: u32) -> String {
        self.show = [true; 4];
        self.bottom_shut = true;
        self.lay = self.layout();
        let (w, h) = (self.lay.w.max(10.0), self.lay.h.max(10.0));
        let (gap, title, top) = (60.0, 34.0, 110.0);
        let mut o = String::new();
        let (ov, ..) = self.overlay_svg([0.0, 0.0, 2.0 * w + gap, 80.0]);
        let _ = write!(o, r#"<g transform="translate(30,20)">{ov}</g>"#);
        for (i, (name, sel)) in [("NOTHING SELECTED", None), ("A NODE SELECTED", Some(Sel::Node(node))), ("A PORT SELECTED", Some(Sel::Port(port))), ("A LINK SELECTED", Some(Sel::Link(link)))].into_iter().enumerate() {
            self.sel = sel;
            let (x, y) = (30.0 + (i % 2) as f64 * (w + gap), top + (i / 2) as f64 * (h + title + gap));
            let _ = write!(o, r##"<text x="{x}" y="{}" font-family="Noto Sans" font-weight="bold" font-size="14" letter-spacing="1.5" fill="#e58f2c">{name}</text><g transform="translate({x},{})">{}</g>"##, y + 18.0, y + title, self.graph_svg(true));
        }
        self.sel = None;
        let (tw, th) = (2.0 * w + gap + 60.0, top + 2.0 * (h + title) + gap + 30.0);
        format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{tw}" height="{th}" viewBox="0 0 {tw} {th}"><rect width="{tw}" height="{th}" fill="#1b1c1e"/>{o}</svg>"##)
    }

    /// An SVG drawn as ENGINE draws (its fonts), `scale` pixels per unit.
    pub fn picture(svg: &str, scale: f64) -> Option<resvg::tiny_skia::Pixmap> {
        rasterize(&text::svg_options(), svg, scale)
    }

    /// For pictures (`--render … engine`): selects the node named so, and opens these tabs.
    pub fn demo(&mut self, node: Option<&str>, tab: Option<&str>, itab: Option<&str>, shut: Option<&str>) {
        let shut = shut.unwrap_or_default();
        self.bottom_shut = shut.contains("bottom");
        if let Some(name) = node
            && let Some(n) = self.model.nodes.iter().find(|n| n.name == name || n.desc == name)
        {
            self.sel = Some(Sel::Node(n.id));
        }
        self.tab = match tab {
            Some("top") => Tab::Top,
            Some("issues") => Tab::Issues,
            Some("objects") => Tab::Objects,
            Some("metadata") => Tab::Metadata,
            Some("events") => Tab::Events,
            _ => Tab::Server,
        };
        self.itab = match itab {
            Some("conn") => ITab::Conn,
            Some("props") => ITab::Props,
            Some("params") => ITab::Params,
            Some("timing") => ITab::Timing,
            _ => ITab::Info,
        };
        if self.itab == ITab::Params
            && let Some(Sel::Node(id)) = self.sel
            && self.asked.insert(id)
        {
            self.outbox.push(Cmd::Params(id));
        }
    }

    /// What a chip or button under window pixel `p` does, for the status bar.
    pub fn hover(&self, p: (f64, f64), dpi: f64) -> Option<String> {
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        let (hit, _) = self.hits.iter().find(|(_, [x, y, w, h])| lx >= *x && lx <= x + w && ly >= *y && ly <= y + h)?;
        Some(match hit {
            Hit::Show(i) => format!("{}: {}", SHOW[*i].0, SHOW[*i].1),
            Hit::Bottom => if self.bottom_shut { "show the diagnostics: SERVER, TOP, ISSUES, OBJECTS, METADATA, EVENTS (D)" } else { "put the diagnostics away (D)" }.into(),
            Hit::Find => if self.find_open { "close FIND · SHOW (Esc)" } else { "find a block by its name, id, class or a port; what else the graph shows (Ctrl+F)" }.into(),
            Hit::FindStep(d) => if *d < 0 { "the previous one (Shift+Enter, ↑)" } else { "the next one (Enter, ↓)" }.into(),
            Hit::Legend => if self.legend_open { "close the legend (? or Esc)" } else { "how to read the graph: colours, lights, tags, links (?)" }.into(),
            Hit::Close => "close the details (choosing something opens them again)".into(),
            Hit::Report => "save what PipeWire has to a text file for a bug report, without names, addresses or what plays".into(),
            Hit::Tab(_) => "click: show this table".into(),
            _ => return None,
        })
    }

    /// What the pointer over the graph may do with a link: carry one (from a port, or a block's
    /// head), let one go, take one away.
    pub fn graph_hint(&self, p: (f64, f64), size: (u32, u32), dpi: f64) -> Option<String> {
        if let Some(c) = self.carry.filter(|_| self.carrying()) {
            let n = self.carry_pairs().len();
            return Some(match (c.from, n) {
                (Carry::Port(_), 0) => "carry it to a port with a ring: an input for an output, an output for an input (near the edge the graph follows)".into(),
                (Carry::Node(_), 0) => "carry it onto a block that can take it: all its channels at once".into(),
                (_, 1) => "let go: link them".into(),
                (_, n) => format!("let go: {n} links, channel to channel"),
            });
        }
        if !self.over_graph(p, size, dpi) {
            return None;
        }
        let w = ((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s);
        match self.graph_hit(w)? {
            Sel::Port(_) => Some("drag: link it to another port · click: its details, where it can go".into()),
            Sel::Node(id) if self.lay.pos.get(&id).is_some_and(|&(_, ny)| w.1 - ny <= HEAD_H) => Some("drag the head onto another block: all its channels, FL to FL, FR to FR · click: its details".into()),
            Sel::Link(id) => {
                let ours = self.model.links.iter().find(|l| l.id == id).is_some_and(|l| self.patchcords((l.out_port, l.in_port)));
                Some(if ours { "Patchcord's own link (a signal meter or the cue): it stays" } else { "click: choose it · Delete: take it away" }.into())
            }
            _ => None,
        }
    }

    /// The line about a tab of the diagnostics, while the pointer is over it: it takes the
    /// status bar's left.
    pub fn tab_hint_at(&self, p: (f64, f64), dpi: f64) -> Option<&'static str> {
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        match self.hits.iter().find(|(_, [x, y, w, h])| lx >= *x && lx <= x + w && ly >= *y && ly <= y + h)? {
            (Hit::Tab(t), _) => Some(tab_hint(t)),
            _ => None,
        }
    }

    pub fn summary(&self) -> String {
        let ports: usize = self.model.nodes.iter().map(|n| n.ins.len() + n.outs.len()).sum();
        format!("ENGINE · what PipeWire really has · {} nodes · {ports} ports · {} links", self.model.nodes.len(), self.model.links.len())
    }
}

/// A number on the graph that changes every moment (see `Engine::numbers`).
struct Num {
    key: String,
    x: f64,
    y: f64,
    end: bool,
    text: String,
    size: f64,
    fill: &'static str,
    alpha: f64,
}

/// TOP's columns in a table `w` wide: (name, x, right-aligned); a column left out has no name.
/// When narrow, the columns that say least go first, from the right; S, ID, BUSY, ERR and NAME
/// stay.
fn top_columns(w: f64) -> [(&'static str, f64, bool); 12] {
    // names, right-aligned ones' widths (left-aligned: what they take after a 20 gap)
    const COLS: [(&str, f64, bool); 12] = [("S", 0.0, false), ("ID", 56.0, true), ("QUANT", 60.0, true), ("RATE", 60.0, true), ("WAIT", 80.0, true), ("BUSY", 80.0, true), ("PEAK", 80.0, true), ("W/Q", 50.0, true), ("B/Q", 120.0, true), ("ERR", 50.0, true), ("FORMAT", 110.0, false), ("NAME", 0.0, false)];
    let place = |on: &[bool; 12]| {
        let mut out = [("", 0.0, false); 12];
        let mut x = 0.0;
        for (i, &(c, cw, right)) in COLS.iter().enumerate() {
            if !on[i] {
                continue;
            }
            let cx = if right { x + cw } else if i == 0 { 14.0 } else { x + 20.0 };
            x = if right { cx } else { cx + cw };
            out[i] = (c, cx, right);
        }
        out
    };
    let mut on = [true; 12];
    for drop in [10, 8, 7, 6, 4, 3, 2] {
        if place(&on)[11].1 + 220.0 <= w {
            break;
        }
        on[drop] = false;
    }
    place(&on)
}

/// Where a key's name starts over the graph and in the tables' row: after its icon.
const KEY_TEXT: f64 = 28.0;
// ENGINE's keys' icons, on the panel's 24-unit grid, 16 units wide as the panel's (4 to 20)
const ICON_FIND: &str = r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 4.5 4.5"/>"#;
const ICON_LEGEND: &str = r#"<circle cx="12" cy="12" r="8"/><path d="M9.6 9.6a2.5 2.5 0 1 1 3.4 2.3c-.6.3-1 .8-1 1.5v.7"/><path d="M12 17.2v.1"/>"#;
const ICON_DIAGNOSTICS: &str = r#"<path d="M4 12h3.5l2-5.5 4 11 2-5.5H20"/>"#;

/// An icon on a key 24 high whose left edge is at `x`, top at `y`: 16 units square, its grid's
/// 4 to 20 in the key's first 32 units, as light as the key's name.
fn key_icon(x: f64, y: f64, icon: &str) -> String {
    format!(r##"<g transform="translate({},{}) scale(.6667)" fill="none" stroke="#e6e7e8" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">{icon}</g>"##, x + 7.0, y + 4.0)
}

/// A row of tabs from `y0` to `y1`, `w` wide, drawn into `o`: real tabs, the chosen one of a piece
/// with what it shows (`under`, the colour right below it, and no line under it), the others
/// standing back, darker, on a line between them and the content. Where each tab is (x, width).
fn tab_row(o: &mut String, y0: f64, y1: f64, w: f64, tabs: &[(&str, bool)], under: &str) -> Vec<(f64, f64)> {
    let top = y0 + 6.0;
    let _ = write!(o, r##"<rect y="{y0}" width="{w}" height="{}" fill="#0e0f10"/>"##, y1 - y0);
    let mut at = Vec::new();
    let mut chosen = (0.0, 0.0);
    let mut x = 10.0;
    for &(name, on) in tabs {
        let tw = text::width(text::BOLD, name, 10.0, 0.6) + 24.0;
        if on {
            chosen = (x, tw);
        }
        let (l, r) = (x + 0.5, x + tw - 0.5);
        let _ = write!(
            o,
            r##"<path d="M{l},{y1} V{} A4,4 0 0 1 {},{top} H{} A4,4 0 0 1 {r},{} V{y1}" fill="{}" stroke="#000"/>{}<text x="{}" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".6" fill="{}" text-anchor="middle">{}</text>"##,
            top + 4.0,
            l + 4.0,
            r - 4.0,
            top + 4.0,
            if on { under } else { "#1c1d20" },
            if on { format!(r##"<path d="M{},{} H{}" stroke="#e58f2c" stroke-width="2"/>"##, x + 3.0, top + 1.5, x + tw - 3.0) } else { String::new() },
            x + tw / 2.0,
            (top + y1) / 2.0 + 3.5,
            if on { "#f3efe6" } else { "#8d9095" },
            esc(name)
        );
        at.push((x, tw));
        x += tw + 4.0;
    }
    // the line under the tabs, but not under the chosen one: it opens into its content
    let _ = write!(o, r##"<path d="M0,{y} H{} M{},{y} H{w}" stroke="#000"/>"##, chosen.0 + 0.5, chosen.0 + chosen.1 - 0.5, y = y1 - 0.5);
    at
}

/// What a tab of the diagnostics shows, for the status bar while the pointer is over it.
fn tab_hint(t: &Tab) -> &'static str {
    match t {
        Tab::Server => "SERVER: what PipeWire itself is doing: its clock, quantum, load, xruns, drivers, version",
        Tab::Top => "TOP: as pw-top: each driver and the nodes it clocks; the last half second's cycles, averaged, with the peak",
        Tab::Issues => "ISSUES: what looks wrong or is worth knowing, worst first",
        Tab::Objects => "OBJECTS: every object PipeWire lists",
        Tab::Metadata => "METADATA: shared settings: defaults, clock, WirePlumber's",
        Tab::Events => "EVENTS: what happened while ENGINE watched",
    }
}

/// A port's name as its block shows it: cut to half the block's width when it has ports on both
/// sides.
fn port_label(n: &ENode, p: &EPort) -> String {
    cut(&p.name, if !n.ins.is_empty() && !n.outs.is_empty() { 17 } else { 34 })
}

/// How wide a port's ear is, from the block's edge: its name and room round it.
fn ear_w(label: &str) -> f64 {
    text::width(text::MONO, label, 10.0, 0.0) + 20.0
}

/// A group's band header after its driver's name: its class and id, rate, quantum and cycle.
fn band_rest(g: &Group, d: Option<&ENode>) -> String {
    format!("{} #{} · {} Hz · QUANT {} · {}", d.map_or(String::new(), |n| n.class.clone()), g.driver, g.rate, g.quantum, ms(g.quantum, g.rate))
}

/// What changes every moment in a group's band header: its load, and the time its worst cycle left.
fn band_load(g: &Group) -> String {
    format!(" · DSP {:.1}%{}", g.load * 100.0, if g.end_max_ns > 0 { format!(" · {:.0}% OF THE CYCLE LEFT", 100.0 * (1.0 - g.end_max_ns as f64 / g.period_ns())) } else { String::new() })
}

/// The legend window's size, from its picture.
fn legend_size() -> (f64, f64) {
    let num = |k: &str| LEGEND_SVG.split(k).nth(1).and_then(|v| v.split('"').next()).and_then(|v| v.parse().ok()).unwrap_or(600.0);
    (num(" width=\""), num(" height=\""))
}

/// A layer of one pixel of `rgba`, stretched where it is drawn.
fn pixel(rgba: [u8; 4]) -> Layer {
    let mut pm = resvg::tiny_skia::Pixmap::new(1, 1).expect("1×1");
    pm.data_mut().copy_from_slice(&rgba);
    Layer { id: next_id(), pm: Some(pm), x: 0.0, y: 0.0, w: 1.0, h: 1.0 }
}

fn hash_str(s: &str, dpi: f64) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    std::hash::Hash::hash(&(s, dpi.to_bits()), &mut h);
    std::hash::Hasher::finish(&h)
}

fn port_name(n: &ENode, port: u32) -> String {
    n.ins.iter().chain(&n.outs).find(|p| p.id == port).map_or_else(|| port.to_string(), |p| p.name.clone())
}

/// The time of day, from seconds since 1970, in the local time zone.
fn local_clock(secs: u64) -> String {
    let t = (secs as i64 + utc_offset()).rem_euclid(86400);
    format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

/// The local time zone's offset from UTC, in seconds, asked of `date` once (UTC if it can't say).
fn utc_offset() -> i64 {
    static OFF: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| {
        let out = std::process::Command::new("date").arg("+%z").output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
        let z = out.trim();
        if z.len() != 5 {
            return 0;
        }
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let (h, m) = (z[1..3].parse::<i64>().unwrap_or(0), z[3..5].parse::<i64>().unwrap_or(0));
        sign * (h * 3600 + m * 60)
    })
}

// ---------------- the report for bug tickets ----------------

/// What makes a report anonymous: the user's name, the host's, the home folder, and personal
/// properties; MAC addresses keep only their last two bytes.
struct Redact {
    pairs: Vec<(String, &'static str)>,
}

impl Redact {
    fn new() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| v.len() > 1);
        let host = env("HOSTNAME").or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_owned()).filter(|h| h.len() > 1));
        let mut pairs = Vec::new();
        if let Some(h) = env("HOME") {
            pairs.push((h, "~"));
        }
        if let Some(u) = env("USER").or_else(|| env("LOGNAME")).filter(|u| u.len() >= 3) {
            pairs.push((u, "<user>"));
        }
        if let Some(h) = host {
            pairs.push((h, "<host>"));
        }
        Redact { pairs }
    }

    fn text(&self, v: &str) -> String {
        let mut s = v.to_owned();
        for (from, to) in &self.pairs {
            s = s.replace(from.as_str(), to);
        }
        mask_macs(&s)
    }

    /// What an object plays (a song, a video's title, a page: its `media.*` values but its kind),
    /// to be left out wherever it shows. Not when it is only the program's name (Hydrogen's
    /// `media.name` is "Hydrogen"), nor in Patchcord's own meters and cue: removing those would
    /// remove the name everywhere. A node's own name may be a song's, so it doesn't count.
    fn private(p: &Props) -> Vec<String> {
        const KIND: [&str; 5] = ["media.class", "media.type", "media.category", "media.role", "media.software"];
        let ours = p.get("node.name").is_some_and(|n| n.starts_with(crate::pw::METER_PREFIX) || n == crate::pw::CUE_NODE);
        if ours {
            return Vec::new();
        }
        let names: Vec<&str> = ["application.name", "application.process.binary"].iter().filter_map(|k| p.get(*k)).map(|v| v.trim()).collect();
        p.iter()
            .filter(|(k, v)| k.starts_with("media.") && !KIND.contains(&k.as_str()) && !names.iter().any(|n| n.eq_ignore_ascii_case(v.trim())))
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// A property as it goes into the report, `private` being the object's (`Redact::private`):
    /// None leaves it out.
    fn prop(&self, key: &str, value: &str, private: &[String]) -> Option<String> {
        const OUT: [&str; 7] = ["application.process.user", "application.process.host", "application.process.machine-id", "application.process.session-id", "application.process.id", "window.x11.display", "object.serial"];
        if OUT.contains(&key) || key.starts_with("pipewire.sec.") {
            return None;
        }
        if key.starts_with("media.") && private.iter().any(|p| p == value) {
            return Some("<removed>".into());
        }
        let mut v = self.text(value);
        // (a short one, as "on", would cut through words)
        for p in private.iter().filter(|p| p.len() > 3) {
            v = v.replace(p.as_str(), "<removed>");
        }
        Some(v)
    }
}

/// `02:00:00:00:00:01` (or with `_`) as `XX:XX:XX:XX:00:01`.
pub(crate) fn mask_macs(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let hex = |c: char| c.is_ascii_hexdigit();
    while i < b.len() {
        if i + 17 <= b.len() {
            let sep = b[i + 2];
            let ok = (sep == ':' || sep == '_') && (0..6).all(|k| hex(b[i + k * 3]) && hex(b[i + k * 3 + 1]) && (k == 5 || b[i + k * 3 + 2] == sep)) && (i == 0 || !hex(b[i - 1])) && (i + 17 == b.len() || !hex(b[i + 17]));
            if ok {
                for _ in 0..4 {
                    out.push_str("XX");
                    out.push(sep);
                }
                out.extend(&b[i + 12..i + 17]);
                i += 17;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// The local date and time, `2026-11-19 00:47:12` (and a file-name form `20261119-004712`).
pub fn local_stamp() -> (String, String) {
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) + utc_offset();
    let (days, t) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil from days (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (hh, mm, ss) = (t / 3600, t / 60 % 60, t % 60);
    (format!("{y}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}"), format!("{y}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}"))
}

impl Engine {
    /// Everything needed to see, and set up again, what this PipeWire had: the system, the
    /// clock, the drivers and their timings, what looks wrong, the graph, what happened, and every
    /// object with all its properties, the metadata and the params read. Without what is
    /// personal (see `Redact`).
    pub fn report(&self, pipewire: &str, state: &str) -> String {
        let r = Redact::new();
        let m = &self.model;
        let mut o = String::new();
        let rule = |o: &mut String, t: &str| {
            let _ = write!(o, "\n==== {t} {}\n\n", "=".repeat(60usize.saturating_sub(t.len())));
        };
        let file = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
        let os = file("/etc/os-release").lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|v| v.trim_matches('"').to_owned()).unwrap_or_else(|| "?".into());
        let cpu = file("/proc/cpuinfo").lines().find_map(|l| l.strip_prefix("model name").and_then(|v| v.split_once(':')).map(|(_, v)| v.trim().to_owned())).unwrap_or_else(|| "?".into());
        let cores = file("/proc/cpuinfo").lines().filter(|l| l.starts_with("processor")).count();
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| "?".into());
        let _ = writeln!(o, "Patchcord report · {}", local_stamp().0);
        let _ = writeln!(o, "Made by ENGINE's SAVE REPORT, to attach to a ticket at https://github.com/bewzaalex/patchcord/issues");
        let _ = writeln!(o, "Left out: user and host names, the home folder (shown as ~), process, machine and session ids, security labels, and what plays (media titles). MAC addresses keep only their last two bytes.");
        rule(&mut o, "SYSTEM");
        let _ = writeln!(o, "patchcord     {}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(o, "pipewire      {pipewire}");
        for c in self.objects.iter().filter(|o| o.0 == "Client") {
            if c.2.to_lowercase().contains("wireplumber") {
                let v = self.props.get(&c.1).and_then(|p| p.get("application.version").or(p.get("core.version"))).cloned().unwrap_or_default();
                let _ = writeln!(o, "session       {} {v}", c.2);
                break;
            }
        }
        let _ = writeln!(o, "os            {os}");
        let _ = writeln!(o, "kernel        {}", file("/proc/sys/kernel/osrelease").trim());
        let _ = writeln!(o, "cpu           {cpu} ({cores} threads)");
        let _ = writeln!(o, "desktop       {} · {}", env("XDG_CURRENT_DESKTOP"), env("XDG_SESSION_TYPE"));
        // the system, as it is (nothing personal in it; a host named like the distribution would
        // spoil the kernel's name); everything after it, made anonymous
        let system = std::mem::take(&mut o);
        rule(&mut o, "CLOCK (settings metadata)");
        for ((id, s, k), (_, v)) in &self.meta {
            if self.meta_name(*id) == "settings" && *s == 0 {
                let _ = writeln!(o, "{k:<28} {}", r.text(v));
            }
        }
        rule(&mut o, "DRIVERS (profiler, the last half second: average and peak)");
        if m.groups.is_empty() {
            let _ = writeln!(o, "nothing running (play something while saving, for timings)");
        }
        for g in &m.groups {
            let d = m.node(g.driver);
            let per = g.period_ns();
            let _ = writeln!(
                o,
                "driver {} {} · {} Hz · quantum {} ({}) · DSP {:.1}% · graph done in {:.0} µs avg, {:.0} µs worst, of {:.0} µs",
                g.driver,
                r.text(&d.map_or("?".into(), |n| n.name.clone())),
                g.rate,
                g.quantum,
                ms(g.quantum, g.rate),
                g.load * 100.0,
                g.end_ns as f64 / 1000.0,
                g.end_max_ns as f64 / 1000.0,
                per / 1000.0
            );
            let _ = writeln!(o, "  {:>5} {:>11} {:>9} {:>9} {:>9} {:>9} {:>5}  NAME", "ID", "QUANT/RATE", "WAIT", "BUSY", "PEAK", "B/Q", "ERR");
            for id in &g.members {
                let (Some(n), Some(t)) = (m.node(*id), m.timing.get(id)) else { continue };
                let _ = writeln!(
                    o,
                    "  {:>5} {:>11} {:>7.1}µs {:>7.1}µs {:>7.1}µs {:>9.3} {:>5}  {}{}",
                    id,
                    if *id == g.driver { format!("{}/{}", g.quantum, g.rate) } else { format!("{}/{}", t.latency.0, t.latency.1) },
                    t.wait_ns as f64 / 1000.0,
                    t.busy_ns as f64 / 1000.0,
                    t.busy_max_ns as f64 / 1000.0,
                    t.busy_ns as f64 / per,
                    self.xruns.get(id).map_or(0, |x| x.0),
                    if *id == g.driver { "" } else { "+ " },
                    r.text(&n.name)
                );
            }
        }
        rule(&mut o, "ISSUES");
        if self.issues.is_empty() {
            let _ = writeln!(o, "none");
        }
        for i in &self.issues {
            let _ = writeln!(o, "[{}] {}: {}\n      {}", if i.level >= 2 { "ERROR" } else { "WARNING" }, i.tag, r.text(&i.title), i.hint);
        }
        rule(&mut o, "GRAPH (nodes, their ports and links)");
        for n in &m.nodes {
            let state = if m.group_of.contains_key(&n.id) { "running" } else { n.state };
            let driver = m.group(n.id).map_or(String::new(), |g| format!(" · driver {}", g.driver));
            let desk = n.desk.as_ref().map_or(String::new(), |d| format!(" · desk: {} {}{}", r.text(&d.1), d.2.join("+"), if d.3 { " (hidden)" } else { "" }));
            let _ = writeln!(o, "node {} {} \"{}\" [{}] {state}{driver}{desk}", n.id, r.text(&n.name), r.text(&n.desc), n.class);
            for (dir, p) in n.ins.iter().map(|p| ("in ", p)).chain(n.outs.iter().map(|p| ("out", p))) {
                let ls: Vec<String> = m
                    .links
                    .iter()
                    .filter(|l| l.in_port == p.id || l.out_port == p.id)
                    .map(|l| {
                        let (other, port, arrow) = if l.out_port == p.id { (l.in_node, l.in_port, "→") } else { (l.out_node, l.out_port, "←") };
                        let on = m.node(other).map_or("?".into(), |x| format!("{}:{}", r.text(&x.name), port_name(x, port)));
                        format!("{arrow} {on} (link {} {}{})", l.id, l.state, if l.passive { " passive" } else { "" })
                    })
                    .collect();
                let _ = writeln!(o, "    {dir} {} {}{}", p.id, p.name, if ls.is_empty() { String::new() } else { format!("  {}", ls.join(", ")) });
            }
        }
        rule(&mut o, "STATE (volumes and mute; cards' profiles and ports, * in use)");
        o.push_str(if state.is_empty() { "nothing yet\n" } else { state });
        rule(&mut o, "EVENTS (while ENGINE was open)");
        for (t, kind, text, _) in self.events.iter().rev() {
            let secs = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let _ = writeln!(o, "{} {kind:<5} {}", local_clock(secs), r.text(text));
        }
        rule(&mut o, "METADATA");
        for ((id, s, k), (ty, v)) in &self.meta {
            let _ = writeln!(o, "{} [{s}] {k} ({}) = {}", self.meta_name(*id), ty.as_deref().unwrap_or(""), r.text(v));
        }
        rule(&mut o, "OBJECTS (all, with their properties)");
        for (ty, id, _, _) in &self.objects {
            let Some(p) = self.props.get(id) else { continue };
            // what this object plays: left out wherever it shows (a stream named after its song)
            let private = Redact::private(p);
            let _ = writeln!(o, "{ty} {id}");
            for (k, v) in p {
                if let Some(v) = r.prop(k, v, &private) {
                    let _ = writeln!(o, "    {k} = {v}");
                }
            }
        }
        if !self.params.is_empty() {
            rule(&mut o, "PARAMS (of the nodes inspected)");
            for (id, ps) in &self.params {
                for ((kind, i), text) in ps {
                    let _ = writeln!(o, "node {id} {kind} #{i}\n{text}");
                }
            }
        }
        // names, the home folder and MAC addresses wherever they show (keys and ids too)
        system + &r.text(&o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pw::{Event, Kind, Props};

    /// What plays is left out of a report wherever it shows, a node's description named after the
    /// song too; a `media.name` that is only the program's name (Hydrogen) is kept, and so is the
    /// name, and Patchcord's own meters keep theirs.
    #[test]
    fn a_report_leaves_out_what_plays_but_not_names() {
        let r = Redact { pairs: Vec::new() };
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
        fn out<'a>(r: &Redact, p: &'a Props) -> Vec<(&'a str, String)> {
            p.iter().filter_map(|(k, v)| r.prop(k, v, &Redact::private(p)).map(|v| (k.as_str(), v))).collect()
        }
        let hydrogen = props(&[("application.name", "Hydrogen"), ("media.name", "Hydrogen"), ("node.name", "Hydrogen"), ("media.class", "Stream/Output/Audio")]);
        assert_eq!(out(&r, &hydrogen), vec![("application.name", "Hydrogen".into()), ("media.class", "Stream/Output/Audio".into()), ("media.name", "Hydrogen".into()), ("node.name", "Hydrogen".into())]);
        let meter = props(&[("media.name", "patchcord-meter-61"), ("node.name", "patchcord-meter-61"), ("node.description", "Patchcord signal meter")]);
        assert!(out(&r, &meter).iter().all(|(_, v)| !v.contains("<removed>")), "{:?}", out(&r, &meter));
        let player = props(&[("application.name", "mpv"), ("media.name", "Some Song - mpv"), ("media.title", "Some Song"), ("node.description", "Some Song - mpv"), ("node.name", "mpv"), ("media.role", "Music")]);
        assert_eq!(
            out(&r, &player),
            vec![
                ("application.name", "mpv".into()),
                ("media.name", "<removed>".into()),
                ("media.role", "Music".into()),
                ("media.title", "<removed>".into()),
                ("node.description", "<removed>".into()),
                ("node.name", "mpv".into())
            ]
        );
    }

    /// A port: (id, direction, name).
    type TPort<'a> = (u32, &'a str, &'a str);
    /// A node: (id, name, class, ports).
    type TNode<'a> = (u32, &'a str, &'a str, &'a [TPort<'a>]);

    /// A graph from (node, name, class, [(port, direction, name)]) and (link, out node, out port,
    /// in node, in port, passive), laid out (as idle: no profiler).
    fn laid(nodes: &[TNode], links: &[(u32, u32, u32, u32, u32, bool)]) -> (Engine, Layout) {
        let (e, lay, _) = laid_with(nodes, links);
        (e, lay)
    }

    /// The same, and the graph it was built from.
    fn laid_with(nodes: &[TNode], links: &[(u32, u32, u32, u32, u32, bool)]) -> (Engine, Layout, Graph) {
        let mut g = Graph::default();
        let props = |kv: &[(&str, String)]| kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<Props>();
        for &(id, name, class, ports) in nodes {
            let p = props(&[("node.name", name.into()), ("node.description", name.into()), ("media.class", class.into())]);
            g.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
            g.apply(&Event::Info { id, props: p, state: Some("running") });
            for &(pid, dir, pname) in ports {
                let p = props(&[("node.id", id.to_string()), ("port.direction", dir.into()), ("port.name", pname.into()), ("format.dsp", "32 bit float mono audio".into())]);
                g.apply(&Event::Added { id: pid, kind: Kind::Port, props: p });
            }
        }
        for &(id, on, op, inn, ip, passive) in links {
            let p = props(&[("link.output.node", on.to_string()), ("link.output.port", op.to_string()), ("link.input.node", inn.to_string()), ("link.input.port", ip.to_string()), ("link.passive", passive.to_string())]);
            g.apply(&Event::Added { id, kind: Kind::Link, props: p.clone() });
            g.apply(&Event::Info { id, props: p, state: Some("active") });
        }
        g.apply(&Event::Synced);
        let mut e = Engine::new();
        e.dirty = true;
        e.build(&g, &[], &HashSet::new(), &HashMap::new());
        let lay = e.layout();
        (e, lay, g)
    }

    /// ISSUES has only what is wrong: nothing on a quiet graph (an app linked to nothing, a card
    /// Off, Patchcord's meters aren't issues), and a node in error with PipeWire's own reason.
    #[test]
    fn issues_are_only_what_is_wrong() {
        let out: &[(u32, &str, &str)] = &[(11, "out", "output_FL"), (12, "out", "output_FR")];
        let (mut e, _, mut g) = laid_with(&[(10, "Lonely App", "Stream/Output/Audio", out)], &[]);
        assert!(e.issues.is_empty(), "{:?}", e.issues.iter().map(|i| i.tag).collect::<Vec<_>>());
        let p = g.props_of(10).cloned().unwrap_or_default();
        g.apply(&Event::Info { id: 10, props: p, state: Some("error") });
        g.apply(&Event::Failed { id: 10, why: "Device or resource busy".into() });
        e.dirty = true;
        e.build(&g, &[], &HashSet::new(), &HashMap::new());
        let i = e.issues.first().expect("the error");
        assert_eq!((i.level, i.tag), (2, "ERROR"));
        assert_eq!(i.hint, "PipeWire says: Device or resource busy");
    }

    /// FIND picks out the blocks that match (a name, an id, a class, a port's name), shown ones
    /// only, top to bottom; Enter goes through them and round; one a switch hides is said, with
    /// a key to show it.
    #[test]
    fn find_picks_out_goes_round_and_says_what_is_hidden() {
        let out: &[(u32, &str, &str)] = &[(11, "out", "output_FL"), (12, "out", "output_FR")];
        let inp: &[(u32, &str, &str)] = &[(21, "in", "input_FL"), (22, "in", "input_FR")];
        let sink: &[(u32, &str, &str)] = &[(31, "in", "playback_FL"), (32, "in", "playback_FR")];
        let meter: &[(u32, &str, &str)] = &[(41, "in", "input_FL"), (42, "in", "input_FR")];
        let (mut e, lay) = laid(&[(10, "Teams for Linux", "Stream/Output/Audio", out), (20, "Teams for Linux", "Stream/Input/Audio", inp), (30, "Speakers", "Audio/Sink", sink), (40, "patchcord-meter-7", "Stream/Input/Audio", meter)], &[]);
        e.show = [true; 4];
        e.lay = e.layout();
        let _ = lay;
        e.set_find(true);
        e.find_text().push_str("teams");
        assert_eq!(e.found().len(), 2);
        let first = e.found()[0];
        e.find_step(1);
        assert_eq!(e.sel, Some(Sel::Node(first)));
        e.find_step(1);
        e.find_step(1);
        assert_eq!(e.sel, Some(Sel::Node(first)), "round again");
        // by a port's name, by id, whatever the case
        e.find = "PLAYBACK_FL".into();
        assert_eq!(e.found(), [30]);
        e.find = "30".into();
        assert_eq!(e.found(), [30]);
        // Patchcord's meter, with PATCHCORD off: not picked out, and the card says so
        e.show[0] = false;
        e.find = "meter".into();
        assert!(e.found().is_empty());
        let (svg, ..) = e.find_svg();
        assert!(svg.contains("1 more hidden by PATCHCORD") && svg.contains("SHOW PATCHCORD"), "{svg}");
        e.show[0] = true;
        assert_eq!(e.found(), [40]);
        // closed, it picks out nothing
        e.set_find(false);
        assert!(e.found().is_empty());
    }

    /// Links made in ENGINE: a port takes one only from the other way, of its kind, on another
    /// block, and not one it has; a block's head links all its channels, in order where no
    /// channel names match, mono to both, leaving out what is there.
    #[test]
    fn a_link_goes_out_to_in_once_and_a_head_links_every_channel() {
        let app: &[(u32, &str, &str)] = &[(11, "out", "output_FL"), (12, "out", "output_FR")];
        let sink: &[(u32, &str, &str)] = &[(21, "in", "playback_FL"), (22, "in", "playback_FR"), (23, "out", "monitor_FL"), (24, "out", "monitor_FR")];
        let mic: &[(u32, &str, &str)] = &[(31, "out", "capture_MONO")];
        let rec: &[(u32, &str, &str)] = &[(41, "in", "input_FL"), (42, "in", "input_FR")];
        let (e, _) = laid(&[(10, "app", "Stream/Output/Audio", app), (20, "sink", "Audio/Sink", sink), (30, "mic", "Audio/Source", mic), (40, "rec", "Stream/Input/Audio", rec)], &[(90, 10, 11, 20, 21, false)]);
        let m = &e.model;
        // out to in, either way round; not to itself, not out to out, not one it has
        assert!(m.takes(12, 22) && m.takes(22, 12) && m.takes(23, 41));
        assert!(!m.takes(11, 12) && !m.takes(11, 23) && !m.takes(11, 21));
        assert_eq!(m.pair(22, 12), (12, 22));
        // the app's head onto the sink: FR only, FL is linked already
        assert_eq!(m.pairs(10, 20), vec![(12, 22)]);
        // the mic's head onto the recorder: mono to both
        assert_eq!(m.pairs(30, 40), vec![(31, 41), (31, 42)]);
        // an input-only block's head: its inputs from the other's outputs
        assert_eq!(m.pairs(40, 10), vec![(11, 41), (12, 42)]);
    }

    /// An app into the headphones, measured, and the headphones' monitor measured: the app's
    /// meter link must not run straight through the headphones' ports (it looked as if it went
    /// in there); outputs only on the left, the headphones in the middle, the meters on the right.
    #[test]
    fn no_link_goes_through_another_port() {
        let st_out: &[(u32, &str, &str)] = &[(711, "out", "output_FL"), (712, "out", "output_FR")];
        let sink: &[(u32, &str, &str)] = &[(1791, "in", "playback_FL"), (1792, "in", "playback_FR"), (1793, "out", "monitor_FL"), (1794, "out", "monitor_FR")];
        let m70: &[(u32, &str, &str)] = &[(701, "in", "input_FL"), (702, "in", "input_FR")];
        let m88: &[(u32, &str, &str)] = &[(881, "in", "input_FL"), (882, "in", "input_FR")];
        let (e, lay) = laid(
            &[(71, "ALSA plug-in [kadence]", "Stream/Output/Audio", st_out), (179, "JLab GO Work 2", "Audio/Sink", sink), (70, "patchcord-meter-1", "Stream/Input/Audio", m70), (88, "patchcord-meter-2", "Stream/Input/Audio", m88)],
            &[(1, 71, 711, 179, 1791, false), (2, 71, 712, 179, 1792, false), (3, 71, 711, 70, 701, true), (4, 71, 712, 70, 702, true), (5, 179, 1793, 88, 881, true), (6, 179, 1794, 88, 882, true)],
        );
        let ls: Vec<&ELink> = e.model.links.iter().collect();
        let through = e.through(&lay.pos, &ls);
        assert!(through.is_empty(), "links through other ports: {:?}", through.iter().map(|(l, n)| (l.id, *n)).collect::<Vec<_>>());
        let x = |id: u32| lay.pos[&id].0;
        assert!(x(71) < x(179) && x(179) < x(70) && x(70) == x(88), "out only, in and out, in only: {:?}", lay.pos);
    }

    /// A loop (an effect fed back into itself through another) keeps its place: a column each,
    /// not pushed on and on to the right.
    #[test]
    fn a_loop_back_keeps_its_columns() {
        let app: &[TPort] = &[(11, "out", "output_MONO")];
        let f1: &[TPort] = &[(21, "in", "input_MONO"), (22, "out", "output_MONO")];
        let f2: &[TPort] = &[(31, "in", "input_MONO"), (32, "out", "output_MONO")];
        let sink: &[TPort] = &[(41, "in", "playback_MONO")];
        let (_, lay) = laid(
            &[(1, "app", "Stream/Output/Audio", app), (2, "effect one", "Audio/Filter", f1), (3, "effect two", "Audio/Filter", f2), (4, "speaker", "Audio/Sink", sink)],
            &[(101, 1, 11, 2, 21, false), (102, 2, 22, 3, 31, false), (103, 3, 32, 2, 21, false), (104, 3, 32, 4, 41, false)],
        );
        let x = |id: u32| lay.pos[&id].0;
        assert_eq!((x(2) - x(1), x(3) - x(2), x(4) - x(3)), (COLW, COLW, COLW), "app, effect one, effect two, speaker: a column each: {:?}", lay.pos);
    }

    /// A busy graph: 12 apps into 3 outputs, each app and output measured.
    fn busy() -> (Engine, Layout, usize, Graph) {
        #[allow(clippy::type_complexity)]
        let mut nodes: Vec<(u32, String, &str, Vec<(u32, &str, String)>)> = Vec::new();
        let mut links = Vec::new();
        let mut lid = 10_000;
        for s in 0..3u32 {
            let id = 500 + s;
            nodes.push((id, format!("Output {s}"), "Audio/Sink", vec![(id * 10, "in", "playback_FL".into()), (id * 10 + 1, "in", "playback_FR".into()), (id * 10 + 2, "out", "monitor_FL".into()), (id * 10 + 3, "out", "monitor_FR".into())]));
        }
        for a in 0..12u32 {
            let id = 100 + a;
            nodes.push((id, format!("App {a}"), "Stream/Output/Audio", vec![(id * 10, "out", "output_FL".into()), (id * 10 + 1, "out", "output_FR".into())]));
            let s = 500 + a % 3;
            for c in 0..2 {
                links.push((lid, id, id * 10 + c, s, s * 10 + c, false));
                lid += 1;
            }
        }
        for (k, src) in (100..112u32).chain(500..503).enumerate() {
            let id = 900 + k as u32;
            nodes.push((id, format!("patchcord-meter-{k}"), "Stream/Input/Audio", vec![(id * 10, "in", "input_FL".into()), (id * 10 + 1, "in", "input_FR".into())]));
            let base = if src >= 500 { src * 10 + 2 } else { src * 10 };
            for c in 0..2 {
                links.push((lid, src, base + c, id, id * 10 + c, true));
                lid += 1;
            }
        }
        let owned: Vec<Vec<TPort>> = nodes.iter().map(|n| n.3.iter().map(|(p, d, nm)| (*p, *d, nm.as_str())).collect()).collect();
        let ns: Vec<TNode> = nodes.iter().zip(&owned).map(|(n, o)| (n.0, n.1.as_str(), n.2, o.as_slice())).collect();
        let (e, lay, g) = laid_with(&ns, &links);
        (e, lay, links.len(), g)
    }

    /// How long laying out a busy graph takes (it is done on the main thread). Run with
    /// --ignored --nocapture.
    #[test]
    #[ignore]
    fn layout_time() {
        let t = std::time::Instant::now();
        let (e, lay, nlinks, _) = busy();
        let built = t.elapsed();
        let t = std::time::Instant::now();
        let lay2 = e.layout();
        println!("build+layout {built:?}, layout {:?}, {} nodes, {nlinks} links", t.elapsed(), lay.pos.len());
        let ls: Vec<&ELink> = e.model.links.iter().collect();
        println!("through: {}", e.through(&lay2.pos, &ls).len());
    }

    /// How long one frame of ENGINE takes on the main thread when nothing changes: the details
    /// and the tables open, each tab in turn. Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn frame_time() {
        let (mut e, _, _, _) = busy();
        e.bottom_shut = false;
        e.sel = Some(Sel::Node(100));
        for tab in [Tab::Server, Tab::Top, Tab::Issues, Tab::Objects, Tab::Metadata, Tab::Events] {
            e.tab = tab;
            // the first frames draw; the rest only check
            for _ in 0..3 {
                e.sprites((1600, 1000), 1.0, "0");
            }
            let n = 200;
            let t = std::time::Instant::now();
            let mut parts: HashMap<&str, Duration> = HashMap::new();
            for _ in 0..n {
                e.sprites((1600, 1000), 1.0, "0");
                for (w, d) in &e.spent {
                    *parts.entry(w).or_default() += *d;
                }
            }
            let mut parts: Vec<_> = parts.into_iter().map(|(w, d)| (w, d / n)).collect();
            parts.sort_by_key(|p| std::cmp::Reverse(p.1));
            println!("{tab:?}: {:?} a frame; {parts:?}", t.elapsed() / n);
        }
    }

    /// How long `build` takes on the busy graph: done whenever something changes, and now on
    /// every profiler report (twice a second). Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn build_time() {
        let (mut e, _, _, g) = busy();
        let n = 200;
        let t = std::time::Instant::now();
        for _ in 0..n {
            e.dirty = true;
            e.build(&g, &[], &HashSet::new(), &HashMap::new());
        }
        println!("build: {:?}, {} nodes, {} objects", t.elapsed() / n, e.model.nodes.len(), e.objects.len());
    }
}
