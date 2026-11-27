//! The PipeWire connection. PipeWire runs its own loop on a thread of its own; what it sees goes
//! to the app over a channel, and the app sends commands back (see docs/research/pipewire).
//!
//! Besides the registry (objects appearing and going away) it binds every node and device, for
//! their full properties and the nodes' state, and runs the signal meters: small passive capture
//! streams that Patchcord links to the ports it wants to measure.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use pw::types::ObjectType;

pub type Props = BTreeMap<String, String>;

/// The prefix of the meter streams' node names: Patchcord hides its own nodes.
pub const METER_PREFIX: &str = "patchcord-meter-";
/// The cue: what is listened to goes into its recording side (`patchcord-cue`) and comes out of
/// its playing side (`patchcord-cue-out`) in the default output. Both are hidden on the desk.
pub const CUE_NODE: &str = "patchcord-cue";
const CUE_OUT: &str = "patchcord-cue-out";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Device,
    Node,
    Port,
    Link,
    /// a program connected to PipeWire: which process its streams belong to
    Client,
    /// everything else PipeWire lists (the core, modules, factories, metadata, the profiler…),
    /// for the ENGINE screen; its interface is in its props as `object.type`
    Other,
}

/// Whether a card's profile can work now (an HDMI output with nothing plugged in can't).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Avail {
    Yes,
    No,
    Unknown,
}

/// One of a card's profiles: which inputs and outputs it has (PipeWire's device `Profile`).
#[derive(Clone, PartialEq, Debug)]
pub struct Profile {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub available: Avail,
    pub priority: i32,
}

/// One of a card's ports, as PipeWire calls them routes: where its sound goes (Speakers,
/// Headphones) or comes from (Internal Microphone). The ones in use (`device` set) carry the
/// card's real volume, mute and latency offset.
#[derive(Clone, PartialEq, Debug)]
pub struct Route {
    pub index: u32,
    pub output: bool,
    /// the card's device (its sink or source, `card.profile.device`) it is in use for
    pub device: Option<i32>,
    pub name: String,
    pub description: String,
    pub available: Avail,
    /// the devices and profiles it can be used with
    pub devices: Vec<i32>,
    pub profiles: Vec<i32>,
    pub volumes: Option<Vec<f32>>,
    pub mute: bool,
    pub latency_ns: i64,
}

/// An output port listened to: its node, the port, and its channel (0 left, 1 right, None both).
pub type CueSource = (u32, u32, Option<usize>);

/// One node's cycles over the last half second, as the profiler reports them (what pw-top
/// shows, but every cycle counted, not only the last): how long it waited to start after it was
/// signalled and how long it was busy, on average and at worst; its own quantum and rate; its
/// xruns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeTiming {
    pub id: u32,
    pub name: String,
    pub wait_ns: i64,
    pub busy_ns: i64,
    pub wait_max_ns: i64,
    pub busy_max_ns: i64,
    /// the latency it runs at: quantum / rate
    pub latency: (u32, u32),
    pub xruns: i32,
    /// cycles counted (while summing, wait and busy are totals)
    pub cycles: u32,
    /// when it was signalled and finished, in the last cycle (to find when the graph finished)
    signal_ns: i64,
    finish_ns: i64,
    /// the last cycle as the profiler gave it, for the inspector (ADR 0024): its blocks by kind
    /// (info, clock, driverBlock for a driver; followerBlock, followerClock for a follower), each
    /// field named as spa/param/profiler.h names it
    pub raw: Vec<(String, PVal)>,
}

/// A driver's cycles over the last half second: its clock (quantum and rate), the DSP load, when
/// the whole graph had finished (from the start of the cycle, on average and at worst: how close
/// it comes to the deadline, the period), and the nodes it clocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DriverTiming {
    pub node: NodeTiming,
    pub quantum: u64,
    pub rate: u32,
    /// CPU load of the cycle, 0…1 (the profiler's fast average)
    pub load: f32,
    pub end_ns: i64,
    pub end_max_ns: i64,
    pub followers: Vec<NodeTiming>,
}

impl NodeTiming {
    /// Adds a cycle (its values as totals and peaks).
    fn add(&mut self, n: &NodeTiming) {
        self.wait_ns += n.wait_ns;
        self.busy_ns += n.busy_ns;
        self.wait_max_ns = self.wait_max_ns.max(n.wait_ns);
        self.busy_max_ns = self.busy_max_ns.max(n.busy_ns);
        self.cycles += 1;
        (self.name, self.latency, self.xruns) = (n.name.clone(), n.latency, n.xruns);
        self.raw = n.raw.clone();
    }

    /// Totals into averages.
    fn averaged(&self) -> NodeTiming {
        let k = self.cycles.max(1) as i64;
        NodeTiming { wait_ns: self.wait_ns / k, busy_ns: self.busy_ns / k, ..self.clone() }
    }
}

impl DriverTiming {
    /// Adds a cycle of the same driver.
    fn add(&mut self, d: &DriverTiming) {
        self.node.add(&d.node);
        for f in &d.followers {
            match self.followers.iter_mut().find(|x| x.id == f.id) {
                Some(x) => x.add(f),
                None => self.followers.push(f.clone()),
            }
        }
        self.end_ns += d.end_ns;
        self.end_max_ns = self.end_max_ns.max(d.end_ns);
        (self.quantum, self.rate, self.load) = (d.quantum, d.rate, d.load);
    }

    fn averaged(&self) -> DriverTiming {
        DriverTiming {
            node: self.node.averaged(),
            end_ns: self.end_ns / self.node.cycles.max(1) as i64,
            followers: self.followers.iter().map(NodeTiming::averaged).collect(),
            ..self.clone()
        }
    }
}

/// A value as PipeWire gives it, for ENGINE's inspector (ADR 0024): a plain one written as pw-dump
/// writes it (a string quoted, a number, an id by its short name: F32P, audio), a list, or an
/// object's keys in their order.
#[derive(Clone, Debug, PartialEq)]
pub enum PVal {
    Text(String),
    List(Vec<PVal>),
    Obj(Vec<(String, PVal)>),
}

impl PVal {
    /// The same with every plain value passed through `f` (MAC addresses masked, for pictures).
    pub fn map(&self, f: &dyn Fn(&str) -> String) -> PVal {
        match self {
            PVal::Text(t) => PVal::Text(f(t)),
            PVal::List(l) => PVal::List(l.iter().map(|v| v.map(f)).collect()),
            PVal::Obj(o) => PVal::Obj(o.iter().map(|(k, v)| (k.clone(), v.map(f))).collect()),
        }
    }
}

/// From PipeWire to the app.
pub enum Event {
    Added { id: u32, kind: Kind, props: Props },
    Removed(u32),
    /// A node's or device's full properties, and a node's state ("running", "idle", …).
    Info { id: u32, props: Props, state: Option<&'static str> },
    /// Why a node or a link is in error, as PipeWire says it (sent with its "error" state).
    Failed { id: u32, why: String },
    /// A card's profiles (in PipeWire's order) and the one it is in.
    Profiles { id: u32, list: Vec<Profile>, current: Option<u32> },
    /// A card's ports (`all`, in PipeWire's order) and the ones in use, with their volumes.
    Routes { id: u32, all: Vec<Route>, active: Vec<Route> },
    /// An app stream's volume (playing) or gain (recording): each channel's, linear as PipeWire
    /// keeps them, and its mute.
    Mix { id: u32, volumes: Vec<f32>, mute: bool },
    /// Peak level of each meter's channels since the last report, 0…1.
    Levels(Vec<(u32, Vec<f32>)>),
    /// A value of PipeWire's "settings" metadata (clock.rate, clock.quantum…).
    Setting { key: String, value: String },
    /// The PipeWire version.
    Version(String),
    /// A value of one of the metadata (`settings`, `default`, `sm-settings`…), for ENGINE: the
    /// metadata's id, the subject, the key (None: all of that subject's are cleared), its type
    /// and its value (None: removed).
    Meta { id: u32, subject: u32, key: Option<String>, ty: Option<String>, value: Option<String> },
    /// Each driver's last cycle, twice a second while the profiler runs (ENGINE is open).
    Profile(Vec<DriverTiming>),
    /// One of a node's, port's or device's params, asked for by ENGINE's inspector: its kind
    /// (Format, Props…), its index and its value as PipeWire gives it.
    Param { id: u32, kind: String, index: u32, value: PVal },
    /// An object's info fields as PipeWire gives them (state, error, n-input-ports, direction,
    /// format, filename, args, version…: all but its properties), each time they change.
    Fields { id: u32, fields: Vec<(String, PVal)> },
    /// An object as the registry gives it: its type, version and permissions (pw-cli's r-x-).
    Object { id: u32, ty: String, version: u32, perms: String },
    /// Everything that existed when we connected has been reported.
    Synced,
    Error(String),
}

/// From the app to PipeWire.
pub enum Cmd {
    Link { out_node: u32, out_port: u32, in_node: u32, in_port: u32 },
    Unlink(u32),
    /// Measure these output ports, (node, port) in channel order, as meter `id`.
    Meter { id: u32, ports: Vec<(u32, u32)>, channels: Vec<String> },
    Unmeter(u32),
    /// Switch card `device` to its profile `index` (remembered by PipeWire for next time).
    SetProfile { device: u32, index: u32 },
    /// Set an app stream's channel volumes (linear) and mute.
    SetMix { node: u32, volumes: Vec<f32>, mute: bool },
    /// Switch card `device`'s sink or source `route_device` to its port `index`, or set that
    /// port's volumes, mute or latency offset (remembered by PipeWire).
    SetRoute { device: u32, index: u32, route_device: i32, volumes: Option<Vec<f32>>, mute: Option<bool>, latency_ns: Option<i64> },
    /// Make a node the default output or input (`default.configured.audio.sink` / `.source`).
    SetDefault { key: String, node_name: String },
    /// Listen: these output ports (node, port, channel: 0 left, 1 right, None both) in the
    /// default output, at `level` (linear). No sources: nothing is heard, and the cue goes.
    Cue { sources: Vec<CueSource>, level: f32 },
    /// Run the profiler (ENGINE is open) or stop it: it costs a little on every cycle.
    Profile(bool),
    /// Read a node's, port's or device's params (all it has) for ENGINE's inspector.
    Params(u32),
    Quit,
}

pub struct Connection {
    pub events: mpsc::Receiver<Event>,
    cmds: pw::channel::Sender<Cmd>,
    thread: Option<JoinHandle<()>>,
}

impl Connection {
    /// `wake` is called after every event, so the app's event loop can pick it up.
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (ev_tx, events) = mpsc::channel();
        let (cmds, cmd_rx) = pw::channel::channel();
        let thread = thread::Builder::new()
            .name("pipewire".into())
            .spawn(move || run(Sender { tx: ev_tx, wake }, cmd_rx))
            .expect("PipeWire thread");
        Connection { events, cmds, thread: Some(thread) }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[derive(Clone)]
struct Sender {
    tx: mpsc::Sender<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Sender {
    fn send(&self, ev: Event) {
        if self.tx.send(ev).is_ok() {
            (self.wake)();
        }
    }
}

fn props_of(d: &spa::utils::dict::DictRef) -> Props {
    d.iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

/// A change mask as pw-dump writes it: its flags' names (input-ports, state, params…).
fn mask_names(m: &dyn std::fmt::Debug) -> PVal {
    let d = format!("{m:?}");
    let inner = d.split_once('(').map_or("", |(_, r)| r.trim_end_matches(')'));
    PVal::List(inner.split(" | ").filter(|n| !n.is_empty() && !n.starts_with("0x")).map(|n| PVal::Text(format!("{:?}", n.to_lowercase().replace('_', "-")))).collect())
}

/// The params an object says it has, as pw-cli writes them: each id and whether it can be read
/// and written (EnumFormat r-, Props rw).
fn params_list(ps: &[spa::param::ParamInfo]) -> PVal {
    use spa::param::ParamInfoFlags as F;
    PVal::List(
        ps.iter()
            .map(|p| PVal::Text(format!("{} {}{}", param_name(p.id()), if p.flags().contains(F::READ) { 'r' } else { '-' }, if p.flags().contains(F::WRITE) { 'w' } else { '-' })))
            .collect(),
    )
}

/// The params of these that can be read.
fn readable(ps: &[spa::param::ParamInfo]) -> Vec<spa::param::ParamType> {
    ps.iter().filter(|p| p.flags().contains(spa::param::ParamInfoFlags::READ)).map(|p| p.id()).collect()
}

/// A param id's name (EnumFormat, Props, Route…).
fn param_name(id: spa::param::ParamType) -> String {
    format!("{id:?}").trim_start_matches("ParamType::").to_owned()
}

/// A plain value for the inspector: a number or a word as it is, a string quoted.
fn num(n: impl std::fmt::Display) -> PVal {
    PVal::Text(n.to_string())
}
fn quoted(s: &str) -> PVal {
    PVal::Text(format!("{s:?}"))
}

// keys of a profile object (spa/param/profile.h) and the availability values
const PROFILE_INDEX: u32 = 1;
const PROFILE_NAME: u32 = 2;
const PROFILE_DESCRIPTION: u32 = 3;
const PROFILE_PRIORITY: u32 = 4;
const PROFILE_AVAILABLE: u32 = 5;
const PROFILE_SAVE: u32 = 8;

/// A profile param, as PipeWire sends it.
fn profile_of(pod: &spa::pod::Pod) -> Option<Profile> {
    use spa::pod::Value;
    let (_, Value::Object(obj)) = spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()? else { return None };
    let mut p = Profile { index: u32::MAX, name: String::new(), description: String::new(), available: Avail::Unknown, priority: 0 };
    for prop in obj.properties {
        match (prop.key, prop.value) {
            (PROFILE_INDEX, Value::Int(v)) => p.index = v as u32,
            (PROFILE_NAME, Value::String(v)) => p.name = v,
            (PROFILE_DESCRIPTION, Value::String(v)) => p.description = v,
            (PROFILE_PRIORITY, Value::Int(v)) => p.priority = v,
            (PROFILE_AVAILABLE, Value::Id(id)) => p.available = [Avail::Unknown, Avail::No, Avail::Yes].get(id.0 as usize).copied().unwrap_or(Avail::Unknown),
            _ => {}
        }
    }
    (p.index != u32::MAX).then_some(p)
}

/// The profile param that switches a card to profile `index`, and has PipeWire remember it.
fn profile_param(index: u32) -> Option<Vec<u8>> {
    use spa::pod::{Property, PropertyFlags, Value};
    let prop = |key, value| Property { key, flags: PropertyFlags::empty(), value };
    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamProfile.as_raw(),
        id: spa::param::ParamType::Profile.as_raw(),
        properties: vec![prop(PROFILE_INDEX, Value::Int(index as i32)), prop(PROFILE_SAVE, Value::Bool(true))],
    };
    pod_bytes(obj)
}

// keys of a props object (spa/param/props.h)
const PROP_MUTE: u32 = 0x10004;
const PROP_CHANNEL_VOLUMES: u32 = 0x10008;
const PROP_LATENCY_OFFSET: u32 = 0x1000e;

// keys of a route object (spa/param/route.h)
const ROUTE_INDEX: u32 = 1;
const ROUTE_DIRECTION: u32 = 2;
const ROUTE_DEVICE: u32 = 3;
const ROUTE_NAME: u32 = 4;
const ROUTE_DESCRIPTION: u32 = 5;
const ROUTE_AVAILABLE: u32 = 7;
const ROUTE_PROFILES: u32 = 9;
const ROUTE_PROPS: u32 = 10;
const ROUTE_DEVICES: u32 = 11;
const ROUTE_SAVE: u32 = 13;

/// A route param, as PipeWire sends it.
fn route_of(pod: &spa::pod::Pod) -> Option<Route> {
    use spa::pod::{Value, ValueArray};
    let (_, Value::Object(obj)) = spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()? else { return None };
    let mut r = Route { index: u32::MAX, output: false, device: None, name: String::new(), description: String::new(), available: Avail::Unknown, devices: Vec::new(), profiles: Vec::new(), volumes: None, mute: false, latency_ns: 0 };
    for prop in obj.properties {
        match (prop.key, prop.value) {
            (ROUTE_INDEX, Value::Int(v)) => r.index = v as u32,
            // SPA_DIRECTION_OUTPUT is 1
            (ROUTE_DIRECTION, Value::Id(id)) => r.output = id.0 == 1,
            (ROUTE_DEVICE, Value::Int(v)) => r.device = Some(v),
            (ROUTE_NAME, Value::String(v)) => r.name = v,
            (ROUTE_DESCRIPTION, Value::String(v)) => r.description = v,
            (ROUTE_AVAILABLE, Value::Id(id)) => r.available = [Avail::Unknown, Avail::No, Avail::Yes].get(id.0 as usize).copied().unwrap_or(Avail::Unknown),
            (ROUTE_PROFILES, Value::ValueArray(ValueArray::Int(v))) => r.profiles = v,
            (ROUTE_DEVICES, Value::ValueArray(ValueArray::Int(v))) => r.devices = v,
            (ROUTE_PROPS, Value::Object(props)) => {
                for p in props.properties {
                    match (p.key, p.value) {
                        (PROP_CHANNEL_VOLUMES, Value::ValueArray(ValueArray::Float(v))) if !v.is_empty() => r.volumes = Some(v),
                        (PROP_MUTE, Value::Bool(m)) => r.mute = m,
                        (PROP_LATENCY_OFFSET, Value::Long(n)) => r.latency_ns = n,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    (r.index != u32::MAX).then_some(r)
}

/// The route param that switches a card's sink or source to a port, or sets its volume, mute or
/// latency offset, and has PipeWire remember it.
fn route_param(index: u32, device: i32, volumes: Option<Vec<f32>>, mute: Option<bool>, latency_ns: Option<i64>) -> Option<Vec<u8>> {
    use spa::pod::{Property, PropertyFlags, Value, ValueArray};
    let prop = |key, value| Property { key, flags: PropertyFlags::empty(), value };
    let mut props = Vec::new();
    if let Some(v) = volumes {
        props.push(prop(PROP_CHANNEL_VOLUMES, Value::ValueArray(ValueArray::Float(v))));
    }
    if let Some(m) = mute {
        props.push(prop(PROP_MUTE, Value::Bool(m)));
    }
    if let Some(n) = latency_ns {
        props.push(prop(PROP_LATENCY_OFFSET, Value::Long(n)));
    }
    let mut properties = vec![prop(ROUTE_INDEX, Value::Int(index as i32)), prop(ROUTE_DEVICE, Value::Int(device))];
    if !props.is_empty() {
        properties.push(prop(ROUTE_PROPS, Value::Object(spa::pod::Object { type_: spa::utils::SpaTypes::ObjectParamProps.as_raw(), id: spa::param::ParamType::Route.as_raw(), properties: props })));
    }
    properties.push(prop(ROUTE_SAVE, Value::Bool(true)));
    let obj = spa::pod::Object { type_: spa::utils::SpaTypes::ObjectParamRoute.as_raw(), id: spa::param::ParamType::Route.as_raw(), properties };
    pod_bytes(obj)
}

/// A node's volumes and mute, from its props param (None when it has no channel volumes).
fn mix_of(pod: &spa::pod::Pod) -> Option<(Vec<f32>, bool)> {
    use spa::pod::{Value, ValueArray};
    let (_, Value::Object(obj)) = spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()? else { return None };
    let (mut vols, mut mute) = (None, false);
    for prop in obj.properties {
        match (prop.key, prop.value) {
            (PROP_CHANNEL_VOLUMES, Value::ValueArray(ValueArray::Float(v))) if !v.is_empty() => vols = Some(v),
            (PROP_MUTE, Value::Bool(m)) => mute = m,
            _ => {}
        }
    }
    Some((vols?, mute))
}

/// The props param that sets a node's channel volumes and mute.
fn mix_param(volumes: Vec<f32>, mute: bool) -> Option<Vec<u8>> {
    use spa::pod::{Property, PropertyFlags, Value, ValueArray};
    let prop = |key, value| Property { key, flags: PropertyFlags::empty(), value };
    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamProps.as_raw(),
        id: spa::param::ParamType::Props.as_raw(),
        properties: vec![prop(PROP_CHANNEL_VOLUMES, Value::ValueArray(ValueArray::Float(volumes))), prop(PROP_MUTE, Value::Bool(mute))],
    };
    pod_bytes(obj)
}

/// The cue: two streams of Patchcord's, made while something is listened to. The recording
/// one (`CUE_NODE`) takes in what is heard on active links: what is listened to wakes, and it
/// runs on the clock of what it hears. The playing one (`CUE_OUT`) plays
/// into the default output on that output's clock. Between them a little buffer: the playing
/// side takes from it what it has and plays silence when it has nothing, so it never waits on
/// the other (PipeWire's loopback did: its playing side, made to wait for its recording side,
/// held the output up for a whole cycle when what it heard had no clock running), and the two
/// clocks never join. Both sides at 48 kHz, PipeWire resampling each to its own clock. Both run
/// on PipeWire's real-time thread (on Patchcord's main loop the playing side missed cycles: xruns),
/// the buffer shared through a lock neither holds for more than a cycle's copy.
#[derive(Default)]
struct Cue {
    /// the links into its recording side (dropped first)
    links: HashMap<(u32, u32), pw::link::Link>,
    /// one of its links was taken away: when to make it again
    relink: Option<Instant>,
    streams: Option<CueStreams>,
    /// its recording side's node and input ports (by channel: 0 left, 1 right)
    node: Option<u32>,
    inputs: BTreeMap<u32, u32>,
    /// what is wanted heard
    want: Vec<CueSource>,
    /// how loud, as the playing side multiplies it in (linear, as f32 bits)
    level: Arc<std::sync::atomic::AtomicU32>,
    /// what the two sides share (emptied when what is heard changes: no sound of the last)
    buf: Option<Arc<std::sync::Mutex<CueBuf>>>,
    /// a side went wrong (its format not agreed, its node gone): it is made again
    failed: Rc<std::cell::Cell<bool>>,
    /// when it was last made again after going wrong (at most every two seconds)
    retried: Option<Instant>,
}

/// The cue's two streams, alive together.
struct CueStreams {
    _capture: CueSide,
    _playback: CueSide,
}

/// One of the cue's streams and its listener: the listener goes first (see `Bound`), also when
/// the other side fails to start and this one is dropped on its own.
struct CueSide {
    listener: Option<pw::stream::StreamListener<()>>,
    stream: pw::stream::StreamRc,
}

impl Drop for CueSide {
    /// Off the real-time thread first (its process callback can't run while it goes), then the
    /// listener, then the stream.
    fn drop(&mut self) {
        let _ = self.stream.disconnect();
        self.listener.take();
    }
}

/// What the cue's recording side heard, for its playing side: stereo frames, and whether it is
/// playing them out (it waits for a cushion of two of its cycles first, and again after it ran
/// dry, so that the two sides' jitter doesn't break the sound up).
#[derive(Default)]
struct CueBuf {
    frames: std::collections::VecDeque<[f32; 2]>,
    playing: bool,
    /// how much the recording side brings at once (its own clock's quantum, which may be
    /// bigger than the playing side's): the cushion and the catching up allow for it
    chunk: usize,
}

/// At most this much heard and not yet played (half a second at 48 kHz): beyond it the oldest
/// goes (the two clocks drift apart, or the playing side stalled).
const CUE_MAX: usize = 24_000;

impl Cue {
    /// Makes its streams if it has none (or a side went wrong: both again), and links what is
    /// wanted into them.
    fn run(&mut self, core: &pw::core::CoreRc, events: &Sender) {
        if self.failed.get() {
            self.retried = Some(Instant::now());
            self.links.clear();
            self.streams = None;
            (self.node, self.buf) = (None, None);
            self.inputs.clear();
        }
        if self.streams.is_none()
            && let Err(e) = self.start(core)
        {
            events.send(Event::Error(format!("cannot start the cue: {e}")));
        }
        self.reconcile(core);
    }

    /// Makes its two streams.
    fn start(&mut self, core: &pw::core::CoreRc) -> Result<(), pw::Error> {
        // room for all it may hold, made here: the real-time thread never allocates
        let buf = Arc::new(std::sync::Mutex::new(CueBuf { frames: std::collections::VecDeque::with_capacity(CUE_MAX + 8192), playing: false, chunk: 0 }));
        self.buf = Some(buf.clone());
        self.failed.set(false);
        let capture = {
            let props = pw::properties::properties! {
                "media.type" => "Audio",
                "media.category" => "Capture",
                "node.name" => CUE_NODE,
                "node.description" => "Patchcord cue in",
                // never moved by the session; not passive: a passive node makes every link into it
                // passive, and what is listened to must wake (ADR 0009)
                "stream.monitor" => "true",
                "node.dont-reconnect" => "true",
            };
            let stream = pw::stream::StreamRc::new(core.clone(), CUE_NODE, props)?;
            let listener = stream
                .add_local_listener_with_user_data(())
                .state_changed(failed(&self.failed))
                .process({
                    let buf = buf.clone();
                    move |stream, _| {
                        let Some(mut buffer) = stream.dequeue_buffer() else { return };
                        let Some(d) = buffer.datas_mut().first_mut() else { return };
                        let Some(bytes) = chunk_bytes(d) else { return };
                        let Ok(mut b) = buf.lock() else { return };
                        // the oldest goes first, so it never holds more than CUE_MAX
                        b.chunk = bytes.len() / 8;
                        let over = (b.frames.len() + bytes.len() / 8).saturating_sub(CUE_MAX).min(b.frames.len());
                        b.frames.drain(..over);
                        for f in bytes.chunks_exact(8).take(CUE_MAX) {
                            b.frames.push_back([f32::from_le_bytes([f[0], f[1], f[2], f[3]]), f32::from_le_bytes([f[4], f[5], f[6], f[7]])]);
                        }
                    }
                })
                .register()?;
            let pod = stereo_format()?;
            let mut params = [spa::pod::Pod::from_bytes(&pod).ok_or(pw::Error::CreationFailed)?];
            let side = CueSide { listener: Some(listener), stream };
            side.stream.connect(spa::utils::Direction::Input, None, pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS, &mut params)?;
            side
        };
        let playback = {
            let props = pw::properties::properties! {
                "media.type" => "Audio",
                "media.category" => "Playback",
                "node.name" => CUE_OUT,
                "node.description" => "Patchcord cue out",
            };
            let stream = pw::stream::StreamRc::new(core.clone(), CUE_OUT, props)?;
            let level = self.level.clone();
            let listener = stream
                .add_local_listener_with_user_data(())
                .state_changed(failed(&self.failed))
                .process({
                    // the level it played at last: a change goes over a whole cycle, not at once
                    // (a step at a cycle's edge clicks, and a turned knob crackles)
                    let mut was: Option<f32> = None;
                    move |stream, _| {
                    let Some(mut buffer) = stream.dequeue_buffer() else { return };
                    let asked = buffer.requested() as usize;
                    let Some(d) = buffer.datas_mut().first_mut() else { return };
                    let n = {
                        let Some(bytes) = d.data() else {
                            *d.chunk_mut().size_mut() = 0;
                            return;
                        };
                        let room = bytes.len() / 8;
                        let n = if asked > 0 { asked.min(room) } else { room.min(1024) };
                        let Ok(mut b) = buf.lock() else { return };
                        // what to keep in hand: two of its cycles, or one and what the recording side
                        // brings at once if that is more (it may run at a bigger quantum)
                        let cushion = (2 * n).max(n + b.chunk);
                        // far behind (the clocks drifted, or a stall): catch up to the cushion
                        if b.frames.len() > 3 * cushion.max(256) {
                            let extra = b.frames.len() - cushion;
                            b.frames.drain(..extra);
                        }
                        if !b.playing && b.frames.len() >= cushion {
                            b.playing = true;
                        } else if b.playing && b.frames.len() < n {
                            b.playing = false;
                        }
                        let to = f32::from_bits(level.load(std::sync::atomic::Ordering::Relaxed));
                        let from = was.unwrap_or(to);
                        was = Some(to);
                        for i in 0..n {
                            let g = from + (to - from) * (i + 1) as f32 / n as f32;
                            let f = if b.playing { b.frames.pop_front().unwrap_or([0.0; 2]) } else { [0.0; 2] };
                            bytes[i * 8..i * 8 + 4].copy_from_slice(&(f[0] * g).to_le_bytes());
                            bytes[i * 8 + 4..i * 8 + 8].copy_from_slice(&(f[1] * g).to_le_bytes());
                        }
                        n
                    };
                    let c = d.chunk_mut();
                    *c.offset_mut() = 0;
                    *c.stride_mut() = 8;
                    *c.size_mut() = (n * 8) as u32;
                    }
                })
                .register()?;
            let pod = stereo_format()?;
            let mut params = [spa::pod::Pod::from_bytes(&pod).ok_or(pw::Error::CreationFailed)?];
            let side = CueSide { listener: Some(listener), stream };
            side.stream.connect(spa::utils::Direction::Output, None, pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS, &mut params)?;
            side
        };
        self.streams = Some(CueStreams { _capture: capture, _playback: playback });
        Ok(())
    }

    /// Links what is wanted into its recording side, once its ports exist, and unlinks the rest.
    fn reconcile(&mut self, core: &pw::core::CoreRc) {
        let want: Vec<(u32, u32, u32, u32)> = match self.node {
            Some(me) if self.inputs.len() >= 2 => {
                let ins: Vec<u32> = self.inputs.values().copied().collect();
                let mut v = Vec::new();
                for &(node, port, ch) in &self.want {
                    for c in ch.map_or(vec![0, 1], |c| vec![c.min(1)]) {
                        v.push((node, port, me, ins[c]));
                    }
                }
                v
            }
            _ => Vec::new(),
        };
        self.links.retain(|(o, i), _| want.iter().any(|w| w.1 == *o && w.3 == *i));
        for (on, op, inn, ip) in want {
            if self.links.contains_key(&(op, ip)) {
                continue;
            }
            // active: what is listened to wakes, as a cord into it would (a mic asleep), and
            // sleeps again when it is let go (ADR 0009)
            if let Ok(l) = make_link(core, (on, op), (inn, ip), Linked::Active) {
                self.links.insert((op, ip), l);
            }
        }
    }
}

/// Binds the profiler `id` (its protocol comes with its module, loaded into our context once, as
/// pw-top does it).
fn start_profiler(context: &pw::context::ContextRc, registry: &pw::registry::RegistryRc, id: u32, loaded: &std::cell::Cell<bool>, events: &Sender) -> Option<Profiler> {
    if !loaded.get() {
        let name = std::ffi::CString::new("libpipewire-module-profiler").expect("name");
        // SAFETY: the context is alive and the name outlives the call
        let m = unsafe { pw::sys::pw_context_load_module(context.as_raw_ptr(), name.as_ptr(), std::ptr::null(), std::ptr::null_mut()) };
        loaded.set(!m.is_null());
    }
    let p = Profiler::bind(registry, id);
    if p.is_none() {
        events.send(Event::Error("cannot start the profiler".into()));
    }
    p
}

/// A POD object as bytes, to send.
fn pod_bytes(obj: spa::pod::Object) -> Option<Vec<u8>> {
    Some(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj)).ok()?.0.into_inner())
}

/// How a link of Patchcord's is made.
enum Linked {
    /// it only listens (a meter): never keeps anything awake
    Passive,
    /// it wakes what it comes from while it is there (the cue), and goes with Patchcord
    Active,
    /// a cord: it stays after Patchcord quits
    Lingering,
}

/// Links an output port to an input port: (node, port) each.
fn make_link(core: &pw::core::CoreRc, (on, op): (u32, u32), (inn, ip): (u32, u32), how: Linked) -> Result<pw::link::Link, pw::Error> {
    let mut props = pw::properties::properties! {
        "link.output.node" => on.to_string(),
        "link.output.port" => op.to_string(),
        "link.input.node" => inn.to_string(),
        "link.input.port" => ip.to_string(),
    };
    match how {
        Linked::Passive => props.insert("link.passive", "true"),
        Linked::Lingering => props.insert("object.linger", "true"),
        Linked::Active => {}
    }
    core.create_object::<pw::link::Link>("link-factory", &props)
}

/// What a buffer's data holds: its chunk (offset and size), within what is mapped.
fn chunk_bytes(d: &mut spa::buffer::Data) -> Option<&[u8]> {
    let (off, size) = (d.chunk().offset() as usize, d.chunk().size() as usize);
    let bytes = d.data()?;
    let end = off.saturating_add(size).min(bytes.len());
    Some(&bytes[off.min(end)..end])
}

/// A cue stream's state listener: an error marks the cue to be made again.
fn failed(flag: &Rc<std::cell::Cell<bool>>) -> impl Fn(&pw::stream::Stream, &mut (), pw::stream::StreamState, pw::stream::StreamState) + 'static {
    let flag = flag.clone();
    move |_, _, _, new| {
        if matches!(new, pw::stream::StreamState::Error(_)) {
            flag.set(true);
        }
    }
}

/// The format of the cue's streams: 32-bit float stereo (left, right) at 48 kHz.
fn stereo_format() -> Result<Vec<u8>, pw::Error> {
    audio_format(&["FL".into(), "FR".into()], Some(48_000))
}

/// 32-bit float audio of these channels (by name: MONO, FL, FR…; AUX for the rest), at a rate
/// or at any.
fn audio_format(channels: &[String], rate: Option<u32>) -> Result<Vec<u8>, pw::Error> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_channels(channels.len().max(1) as u32);
    if let Some(r) = rate {
        info.set_rate(r);
    }
    let mut pos = [0u32; 64];
    for (i, c) in channels.iter().enumerate().take(64) {
        // SPA channel positions by name (MONO 2, FL 3, FR 4…); AUX0 4096… for the rest
        pos[i] = channel_id(c).unwrap_or(4096 + i as u32);
    }
    info.set_position(pos);
    let obj = spa::pod::Object { type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(), id: spa::param::ParamType::EnumFormat.as_raw(), properties: info.into() };
    pod_bytes(obj).ok_or(pw::Error::CreationFailed)
}

/// A node or device bound to keep its info coming: its listener and its proxy, dropped in that
/// order (a struct drops its fields as they are declared). Destroyed, a proxy (or a stream)
/// frees the list its listeners' hooks are linked into, and a hook removed after that writes
/// into freed memory: it corrupted the heap, and something else crashed later, anywhere.
struct Bound {
    _listener: Box<dyn Any>,
    proxy: Box<dyn Any>,
}
/// A meter's stream, its listener and the peaks it measures.
type MeterStream = (pw::stream::StreamRc, pw::stream::StreamListener<()>, Rc<RefCell<Vec<f32>>>);

/// A signal meter: a capture stream, its ports once they exist, and the passive links to them.
struct Meter {
    /// the links to the ports, by (the port measured, its own port), and which ports (global ids)
    /// they were made to; dropped before the stream, whose going takes its links with it on the
    /// server (one made again replaces the one taken away)
    links: HashMap<(u32, u32), pw::link::Link>,
    linked: Vec<u32>,
    /// (the listener before the stream: see `Bound`)
    _listener: pw::stream::StreamListener<()>,
    _stream: pw::stream::StreamRc,
    targets: Vec<(u32, u32)>,
    node: Option<u32>,
    /// our input ports now, by channel (`port.id`): their global ids. A stream's ports can go
    /// and come back with new ids while its format is set up, so this keeps the latest of each.
    ports: BTreeMap<u32, u32>,
    peaks: Rc<RefCell<Vec<f32>>>,
    /// one of its links was taken away (in ENGINE, or by another program): when to see which
    /// are missing, a moment later, and make those again while what it measures is there
    relink: Option<Instant>,
}

impl Meter {
    /// Links its ports, channel by channel, to what it measures, once it has a port for each
    /// (again if they changed). A meter never keeps a device awake: its links are passive.
    fn link(&mut self, core: &pw::core::CoreRc) {
        let Some(me) = self.node else { return };
        let now: Vec<u32> = self.ports.values().copied().collect();
        if now.len() != self.targets.len() || now == self.linked {
            return;
        }
        self.links.clear();
        self.linked = now.clone();
        for (&(tn, tp), &mine) in self.targets.iter().zip(&now) {
            if let Ok(l) = make_link(core, (tn, tp), (me, mine), Linked::Passive) {
                self.links.insert((tp, mine), l);
            }
        }
    }

    /// Makes again the links it should have and hasn't (`have`: every link's output and input
    /// port), leaving the ones there as they are.
    fn mend(&mut self, core: &pw::core::CoreRc, have: &HashMap<u32, (u32, u32)>) {
        let Some(me) = self.node else { return };
        let mine: Vec<u32> = self.ports.values().copied().collect();
        if mine.len() != self.targets.len() {
            return;
        }
        for (&(tn, tp), &p) in self.targets.iter().zip(&mine) {
            if !have.values().any(|&(o, i)| o == tp && i == p)
                && let Ok(l) = make_link(core, (tn, tp), (me, p), Linked::Passive)
            {
                self.links.insert((tp, p), l);
            }
        }
    }
}

fn run(events: Sender, cmds: pw::channel::Receiver<Cmd>) {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).expect("PipeWire main loop");
    let context = pw::context::ContextRc::new(&mainloop, None).expect("PipeWire context");
    let core = match context.connect_rc(None) {
        Ok(core) => core,
        Err(e) => return events.send(Event::Error(format!("cannot connect to PipeWire: {e}"))),
    };
    let registry = core.get_registry_rc().expect("PipeWire registry");

    // bound nodes and devices (proxy and listener), so their info keeps coming
    let bound: Rc<RefCell<HashMap<u32, Bound>>> = Default::default();
    // devices whose profiles changed and app streams whose volume changed, to ask again
    let stale: Rc<RefCell<Vec<u32>>> = Default::default();
    let meters: Rc<RefCell<HashMap<u32, Meter>>> = Default::default();
    // the "default" metadata, to set the default output and input
    let default_md: Rc<std::cell::Cell<Option<u32>>> = Default::default();
    let cue: Rc<RefCell<Cue>> = Default::default();
    // the profiler (bound while ENGINE is open) and the nodes whose params ENGINE asked for
    let profiler_id: Rc<std::cell::Cell<Option<u32>>> = Default::default();
    let profiler: Rc<RefCell<Option<Profiler>>> = Default::default();
    // ENGINE wants the profiler (it may ask before PipeWire has said where it is: it is started
    // when it comes), and whether its module is loaded into our context yet
    let want_profiler: Rc<std::cell::Cell<bool>> = Default::default();
    let profiler_module: Rc<std::cell::Cell<bool>> = Default::default();
    let inspect: Rc<RefCell<std::collections::HashSet<u32>>> = Default::default();
    // the params each node, port and device says it has and can be read (from its info): what
    // the inspector asks for, one kind at a time (asking for all at once, PipeWire refuses)
    let param_ids: Rc<RefCell<HashMap<u32, Vec<spa::param::ParamType>>>> = Default::default();
    // the core's properties, from its info, sent once the registry has added it
    let core_props: Rc<RefCell<Option<Props>>> = Default::default();
    // every link's output and input port, and every port there is: a meter's link gone is made
    // again while what it measures is there
    let link_in: Rc<RefCell<HashMap<u32, (u32, u32)>>> = Default::default();
    let ports_now: Rc<RefCell<std::collections::HashSet<u32>>> = Default::default();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let events = events.clone();
            let registry = registry.clone();
            let bound = bound.clone();
            let core_props = core_props.clone();
            let param_ids = param_ids.clone();
            let meters = meters.clone();
            let stale = stale.clone();
            let core = core.clone();
            let default_md = default_md.clone();
            let cue = cue.clone();
            let profiler_id = profiler_id.clone();
            let inspect = inspect.clone();
            let link_in = link_in.clone();
            let ports_now = ports_now.clone();
            let context = context.clone();
            let profiler = profiler.clone();
            let want_profiler = want_profiler.clone();
            let profiler_module = profiler_module.clone();
            move |g| {
                // every metadata, for ENGINE; the "settings" one (the clock rate and quantum) and
                // the "default" one (the default output and input) for the panel too
                let md_name = g.props.and_then(|p| p.get("metadata.name"));
                if g.type_ == ObjectType::Metadata
                    && let Ok(md) = registry.bind::<pw::metadata::Metadata, _>(g)
                {
                    if md_name == Some("default") {
                        default_md.set(Some(g.id));
                    }
                    let setting = matches!(md_name, Some("settings" | "default"));
                    let ev = events.clone();
                    let mid = g.id;
                    let l = md
                        .add_listener_local()
                        .property(move |subject, key, ty, value| {
                            if setting && let Some(key) = key {
                                ev.send(Event::Setting { key: key.to_owned(), value: value.unwrap_or("").to_owned() });
                            }
                            ev.send(Event::Meta { id: mid, subject, key: key.map(str::to_owned), ty: ty.map(str::to_owned), value: value.map(str::to_owned) });
                            0
                        })
                        .register();
                    bound.borrow_mut().insert(g.id, Bound { _listener: Box::new(l), proxy: Box::new(md) });
                }
                if g.type_ == ObjectType::Profiler {
                    profiler_id.set(Some(g.id));
                    if want_profiler.get() && profiler.borrow().is_none() {
                        *profiler.borrow_mut() = start_profiler(&context, &registry, g.id, &profiler_module, &events);
                    }
                }
                let kind = match g.type_ {
                    ObjectType::Device => Kind::Device,
                    ObjectType::Node => Kind::Node,
                    ObjectType::Port => Kind::Port,
                    ObjectType::Link => Kind::Link,
                    ObjectType::Client => Kind::Client,
                    _ => Kind::Other,
                };
                let mut props = g.props.map(props_of).unwrap_or_default();
                if kind == Kind::Other {
                    props.insert("object.type".into(), g.type_.to_str().to_owned());
                }
                let id = g.id;
                match kind {
                    Kind::Node => {
                        if props.get("node.name").is_some_and(|n| n == CUE_NODE) {
                            cue.borrow_mut().node = Some(id);
                        }
                        if let Some(n) = props.get("node.name").and_then(|n| n.strip_prefix(METER_PREFIX)).and_then(|n| n.parse::<u32>().ok())
                            && let Some(m) = meters.borrow_mut().get_mut(&n)
                        {
                            m.node = Some(id);
                        }
                        if let Ok(node) = registry.bind::<pw::node::Node, _>(g) {
                            let ev = events.clone();
                            // an app that plays or records, or a device's output or input: its volume and
                            // mute, now and whenever they change (a card's real ones are on its routes)
                            let app = props.get("media.class").is_some_and(|c| matches!(c.as_str(), "Stream/Output/Audio" | "Stream/Input/Audio" | "Audio/Sink" | "Audio/Source"));
                            let mev = events.clone();
                            let stale = stale.clone();
                            let inspect = inspect.clone();
                            let param_ids = param_ids.clone();
                            let l = node
                                .add_listener_local()
                                .param(move |_, kind, index, _, pod| {
                                    if kind == spa::param::ParamType::Props
                                        && let Some((volumes, mute)) = pod.and_then(mix_of)
                                    {
                                        mev.send(Event::Mix { id, volumes, mute });
                                    }
                                    // the inspector asked for this node's params
                                    if let Some(pod) = pod
                                        && inspect.borrow().contains(&id)
                                    {
                                        mev.send(Event::Param { id, kind: param_name(kind), index, value: pod_value(pod) });
                                    }
                                })
                                .info(move |info| {
                                    if app && info.change_mask().contains(pw::node::NodeChangeMask::PARAMS) && info.params().iter().any(|p| p.id() == spa::param::ParamType::Props) {
                                        let mut s = stale.borrow_mut();
                                        if !s.contains(&id) {
                                            s.push(id);
                                        }
                                    }
                                    let state = match info.state() {
                                        pw::node::NodeState::Running => "running",
                                        pw::node::NodeState::Idle => "idle",
                                        pw::node::NodeState::Suspended => "suspended",
                                        pw::node::NodeState::Creating => "creating",
                                        pw::node::NodeState::Error(_) => "error",
                                    };
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: Some(state) });
                                    let error = match info.state() {
                                        pw::node::NodeState::Error(why) => {
                                            ev.send(Event::Failed { id, why: why.to_owned() });
                                            quoted(why)
                                        }
                                        _ => num("null"),
                                    };
                                    param_ids.borrow_mut().insert(id, readable(info.params()));
                                    ev.send(Event::Fields {
                                        id,
                                        fields: vec![
                                            ("max-input-ports".into(), num(info.max_input_ports())),
                                            ("max-output-ports".into(), num(info.max_output_ports())),
                                            ("change-mask".into(), mask_names(&info.change_mask())),
                                            ("n-input-ports".into(), num(info.n_input_ports())),
                                            ("n-output-ports".into(), num(info.n_output_ports())),
                                            ("state".into(), quoted(state)),
                                            ("error".into(), error),
                                            ("params".into(), params_list(info.params())),
                                        ],
                                    });
                                })
                                .register();
                            if app {
                                node.subscribe_params(&[spa::param::ParamType::Props]);
                            }
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(node) });
                        }
                    }
                    Kind::Device => {
                        if let Ok(dev) = registry.bind::<pw::device::Device, _>(g) {
                            let ev = events.clone();
                            // its profiles, as they come (one param each) and whenever they change
                            let profiles: RefCell<(BTreeMap<u32, Profile>, Option<u32>)> = Default::default();
                            // its ports, and the ones in use by sink or source
                            let routes: RefCell<(BTreeMap<u32, Route>, BTreeMap<i32, Route>)> = Default::default();
                            let pev = events.clone();
                            let stale = stale.clone();
                            let dinspect = inspect.clone();
                            let dparam_ids = param_ids.clone();
                            let l = dev
                                .add_listener_local()
                                .info(move |info| {
                                    // its profile changed (or what is plugged in): ask again, on the
                                    // next tick (a listener can't reach its own device)
                                    let profile = |p: &spa::param::ParamInfo| matches!(p.id(), spa::param::ParamType::Profile | spa::param::ParamType::EnumProfile | spa::param::ParamType::Route | spa::param::ParamType::EnumRoute);
                                    if info.change_mask().contains(pw::device::DeviceChangeMask::PARAMS) && info.params().iter().any(profile) {
                                        let mut s = stale.borrow_mut();
                                        if !s.contains(&id) {
                                            s.push(id);
                                        }
                                    }
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: None });
                                    dparam_ids.borrow_mut().insert(id, readable(info.params()));
                                    ev.send(Event::Fields { id, fields: vec![("change-mask".into(), mask_names(&info.change_mask())), ("params".into(), params_list(info.params()))] });
                                })
                                .param(move |_, kind, index, _, pod| {
                                    // the inspector asked for this device's params
                                    if let Some(pod) = pod
                                        && dinspect.borrow().contains(&id)
                                    {
                                        pev.send(Event::Param { id, kind: param_name(kind), index, value: pod_value(pod) });
                                    }
                                    if matches!(kind, spa::param::ParamType::EnumRoute | spa::param::ParamType::Route) {
                                        let Some(r) = pod.and_then(route_of) else { return };
                                        let mut s = routes.borrow_mut();
                                        if kind == spa::param::ParamType::EnumRoute {
                                            if index == 0 {
                                                s.0.clear();
                                            }
                                            s.0.insert(r.index, r);
                                        } else {
                                            if index == 0 {
                                                s.1.clear();
                                            }
                                            s.1.insert(r.device.unwrap_or(-1), r);
                                        }
                                        pev.send(Event::Routes { id, all: s.0.values().cloned().collect(), active: s.1.values().cloned().collect() });
                                        return;
                                    }
                                    let Some(p) = pod.and_then(profile_of) else { return };
                                    let mut s = profiles.borrow_mut();
                                    match kind {
                                        spa::param::ParamType::EnumProfile => {
                                            // a new round of the list: what is gone goes
                                            if index == 0 {
                                                s.0.clear();
                                            }
                                            s.0.insert(p.index, p);
                                        }
                                        spa::param::ParamType::Profile => s.1 = Some(p.index),
                                        _ => return,
                                    }
                                    pev.send(Event::Profiles { id, list: s.0.values().cloned().collect(), current: s.1 });
                                })
                                .register();
                            dev.subscribe_params(&[spa::param::ParamType::EnumProfile, spa::param::ParamType::Profile, spa::param::ParamType::EnumRoute, spa::param::ParamType::Route]);
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(dev) });
                        }
                    }
                    Kind::Port => {
                        // one of our meters' input ports: once each channel has one, link them,
                        // channel by channel, to what they measure (again if they changed)
                        let node: Option<u32> = props.get("node.id").and_then(|n| n.parse().ok());
                        let index: u32 = props.get("port.id").and_then(|n| n.parse().ok()).unwrap_or(0);
                        let input = props.get("port.direction").is_some_and(|d| d == "in");
                        // the cue's ports: once it has them, it is linked
                        {
                            let mut c = cue.borrow_mut();
                            if c.node.is_some() && c.node == node && input {
                                c.inputs.insert(index, id);
                                c.reconcile(&core);
                            }
                        }
                        ports_now.borrow_mut().insert(id);
                        for m in meters.borrow_mut().values_mut() {
                            if m.node.is_some() && m.node == node && input {
                                m.ports.insert(index, id);
                                m.link(&core);
                            }
                        }
                        // its full properties, its direction, and its params when the inspector
                        // asks (ADR 0024)
                        if let Ok(port) = registry.bind::<pw::port::Port, _>(g) {
                            let ev = events.clone();
                            let pev = events.clone();
                            let inspect = inspect.clone();
                            let pparam_ids = param_ids.clone();
                            let l = port
                                .add_listener_local()
                                .info(move |info| {
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: None });
                                    let dir = if info.direction() == spa::utils::Direction::Input { "input" } else { "output" };
                                    pparam_ids.borrow_mut().insert(id, readable(info.params()));
                                    ev.send(Event::Fields { id, fields: vec![("direction".into(), quoted(dir)), ("change-mask".into(), mask_names(&info.change_mask())), ("params".into(), params_list(info.params()))] });
                                })
                                .param(move |_, kind, index, _, pod| {
                                    if let Some(pod) = pod
                                        && inspect.borrow().contains(&id)
                                    {
                                        pev.send(Event::Param { id, kind: param_name(kind), index, value: pod_value(pod) });
                                    }
                                })
                                .register();
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(port) });
                        }
                    }
                    Kind::Link => {
                        let port = |k: &str| props.get(k).and_then(|p| p.parse::<u32>().ok());
                        if let (Some(o), Some(i)) = (port("link.output.port"), port("link.input.port")) {
                            link_in.borrow_mut().insert(id, (o, i));
                        }
                        // its state (active, paused…) and full properties (passive or not), for ENGINE
                        if let Ok(link) = registry.bind::<pw::link::Link, _>(g) {
                            let ev = events.clone();
                            let l = link
                                .add_listener_local()
                                .info(move |info| {
                                    let state = match info.state() {
                                        pw::link::LinkState::Active => "active",
                                        pw::link::LinkState::Paused => "paused",
                                        pw::link::LinkState::Allocating => "allocating",
                                        pw::link::LinkState::Negotiating => "negotiating",
                                        pw::link::LinkState::Init => "init",
                                        pw::link::LinkState::Unlinked => "unlinked",
                                        pw::link::LinkState::Error(_) => "error",
                                    };
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: Some(state) });
                                    let error = match info.state() {
                                        pw::link::LinkState::Error(why) => {
                                            ev.send(Event::Failed { id, why: why.to_owned() });
                                            quoted(why)
                                        }
                                        _ => num("null"),
                                    };
                                    ev.send(Event::Fields {
                                        id,
                                        fields: vec![
                                            ("output-node-id".into(), num(info.output_node_id())),
                                            ("output-port-id".into(), num(info.output_port_id())),
                                            ("input-node-id".into(), num(info.input_node_id())),
                                            ("input-port-id".into(), num(info.input_port_id())),
                                            ("change-mask".into(), mask_names(&info.change_mask())),
                                            ("state".into(), quoted(state)),
                                            ("error".into(), error),
                                            ("format".into(), info.format().map_or(num("null"), pod_value)),
                                        ],
                                    });
                                })
                                .register();
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(link) });
                        }
                    }
                    Kind::Client => {
                        // its full properties: the registry gives only its name and socket, not
                        // its process (binary, pid) nor whether it is WirePlumber, which tell
                        // programs apart (graph::Graph::program)
                        if let Ok(client) = registry.bind::<pw::client::Client, _>(g) {
                            let ev = events.clone();
                            let l = client
                                .add_listener_local()
                                .info(move |info| {
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: None });
                                    ev.send(Event::Fields { id, fields: vec![("change-mask".into(), mask_names(&info.change_mask()))] });
                                })
                                .register();
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(client) });
                        }
                    }
                    Kind::Other => {
                        // a module's and a factory's info (its file and arguments; what it makes):
                        // the registry gives little of them (ADR 0024)
                        if g.type_ == ObjectType::Module
                            && let Ok(module) = registry.bind::<pw::module::Module, _>(g)
                        {
                            let ev = events.clone();
                            let l = module
                                .add_listener_local()
                                .info(move |info| {
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: None });
                                    ev.send(Event::Fields {
                                        id,
                                        fields: vec![("name".into(), quoted(info.name())), ("filename".into(), quoted(info.filename())), ("args".into(), info.args().map_or(num("null"), quoted)), ("change-mask".into(), mask_names(&info.change_mask()))],
                                    });
                                })
                                .register();
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(module) });
                        }
                        if g.type_ == ObjectType::Factory
                            && let Ok(factory) = registry.bind::<pw::factory::Factory, _>(g)
                        {
                            let ev = events.clone();
                            let l = factory
                                .add_listener_local()
                                .info(move |info| {
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: None });
                                    // SAFETY: the info's own name, a C string while the info lives
                                    let name = unsafe { std::ffi::CStr::from_ptr(info.as_raw().name).to_string_lossy().into_owned() };
                                    ev.send(Event::Fields {
                                        id,
                                        fields: vec![("name".into(), quoted(&name)), ("type".into(), quoted(info.type_().to_str())), ("version".into(), num(info.version())), ("change-mask".into(), mask_names(&info.change_mask()))],
                                    });
                                })
                                .register();
                            bound.borrow_mut().insert(id, Bound { _listener: Box::new(l), proxy: Box::new(factory) });
                        }
                    }
                }
                let p = g.permissions;
                use pw::permissions::PermissionFlags as P;
                let perms: String = [(P::R, 'r'), (P::W, 'w'), (P::X, 'x'), (P::M, 'm')].iter().map(|(f, c)| if p.contains(*f) { *c } else { '-' }).collect();
                events.send(Event::Object { id, ty: g.type_.to_str().to_owned(), version: g.version, perms });
                events.send(Event::Added { id, kind, props });
                // the core's full properties came with its info, before it was added
                if g.type_ == ObjectType::Core
                    && let Some(p) = core_props.borrow_mut().take()
                {
                    events.send(Event::Info { id, props: p, state: None });
                }
            }
        })
        .global_remove({
            let events = events.clone();
            let bound = bound.clone();
            let param_ids = param_ids.clone();
            let meters = meters.clone();
            let cue = cue.clone();
            let inspect = inspect.clone();
            let link_in = link_in.clone();
            let ports_now = ports_now.clone();
            let profiler_id = profiler_id.clone();
            move |id| {
                bound.borrow_mut().remove(&id);
                param_ids.borrow_mut().remove(&id);
                if profiler_id.get() == Some(id) {
                    profiler_id.set(None);
                }
                ports_now.borrow_mut().remove(&id);
                // a link into a meter gone: the meter is linked again (on the next tick, once
                // what went with it has gone too)
                if let Some((o, p)) = link_in.borrow_mut().remove(&id) {
                    for m in meters.borrow_mut().values_mut().filter(|m| m.ports.values().any(|&q| q == p)) {
                        m.relink = Some(Instant::now() + Duration::from_millis(300));
                    }
                    // one into the cue taken away (in ENGINE, by another program): made again, a
                    // moment later, while it is wanted
                    let mut c = cue.borrow_mut();
                    if c.links.remove(&(o, p)).is_some() {
                        c.relink = Some(Instant::now() + Duration::from_millis(300));
                    }
                }
                // (ids come back: a new node at this one isn't inspected)
                inspect.borrow_mut().remove(&id);
                {
                    let mut c = cue.borrow_mut();
                    c.inputs.retain(|_, p| *p != id);
                    if c.node == Some(id) {
                        c.node = None;
                    }
                }
                // a meter's port gone: it is linked again when a new one comes
                for m in meters.borrow_mut().values_mut() {
                    m.ports.retain(|_, p| *p != id);
                }
                events.send(Event::Removed(id));
            }
        })
        .register();

    // the answer to the first sync arrives after every object that already exists; the answer
    // to a second one, after the full properties of the nodes and devices bound meanwhile
    let pending = Rc::new(RefCell::new((core.sync(0).expect("PipeWire sync"), 0u8)));
    let _core_listener = core
        .add_listener_local()
        .done({
            let events = events.clone();
            let core = core.clone();
            move |id, seq| {
                let mut p = pending.borrow_mut();
                if id != pw::core::PW_ID_CORE || seq != p.0 || p.1 >= 2 {
                    return;
                }
                p.1 += 1;
                if p.1 == 1 {
                    if let Ok(next) = core.sync(0) {
                        p.0 = next;
                    }
                } else {
                    events.send(Event::Synced);
                }
            }
        })
        .error({
            let events = events.clone();
            move |id, _seq, res, message| {
                // the meters' links are made again while PipeWire sets up their ports: a link
                // made a moment too early fails to agree a format, one already gone is destroyed
                // twice, and a meter of a stream not playing can't set its format until it plays.
                // Harmless (they are made again, and set up once there is sound); shown only when
                // debugging
                let noise = (res == -2 && message.starts_with("unknown resource")) || (res == -22 && (message.contains("format") || message.contains("negotiation")));
                if !noise || std::env::var_os("PATCHCORD_DEBUG").is_some() {
                    events.send(Event::Error(format!("object {id}: {message} ({res})")));
                }
            }
        })
        .info({
            let events = events.clone();
            let core_props = core_props.clone();
            move |info| {
                events.send(Event::Version(info.version().to_owned()));
                *core_props.borrow_mut() = info.props().map(props_of);
                events.send(Event::Fields {
                    id: info.id(),
                    fields: vec![
                        ("cookie".into(), num(info.cookie())),
                        ("user-name".into(), quoted(info.user_name())),
                        ("host-name".into(), quoted(info.host_name())),
                        ("version".into(), quoted(info.version())),
                        ("name".into(), quoted(info.name())),
                        ("change-mask".into(), mask_names(&info.change_mask())),
                    ],
                });
            }
        })
        .register();

    // meter levels go out 25 times a second, while there is something to show; devices whose
    // profiles changed are asked for them again
    let quiet = Rc::new(RefCell::new(true));
    let ticks = std::cell::Cell::new(0u32);
    let timer = mainloop.loop_().add_timer({
        let meters = meters.clone();
        let cue = cue.clone();
        let core = core.clone();
        let events = events.clone();
        let bound = bound.clone();
        let profiler = profiler.clone();
        move |_| {
            // a meter whose link was taken away: what is missing linked again, while what it
            // measures is there (gone, the rack measures something else, or nothing)
            let now = Instant::now();
            for m in meters.borrow_mut().values_mut().filter(|m| m.relink.is_some_and(|t| t <= now)) {
                m.relink = None;
                let alive = ports_now.borrow();
                if m.targets.iter().all(|(_, tp)| alive.contains(tp)) && m.ports.values().all(|p| alive.contains(p)) {
                    m.mend(&core, &link_in.borrow());
                }
            }
            // a side of the cue went wrong while something is heard: it is made again
            {
                let mut c = cue.borrow_mut();
                if c.failed.get() && !c.want.is_empty() && c.retried.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
                    c.run(&core, &events);
                }
                if c.relink.is_some_and(|t| t <= now) {
                    c.relink = None;
                    c.reconcile(&core);
                }
            }
            // each driver's cycles of the last half second, averaged, with their peaks (drivers
            // that stopped reporting are left out); then counting starts again
            ticks.set(ticks.get().wrapping_add(1));
            if ticks.get().is_multiple_of(12)
                && let Some(p) = profiler.borrow().as_ref()
            {
                let now = Instant::now();
                let mut l = p.latest.borrow_mut();
                l.retain(|_, (t, _, _)| now.duration_since(*t) < Duration::from_millis(1500));
                events.send(Event::Profile(l.values().map(|(_, d, _)| d.averaged()).collect()));
                for (_, _, open) in l.values_mut() {
                    *open = false;
                }
            }
            for id in std::mem::take(&mut *stale.borrow_mut()) {
                let b = bound.borrow();
                let Some(Bound { proxy, .. }) = b.get(&id) else { continue };
                if let Some(dev) = proxy.downcast_ref::<pw::device::Device>() {
                    for kind in [spa::param::ParamType::EnumProfile, spa::param::ParamType::Profile, spa::param::ParamType::EnumRoute, spa::param::ParamType::Route] {
                        dev.enum_params(0, Some(kind), 0, u32::MAX);
                    }
                } else if let Some(node) = proxy.downcast_ref::<pw::node::Node>() {
                    node.enum_params(0, Some(spa::param::ParamType::Props), 0, u32::MAX);
                }
            }
            let mut out = Vec::new();
            let mut loud = false;
            for (id, m) in meters.borrow().iter() {
                let mut p = m.peaks.borrow_mut();
                loud |= p.iter().any(|v| *v > 0.0005);
                out.push((*id, p.clone()));
                p.iter_mut().for_each(|v| *v = 0.0);
            }
            // one report of silence after the sound stops, then nothing until it comes back
            if loud || !*quiet.borrow() {
                events.send(Event::Levels(out));
            }
            *quiet.borrow_mut() = !loud;
        }
    });
    timer.update_timer(Some(Duration::from_millis(40)), Some(Duration::from_millis(40)));

    // links we made, while the server makes them: with object.linger they stay once it has (and
    // after we quit), so a proxy is let go a while later
    let made: RefCell<Vec<(Instant, pw::link::Link)>> = Default::default();
    let _commands = cmds.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |cmd| match cmd {
            Cmd::Link { out_node, out_port, in_node, in_port } => {
                match make_link(&core, (out_node, out_port), (in_node, in_port), Linked::Lingering) {
                    Ok(link) => {
                        let mut made = made.borrow_mut();
                        made.retain(|(t, _)| t.elapsed() < Duration::from_secs(10));
                        made.push((Instant::now(), link));
                    }
                    Err(e) => events.send(Event::Error(format!("cannot create link: {e}"))),
                }
            }
            Cmd::Unlink(id) => {
                registry.destroy_global(id);
            }
            Cmd::Meter { id, ports, channels } => match meter(&core, id, &channels) {
                Ok((stream, listener, peaks)) => {
                    meters.borrow_mut().insert(id, Meter { _stream: stream, _listener: listener, targets: ports, node: None, ports: BTreeMap::new(), links: HashMap::new(), linked: Vec::new(), peaks, relink: None });
                }
                Err(e) => events.send(Event::Error(format!("cannot start meter: {e}"))),
            },
            Cmd::Unmeter(id) => {
                meters.borrow_mut().remove(&id);
            }
            Cmd::SetProfile { device, index } => {
                let b = bound.borrow();
                let dev = b.get(&device).and_then(|b| b.proxy.downcast_ref::<pw::device::Device>());
                match (dev, profile_param(index)) {
                    (Some(dev), Some(bytes)) => {
                        if let Some(pod) = spa::pod::Pod::from_bytes(&bytes) {
                            dev.set_param(spa::param::ParamType::Profile, 0, pod);
                        }
                    }
                    _ => events.send(Event::Error(format!("cannot switch the profile of device {device}"))),
                }
            }
            Cmd::SetRoute { device, index, route_device, volumes, mute, latency_ns } => {
                let b = bound.borrow();
                let dev = b.get(&device).and_then(|b| b.proxy.downcast_ref::<pw::device::Device>());
                match (dev, route_param(index, route_device, volumes, mute, latency_ns)) {
                    (Some(dev), Some(bytes)) => {
                        if let Some(pod) = spa::pod::Pod::from_bytes(&bytes) {
                            dev.set_param(spa::param::ParamType::Route, 0, pod);
                        }
                    }
                    _ => events.send(Event::Error(format!("cannot set the port of device {device}"))),
                }
            }
            Cmd::SetDefault { key, node_name } => {
                let b = bound.borrow();
                match default_md.get().and_then(|id| b.get(&id)).and_then(|b| b.proxy.downcast_ref::<pw::metadata::Metadata>()) {
                    Some(md) => {
                        let name = node_name.replace('\\', "\\\\").replace('"', "\\\"");
                        md.set_property(0, &key, Some("Spa:String:JSON"), Some(&format!("{{\"name\":\"{name}\"}}")));
                    }
                    None => events.send(Event::Error("cannot set the default device".into())),
                }
            }
            Cmd::SetMix { node, volumes, mute } => {
                let b = bound.borrow();
                let n = b.get(&node).and_then(|b| b.proxy.downcast_ref::<pw::node::Node>());
                match (n, mix_param(volumes, mute)) {
                    (Some(n), Some(bytes)) => {
                        if let Some(pod) = spa::pod::Pod::from_bytes(&bytes) {
                            n.set_param(spa::param::ParamType::Props, 0, pod);
                        }
                    }
                    _ => events.send(Event::Error(format!("cannot set the volume of node {node}"))),
                }
            }
            Cmd::Cue { sources, level } => {
                let mut c = cue.borrow_mut();
                if sources.is_empty() {
                    // nothing to hear: its links and its streams go
                    *c = Cue::default();
                    return;
                }
                c.level.store(level.to_bits(), std::sync::atomic::Ordering::Relaxed);
                if c.want != sources
                    && let Some(Ok(mut b)) = c.buf.as_ref().map(|b| b.lock())
                {
                    b.frames.clear();
                    b.playing = false;
                }
                c.want = sources;
                c.run(&core, &events);
            }
            Cmd::Profile(on) => {
                want_profiler.set(on);
                let mut p = profiler.borrow_mut();
                if !on {
                    *p = None;
                } else if p.is_none()
                    && let Some(id) = profiler_id.get()
                {
                    *p = start_profiler(&context, &registry, id, &profiler_module, &events);
                }
            }
            Cmd::Params(id) => {
                // every param it has (no id: all of them)
                let b = bound.borrow();
                let Some(p) = b.get(&id).map(|b| &b.proxy) else { return };
                inspect.borrow_mut().insert(id);
                for kind in param_ids.borrow().get(&id).into_iter().flatten().copied() {
                    if let Some(n) = p.downcast_ref::<pw::node::Node>() {
                        n.enum_params(0, Some(kind), 0, u32::MAX);
                    } else if let Some(n) = p.downcast_ref::<pw::port::Port>() {
                        n.enum_params(0, Some(kind), 0, u32::MAX);
                    } else if let Some(n) = p.downcast_ref::<pw::device::Device>() {
                        n.enum_params(0, Some(kind), 0, u32::MAX);
                    }
                }
            }
            Cmd::Quit => mainloop.quit(),
        }
    });

    mainloop.run();
}

/// PipeWire's profiler, bound while ENGINE is open: each driver's last cycle, as pw-top shows it.
/// pipewire-rs has no binding for it, so it is bound and listened to through the C interface.
struct Profiler {
    proxy: *mut pw::sys::pw_proxy,
    _hook: Box<spa::sys::spa_hook>,
    _events: Box<pw::sys::pw_profiler_events>,
    /// each driver's latest cycle and when it came, by driver
    latest: Box<ProfileLatest>,
}

/// Each driver's cycles since the last report: when the last came, their sum, and whether the
/// sum is still being added to (false: the next cycle starts a new one).
type ProfileLatest = RefCell<BTreeMap<u32, (Instant, DriverTiming, bool)>>;

impl Profiler {
    fn bind(registry: &pw::registry::RegistryRc, id: u32) -> Option<Self> {
        // SAFETY: the registry is alive on this thread; the hook, the events and the data the
        // listener is given are boxed, so they stay put, and live as long as the proxy (Drop
        // destroys the proxy first)
        unsafe {
            let ty: *const std::os::raw::c_char = pw::sys::PW_TYPE_INTERFACE_Profiler.as_ptr().cast();
            let raw: *mut pw::sys::pw_registry = registry.as_raw_ptr();
            let proxy: *mut pw::sys::pw_proxy = spa::spa_interface_call_method!(raw, pw::sys::pw_registry_methods, bind, id, ty, pw::sys::PW_VERSION_PROFILER, 0).cast();
            if proxy.is_null() {
                return None;
            }
            let mut hook: Box<spa::sys::spa_hook> = Box::new(std::mem::zeroed());
            let events = Box::new(pw::sys::pw_profiler_events { version: pw::sys::PW_VERSION_PROFILER_EVENTS, profile: Some(on_profile) });
            let latest: Box<ProfileLatest> = Box::default();
            let hook_ptr: *mut spa::sys::spa_hook = &mut *hook;
            let data: *const ProfileLatest = &*latest;
            spa::spa_interface_call_method!(proxy, pw::sys::pw_profiler_methods, add_listener, hook_ptr.cast(), &*events, data.cast_mut().cast());
            Some(Profiler { proxy, _hook: hook, _events: events, latest })
        }
    }
}

impl Drop for Profiler {
    fn drop(&mut self) {
        // SAFETY: bound by `bind` on this thread, destroyed once; its listener goes with it
        unsafe { pw::sys::pw_proxy_destroy(self.proxy) };
    }
}

unsafe extern "C" fn on_profile(data: *mut std::os::raw::c_void, pod: *const spa::sys::spa_pod) {
    if data.is_null() || pod.is_null() {
        return;
    }
    // SAFETY: `data` is the profiler's boxed `latest`, alive while the proxy is; the pod is valid
    // for the call
    let (latest, pod) = unsafe { (&*(data as *const ProfileLatest), spa::pod::Pod::from_raw(pod)) };
    let now = Instant::now();
    if let Ok(mut l) = latest.try_borrow_mut() {
        for d in timings_of(pod) {
            match l.get_mut(&d.node.id) {
                Some((t, sum, open)) if *open => {
                    sum.add(&d);
                    *t = now;
                }
                _ => {
                    let mut sum = DriverTiming { node: NodeTiming { id: d.node.id, ..Default::default() }, end_ns: 0, end_max_ns: 0, followers: Vec::new(), ..d.clone() };
                    sum.add(&d);
                    l.insert(d.node.id, (now, sum, true));
                }
            }
        }
    }
}

/// A profile: a struct of objects, one per driver: its info (load, xruns), its clock (quantum,
/// rate), its own block and its followers' (see spa/param/profiler.h).
fn timings_of(pod: &spa::pod::Pod) -> Vec<DriverTiming> {
    use spa::pod::Value;
    let Ok((_, v)) = spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()) else { return Vec::new() };
    let objects = match v {
        Value::Struct(items) => items,
        o @ Value::Object(_) => vec![o],
        _ => return Vec::new(),
    };
    let int = |v: Option<&Value>| match v {
        Some(Value::Int(i)) => *i as i64,
        Some(Value::Long(l)) => *l,
        _ => 0,
    };
    let block = |f: &[Value]| {
        let (signal, awake, finish) = (int(f.get(3)), int(f.get(4)), int(f.get(5)));
        let latency = match f.get(7) {
            Some(Value::Fraction(r)) => (r.num, r.denom),
            _ => (0, 0),
        };
        NodeTiming {
            id: int(f.first()) as u32,
            name: match f.get(1) {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            },
            wait_ns: if signal > 0 && awake >= signal { awake - signal } else { 0 },
            busy_ns: if awake > 0 && finish >= awake { finish - awake } else { 0 },
            latency,
            xruns: int(f.get(8)) as i32,
            signal_ns: signal,
            finish_ns: finish,
            ..Default::default()
        }
    };
    // each block's fields by the names spa/param/profiler.h gives them; one it doesn't name, by its
    // place
    const INFO: &[&str] = &["counter", "cpu_load fast", "cpu_load medium", "cpu_load slow", "xrun-count"];
    const CLOCK: &[&str] = &["clock flags", "clock id", "clock name", "clock nsec", "clock rate", "clock position", "clock duration", "clock delay", "clock rate_diff", "clock next_nsec", "transport_state", "clock cycle", "xrun duration"];
    const DRIVER: &[&str] = &["driver_id", "name", "driver prev_signal", "driver signal", "driver awake", "driver finish", "driver status", "latency", "xrun_count"];
    const FOLLOWER: &[&str] = &["id", "name", "prev_signal", "signal", "awake", "finish", "status", "latency", "xrun_count"];
    const FOLLOWER_CLOCK: &[&str] = &["clock id", "clock name", "clock nsec", "clock rate", "clock position", "clock duration", "clock delay", "clock rate_diff", "clock next_nsec", "transport_state", "clock cycle", "xrun duration"];
    let named = |f: &[Value], names: &[&str]| PVal::Obj(f.iter().enumerate().map(|(i, v)| (names.get(i).map_or_else(|| format!("[{i}]"), |n| (*n).to_owned()), value_of(v, std::ptr::null()))).collect());
    let mut out = Vec::new();
    for o in objects {
        let Value::Object(o) = o else { continue };
        let mut d = DriverTiming::default();
        let mut driver = false;
        let mut raw = Vec::new();
        for p in o.properties {
            let Value::Struct(f) = p.value else { continue };
            match p.key {
                spa::sys::SPA_PROFILER_info => raw.push(("info".to_owned(), named(&f, INFO))),
                spa::sys::SPA_PROFILER_clock => raw.push(("clock".to_owned(), named(&f, CLOCK))),
                spa::sys::SPA_PROFILER_driverBlock => raw.push(("driverBlock".to_owned(), named(&f, DRIVER))),
                _ => {}
            }
            match p.key {
                spa::sys::SPA_PROFILER_info => {
                    d.load = match f.get(1) {
                        Some(Value::Float(x)) => *x,
                        Some(Value::Double(x)) => *x as f32,
                        _ => 0.0,
                    };
                    // (its xruns are counted per node: ENGINE adds them up)
                }
                spa::sys::SPA_PROFILER_clock => {
                    if let Some(Value::Fraction(r)) = f.get(4) {
                        d.rate = r.denom;
                    }
                    d.quantum = int(f.get(6)).max(0) as u64;
                }
                spa::sys::SPA_PROFILER_driverBlock => {
                    d.node = block(&f);
                    driver = true;
                }
                spa::sys::SPA_PROFILER_followerBlock => {
                    let mut b = block(&f);
                    b.raw.push(("followerBlock".to_owned(), named(&f, FOLLOWER)));
                    d.followers.push(b);
                }
                spa::sys::SPA_PROFILER_followerClock => {
                    if let Some(b) = d.followers.last_mut() {
                        b.raw.push(("followerClock".to_owned(), named(&f, FOLLOWER_CLOCK)));
                    }
                }
                _ => {}
            }
        }
        if driver {
            d.node.raw = raw;
            // when the last of its nodes finished, from the start of the cycle
            let start = d.node.signal_ns;
            d.end_ns = std::iter::once(&d.node).chain(&d.followers).filter(|n| start > 0 && n.finish_ns >= start).map(|n| n.finish_ns - start).max().unwrap_or(0);
            out.push(d);
        }
    }
    out
}

/// A param (or a link's format) as PipeWire gives it, for ENGINE's inspector (ADR 0024).
pub fn pod_value(pod: &spa::pod::Pod) -> PVal {
    match spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()) {
        Ok((_, v)) => value_of(&v, std::ptr::null()),
        Err(_) => PVal::Text("(cannot read)".into()),
    }
}

/// The short name of `id` in a SPA type table, if it has one.
fn type_name(table: *const spa::sys::spa_type_info, id: u32) -> Option<String> {
    if table.is_null() {
        return None;
    }
    // SAFETY: the tables are libspa's static ones, ending in a zeroed entry
    unsafe {
        let n = spa::sys::spa_debug_type_find_short_name(table, id);
        (!n.is_null()).then(|| std::ffi::CStr::from_ptr(n).to_string_lossy().into_owned())
    }
}

/// A channel position's id from its short name (MONO, FL, RR…), from libspa's table.
fn channel_id(name: &str) -> Option<u32> {
    // SAFETY: libspa's static table, ending in an entry without a name
    unsafe {
        let mut t = spa::sys::spa_type_audio_channel;
        while !t.is_null() && !(*t).name.is_null() {
            let n = spa::sys::spa_debug_type_short_name((*t).name);
            if !n.is_null() && std::ffi::CStr::from_ptr(n).to_str().is_ok_and(|n| n == name) {
                return Some((*t).type_);
            }
            t = t.add(1);
        }
    }
    None
}

/// A float as pw-dump writes it: always with its decimal point (1.0, not 1).
fn float(f: f64) -> String {
    format!("{f:?}")
}

/// A value as PipeWire gives it (ADR 0024): an object's keys by name (rate, channels,
/// channelVolumes…) in their order, ids by name (audio, raw, F32P), a choice as pw-dump writes it
/// ({ default, min, max }, { default, alt1, alt2… }), a fraction and a rectangle as pw-cli writes
/// them (1/48000, 640x480). `enums`: the table an Id value is named from (the property's own).
fn value_of(v: &spa::pod::Value, enums: *const spa::sys::spa_type_info) -> PVal {
    use spa::pod::{Value, ValueArray as A};
    let t = |s: String| PVal::Text(s);
    let id = |i: u32| type_name(enums, i).unwrap_or_else(|| i.to_string());
    match v {
        Value::Object(o) => {
            // SAFETY: libspa's static type tables
            let props_table = unsafe {
                let t = spa::sys::spa_debug_type_find(spa::sys::spa_types, o.type_);
                if t.is_null() { std::ptr::null() } else { (*t).values }
            };
            let mut keys = Vec::new();
            for p in &o.properties {
                // SAFETY: as above
                let (name, values) = unsafe {
                    let t = if props_table.is_null() { std::ptr::null() } else { spa::sys::spa_debug_type_find(props_table, p.key) };
                    if t.is_null() {
                        (p.key.to_string(), std::ptr::null())
                    } else {
                        let n = spa::sys::spa_debug_type_short_name((*t).name);
                        (if n.is_null() { p.key.to_string() } else { std::ffi::CStr::from_ptr(n).to_string_lossy().into_owned() }, (*t).values)
                    }
                };
                // channel positions are arrays of ids: named from the channel table
                // SAFETY: libspa's static table
                let values = if name == "position" || name == "channelMap" { unsafe { spa::sys::spa_type_audio_channel } } else { values };
                keys.push((name, value_of(&p.value, values)));
            }
            PVal::Obj(keys)
        }
        Value::Struct(items) => PVal::List(items.iter().map(|i| value_of(i, std::ptr::null())).collect()),
        Value::None => t("null".into()),
        Value::Id(i) => t(id(i.0)),
        Value::String(s) => t(format!("{s:?}")),
        Value::Bool(b) => t(b.to_string()),
        Value::Int(i) => t(i.to_string()),
        Value::Long(l) => t(l.to_string()),
        Value::Float(f) => t(float(*f as f64)),
        Value::Double(f) => t(float(*f)),
        Value::Fraction(f) => t(format!("{}/{}", f.num, f.denom)),
        Value::Rectangle(r) => t(format!("{}x{}", r.width, r.height)),
        Value::ValueArray(a) => PVal::List(match a {
            A::None(v) => v.iter().map(|_| t("null".into())).collect(),
            A::Bool(v) => v.iter().map(|x| t(x.to_string())).collect(),
            A::Id(v) => v.iter().map(|x| t(id(x.0))).collect(),
            A::Int(v) => v.iter().map(|x| t(x.to_string())).collect(),
            A::Long(v) => v.iter().map(|x| t(x.to_string())).collect(),
            A::Float(v) => v.iter().map(|x| t(float(*x as f64))).collect(),
            A::Double(v) => v.iter().map(|x| t(float(*x))).collect(),
            A::Rectangle(v) => v.iter().map(|r| t(format!("{}x{}", r.width, r.height))).collect(),
            A::Fraction(v) => v.iter().map(|f| t(format!("{}/{}", f.num, f.denom))).collect(),
            A::Fd(v) => v.iter().map(|f| t(format!("{f:?}"))).collect(),
        }),
        Value::Choice(c) => choice_of(c, enums),
        other => t(format!("{other:?}")),
    }
}

/// A choice as pw-dump writes it: the value alone when there is no choice; else its default and
/// its range ({ default, min, max[, step] }) or its alternatives ({ default, alt1, alt2… }).
fn choice_of(c: &spa::pod::ChoiceValue, enums: *const spa::sys::spa_type_info) -> PVal {
    use spa::pod::ChoiceValue as C;
    use spa::utils::ChoiceEnum as E;
    fn of<T: spa::pod::CanonicalFixedSizedPod>(e: &E<T>, f: &dyn Fn(&T) -> String) -> PVal {
        let o = |kv: Vec<(String, &T)>| PVal::Obj(kv.into_iter().map(|(k, v)| (k, PVal::Text(f(v)))).collect());
        match e {
            E::None(v) => PVal::Text(f(v)),
            E::Range { default, min, max } => o(vec![("default".into(), default), ("min".into(), min), ("max".into(), max)]),
            E::Step { default, min, max, step } => o(vec![("default".into(), default), ("min".into(), min), ("max".into(), max), ("step".into(), step)]),
            E::Enum { default, alternatives } => o(std::iter::once(("default".to_owned(), default)).chain(alternatives.iter().enumerate().map(|(i, a)| (format!("alt{}", i + 1), a))).collect()),
            E::Flags { default, flags } => o(std::iter::once(("default".to_owned(), default)).chain(flags.iter().enumerate().map(|(i, a)| (format!("alt{}", i + 1), a))).collect()),
        }
    }
    match c {
        C::Bool(ch) => of(&ch.1, &|v| v.to_string()),
        C::Int(ch) => of(&ch.1, &|v| v.to_string()),
        C::Long(ch) => of(&ch.1, &|v| v.to_string()),
        C::Float(ch) => of(&ch.1, &|v| float(*v as f64)),
        C::Double(ch) => of(&ch.1, &|v| float(*v)),
        C::Id(ch) => of(&ch.1, &|v| type_name(enums, v.0).unwrap_or_else(|| v.0.to_string())),
        C::Rectangle(ch) => of(&ch.1, &|r| format!("{}x{}", r.width, r.height)),
        C::Fraction(ch) => of(&ch.1, &|f| format!("{}/{}", f.num, f.denom)),
        other => PVal::Text(format!("{other:?}")),
    }
}

/// A passive capture stream with one channel per measured port, not connected to anything:
/// Patchcord links it itself once its ports appear.
fn meter(core: &pw::core::CoreRc, id: u32, channels: &[String]) -> Result<MeterStream, pw::Error> {
    let props = pw::properties::properties! {
        "media.type" => "Audio",
        "media.category" => "Capture",
        "media.role" => "Production",
        "node.name" => format!("{METER_PREFIX}{id}"),
        "node.description" => "Patchcord signal meter",
        // tells PipeWire (and desktop volume panels) it only watches, like their peak meters
        "stream.monitor" => "true",
        "node.passive" => "true",
        "node.dont-reconnect" => "true",
    };
    let stream = pw::stream::StreamRc::new(core.clone(), &format!("{METER_PREFIX}{id}"), props)?;
    let n = channels.len().max(1);
    let peaks = Rc::new(RefCell::new(vec![0.0f32; n]));
    let listener = stream
        .add_local_listener_with_user_data(())
        .process({
            let peaks = peaks.clone();
            move |stream, _| {
                let Some(mut buffer) = stream.dequeue_buffer() else { return };
                let datas = buffer.datas_mut();
                let Some(d) = datas.first_mut() else { return };
                let Some(bytes) = chunk_bytes(d) else { return };
                let mut pk = peaks.borrow_mut();
                for (i, s) in bytes.chunks_exact(4).enumerate() {
                    let v = f32::from_le_bytes([s[0], s[1], s[2], s[3]]).abs();
                    let c = i % n;
                    if v > pk[c] {
                        pk[c] = v;
                    }
                }
            }
        })
        .register()?;

    let values = audio_format(channels, None)?;
    let mut params = [spa::pod::Pod::from_bytes(&values).ok_or(pw::Error::CreationFailed)?];
    stream.connect(spa::utils::Direction::Input, None, pw::stream::StreamFlags::MAP_BUFFERS, &mut params)?;
    Ok((stream, listener, peaks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_are_named_from_libspa() {
        assert_eq!([channel_id("MONO"), channel_id("FL"), channel_id("FR"), channel_id("nope")], [Some(2), Some(3), Some(4), None]);
    }

    #[test]
    fn a_route_sent_reads_back_the_same() {
        let bytes = route_param(2, 3, Some(vec![0.25, 0.5]), Some(true), Some(-20_000_000)).expect("serialized");
        let r = route_of(spa::pod::Pod::from_bytes(&bytes).expect("a pod")).expect("a route");
        assert_eq!((r.index, r.device, r.volumes, r.mute, r.latency_ns), (2, Some(3), Some(vec![0.25, 0.5]), true, -20_000_000));
    }

    #[test]
    fn a_volume_sent_reads_back_the_same() {
        let bytes = mix_param(vec![0.6141, 0.1327], true).expect("serialized");
        let pod = spa::pod::Pod::from_bytes(&bytes).expect("a pod");
        assert_eq!(mix_of(pod), Some((vec![0.6141, 0.1327], true)));
    }
}
