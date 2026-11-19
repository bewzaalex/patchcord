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

use super::{painter, panel};
use super::scene::{Layer, View, next_id, rasterize};
use super::text;
use crate::graph::{Device, Graph};
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
    /// the strip's i cell: the cells that didn't fit
    Info,
}

/// What the app has to do after a click.
pub enum Action {
    /// leave ENGINE and find this device on the desk
    Desk(String),
    /// save a report (`Engine::report`) for a bug ticket
    Report,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
enum NKind {
    Device,
    Midi,
    Video,
    Stream,
    Meter,
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

// sizes of the window parts, in logical pixels
const STRIP_H: f64 = 50.0;
const BOTTOM_H: f64 = 250.0;
const INSP_W: f64 = 380.0;
const TABS_H: f64 = 30.0;
const ROW_H: f64 = 19.0;
const REPORT_W: f64 = 116.0;
/// the strip's HEALTH cell, its i cell and DSP LOAD's curve
const HEALTH_W: f64 = 112.0;
const INFO_W: f64 = 30.0;
const CURVE_W: f64 = 186.0;
/// a window narrower than this has the details over the graph
const NARROW: f64 = 700.0;
/// What a block is, by its colour; its ports and the links out of it take it too.
const C_DEVICE: [u8; 3] = [0x4f, 0x86, 0xd9];
const C_APP: [u8; 3] = [0x9b, 0x6b, 0xd6];
const C_SYSTEM: [u8; 3] = [0x2f, 0xb0, 0xa6];
const C_MIDI: [u8; 3] = [0xe0, 0x8a, 0x3c];
const C_VIDEO: [u8; 3] = [0xd9, 0x57, 0x8f];
const C_PATCHCORD: [u8; 3] = [0x5f, 0xa3, 0x6a];
/// What is chosen: the LCD's amber, and text on it as on the LCD.
const SEL: &str = "#f7bb62";
const SEL_TEXT: &str = "#3a2408";
/// The legend card, made by docs/design/engine/legend2.py.
const LEGEND_SVG: &str = include_str!("legend.svg");

const SHOW: [(&str, &str); 4] = [("PATCHCORD", "Patchcord's own signal meters"), ("MIDI", "MIDI nodes"), ("NOT RUNNING", "nodes no driver clocks now: suspended devices, idle streams"), ("INTERNAL", "Dummy-Driver and Freewheel-Driver: PipeWire's own clocks, idle")];
const OBJECT_TYPES: [&str; 12] = ["Core", "Module", "Factory", "Client", "Device", "Node", "Port", "Link", "Metadata", "Profiler", "SecurityContext", "Other"];

fn esc(t: &str) -> String {
    // control characters (a NUL in a driver's string) are not allowed in SVG
    t.chars().filter(|c| !c.is_control()).collect::<String>().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn cut(t: &str, n: usize) -> String {
    if t.chars().count() > n { format!("{}…", t.chars().take(n.saturating_sub(1)).collect::<String>()) } else { t.to_owned() }
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

/// Where a link at height `y` crosses a column with nodes at `rects` (top, bottom): at `y` when
/// that is clear of them, else just above or below the nearest of them.
fn free_y(rects: &[(f64, f64)], y: f64, extent: f64) -> f64 {
    let clear = |f: f64, m: f64| !rects.iter().any(|(t, b)| f + extent > t - m && f < b + m);
    if clear(y, 5.0) {
        return y;
    }
    // just above or below a node (half a gap off it: the middle of the gap between two), with room
    // for a bundle `extent` deep under its first line
    let free = rects.iter().flat_map(|(t, b)| [t - NGAP / 2.0 - extent, b + NGAP / 2.0]);
    free.filter(|f| clear(*f, 2.0)).min_by(|p, q| (p - y).abs().total_cmp(&(q - y).abs())).unwrap_or(y)
}

/// A cubic piece in two halves (de Casteljau).
fn split(c: [(f64, f64); 4]) -> [[(f64, f64); 4]; 2] {
    let mid = |p: (f64, f64), q: (f64, f64)| ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
    let (a, b, d) = (mid(c[0], c[1]), mid(c[1], c[2]), mid(c[2], c[3]));
    let (e, f) = (mid(a, b), mid(b, d));
    let g = mid(e, f);
    [[c[0], a, e, g], [g, f, d, c[3]]]
}

/// A cubic piece moved `s` off itself, square to its way (down, going right), its shape kept:
/// the edges of its control polygon moved and met again (Tiller and Hanson). So the links of a
/// stereo pair keep the same distance apart all along, on the slopes too.
fn offset_cubic(c: [(f64, f64); 4], s: f64) -> [(f64, f64); 4] {
    if s == 0.0 {
        return c;
    }
    let normal = |p: (f64, f64), q: (f64, f64)| {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let l = dx.hypot(dy);
        (l > 1e-6).then(|| (-dy / l, dx / l))
    };
    // each edge's normal, a short one taking its neighbour's
    let n = [normal(c[0], c[1]), normal(c[1], c[2]), normal(c[2], c[3])];
    let any = n.iter().flatten().next().copied().unwrap_or((0.0, 1.0));
    let n = [n[0].or(n[1]).unwrap_or(any), n[1].or(n[0]).or(n[2]).unwrap_or(any), n[2].or(n[1]).unwrap_or(any)];
    let moved = |p: (f64, f64), k: usize| (p.0 + n[k].0 * s, p.1 + n[k].1 * s);
    // where two moved edges meet (the point moved square to the first, when they run alike)
    let meet = |a: (f64, f64), da: (f64, f64), b: (f64, f64), db: (f64, f64), fallback: (f64, f64)| {
        let den = da.0 * db.1 - da.1 * db.0;
        if den.abs() < 1e-6 {
            return fallback;
        }
        let t = ((b.0 - a.0) * db.1 - (b.1 - a.1) * db.0) / den;
        (a.0 + da.0 * t, a.1 + da.1 * t)
    };
    let dir = |k: usize| (n[k].1, -n[k].0);
    let p0 = moved(c[0], 0);
    let p3 = moved(c[3], 2);
    let p1 = meet(p0, dir(0), moved(c[1], 1), dir(1), moved(c[1], 0));
    let p2 = meet(moved(c[2], 1), dir(1), p3, dir(2), moved(c[2], 2));
    [p0, p1, p2, p3]
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
    /// the graph being drawn on the painter's threads: its sig, what it is drawn from, the
    /// view's scale then, its size
    graph_want: Option<(u64, u64, f64, f64, f64)>,
    inbox: painter::Inbox,
    strip: Option<(u64, Layer)>,
    insp: Option<(u64, Layer)>,
    bottom: Option<(u64, Layer)>,
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
    zoomed: Option<Instant>,
    /// the selection came from a list: bring it into view
    reveal: bool,
    /// the tabs below are put away (the inspector shows only with a selection)
    bottom_shut: bool,
    /// the legend is open over the graph
    legend_open: bool,
    /// content heights of the inspector's and the tabs' bodies, to clamp scrolling
    content_h: [f64; 2],
    /// the i cell's card, and whether it is open
    info: Option<(u64, Layer)>,
    info_open: bool,
    /// MAC addresses masked in names, for pictures others see
    pub private: bool,
    /// the details and the tables being drawn on the painter's threads: what for, where
    insp_want: Option<(u64, [f64; 4])>,
    bottom_want: Option<(u64, [f64; 4])>,
    strip_want: Option<(u64, [f64; 4])>,
}

/// What the last look saw: each link's ends by name, each node's name and state.
type Seen = (HashMap<u32, String>, HashMap<u32, (String, &'static str)>);
/// A press in the graph: where, the view then, what was under it, whether it moved since.
type Press = ((f64, f64), View, Option<Sel>, bool);
/// Where a node is on the desk: its device's key and name, its frames, hidden, its colour.
type DeskPlace = (String, String, Vec<String>, bool);
/// A block of the graph: its sort key, (node, x, y), its width and height.
type Block = (String, Vec<(u32, f64, f64)>, f64, f64);

#[derive(Default)]
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
            tab: Tab::Top,
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
            graph_want: None,
            inbox: painter::Inbox::default(),
            strip: None,
            insp: None,
            bottom: None,
            overlay: None,
            legend: None,
            spent: Vec::new(),
            nums: HashMap::new(),
            hits: Vec::new(),
            press: None,
            zoomed: None,
            reveal: false,
            bottom_shut: true,
            legend_open: false,
            info: None,
            info_open: false,
            private: false,
            insp_want: None,
            bottom_want: None,
            strip_want: None,
            content_h: [0.0, 0.0],
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
            (self.strip, self.strip_want) = (None, None);
            (self.insp, self.insp_want) = (None, None);
            (self.bottom, self.bottom_want) = (None, None);
            self.overlay = None;
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
                        self.meta.insert((*id, *subject, k.clone()), (ty.clone(), v.clone()));
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
        self.events.push_front((SystemTime::now(), kind, text, node));
        if self.events.len() > 300 {
            self.events.pop_back();
        }
    }

    /// Builds what ENGINE shows from the graph and the desk's devices.
    pub fn build(&mut self, graph: &Graph, devices: &[&Device], hidden: &HashSet<String>, settings: &HashMap<String, String>) {
        let now = Instant::now();
        if !self.dirty && self.built.is_some_and(|t| now - t < Duration::from_millis(1000)) {
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
                    let class = get(p, "media.class");
                    let desc = p.get("node.description").or(p.get("node.nick")).cloned().unwrap_or_else(|| name.clone());
                    let nkind = if name == "Dummy-Driver" || name == "Freewheel-Driver" || p.get("factory.name").is_some_and(|f| f == "support.node.driver") {
                        NKind::Driver
                    } else if name.starts_with(METER_PREFIX) || name.starts_with(CUE_NODE) {
                        NKind::Meter
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
                    let system = class.ends_with("/Internal") || (nkind != NKind::Meter && p.get("stream.monitor").is_some_and(|v| v == "true"));
                    // a block's colour says what it is (the legend's COLOURS)
                    let color = match nkind {
                        NKind::Meter => C_PATCHCORD,
                        NKind::Midi => C_MIDI,
                        NKind::Video => C_VIDEO,
                        _ if system => C_SYSTEM,
                        NKind::Stream => C_APP,
                        NKind::Device => C_DEVICE,
                        NKind::Driver | NKind::Other => C_SYSTEM,
                    };
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
        self.props = graph.objects().map(|(id, _, p)| (id, p.clone())).collect();
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
        self.issues = self.find_issues(graph, devices, settings);
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
    fn find_issues(&self, graph: &Graph, devices: &[&Device], settings: &HashMap<String, String>) -> Vec<Issue> {
        let m = &self.model;
        let mut v = Vec::new();
        for n in &m.nodes {
            if n.state == "error" {
                v.push(Issue { level: 2, tag: "ERROR", node: Some(n.id), title: format!("{} is in error", n.desc), hint: "PipeWire could not start it: its device may be busy or gone.".into() });
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
                v.push(Issue { level: 2, tag: "LINK", node: Some(l.out_node), title: format!("link {} is in error", l.id), hint: "Its two ends could not agree on a format.".into() });
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
        // apps whose streams are linked to nothing (Patchcord's passive meters don't count)
        let mut lone: BTreeMap<String, (usize, u32)> = BTreeMap::new();
        for n in &m.nodes {
            if n.kind == NKind::Stream && !n.system && !m.links.iter().any(|l| !l.passive && (l.out_node == n.id || l.in_node == n.id)) {
                let app = graph.props_of(n.id).and_then(|p| p.get("application.name").cloned()).unwrap_or_else(|| n.desc.clone());
                let e = lone.entry(app).or_insert((0, n.id));
                e.0 += 1;
            }
        }
        for (app, (k, id)) in lone {
            v.push(Issue { level: 1, tag: "NO LINKS", node: Some(id), title: format!("{app}: {k} stream{} linked to nothing", if k == 1 { "" } else { "s" }), hint: "It plays nowhere or records silence. Usually a paused app; if not, its target is gone.".into() });
        }
        let quantum = settings.get("clock.quantum").and_then(|v| v.parse::<u64>().ok()).unwrap_or(1024);
        for g in &m.groups {
            if g.quantum >= quantum {
                continue;
            }
            let asks = g.members.iter().filter_map(|id| {
                let lat = graph.props_of(*id)?.get("node.latency")?.split_once('/').map(|(q, r)| (q.parse::<u64>().ok(), r.parse::<u64>().ok()))?;
                let (Some(q), Some(r)) = lat else { return None };
                Some((q * g.rate as u64 / r.max(1), *id))
            });
            // only the one that made it this small: what it asks for is the group's quantum
            if let Some((q, id)) = asks.min().filter(|(q, _)| *q <= g.quantum + g.quantum / 50)
                && let Some(n) = m.node(id)
            {
                let d = m.node(g.driver).map_or("?".into(), |n| n.desc.clone());
                v.push(Issue { level: 0, tag: "QUANTUM", node: Some(id), title: format!("{} asks for {q} samples: {d} runs at quantum {} ({})", n.desc, g.quantum, ms(g.quantum, g.rate)), hint: "Lower latency for the whole group, and more wake-ups a second. Fine for recording.".into() });
            }
        }
        for g in &m.groups {
            let cards: HashSet<String> = g.members.iter().filter_map(|id| graph.props_of(*id)?.get("device.id").cloned()).collect();
            if cards.len() > 1 {
                let d = m.node(g.driver).map_or("?".into(), |n| n.desc.clone());
                v.push(Issue { level: 0, tag: "ONE CLOCK", node: Some(g.driver), title: format!("{} devices run on one clock: {d}'s", cards.len()), hint: "Two devices never tick exactly together: the followers are resampled a tiny bit all the time to stay in step.".into() });
            }
        }
        for d in devices {
            if let Some(c) = &d.card
                && c.profile().is_some_and(|p| p.name == "off")
            {
                v.push(Issue { level: 0, tag: "PROFILE OFF", node: None, title: format!("{}: profile Off, so it has no nodes", d.name), hint: "PROFILE on its face (DESK) turns it on.".into() });
            }
        }
        let meters: Vec<&ENode> = m.nodes.iter().filter(|n| n.kind == NKind::Meter).collect();
        if !meters.is_empty() {
            let busy: i64 = meters.iter().filter_map(|n| m.timing.get(&n.id)).map(|t| t.busy_ns).sum();
            v.push(Issue {
                level: 0,
                tag: "PATCHCORD",
                node: Some(meters[0].id),
                title: format!("Patchcord measures with {} meter nodes: {:.0} µs per cycle", meters.len(), busy as f64 / 1000.0),
                hint: "Passive nodes and links: they follow what runs and wake nothing. They go with Patchcord.".into(),
            });
        }
        v.sort_by_key(|i| std::cmp::Reverse(i.level));
        v
    }

    // ---------------- layout of the graph ----------------

    fn visible(&self, n: &ENode) -> bool {
        let running = self.model.group_of.contains_key(&n.id);
        (self.show[0] || n.kind != NKind::Meter) && (self.show[1] || n.kind != NKind::Midi) && (self.show[2] || running) && (self.show[3] || n.kind != NKind::Driver || running)
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
        let mut lay = Layout::default();
        let mut y = 0.0;
        for (gi, what, ids) in bands {
            if ids.is_empty() {
                continue;
            }
            let set: HashSet<u32> = ids.iter().copied().collect();
            let ls: Vec<&ELink> = m.links.iter().filter(|l| set.contains(&l.out_node) && set.contains(&l.in_node) && l.out_node != l.in_node).collect();
            // blocks: nodes joined by links
            let mut root: HashMap<u32, u32> = ids.iter().map(|i| (*i, *i)).collect();
            fn find(r: &mut HashMap<u32, u32>, x: u32) -> u32 {
                let p = r[&x];
                if p == x {
                    return x;
                }
                let q = find(r, p);
                r.insert(x, q);
                q
            }
            for l in &ls {
                let (a, b) = (find(&mut root, l.out_node), find(&mut root, l.in_node));
                root.insert(a, b);
            }
            let mut comps: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for id in &ids {
                let r = find(&mut root, *id);
                comps.entry(r).or_default().push(*id);
            }
            let mut blocks: Vec<Block> = comps.into_values().map(|c| self.block(&c, &ls)).collect();
            blocks.sort_by(|a, b| a.0.cmp(&b.0));
            let cap = blocks.iter().map(|b| b.3).fold(380.0, f64::max);
            let (mut x, mut cy, mut cw, mut h) = (BPAD, 0.0, 0.0f64, 0.0f64);
            for (_, place, bw, bh) in &blocks {
                if cy > 0.0 && cy + bh > cap {
                    x += cw + 70.0;
                    cy = 0.0;
                    cw = 0.0;
                }
                for (id, dx, dy) in place {
                    lay.pos.insert(*id, (x + dx, y + BHEAD + cy + dy));
                }
                cy += bh + NGAP * 2.0;
                cw = cw.max(*bw);
                h = h.max(cy - NGAP * 2.0);
            }
            lay.w = lay.w.max(x + cw + BPAD);
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

    /// One block: its nodes in columns by signal flow (meters last), each column ordered after
    /// the one before it so links cross less. (sort key, (node, x, y), width, height)
    fn block(&self, ids: &[u32], ls: &[&ELink]) -> Block {
        let m = &self.model;
        let set: HashSet<u32> = ids.iter().copied().collect();
        let ls: Vec<&&ELink> = ls.iter().filter(|l| set.contains(&l.out_node)).collect();
        let kind = |id: u32| m.node(id).map_or(NKind::Other, |n| n.kind);
        let mut col: HashMap<u32, usize> = ids.iter().map(|i| (*i, 0)).collect();
        for _ in 0..ids.len() {
            for l in &ls {
                if kind(l.in_node) != NKind::Meter && col[&l.in_node] <= col[&l.out_node] {
                    col.insert(l.in_node, col[&l.out_node] + 1);
                }
            }
        }
        let mx = ids.iter().filter(|i| kind(**i) != NKind::Meter).map(|i| col[i]).max();
        for i in ids {
            if kind(*i) == NKind::Meter {
                col.insert(*i, mx.map_or(0, |m| m + 1));
            }
        }
        let n_cols = col.values().max().map_or(0, |m| m + 1);
        let mut cols: Vec<Vec<u32>> = vec![Vec::new(); n_cols];
        for i in ids {
            cols[col[i]].push(*i);
        }
        let cols: Vec<Vec<u32>> = cols.into_iter().filter(|c| !c.is_empty()).collect();
        // each placed node's column and top; each column's nodes (top, bottom)
        let mut at: HashMap<u32, (usize, f64)> = HashMap::new();
        let mut rects: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut place = Vec::new();
        let (mut w, mut h) = (0.0f64, 0.0f64);
        for (ci, mut c) in cols.into_iter().enumerate() {
            // the height a link comes in at: its port's, moved to the gaps it takes past the
            // columns between (as link_path draws it), so that the order makes no crossings
            let arrive = |l: &ELink, at: &HashMap<u32, (usize, f64)>, rects: &[Vec<(f64, f64)>]| {
                let (cs, top) = *at.get(&l.out_node)?;
                let i = m.node(l.out_node)?.outs.iter().position(|p| p.id == l.out_port)?;
                let mut y = top + NH0 + i as f64 * PR + PR / 2.0 + 2.0;
                for r in rects.iter().take(ci).skip(cs + 1) {
                    y = free_y(r, y, 0.0);
                }
                Some(y)
            };
            let bary = |id: u32, at: &HashMap<u32, (usize, f64)>, rects: &[Vec<(f64, f64)>]| {
                let ps: Vec<f64> = ls.iter().filter(|l| l.in_node == id).filter_map(|l| arrive(l, at, rects)).collect();
                if ps.is_empty() { 1e6 + kind(id) as u8 as f64 } else { ps.iter().sum::<f64>() / ps.len() as f64 }
            };
            let desc = |id: u32| m.node(id).map_or(String::new(), |n| n.desc.clone());
            if ci == 0 {
                c.sort_by(|a, b| kind(*a).cmp(&kind(*b)).then(desc(*a).cmp(&desc(*b))));
            } else {
                c.sort_by(|a, b| bary(*a, &at, &rects).total_cmp(&bary(*b, &at, &rects)));
            }
            let mut y = 0.0;
            let mut col_rects = Vec::new();
            for id in &c {
                let nh = m.node(*id).map_or(60.0, node_h);
                at.insert(*id, (ci, y));
                col_rects.push((y, y + nh));
                place.push((*id, ci as f64 * COLW, y));
                y += nh + NGAP;
            }
            rects.push(col_rects);
            h = h.max(y - NGAP);
            w = ci as f64 * COLW + NW;
        }
        let first = ids.iter().min_by_key(|i| (kind(**i), m.node(**i).map(|n| n.desc.clone()))).copied().unwrap_or(0);
        let key = format!("{}{}", kind(first) as u8, m.node(first).map_or("", |n| n.desc.as_str()));
        (key, place, w, h)
    }

    fn port_xy(&self, lay: &Layout, node: u32, port: u32) -> Option<(f64, f64)> {
        let (x, y) = *lay.pos.get(&node)?;
        let n = self.model.node(node)?;
        if let Some(i) = n.ins.iter().position(|p| p.id == port) {
            return Some((x, y + NH0 + i as f64 * PR + PR / 2.0 + 2.0));
        }
        let i = n.outs.iter().position(|p| p.id == port)?;
        Some((x + NW, y + NH0 + i as f64 * PR + PR / 2.0 + 2.0))
    }

    /// A link's way from `a` to `b` as cubic pieces: one curve, or, when nodes in a column it
    /// crosses stand in its way, through the gap beside them, straight across that column.
    /// Where a link out of `port` runs in the bundle of its node's links: as far below the first
    /// one as its port is below the first port, and how deep the bundle is.
    fn lane(&self, node: u32, port: u32) -> (f64, f64) {
        let Some(n) = self.model.node(node) else { return (0.0, 0.0) };
        let Some(i) = n.outs.iter().position(|p| p.id == port) else { return (0.0, 0.0) };
        (i as f64 * PR, (n.outs.len() as f64 - 1.0) * PR)
    }

    /// A link's way from `a` to `b`. The links out of one node go as a bundle (see `lane`): the
    /// way is found once, for the first, and each one is that way moved `lane.0` off it, so a
    /// stereo pair keeps its distance apart all along, round the nodes in its way too.
    fn link_path(&self, lay: &Layout, a: (f64, f64), b: (f64, f64), lane: (f64, f64)) -> Vec<[(f64, f64); 4]> {
        let (off, extent) = lane;
        let (a, b) = ((a.0, a.1 - off), (b.0, b.1 - off));
        let curve = |p: (f64, f64), q: (f64, f64)| {
            let d = ((q.0 - p.0).abs() / 2.0).max(50.0);
            [p, (p.0 + d, p.1), (q.0 - d, q.1), q]
        };
        // the nodes between the two ends, column by column
        let mut cols: BTreeMap<i64, Vec<(f64, f64)>> = BTreeMap::new();
        // only its own band's: the next band's nodes in the same column are no gap to take
        let band = lay.bands.iter().find(|(y, bh, ..)| a.1 >= *y && a.1 <= y + bh).map_or((f64::MIN, f64::MAX), |(y, bh, ..)| (*y, y + bh));
        if b.0 > a.0 + NW {
            for n in &self.model.nodes {
                if let Some(&(x, y)) = lay.pos.get(&n.id)
                    && x > a.0 + 1.0
                    && x + NW < b.0 - 1.0
                    && y >= band.0
                    && y <= band.1
                {
                    cols.entry(x.round() as i64).or_default().push((y, y + node_h(n)));
                }
            }
        }
        let mut out = Vec::new();
        let mut from = a;
        for (x, mut rects) in cols {
            let x = x as f64;
            // the curve as it would go: does it cross a node of this column?
            let c = curve(from, b);
            let hits = (0..=48).any(|k| {
                let t = k as f64 / 48.0;
                let u = 1.0 - t;
                let px = u * u * u * c[0].0 + 3.0 * u * u * t * c[1].0 + 3.0 * u * t * t * c[2].0 + t * t * t * c[3].0;
                let py = u * u * u * c[0].1 + 3.0 * u * u * t * c[1].1 + 3.0 * u * t * t * c[2].1 + t * t * t * c[3].1;
                px > x - 4.0 && px < x + NW + 4.0 && rects.iter().any(|(t, b)| py + extent > t - 4.0 && py < b + 4.0)
            });
            if !hits {
                continue;
            }
            rects.sort_by(|p, q| p.0.total_cmp(&q.0));
            let wy = free_y(&rects, from.1, extent);
            let (l, r) = ((x - 12.0, wy), (x + NW + 12.0, wy));
            out.push(curve(from, l));
            out.push([l, l, r, r]);
            from = r;
        }
        out.push(curve(from, b));
        if off == 0.0 {
            return out;
        }
        // in quarters first: a curve moved off itself in one piece bends too far on its slopes
        out.into_iter().flat_map(|c| split(c).into_iter().flat_map(split)).map(|c| offset_cubic(c, off)).collect()
    }

    // ---------------- drawing ----------------

    /// The node a selection belongs to, and the nodes it touches.
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
            let (col, head) = match gi {
                Some(gi) => {
                    let g = &m.groups[*gi];
                    let d = m.node(g.driver);
                    (
                        d.map_or([0x8c, 0x8e, 0x91], |n| n.color),
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
        let any = self.sel.is_some() && (!focus.is_empty() || matches!(self.sel, Some(Sel::Link(_))));
        // what is chosen turns the LCD's amber: a node where it shows its colour (frame, header,
        // ports, the links out of it), a port as a whole, a link and the ports at its ends
        let chosen_node = |id: u32| focus.contains(&id) && matches!(self.sel, Some(Sel::Node(_) | Sel::Obj(_)));
        let lit_ports: HashSet<u32> = match self.sel {
            Some(Sel::Port(p)) => [p].into(),
            Some(Sel::Link(id)) => m.links.iter().filter(|l| l.id == id).flat_map(|l| [l.out_port, l.in_port]).collect(),
            _ => HashSet::new(),
        };
        for l in &m.links {
            let (Some(a), Some(b)) = (self.port_xy(lay, l.out_node, l.out_port), self.port_xy(lay, l.in_node, l.in_port)) else { continue };
            let d = self.link_path(lay, a, b, self.lane(l.out_node, l.out_port)).iter().fold(format!("M{},{}", a.0, a.1), |mut d, c| {
                let _ = write!(d, " C{},{} {},{} {},{}", c[1].0, c[1].1, c[2].0, c[2].1, c[3].0, c[3].1);
                d
            });
            let hl = self.sel == Some(Sel::Link(l.id)) || focus.contains(&l.out_node) || focus.contains(&l.in_node) || self.sel.is_some_and(|s| s == Sel::Port(l.out_port) || s == Sel::Port(l.in_port));
            // the colour of the block the sound comes out of; the state in its width, dashes and fading
            let from = hex(m.node(l.out_node).map_or(C_SYSTEM, |n| n.color));
            let (stroke, width, extra) = if l.state == "error" {
                ("#ff4a32".to_owned(), 2.0, "")
            } else if l.passive {
                (from, 1.2, r#" stroke-dasharray="3 4""#)
            } else if l.state == "active" {
                (from, 2.0, "")
            } else {
                (from, 1.6, r#" stroke-opacity=".45""#)
            };
            let op = if any && !hl { 0.2 } else { 1.0 };
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
            let dim = any && !on && !near.contains(&n.id);
            let running = m.group_of.contains_key(&n.id);
            let op = if dim { 0.35 } else if running { 1.0 } else { 0.72 };
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
            if n.kind == NKind::Meter {
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
            let busy = if numbers { t.filter(|_| running).map_or(String::new(), |t| format!("{:.0}µs", t.busy_ns as f64 / 1000.0)) } else { String::new() };
            let picked = chosen_node(n.id);
            let c = if picked { SEL.to_owned() } else { hex(n.color) };
            // the header strip, then the body over it (rounded as the frame's inside), then the frame
            let _ = write!(
                o,
                r##"<g transform="translate({x},{y})" opacity="{op}"><path d="M0,6 A6,6 0 0 1 6,0 H{} A6,6 0 0 1 {NW},6 V12 H0 Z" fill="{c}"/><rect x=".75" y="4" width="{}" height="{}" rx="5.25" fill="#1d1e21"/><rect width="{NW}" height="{nh}" rx="6" fill="none" stroke="{c}" stroke-width="{}"/><circle cx="12" cy="17" r="4" fill="{}"/><text x="22" y="21" font-size="12" font-weight="bold" fill="#e6e7e8">{}</text><text x="12" y="34" font-family="Noto Sans Mono" font-size="10" fill="#7d8085">{} · {}</text><text x="{}" y="21" text-anchor="end" font-family="Noto Sans Mono" font-size="10" fill="#5cff7a">{busy}</text>"##,
                NW - 6.0,
                NW - 1.5,
                nh - 4.75,
                if picked { 2.5 } else { 1.5 },
                led_color(if running { "running" } else { n.state }, xr),
                esc(&cut(&n.desc, 26)),
                n.id,
                esc(&cut(&n.class, class_n)),
                NW - 10.0
            );
            if !tags.is_empty() {
                let _ = write!(o, r##"<text x="{}" y="34" text-anchor="end" font-size="8.5" font-weight="bold" letter-spacing=".5" fill="{}">{}</text>"##, NW - 10.0, if xr { "#ff6a52" } else { "#e58f2c" }, esc(&tag_s));
            }
            // ports: ears from the edge in the block's colour (square there, round inside), the
            // whole ear the port; inputs on the left, outputs on the right
            for (out, ports) in [(false, &n.ins), (true, &n.outs)] {
                for (i, p) in ports.iter().enumerate() {
                    let py = NH0 + i as f64 * PR + PR / 2.0 + 2.0;
                    let label = port_label(n, p);
                    let w = ear_w(&label);
                    let (px, tx, anchor, d) = if out {
                        (NW, NW - 10.0, r#" text-anchor="end""#, format!("M{},{} H{} A6.5,6.5 0 0 0 {},{} H{} Z", NW - 0.75, py - 6.5, NW - w + 6.5, NW - w + 6.5, py + 6.5, NW - 0.75))
                    } else {
                        (0.0, 10.0, "", format!("M0.75,{} H{} A6.5,6.5 0 0 1 {},{} H0.75 Z", py - 6.5, w - 6.5, w - 6.5, py + 6.5))
                    };
                    let _ = if lit_ports.contains(&p.id) {
                        write!(o, r##"<path d="{d}" fill="{SEL}"/><circle cx="{px}" cy="{py}" r="4" fill="{SEL}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{}"{anchor} font-family="Noto Sans Mono" font-size="10" font-weight="bold" fill="{SEL_TEXT}">{}</text>"##, py + 3.5, esc(&label))
                    } else {
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
            let Some(gi) = gi else { continue };
            let g = &m.groups[*gi];
            let d = m.node(g.driver);
            // after the header drawn in the graph (all in a monospaced font)
            let x = BPAD + text::width(text::MONO_BOLD, "DRIVER", 12.0, 1.0) + text::width(text::MONO_BOLD, &format!("  {}  ", d.map_or("?", |n| n.desc.as_str())), 12.0, 0.0) + text::width(text::MONO, &band_rest(g, d), 12.0, 0.0);
            out.push(Num { key: format!("g{}", g.driver), x, y: y + 22.0, end: false, text: band_load(g), size: 12.0, fill: "#7d8085", alpha: 1.0 });
        }
        let (focus, near) = self.focus();
        let any = self.sel.is_some() && (!focus.is_empty() || matches!(self.sel, Some(Sel::Link(_))));
        for n in &m.nodes {
            let (Some(&(x, y)), Some(t)) = (lay.pos.get(&n.id), m.timing.get(&n.id)) else { continue };
            if !m.group_of.contains_key(&n.id) {
                continue;
            }
            let dim = any && !focus.contains(&n.id) && !near.contains(&n.id);
            out.push(Num { key: format!("n{}", n.id), x: x + NW - 10.0, y: y + 21.0, end: true, text: format!("{:.0}µs", t.busy_ns as f64 / 1000.0), size: 10.0, fill: "#5cff7a", alpha: if dim { 0.35 } else { 1.0 } });
        }
        out
    }

    /// The window's parts, in logical pixels: (strip, graph, inspector, tabs) as [x, y, w, h].
    fn areas(&self, w: f64, h: f64) -> [[f64; 4]; 4] {
        let top = panel::H;
        let bottom_h = if self.bottom_shut { 0.0 } else { BOTTOM_H.min(((h - top - panel::STATUS_H) * 0.4).max(120.0)) };
        let by = h - panel::STATUS_H - bottom_h;
        // the details: at most half the window; in a narrow one they open over the graph, which
        // keeps the whole width
        let iw = if self.sel.is_none() { 0.0 } else { INSP_W.min(w * 0.5) };
        let gw = if w < NARROW { w } else { w - iw };
        let gy = top + STRIP_H;
        [[0.0, top, w, STRIP_H], [0.0, gy, gw, (by - gy).max(0.0)], [w - iw, gy, iw, (by - gy).max(0.0)], [0.0, by, w, bottom_h]]
    }

    /// Whether a window point (in pixels) is over the graph.
    fn in_graph(&self, p: (f64, f64), size: (f64, f64), dpi: f64) -> bool {
        let [_, g, _, _] = self.areas(size.0 / dpi, size.1 / dpi);
        let (x, y) = (p.0 / dpi, p.1 / dpi);
        x >= g[0] && x < g[0] + g[2] && y >= g[1] && y < g[1] + g[3]
    }

    /// Brings every part up to date and returns the pictures to draw, in order: (layer, x, y, w,
    /// h) in window pixels.
    pub fn sprites(&mut self, size: (u32, u32), dpi: f64, version: &str) -> Vec<(&Layer, f64, f64, f64, f64)> {
        let (fw, fh) = (size.0 as f64 / dpi, size.1 as f64 / dpi);
        let [strip_a, graph_a, insp_a, bottom_a] = self.areas(fw, fh);
        self.spent.clear();
        let mut t = Instant::now();
        let mut lap = |spent: &mut Vec<(&'static str, Duration)>, what: &'static str| {
            spent.push((what, t.elapsed()));
            t = Instant::now();
        };
        // the graph: drawn again when what it shows changes, or sharp again after zooming
        let mut hsh = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(&self.model.links, self.sel, self.show), &mut hsh);
        for n in &self.model.nodes {
            std::hash::Hash::hash(&(n.id, n.state, &n.ins, &n.outs, &n.desc, self.model.group_of.get(&n.id), self.xruns.get(&n.id).map(|x| x.0)), &mut hsh);
        }
        for g in &self.model.groups {
            std::hash::Hash::hash(&(g.driver, g.quantum, g.rate, &g.members), &mut hsh);
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
            self.lay = self.layout();
            if !self.fitted && !self.lay.pos.is_empty() {
                self.fitted = true;
                self.fit_to(graph_a, dpi);
            }
            // as sharp as the view, within what one texture may hold
            let cap = 8000.0 / self.lay.w.max(self.lay.h).max(1.0);
            let r = self.view.s.min(cap);
            let want = painter::sig(sig, r);
            self.graph_want = Some((want, sig, self.view.s, self.lay.w.max(10.0), self.lay.h.max(10.0)));
            self.inbox.ask(painter::key("graph", 0u8), want, self.graph_svg(false), r);
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
        // the strip, the inspector, the tabs and the graph's overlay: in window units
        lap(&mut self.spent, "engine.reveal");
        let (strip_svg, info_at, hidden) = self.strip_svg(strip_a[2], version);
        let sig = hash_str(&strip_svg, dpi);
        // it changes with the load, a few times a second: on the painter's threads after the first
        if self.strip.is_none() {
            self.strip = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &strip_svg, dpi), x: strip_a[0], y: strip_a[1], w: strip_a[2], h: strip_a[3] }));
        } else if self.strip.as_ref().is_some_and(|(s, _)| *s != sig) && self.strip_want.map(|w| w.0) != Some(sig) {
            self.strip_want = Some((sig, strip_a));
            self.inbox.ask(painter::key("strip", 0u8), sig, strip_svg, dpi);
        }
        self.hits.clear();
        // the i cell, and its card under it while open, over everything below the strip
        match info_at {
            Some([ix, iy, iw, ih]) => {
                self.hits.push((Hit::Info, [strip_a[0] + ix, strip_a[1] + iy, iw, ih]));
                if self.info_open {
                    let (svg, w, h) = Self::info_svg(&hidden);
                    let sig = hash_str(&svg, dpi);
                    let x = (strip_a[0] + ix).min(fw - 6.0 - w).max(6.0);
                    let y = strip_a[1] + STRIP_H - 4.0;
                    if self.info.as_ref().is_none_or(|(s, _)| *s != sig) {
                        self.info = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x, y, w, h }));
                    }
                    if let Some((_, l)) = &mut self.info {
                        (l.x, l.y) = (x, y);
                    }
                    self.hits.push((Hit::LegendWin, [x, y, w, h]));
                } else {
                    self.info = None;
                }
            }
            None => {
                self.info_open = false;
                self.info = None;
            }
        }
        let info_hits = self.hits.len();
        // the legend, over the graph's bottom left just above its key, made smaller to fit a small
        // window
        if self.legend_open {
            let (lw, lh) = legend_size();
            let s = ((graph_a[2] - 12.0) / lw).min((graph_a[3] - 48.0) / lh).clamp(0.2, 1.0);
            let (w, h) = (lw * s, lh * s);
            let (x, y) = (graph_a[0] + 6.0, graph_a[1] + graph_a[3] - 36.0 - h);
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
        lap(&mut self.spent, "engine.strip+legend");
        // what of the graph the details leave in sight
        let seen = if self.sel.is_some() { graph_a[2].min(insp_a[0] - graph_a[0]) } else { graph_a[2] };
        let (ov, ov_hits) = self.overlay_svg(graph_a, seen);
        let sig = hash_str(&ov, dpi);
        if self.overlay.as_ref().is_none_or(|(s, _)| *s != sig) {
            self.overlay = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &ov, dpi), x: graph_a[0], y: graph_a[1], w: graph_a[2], h: graph_a[3] }));
        }
        self.hits.extend(ov_hits);
        lap(&mut self.spent, "engine.overlay");
        if self.sel.is_none() {
            (self.insp, self.insp_want) = (None, None);
        } else {
            let (isvg, ihits) = self.insp_svg(insp_a);
            let sig = hash_str(&isvg, dpi);
            // the first at once, then on the painter's threads (the old one shows meanwhile)
            if self.insp.is_none() {
                self.insp = Some((sig, Layer { id: next_id(), pm: rasterize(&text::svg_options(), &isvg, dpi), x: insp_a[0], y: insp_a[1], w: insp_a[2], h: insp_a[3] }));
            } else if self.insp.as_ref().is_some_and(|(s, _)| *s != sig) && self.insp_want.map(|w| w.0) != Some(sig) {
                self.insp_want = Some((sig, insp_a));
                self.inbox.ask(painter::key("details", 0u8), sig, isvg, dpi);
            }
            // before the graph's own: in a narrow window the details lie over it; the i cell's
            // card stays first
            self.hits.splice(info_hits..info_hits, ihits.into_iter().chain([(Hit::LegendWin, insp_a)]));
        }
        lap(&mut self.spent, "engine.details");
        if self.bottom_shut {
            (self.bottom, self.bottom_want) = (None, None);
        } else {
            let (bsvg, bhits) = self.bottom_svg(bottom_a);
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
        for (l, at) in [(&mut self.strip, strip_a), (&mut self.overlay, graph_a), (&mut self.insp, insp_a), (&mut self.bottom, bottom_a)] {
            if let Some((_, l)) = l {
                (l.x, l.y) = (at[0], at[1]);
            }
        }

        let v = self.view;
        let mut out = Vec::new();
        if let Some((_, _, l)) = &self.graph_layer {
            out.push((l, v.x, v.y, l.w * v.s, l.h * v.s));
        }
        for (_, l) in self.nums.values() {
            out.push((l, l.x * v.s + v.x, l.y * v.s + v.y, l.w * v.s, l.h * v.s));
        }
        for l in [&self.overlay, &self.legend, &self.strip, &self.insp, &self.bottom, &self.info].into_iter().flatten().map(|(_, l)| l) {
            out.push((l, l.x * dpi, l.y * dpi, l.w * dpi, l.h * dpi));
        }
        out
    }

    /// The GPU has these pictures' pixels now: they are let go. Returns how many bytes.
    pub fn uploaded(&mut self, has: impl Fn(u64) -> bool) -> usize {
        let mut freed = 0;
        let parts = [self.graph_layer.as_mut().map(|(_, _, l)| l), self.strip.as_mut().map(|(_, l)| l), self.insp.as_mut().map(|(_, l)| l), self.bottom.as_mut().map(|(_, l)| l), self.overlay.as_mut().map(|(_, l)| l), self.legend.as_mut().map(|(_, l)| l), self.info.as_mut().map(|(_, l)| l)];
        for l in self.nums.values_mut().map(|(_, l)| l).chain(parts.into_iter().flatten()) {
            if has(l.id) {
                freed += l.pm.take().map_or(0, |pm| pm.data().len());
            }
        }
        freed
    }

    /// ENGINE's strip `w` wide: its SVG, where its i cell is (in the strip) and what is behind it.
    fn strip_svg(&self, w: f64, version: &str) -> (String, Option<[f64; 4]>, Vec<(String, String)>) {
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
        // what matters most first: the clock, the quantum, the load, the xruns
        let cells: Vec<(String, String)> = vec![
            ("CLOCK".into(), format!("{rate} Hz{}", forced[0].as_ref().map_or(String::new(), |f| format!(" · forced {f}")))),
            ("QUANTUM".into(), format!("{q} · {qmin}–{qmax}{}", forced[1].as_ref().map_or(String::new(), |f| format!(" · forced {f}")))),
            ("DSP LOAD".into(), format!("{:.1}%", load * 100.0)),
            ("XRUNS".into(), xr.to_string()),
            ("DRIVERS".into(), format!("{} running", m.groups.len())),
            ("SERVER".into(), format!("PipeWire {version}")),
            ("SESSION".into(), session),
            ("ALLOWED".into(), cut(allowed.trim_matches(|c| c == '[' || c == ']').trim(), 22)),
            ("OBJECTS".into(), format!("{} · {} nodes · {ports} ports · {} links", self.objects.len(), m.nodes.len(), m.links.len())),
        ];
        // what matters first; the rest while there is room, then behind the i cell (DSP LOAD's
        // curve before them)
        let (first, rest) = cells.split_at(4);
        let cw = |k: &str, v: &str| text::width(text::MONO_BOLD, v, 12.0, 0.0).max(text::width(text::MONO_BOLD, k, 9.0, 1.2)) + 20.0;
        let room = w - 12.0 - HEALTH_W - 10.0;
        let used = 12.0 + first.iter().map(|(k, v)| cw(k, v) + 10.0).sum::<f64>();
        let all = used + CURVE_W + rest.iter().map(|(k, v)| cw(k, v) + 10.0).sum::<f64>();
        let (room, curve) = if all <= room { (room, true) } else { (room - INFO_W - 10.0, used + CURVE_W <= room - INFO_W - 10.0) };
        let mut shown: Vec<(&str, &str, bool)> = first.iter().map(|(k, v)| (k.as_str(), v.as_str(), curve && k == "DSP LOAD")).collect();
        let mut x = used + if curve { CURVE_W } else { 0.0 };
        let mut hidden = Vec::new();
        for (k, v) in rest {
            if hidden.is_empty() && x + cw(k, v) <= room {
                shown.push((k, v, false));
                x += cw(k, v) + 10.0;
            } else {
                hidden.push((k.clone(), v.clone()));
            }
        }
        let mut o = String::new();
        let mut x = 12.0;
        for (k, v, with_curve) in shown {
            let cw = cw(k, v) + if with_curve { CURVE_W } else { 0.0 };
            let _ = write!(
                o,
                r##"<rect x="{}" y="8" width="{}" height="36" rx="4" fill="#0c0c0d"/><rect x="{x}" y="10" width="{cw}" height="32" rx="3" fill="url(#lcd)"/><text x="{}" y="22" font-size="9" letter-spacing="1.2" fill="#3a2408" opacity=".65">{k}</text><text x="{}" y="37" font-size="12" fill="#3a2408" xml:space="preserve">{}</text>"##,
                x - 2.0,
                cw + 4.0,
                x + 10.0,
                x + 10.0,
                esc(v)
            );
            if with_curve {
                // the busiest group's load over the last 45 seconds
                let top = self.load.iter().copied().fold(0.01, f32::max) as f64 * 1.2;
                let px = x + cw - 190.0;
                let pts: Vec<String> = self.load.iter().enumerate().map(|(j, v)| format!("{:.1},{:.1}", px + j as f64 * 2.0, 39.0 - *v as f64 / top * 24.0)).collect();
                if pts.len() > 1 {
                    let _ = write!(o, r##"<polyline points="{}" fill="none" stroke="#3a2408" stroke-width="1.4"/>"##, pts.join(" "));
                }
            }
            x += cw + 10.0;
        }
        // the rest behind i: its card drops under it
        let info = (!hidden.is_empty()).then(|| {
            let _ = write!(
                o,
                r##"<rect x="{}" y="8" width="{}" height="36" rx="4" fill="#0c0c0d"/><rect x="{x}" y="10" width="{INFO_W}" height="32" rx="3" fill="url(#lcd)"{}/><text x="{}" y="32" font-family="Noto Sans" font-weight="bold" font-size="17" fill="#3a2408" text-anchor="middle">i</text>"##,
                x - 2.0,
                INFO_W + 4.0,
                if self.info_open { r##" stroke="#3a2408" stroke-width="2""## } else { "" },
                x + INFO_W / 2.0
            );
            [x, 10.0, INFO_W, 32.0]
        });
        // health: errors, warnings, notes
        let hx = w - HEALTH_W - 12.0;
        let _ = write!(
            o,
            r##"<rect x="{}" y="8" width="{}" height="36" rx="4" fill="#0c0c0d"/><rect x="{hx}" y="10" width="{HEALTH_W}" height="32" rx="3" fill="url(#lcd)"/><text x="{}" y="22" font-size="9" letter-spacing="1.2" fill="#3a2408" opacity=".65">HEALTH</text>"##,
            hx - 2.0,
            HEALTH_W + 4.0,
            hx + 10.0
        );
        for (j, (lv, c)) in [(2u8, "#ff4a32"), (1, "#ffb02e"), (0, "#5fd4ff")].iter().enumerate() {
            let cx = hx + 16.0 + j as f64 * 32.0;
            let _ = write!(o, r##"<circle cx="{cx}" cy="33" r="4" fill="{c}" stroke="#3a2408" stroke-opacity=".5"/><text x="{}" y="37" font-size="12" fill="#3a2408">{}</text>"##, cx + 8.0, cnt(*lv));
        }
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{STRIP_H}" viewBox="0 0 {w} {STRIP_H}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="lcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{STRIP_H}" fill="#1a1b1d"/><path d="M0,{} H{w}" stroke="#000"/>{o}</svg>"##,
            STRIP_H - 0.5
        );
        (svg, info, hidden)
    }

    /// The i cell's card: the strip's cells that didn't fit, one a line, as on the LCD.
    fn info_svg(cells: &[(String, String)]) -> (String, f64, f64) {
        let kw = cells.iter().map(|(k, _)| text::width(text::MONO_BOLD, k, 9.0, 1.2)).fold(0.0, f64::max) + 16.0;
        let vw = cells.iter().map(|(_, v)| text::width(text::MONO_BOLD, v, 12.0, 0.0)).fold(0.0, f64::max);
        let (w, h) = (14.0 + kw + vw + 14.0, 12.0 + cells.len() as f64 * 22.0 + 6.0);
        let mut o = String::new();
        for (i, (k, v)) in cells.iter().enumerate() {
            let y = 12.0 + i as f64 * 22.0 + 14.0;
            let _ = write!(o, r##"<text x="14" y="{}" font-size="9" letter-spacing="1.2" fill="#3a2408" opacity=".65">{k}</text><text x="{}" y="{y}" font-size="12" fill="#3a2408" xml:space="preserve">{}</text>"##, y - 1.0, 14.0 + kw, esc(v));
        }
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="lcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#0c0c0d"/><rect x="2" y="2" width="{}" height="{}" rx="4" fill="url(#lcd)"/>{o}</svg>"##,
            w - 4.0,
            h - 4.0
        );
        (svg, w, h)
    }

    fn meta_name(&self, id: u32) -> String {
        self.objects.iter().find(|o| o.1 == id && o.0 == "Metadata").map_or_else(|| id.to_string(), |o| o.2.clone())
    }

    /// The SHOW chips over the graph's top left corner, and the keys to the legend and the diagnostics at its bottom.
    /// `seen`: how much of its width the details leave in sight.
    fn overlay_svg(&self, a: [f64; 4], seen: f64) -> (String, Vec<(Hit, [f64; 4])>) {
        let (w, h) = (a[2].max(1.0), a[3].max(1.0));
        let seen = seen.clamp(1.0, w);
        let mut o = String::new();
        let mut hits = Vec::new();
        let _ = write!(o, r##"<text x="12" y="25" font-size="9" letter-spacing="1.2" fill="#7d8085">ALSO SHOW</text>"##);
        let (mut x, mut y) = (84.0, 10.0);
        for (i, (name, _)) in SHOW.iter().enumerate() {
            let cw = text::width(text::BOLD, name, 10.0, 0.5) + 30.0;
            // what doesn't fit goes on a row below
            if x > 84.0 && x + cw > seen - 12.0 {
                (x, y) = (84.0, y + 28.0);
            }
            let _ = write!(
                o,
                r##"<rect x="{x}" y="{y}" width="{cw}" height="22" rx="4" fill="#2a2b2e" stroke="#0b0b0c"/><circle cx="{}" cy="{}" r="3" fill="{}"/><text x="{}" y="{}" font-size="10" letter-spacing=".5" fill="#e6e7e8" font-family="Noto Sans" font-weight="bold">{name}</text>"##,
                x + 10.0,
                y + 11.0,
                if self.show[i] { "#ff4a32" } else { "#3a1512" },
                x + 19.0,
                y + 15.0
            );
            hits.push((Hit::Show(i), [a[0] + x, a[1] + y, cw, 22.0]));
            x += cw + 8.0;
        }
        let ly = h - 14.0;
        // the diagnostics below, put away: a key to bring them back, level with the legend
        if self.bottom_shut {
            // what is below, and how much is wrong
            let label = match self.issues.len() {
                0 => "DIAGNOSTICS".to_owned(),
                1 => "DIAGNOSTICS · 1 ISSUE".to_owned(),
                n => format!("DIAGNOSTICS · {n} ISSUES"),
            };
            let cw = text::width(text::BOLD, &label, 10.0, 0.5) + 36.0;
            let cx = (seen - cw - 12.0).max(118.0);
            let _ = write!(
                o,
                r##"<rect x="{cx}" y="{}" width="{cw}" height="24" rx="4" fill="#2a2b2e" stroke="#0b0b0c"/><path d="M{},{} l5,-5 l5,5" fill="none" stroke="#9a9da1" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/><text x="{}" y="{}" font-size="10" letter-spacing=".5" fill="#e6e7e8" font-family="Noto Sans" font-weight="bold">{label}</text>"##,
                ly - 16.0,
                cx + 10.0,
                ly - 1.5,
                cx + 26.0,
                ly
            );
            hits.push((Hit::Bottom, [a[0] + cx, a[1] + ly - 16.0, cw, 24.0]));
        }
        // the legend: a key to open it, level with the diagnostics' key
        let _ = write!(
            o,
            r##"<g transform="translate(6,{})"><rect width="104" height="24" rx="4" fill="{}" stroke="#0b0b0c"/><circle cx="14" cy="12" r="7" fill="none" stroke="#9a9da1" stroke-width="1.4"/><text x="14" y="15.5" font-family="Noto Sans" font-weight="bold" font-size="9.5" fill="#e6e7e8" text-anchor="middle">?</text><text x="28" y="16" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".5" fill="#e6e7e8">LEGEND</text></g>"##,
            ly - 16.0,
            if self.legend_open { "#3a3b3f" } else { "#2a2b2e" }
        );
        hits.push((Hit::Legend, [a[0] + 6.0, a[1] + ly - 16.0, 104.0, 24.0]));
        (format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono">{o}</svg>"#), hits)
    }

    /// The inspector: what is selected, with its tabs.
    fn insp_svg(&mut self, a: [f64; 4]) -> (String, Vec<(Hit, [f64; 4])>) {
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
                    ITab::Info => {
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
                            (_, NKind::Meter) => "Patchcord's own: a meter (the LEDs in a frame) or the cue. Passive: it follows what runs and wakes nothing. Not on the desk.".to_owned(),
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
                tabs = vec![ITab::Info, ITab::Props];
                if !tabs.contains(&self.itab) {
                    self.itab = ITab::Info;
                }
                let (n, p, input) = match m.port(id) {
                    Some((n, p, i)) => (n.clone(), p.clone(), i),
                    None => return (String::new(), hits),
                };
                if self.itab == ITab::Props {
                    kv(&mut body, &mut y, &props_of(id).iter().map(|(k, v)| (k.as_str(), v.clone())).collect::<Vec<_>>());
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
                let Some(l) = m.links.iter().find(|l| l.id == id).cloned() else { return (String::new(), hits) };
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
            let _ = write!(head, r##"<rect y="51" width="{w}" height="28" fill="#1a1b1d"/><path d="M0,79.5 H{w}" stroke="#000"/>"##);
            let mut x = 10.0;
            for t in &tabs {
                let name = match t {
                    ITab::Info => "INFO",
                    ITab::Props => "PROPS",
                    ITab::Params => "PARAMS",
                    ITab::Timing => "TIMING",
                };
                let tw = text::width(text::BOLD, name, 10.0, 0.6) + 24.0;
                let on = *t == self.itab;
                let _ = write!(
                    head,
                    r##"<rect x="{x}" y="57" width="{tw}" height="23" rx="3" fill="{}"/>{}<text x="{}" y="73" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".6" fill="{}" text-anchor="middle">{name}</text>"##,
                    if on { "#2e3034" } else { "#232427" },
                    if on { format!(r##"<rect x="{x}" y="57" width="{tw}" height="2" fill="#e58f2c"/>"##) } else { String::new() },
                    x + tw / 2.0,
                    if on { "#e6e7e8" } else { "#9a9da1" }
                );
                hits.push((Hit::ITab(*t), [a[0] + x, a[1] + 57.0, tw, 23.0]));
                x += tw + 4.0;
            }
            top = 80.0;
        }
        // the body, scrolled
        let body_h = h - top;
        self.content_h[0] = y + 16.0;
        let sc = self.scroll[0].clamp(0.0, (y + 16.0 - body_h).max(0.0));
        self.scroll[0] = sc;
        let by = top + 6.0 - sc;
        for (s, ry) in rows {
            let (yy, hh) = (by + ry, 19.0);
            if yy + hh > top && yy < h {
                hits.push((Hit::Sel(s), [a[0], a[1] + yy, w, hh]));
            }
        }
        if let Some((key, ry)) = &desk_button {
            let _ = write!(body, r##"<rect x="12" y="{ry}" width="150" height="24" rx="4" fill="#38393d"/><text x="87" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" fill="#e6e7e8" text-anchor="middle">SHOW ON DESK</text>"##, ry + 16.0);
            let yy = by + ry;
            if yy + 24.0 > top && yy < h {
                hits.push((Hit::Desk(key.clone()), [a[0] + 12.0, a[1] + yy, 150.0, 24.0]));
            }
        }
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono"><defs><clipPath id="c"><rect y="{top}" width="{w}" height="{body_h}"/></clipPath></defs><rect width="{w}" height="{h}" fill="#17181a"/><path d="M.5,0 V{h}" stroke="#000"/><g clip-path="url(#c)"><g transform="translate(0,{by})">{body}</g></g>{head}</svg>"##
        );
        (svg, hits)
    }

    /// The tabs below the graph: TOP, ISSUES, OBJECTS, METADATA, EVENTS.
    fn bottom_svg(&mut self, a: [f64; 4]) -> (String, Vec<(Hit, [f64; 4])>) {
        let (w, h) = (a[2].max(1.0), a[3].max(1.0));
        let m = &self.model;
        let mut hits = Vec::new();
        let mut head = format!(r##"<rect width="{w}" height="{TABS_H}" fill="#1a1b1d"/><path d="M0,.5 H{w} M0,{} H{w}" stroke="#000"/>"##, TABS_H - 0.5);
        let mut x = 10.0;
        for (t, name) in [(Tab::Top, "TOP".to_owned()), (Tab::Issues, format!("ISSUES {}", self.issues.len())), (Tab::Objects, "OBJECTS".into()), (Tab::Metadata, "METADATA".into()), (Tab::Events, "EVENTS".into())] {
            let tw = text::width(text::BOLD, &name, 10.0, 0.6) + 24.0;
            let on = t == self.tab;
            let _ = write!(
                head,
                r##"<rect x="{x}" y="6" width="{tw}" height="24" rx="3" fill="{}"/>{}<text x="{}" y="22" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".6" fill="{}" text-anchor="middle">{}</text>"##,
                if on { "#2e3034" } else { "#232427" },
                if on { format!(r##"<rect x="{x}" y="6" width="{tw}" height="2" fill="#e58f2c"/>"##) } else { String::new() },
                x + tw / 2.0,
                if on { "#e6e7e8" } else { "#9a9da1" },
                esc(&name)
            );
            hits.push((Hit::Tab(t), [a[0] + x, a[1] + 6.0, tw, 24.0]));
            x += tw + 4.0;
        }
        // puts the tabs away (DIAGNOSTICS, by the legend, brings them back)
        let tx = w - 32.0;
        let _ = write!(
            head,
            r##"<rect x="{tx}" y="6" width="24" height="24" rx="3" fill="#232427"/><path d="M{},15 l5,5 l5,-5" fill="none" stroke="#9a9da1" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"##,
            tx + 7.0
        );
        hits.push((Hit::Bottom, [a[0] + tx, a[1] + 6.0, 24.0, 24.0]));
        // a report for a bug ticket, a key as on the panel, where the diagnostics are
        let rx = (tx - 10.0 - REPORT_W).max(x + 6.0);
        let _ = write!(
            head,
            r##"<rect x="{}" y="4" width="{}" height="26" rx="4" fill="#050506" stroke="#55585b" stroke-width="1"/><rect x="{rx}" y="6" width="{REPORT_W}" height="22" rx="3" fill="#262729" stroke="#070708"/><text x="{}" y="21" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".4" fill="#e6e7e8" text-anchor="middle">SAVE REPORT</text>"##,
            rx - 2.0,
            REPORT_W + 4.0,
            rx + REPORT_W / 2.0
        );
        hits.push((Hit::Report, [a[0] + rx, a[1] + 4.0, REPORT_W, 26.0]));
        // the body: a table head (fixed) and rows (scrolled); rows that select carry a Sel
        let mut thead = String::new();
        let mut body = String::new();
        let mut rows: Vec<(Option<Sel>, f64, f64)> = Vec::new();
        let mut y = 0.0;
        let sel_row = |s: Option<Sel>| s.is_some() && s == self.sel;
        match self.tab {
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
                    let (c, tc) = match i.level {
                        2 => ("#ff4a32", "#ff6a52"),
                        1 => ("#ffb02e", "#ffb02e"),
                        _ => ("#5fd4ff", "#5fd4ff"),
                    };
                    let sel = i.node.map(Sel::Node);
                    if sel_row(sel) {
                        let _ = write!(body, r##"<rect y="{y}" width="{w}" height="40" fill="#2a2618"/>"##);
                    }
                    let _ = write!(
                        body,
                        r##"<circle cx="18" cy="{}" r="4" fill="{c}"/><text x="32" y="{}" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".6" fill="{tc}">{}</text><text x="140" y="{}" font-size="11.5" fill="#e6e7e8">{}</text><text x="140" y="{}" font-size="10.5" fill="#7d8085">{}</text><path d="M0,{} H{w}" stroke="#1c1d20"/>"##,
                        y + 14.0,
                        y + 18.0,
                        i.tag,
                        y + 17.0,
                        esc(&cut(&i.title, ((w - 150.0) / 7.0) as usize)),
                        y + 32.0,
                        esc(&cut(&i.hint, ((w - 150.0) / 6.4) as usize)),
                        y + 40.0
                    );
                    rows.push((sel, y, 40.0));
                    y += 40.0;
                }
            }
            Tab::Objects => {
                // the types on the left, the chosen type's objects on the right
                let mut ty = 0.0;
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
                for o in self.objects.iter().filter(|o| o.0 == self.otype) {
                    let sel = Some(match o.0.as_str() {
                        "Node" => Sel::Node(o.1),
                        "Port" => Sel::Port(o.1),
                        "Link" => Sel::Link(o.1),
                        _ => Sel::Obj(o.1),
                    });
                    if sel_row(sel) {
                        let _ = write!(body, r##"<rect x="172" y="{y}" width="{}" height="{ROW_H}" fill="#2a2618"/>"##, w - 172.0);
                    }
                    let _ = write!(
                        body,
                        r##"<text x="230" y="{}" font-size="11" fill="#a8abae" text-anchor="end">{}</text><text x="244" y="{}" font-size="11" fill="#e6e7e8" xml:space="preserve">{}</text><text x="{}" y="{}" font-size="11" fill="#7d8085" xml:space="preserve">{}</text>"##,
                        y + 14.0,
                        o.1,
                        y + 14.0,
                        esc(&cut(&o.2, ((w * 0.62 - 260.0) / 6.6).max(10.0) as usize)),
                        w * 0.62,
                        y + 14.0,
                        esc(&cut(&o.3, ((w * 0.38 - 16.0) / 6.6).max(6.0) as usize))
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
        let thead_h = if matches!(self.tab, Tab::Top | Tab::Metadata) { 20.0 } else { 0.0 };
        let top = TABS_H + 4.0 + thead_h;
        let body_h = h - top;
        self.content_h[1] = y + 8.0;
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
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono"><defs><clipPath id="c"><rect y="{top}" width="{w}" height="{body_h}"/></clipPath></defs><rect width="{w}" height="{h}" fill="#131416"/><g clip-path="url(#c)"><g transform="translate(0,{by})">{body}</g></g>{thead_svg}{head}</svg>"##
        );
        (svg, hits)
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
                Hit::Legend => self.legend_open = !self.legend_open,
                Hit::LegendWin => {}
                Hit::Info => self.info_open = !self.info_open,
            }
            return None;
        }
        if self.in_graph(p, (size.0 as f64, size.1 as f64), dpi) {
            let w = ((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s);
            self.press = Some((p, self.view, self.graph_hit(w), false));
        }
        None
    }

    pub fn drag(&mut self, p: (f64, f64)) -> bool {
        let Some((from, v, _, moved)) = &mut self.press else { return false };
        if !*moved && ((p.0 - from.0).abs() > 4.0 || (p.1 - from.1).abs() > 4.0) {
            *moved = true;
        }
        if *moved {
            self.view.x = v.x + p.0 - from.0;
            self.view.y = v.y + p.1 - from.1;
        }
        *moved
    }

    pub fn release(&mut self) {
        if let Some((_, _, hit, moved)) = self.press.take()
            && !moved
        {
            self.select(hit);
        }
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

    /// Escape: closes the legend if it is open, else drops the selection (which closes the
    /// inspector).
    pub fn deselect(&mut self) {
        if self.legend_open {
            self.legend_open = false;
        } else {
            self.select(None);
        }
    }

    /// The ? key: opens or closes the legend.
    /// Where in the window (pixels) node `name` (its description or name, or how it starts) has
    /// its header, once laid out: for the README's animation.
    pub fn node_point(&self, name: &str) -> Option<(f64, f64)> {
        let n = self.model.nodes.iter().find(|n| n.desc.starts_with(name) || n.name.starts_with(name))?;
        let (x, y) = self.lay.pos.get(&n.id)?;
        Some(((x + 60.0) * self.view.s + self.view.x, (y + 12.0) * self.view.s + self.view.y))
    }

    /// Opens the strip's i card (for pictures without a window).
    pub fn open_info(&mut self) {
        self.info_open = true;
    }

    pub fn toggle_legend(&mut self) {
        self.legend_open = !self.legend_open;
    }

    /// What is under a point of the graph (world units): a port, a node, or a link.
    fn graph_hit(&self, (x, y): (f64, f64)) -> Option<Sel> {
        let lay = &self.lay;
        for n in &self.model.nodes {
            let Some(&(nx, ny)) = lay.pos.get(&n.id) else { continue };
            // a port: its jack, or anywhere on its ear
            for (i, p) in n.ins.iter().enumerate() {
                let py = ny + NH0 + i as f64 * PR + PR / 2.0 + 2.0;
                let w = ear_w(&port_label(n, p));
                if (x - nx).abs() < 8.0 && (y - py).abs() < 7.0 || (x >= nx && x <= nx + w && (y - py).abs() <= PR / 2.0) {
                    return Some(Sel::Port(p.id));
                }
            }
            for (i, p) in n.outs.iter().enumerate() {
                let py = ny + NH0 + i as f64 * PR + PR / 2.0 + 2.0;
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
            let path = self.link_path(lay, a, b, self.lane(l.out_node, l.out_port));
            for (k, c) in (0..=40).flat_map(|k| path.iter().map(move |c| (k, c))) {
                let t = k as f64 / 40.0;
                let u = 1.0 - t;
                let px = u * u * u * c[0].0 + 3.0 * u * u * t * c[1].0 + 3.0 * u * t * t * c[2].0 + t * t * t * c[3].0;
                let py = u * u * u * c[0].1 + 3.0 * u * u * t * c[1].1 + 3.0 * u * t * t * c[2].1 + t * t * t * c[3].1;
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
        let [_, _, insp, bottom] = self.areas(size.0 as f64 / dpi, size.1 as f64 / dpi);
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        let inside = |a: [f64; 4]| lx >= a[0] && lx < a[0] + a[2] && ly >= a[1] && ly < a[1] + a[3];
        let step = if f > 1.0 { -60.0 } else if f < 1.0 { 60.0 } else { 0.0 };
        if inside(insp) {
            self.scroll[0] = (self.scroll[0] + step).max(0.0);
        } else if inside(bottom) {
            self.scroll[1] = (self.scroll[1] + step).max(0.0);
        } else if self.in_graph(p, (size.0 as f64, size.1 as f64), dpi) {
            let w = ((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s);
            let s = (self.view.s * f).clamp(0.15 * dpi, 2.5 * dpi);
            self.view = View { s, x: p.0 - w.0 * s, y: p.1 - w.1 * s };
            self.zoomed = Some(Instant::now());
        }
    }

    /// Whether a re-raster after zooming is still to come (the app keeps drawing until then).
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
            self.graph_layer = Some((sig, vs, Layer { id: next_id(), pm: done.pm, x: 0.0, y: 0.0, w, h }));
            self.graph_want = None;
            return;
        }
        for (key, want, layer) in [("details", &mut self.insp_want, &mut self.insp), ("tables", &mut self.bottom_want, &mut self.bottom), ("strip", &mut self.strip_want, &mut self.strip)] {
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

    pub fn settling(&self) -> bool {
        self.zoomed.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) || self.inbox.busy()
    }

    /// Fits the graph into its part of the window.
    pub fn fit(&mut self, size: (u32, u32), dpi: f64) {
        let [_, g, _, _] = self.areas(size.0 as f64 / dpi, size.1 as f64 / dpi);
        self.lay = self.layout();
        self.fit_to(g, dpi);
        self.zoomed = Some(Instant::now());
    }

    fn fit_to(&mut self, g: [f64; 4], dpi: f64) {
        let (w, h) = (self.lay.w.max(1.0), self.lay.h.max(1.0));
        let s = ((g[2] - 40.0) / w).min((g[3] - 90.0) / h).clamp(0.2, 1.1) * dpi;
        self.view = View { s, x: (g[0] + (g[2] - w * s / dpi) / 2.0) * dpi, y: (g[1] + 44.0) * dpi };
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
        let (ov, _) = self.overlay_svg([0.0, 0.0, 2.0 * w + gap, 80.0], 2.0 * w + gap);
        let _ = write!(o, r#"<g transform="translate(30,20)">{ov}</g>"#);
        for (i, (name, sel)) in [("NOTHING SELECTED", None), ("A NODE SELECTED", Some(Sel::Node(node))), ("A PORT SELECTED", Some(Sel::Port(port))), ("A LINK SELECTED", Some(Sel::Link(link)))].into_iter().enumerate() {
            self.sel = sel;
            // four graphs in one picture: their glows' ids apart
            let (x, y) = (30.0 + (i % 2) as f64 * (w + gap), top + (i / 2) as f64 * (h + title + gap));
            let _ = write!(o, r##"<text x="{x}" y="{}" font-family="Noto Sans" font-weight="bold" font-size="14" letter-spacing="1.5" fill="#e58f2c">{name}</text><g transform="translate({x},{})">{}</g>"##, y + 18.0, y + title, self.graph_svg(true).replace("glow", &format!("p{i}glow")));
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
            Some("issues") => Tab::Issues,
            Some("objects") => Tab::Objects,
            Some("metadata") => Tab::Metadata,
            Some("events") => Tab::Events,
            _ => Tab::Top,
        };
        self.itab = match itab {
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

    /// For the status bar.
    /// What a chip or button under window pixel `p` does, for the status bar.
    pub fn hover(&self, p: (f64, f64), dpi: f64) -> Option<String> {
        let (lx, ly) = (p.0 / dpi, p.1 / dpi);
        let (hit, _) = self.hits.iter().find(|(_, [x, y, w, h])| lx >= *x && lx <= x + w && ly >= *y && ly <= y + h)?;
        Some(match hit {
            Hit::Show(i) => format!("{}: {}", SHOW[*i].0, SHOW[*i].1),
            Hit::Bottom => if self.bottom_shut { "show the diagnostics: TOP, ISSUES, OBJECTS, METADATA, EVENTS" } else { "hide the diagnostics" }.into(),
            Hit::Legend => if self.legend_open { "close the legend (? or Esc)" } else { "how to read the graph: colours, lights, tags, links (?)" }.into(),
            Hit::Close => "close the details (choosing something opens them again)".into(),
            Hit::Report => "save what PipeWire has to a text file for a bug report, without names, addresses or what plays".into(),
            Hit::Info => if self.info_open { "close the card" } else { "the rest: what the strip has no room for" }.into(),
            Hit::Tab(_) => "click: show this table".into(),
            _ => return None,
        })
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
        format!("ENGINE · what PipeWire really has · {} nodes · {ports} ports · {} links · read-only", self.model.nodes.len(), self.model.links.len())
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

/// A port's name as its block shows it: cut to half the block's width when it has ports on both
/// sides.
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

/// What a tab of the diagnostics shows, for the status bar while the pointer is over it.
fn tab_hint(t: &Tab) -> &'static str {
    match t {
        Tab::Top => "TOP: as pw-top: each driver and the nodes it clocks; the last half second's cycles, averaged, with the peak",
        Tab::Issues => "ISSUES: what looks wrong or is worth knowing, worst first",
        Tab::Objects => "OBJECTS: every object PipeWire lists",
        Tab::Metadata => "METADATA: shared settings: defaults, clock, WirePlumber's",
        Tab::Events => "EVENTS: what happened while ENGINE watched",
    }
}

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

    /// A property as it goes into the report: None leaves it out.
    fn prop(&self, key: &str, value: &str, private: &[String]) -> Option<String> {
        const OUT: [&str; 7] = ["application.process.user", "application.process.host", "application.process.machine-id", "application.process.session-id", "application.process.id", "window.x11.display", "object.serial"];
        if OUT.contains(&key) || key.starts_with("pipewire.sec.") {
            return None;
        }
        // what plays (a song, a video's title, a page) is the user's business
        let media_kept = ["media.class", "media.type", "media.category", "media.role", "media.software"];
        if key.starts_with("media.") && !media_kept.contains(&key) {
            return Some("<removed>".into());
        }
        let mut v = self.text(value);
        for p in private {
            v = v.replace(p.as_str(), "<removed>");
        }
        Some(v)
    }
}

/// `C9:C1:60:6A:FD:E3` (or with `_`) as `XX:XX:XX:XX:FD:E3`.
fn mask_macs(s: &str) -> String {
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
    pub fn report(&self, pipewire: &str) -> String {
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
            let _ = writeln!(o, "[{}] {}: {}\n      {}", ["info", "warn", "ERROR"][i.level as usize], i.tag, r.text(&i.title), i.hint);
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
            let private: Vec<String> = p.iter().filter(|(k, v)| k.starts_with("media.") && !["media.class", "media.type", "media.category", "media.role", "media.software"].contains(&k.as_str()) && v.len() > 3).map(|(_, v)| v.clone()).collect();
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

    fn node(id: u32, ins: &[u32], outs: &[u32]) -> ENode {
        let port = |id: &u32| EPort { id: *id, name: format!("p{id}"), channel: String::new(), monitor: false, midi: false };
        ENode { id, name: format!("n{id}"), desc: format!("n{id}"), class: "Stream/Output/Audio".into(), kind: NKind::Stream, state: "running", ins: ins.iter().map(port).collect(), outs: outs.iter().map(port).collect(), color: C_APP, desk: None, system: false }
    }

    /// A stereo pair keeps the distance its ports are apart all along: round a node in its way,
    /// on the slopes too, FL above FR, not one over the other.
    #[test]
    fn a_stereo_pair_keeps_its_distance_all_along() {
        let mut e = Engine::new();
        let nodes = vec![node(1, &[], &[11, 12]), node(2, &[21, 22], &[]), node(3, &[31, 32], &[])];
        e.model.at = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        e.model.nodes = nodes;
        let mut lay = Layout::default();
        // the source at the top left, a node right in the way, the target below it on the right
        lay.pos.insert(1, (0.0, 0.0));
        lay.pos.insert(2, (COLW, 0.0));
        lay.pos.insert(3, (2.0 * COLW, 100.0));
        lay.bands.push((-50.0, 400.0, Some(0), ""));
        let points = |out: u32, inp: u32| {
            let (a, b) = (e.port_xy(&lay, 1, out).unwrap(), e.port_xy(&lay, 3, inp).unwrap());
            let path = e.link_path(&lay, a, b, e.lane(1, out));
            path.iter()
                .flat_map(|c| {
                    (0..=60).map(move |k| {
                        let t = k as f64 / 60.0;
                        let u = 1.0 - t;
                        (u * u * u * c[0].0 + 3.0 * u * u * t * c[1].0 + 3.0 * u * t * t * c[2].0 + t * t * t * c[3].0, u * u * u * c[0].1 + 3.0 * u * u * t * c[1].1 + 3.0 * u * t * t * c[2].1 + t * t * t * c[3].1)
                    })
                })
                .collect::<Vec<_>>()
        };
        let (fl, fr) = (points(11, 31), points(12, 32));
        // it goes round the node: below its bottom while across its column
        let bottom = NH0 + 2.0 * PR + 8.0;
        assert!(fl.iter().filter(|p| p.0 > COLW + 20.0 && p.0 < COLW + NW - 20.0).all(|p| p.1 > bottom), "FL goes round the node in its way");
        for p in &fl {
            let d = fr.iter().map(|q| (q.0 - p.0).hypot(q.1 - p.1)).fold(f64::MAX, f64::min);
            assert!((d - PR).abs() < 1.0, "at ({:.0}, {:.0}) FL is {d:.1} from FR, not {PR}", p.0, p.1);
        }
        assert!(fr[fr.len() / 2].1 > fl[fl.len() / 2].1, "FR runs below FL");
    }
}
