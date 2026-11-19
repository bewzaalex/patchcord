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
use std::time::Duration;

use pipewire as pw;
use pw::spa;
use pw::types::ObjectType;

pub type Props = BTreeMap<String, String>;

/// The prefix of the meter streams' node names: Patchcord hides its own nodes.
pub const METER_PREFIX: &str = "patchcord-meter-";
/// The cue's loopback: what is listened to goes into its recording side (`patchcord-cue`) and
/// comes out of its playing side (`patchcord-cue-out`) in the default output. Both are hidden.
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
    pub xruns: i32,
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
        (self.quantum, self.rate, self.load, self.xruns) = (d.quantum, d.rate, d.load, d.xruns);
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

/// From PipeWire to the app.
pub enum Event {
    Added { id: u32, kind: Kind, props: Props },
    Removed(u32),
    /// A node's or device's full properties, and a node's state ("running", "idle", …).
    Info { id: u32, props: Props, state: Option<&'static str> },
    /// A card's profiles (in PipeWire's order) and the one it is in.
    Profiles { id: u32, list: Vec<Profile>, current: Option<u32> },
    /// A card's ports (`all`, in PipeWire's order) and the ones in use, with their volumes.
    Routes { id: u32, all: Vec<Route>, active: Vec<Route> },
    /// An app stream's volume (playing) or gain (recording): each channel's, linear as PipeWire
    /// keeps them, and its mute.
    /// `soft`: the part of the volumes applied in software (the rest is the hardware's), when
    /// the node tells.
    Mix { id: u32, volumes: Vec<f32>, mute: bool, soft: Option<Vec<f32>> },
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
    /// One of a node's params, asked for by ENGINE's inspector: its kind (Format, Props…), its
    /// index and the pod as text.
    Param { id: u32, kind: String, index: u32, text: String },
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
    /// default output, at `level` (linear). No sources: nothing is heard, and the cue goes unless
    /// `keep` (CUE mode is on: it stays for the next jack).
    Cue { sources: Vec<CueSource>, level: f32, keep: bool },
    /// Run the profiler (ENGINE is open) or stop it: it costs a little on every cycle.
    Profile(bool),
    /// Read a node's params (formats, props, latency…) for ENGINE's inspector.
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
    Some(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj)).ok()?.0.into_inner())
}

// keys of a props object (spa/param/props.h)
const PROP_MUTE: u32 = 0x10004;
const PROP_CHANNEL_VOLUMES: u32 = 0x10008;
const PROP_LATENCY_OFFSET: u32 = 0x1000e;
const PROP_SOFT_VOLUMES: u32 = 0x10010;

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
    Some(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj)).ok()?.0.into_inner())
}

/// A node's volumes and mute, from its props param (None when it has no channel volumes).
fn mix_of(pod: &spa::pod::Pod) -> Option<(Vec<f32>, bool, Option<Vec<f32>>)> {
    use spa::pod::{Value, ValueArray};
    let (_, Value::Object(obj)) = spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()? else { return None };
    let (mut vols, mut mute, mut soft) = (None, false, None);
    for prop in obj.properties {
        match (prop.key, prop.value) {
            (PROP_CHANNEL_VOLUMES, Value::ValueArray(ValueArray::Float(v))) if !v.is_empty() => vols = Some(v),
            (PROP_MUTE, Value::Bool(m)) => mute = m,
            (PROP_SOFT_VOLUMES, Value::ValueArray(ValueArray::Float(v))) if !v.is_empty() => soft = Some(v),
            _ => {}
        }
    }
    Some((vols?, mute, soft))
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
    Some(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj)).ok()?.0.into_inner())
}

/// The cue: a loopback of Patchcord's (PipeWire's loopback module, as `pw-loopback`), made while
/// something is listened to. Its recording side takes in what is heard, on passive links, on the
/// clock of what it hears; its playing side plays into the default output, on that output's
/// clock; a little buffer between them bridges the two. So listening never joins the two clocks
/// (which switched an app's driver, and made some apps stop), and never makes anything play.
#[derive(Default)]
struct Cue {
    module: Option<std::ptr::NonNull<pw::sys::pw_impl_module>>,
    /// its recording side (and its input ports, by channel) and its playing side
    node: Option<u32>,
    inputs: BTreeMap<u32, u32>,
    out: Option<u32>,
    links: HashMap<(u32, u32), pw::link::Link>,
    /// what is wanted: sources and level
    want: (Vec<CueSource>, f32),
}

impl Drop for Cue {
    fn drop(&mut self) {
        self.links.clear();
        if let Some(m) = self.module.take() {
            // SAFETY: the module was loaded by `start` on this thread and is destroyed once
            unsafe { pw::sys::pw_impl_module_destroy(m.as_ptr()) };
        }
    }
}

impl Cue {
    /// Loads the loopback.
    fn start(&mut self, context: &pw::context::ContextRc) -> Result<(), String> {
        let args = format!(
            r#"{{ node.description = "Patchcord cue" audio.position = [ FL FR ]
  capture.props = {{ node.name = "{CUE_NODE}" media.class = "Stream/Input/Audio" node.autoconnect = false node.passive = true }}
  playback.props = {{ node.name = "{CUE_OUT}" node.description = "Patchcord cue" media.class = "Stream/Output/Audio" }} }}"#
        );
        let name = std::ffi::CString::new("libpipewire-module-loopback").expect("name");
        let args = std::ffi::CString::new(args).map_err(|e| e.to_string())?;
        // SAFETY: the context is alive, the strings outlive the call, and no properties are passed
        let m = unsafe { pw::sys::pw_context_load_module(context.as_raw_ptr(), name.as_ptr(), args.as_ptr(), std::ptr::null_mut()) };
        self.module = Some(std::ptr::NonNull::new(m).ok_or("the loopback module did not load")?);
        Ok(())
    }

    /// Links what is wanted into its recording side, once its ports exist, and unlinks the rest.
    fn reconcile(&mut self, core: &pw::core::CoreRc) {
        let want: Vec<(u32, u32, u32, u32)> = match self.node {
            Some(me) if self.inputs.len() >= 2 => {
                let ins: Vec<u32> = self.inputs.values().copied().collect();
                let mut v = Vec::new();
                for &(node, port, ch) in &self.want.0 {
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
            // passive: listening never makes anything play
            let props = pw::properties::properties! {
                "link.output.node" => on.to_string(),
                "link.output.port" => op.to_string(),
                "link.input.node" => inn.to_string(),
                "link.input.port" => ip.to_string(),
                "link.passive" => "true",
            };
            if let Ok(l) = core.create_object::<pw::link::Link>("link-factory", &props) {
                self.links.insert((op, ip), l);
            }
        }
    }

    /// Its playing side's volume: the cue's level.
    fn set_level(&self, bound: &HashMap<u32, Bound>) {
        if let Some(n) = self.out.and_then(|id| bound.get(&id)).and_then(|(n, _)| n.downcast_ref::<pw::node::Node>())
            && let Some(bytes) = mix_param(vec![self.want.1; 2], false)
            && let Some(pod) = spa::pod::Pod::from_bytes(&bytes)
        {
            n.set_param(spa::param::ParamType::Props, 0, pod);
        }
    }
}

/// A signal meter: a capture stream, its ports once they exist, and the passive links to them.
/// A node or device bound to keep its info coming: its proxy and listener.
type Bound = (Box<dyn Any>, Box<dyn Any>);
/// A meter's stream, its listener and the peaks it measures.
type MeterStream = (pw::stream::StreamRc, pw::stream::StreamListener<()>, Rc<RefCell<Vec<f32>>>);

struct Meter {
    _stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<()>,
    targets: Vec<(u32, u32)>,
    node: Option<u32>,
    /// our input ports now, by channel (`port.id`): their global ids. A stream's ports can go
    /// and come back with new ids while its format is set up, so this keeps the latest of each.
    ports: BTreeMap<u32, u32>,
    /// the links to the ports, and which ports (global ids) they were made to
    links: Vec<pw::link::Link>,
    linked: Vec<u32>,
    peaks: Rc<RefCell<Vec<f32>>>,
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
    let inspect: Rc<RefCell<std::collections::HashSet<u32>>> = Default::default();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let events = events.clone();
            let registry = registry.clone();
            let bound = bound.clone();
            let meters = meters.clone();
            let stale = stale.clone();
            let core = core.clone();
            let default_md = default_md.clone();
            let cue = cue.clone();
            let profiler_id = profiler_id.clone();
            let inspect = inspect.clone();
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
                    bound.borrow_mut().insert(g.id, (Box::new(md), Box::new(l)));
                }
                if g.type_ == ObjectType::Profiler {
                    profiler_id.set(Some(g.id));
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
                            let l = node
                                .add_listener_local()
                                .param(move |_, kind, index, _, pod| {
                                    if kind == spa::param::ParamType::Props
                                        && let Some((volumes, mute, soft)) = pod.and_then(mix_of)
                                    {
                                        mev.send(Event::Mix { id, volumes, mute, soft });
                                    }
                                    // the inspector asked for this node's params
                                    if let Some(pod) = pod
                                        && inspect.borrow().contains(&id)
                                    {
                                        let kind = format!("{kind:?}").trim_start_matches("ParamType::").to_owned();
                                        mev.send(Event::Param { id, kind, index, text: pod_text(pod) });
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
                                })
                                .register();
                            if app {
                                node.subscribe_params(&[spa::param::ParamType::Props]);
                            }
                            bound.borrow_mut().insert(id, (Box::new(node), Box::new(l)));
                            if props.get("node.name").is_some_and(|n| n == CUE_OUT) {
                                let mut c = cue.borrow_mut();
                                c.out = Some(id);
                                c.set_level(&bound.borrow());
                            }
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
                                    ev.send(Event::Info { id, props: info.props().map(props_of).unwrap_or_default(), state: None })
                                })
                                .param(move |_, kind, index, _, pod| {
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
                            bound.borrow_mut().insert(id, (Box::new(dev), Box::new(l)));
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
                        for m in meters.borrow_mut().values_mut() {
                            if let Some(me) = m.node
                                && m.node == node
                                && input
                            {
                                m.ports.insert(index, id);
                                let now: Vec<u32> = m.ports.values().copied().collect();
                                if now.len() == m.targets.len() && now != m.linked {
                                    m.links.clear();
                                    m.linked = now.clone();
                                    for (&(tn, tp), &mine) in m.targets.iter().zip(&now) {
                                        let p = pw::properties::properties! {
                                            "link.output.node" => tn.to_string(),
                                            "link.output.port" => tp.to_string(),
                                            "link.input.node" => me.to_string(),
                                            "link.input.port" => mine.to_string(),
                                            // a meter never keeps a device awake
                                            "link.passive" => "true",
                                        };
                                        if let Ok(l) = core.create_object::<pw::link::Link>("link-factory", &p) {
                                            m.links.push(l);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Kind::Link => {
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
                                })
                                .register();
                            bound.borrow_mut().insert(id, (Box::new(link), Box::new(l)));
                        }
                    }
                    Kind::Client | Kind::Other => {}
                }
                events.send(Event::Added { id, kind, props });
            }
        })
        .global_remove({
            let events = events.clone();
            let bound = bound.clone();
            let meters = meters.clone();
            let cue = cue.clone();
            move |id| {
                bound.borrow_mut().remove(&id);
                {
                    let mut c = cue.borrow_mut();
                    c.inputs.retain(|_, p| *p != id);
                    if c.node == Some(id) {
                        c.node = None;
                    }
                    if c.out == Some(id) {
                        c.out = None;
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
            move |info| events.send(Event::Version(info.version().to_owned()))
        })
        .register();

    // meter levels go out 25 times a second, while there is something to show; devices whose
    // profiles changed are asked for them again
    let quiet = Rc::new(RefCell::new(true));
    let ticks = std::cell::Cell::new(0u32);
    let timer = mainloop.loop_().add_timer({
        let meters = meters.clone();
        let events = events.clone();
        let stale = stale.clone();
        let bound = bound.clone();
        let profiler = profiler.clone();
        move |_| {
            // each driver's cycles of the last half second, averaged, with their peaks (drivers
            // that stopped reporting are left out); then counting starts again
            ticks.set(ticks.get().wrapping_add(1));
            if ticks.get().is_multiple_of(12)
                && let Some(p) = profiler.borrow().as_ref()
            {
                let now = std::time::Instant::now();
                let mut l = p.latest.borrow_mut();
                l.retain(|_, (t, _, _)| now.duration_since(*t) < Duration::from_millis(1500));
                events.send(Event::Profile(l.values().map(|(_, d, _)| d.averaged()).collect()));
                for (_, _, open) in l.values_mut() {
                    *open = false;
                }
            }
            for id in std::mem::take(&mut *stale.borrow_mut()) {
                let b = bound.borrow();
                let Some((proxy, _)) = b.get(&id) else { continue };
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

    // links we made; with object.linger they stay after we quit, but we hold the proxies meanwhile
    let made: RefCell<Vec<pw::link::Link>> = Default::default();
    let _commands = cmds.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        let core = core.clone();
        let registry = registry.clone();
        let meters = meters.clone();
        let events = events.clone();
        let bound = bound.clone();
        let default_md = default_md.clone();
        let cue = cue.clone();
        let context = context.clone();
        let profiler = profiler.clone();
        let profiler_id = profiler_id.clone();
        let inspect = inspect.clone();
        let profiler_module = std::cell::Cell::new(false);
        move |cmd| match cmd {
            Cmd::Link { out_node, out_port, in_node, in_port } => {
                let props = pw::properties::properties! {
                    "link.output.node" => out_node.to_string(),
                    "link.output.port" => out_port.to_string(),
                    "link.input.node" => in_node.to_string(),
                    "link.input.port" => in_port.to_string(),
                    "object.linger" => "true",
                };
                match core.create_object::<pw::link::Link>("link-factory", &props) {
                    Ok(link) => made.borrow_mut().push(link),
                    Err(e) => events.send(Event::Error(format!("cannot create link: {e}"))),
                }
            }
            Cmd::Unlink(id) => {
                registry.destroy_global(id);
            }
            Cmd::Meter { id, ports, channels } => match meter(&core, id, &channels) {
                Ok((stream, listener, peaks)) => {
                    meters.borrow_mut().insert(id, Meter { _stream: stream, _listener: listener, targets: ports, node: None, ports: BTreeMap::new(), links: Vec::new(), linked: Vec::new(), peaks });
                }
                Err(e) => events.send(Event::Error(format!("cannot start meter: {e}"))),
            },
            Cmd::Unmeter(id) => {
                meters.borrow_mut().remove(&id);
            }
            Cmd::SetProfile { device, index } => {
                let b = bound.borrow();
                let dev = b.get(&device).and_then(|(d, _)| d.downcast_ref::<pw::device::Device>());
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
                let dev = b.get(&device).and_then(|(d, _)| d.downcast_ref::<pw::device::Device>());
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
                match default_md.get().and_then(|id| b.get(&id)).and_then(|(m, _)| m.downcast_ref::<pw::metadata::Metadata>()) {
                    Some(md) => {
                        let name = node_name.replace('\\', "\\\\").replace('"', "\\\"");
                        md.set_property(0, &key, Some("Spa:String:JSON"), Some(&format!("{{\"name\":\"{name}\"}}")));
                    }
                    None => events.send(Event::Error("cannot set the default device".into())),
                }
            }
            Cmd::SetMix { node, volumes, mute } => {
                let b = bound.borrow();
                let n = b.get(&node).and_then(|(n, _)| n.downcast_ref::<pw::node::Node>());
                match (n, mix_param(volumes, mute)) {
                    (Some(n), Some(bytes)) => {
                        if let Some(pod) = spa::pod::Pod::from_bytes(&bytes) {
                            n.set_param(spa::param::ParamType::Props, 0, pod);
                        }
                    }
                    _ => events.send(Event::Error(format!("cannot set the volume of node {node}"))),
                }
            }
            Cmd::Cue { sources, level, keep } => {
                let mut c = cue.borrow_mut();
                if sources.is_empty() && !keep {
                    // nothing to hear, and no CUE mode: its links and the loopback go
                    *c = Cue::default();
                    return;
                }
                let level_changed = c.want.1 != level;
                c.want = (sources, level);
                if c.module.is_none() {
                    if let Err(e) = c.start(&context) {
                        events.send(Event::Error(format!("cannot start the cue: {e}")));
                    }
                } else if level_changed {
                    c.set_level(&bound.borrow());
                }
                c.reconcile(&core);
            }
            Cmd::Profile(on) => {
                let mut p = profiler.borrow_mut();
                if !on {
                    *p = None;
                } else if p.is_none()
                    && let Some(id) = profiler_id.get()
                {
                    // the profiler's protocol comes with its module, loaded into our context
                    // once (as pw-top does)
                    if !profiler_module.get() {
                        let name = std::ffi::CString::new("libpipewire-module-profiler").expect("name");
                        // SAFETY: the context is alive and the name outlives the call
                        let m = unsafe { pw::sys::pw_context_load_module(context.as_raw_ptr(), name.as_ptr(), std::ptr::null(), std::ptr::null_mut()) };
                        profiler_module.set(!m.is_null());
                    }
                    *p = Profiler::bind(&registry, id);
                    if p.is_none() {
                        events.send(Event::Error("cannot start the profiler".into()));
                    }
                }
            }
            Cmd::Params(id) => {
                inspect.borrow_mut().insert(id);
                let b = bound.borrow();
                if let Some(n) = b.get(&id).and_then(|(n, _)| n.downcast_ref::<pw::node::Node>()) {
                    use spa::param::ParamType as P;
                    for kind in [P::EnumFormat, P::Format, P::Props, P::PortConfig, P::Latency, P::ProcessLatency] {
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
type ProfileLatest = RefCell<BTreeMap<u32, (std::time::Instant, DriverTiming, bool)>>;

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
    let now = std::time::Instant::now();
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
    let mut out = Vec::new();
    for o in objects {
        let Value::Object(o) = o else { continue };
        let mut d = DriverTiming::default();
        let mut driver = false;
        for p in o.properties {
            let Value::Struct(f) = p.value else { continue };
            match p.key {
                spa::sys::SPA_PROFILER_info => {
                    d.load = match f.get(1) {
                        Some(Value::Float(x)) => *x,
                        Some(Value::Double(x)) => *x as f32,
                        _ => 0.0,
                    };
                    d.xruns = int(f.get(4)) as i32;
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
                spa::sys::SPA_PROFILER_followerBlock => d.followers.push(block(&f)),
                _ => {}
            }
        }
        if driver {
            // when the last of its nodes finished, from the start of the cycle
            let start = d.node.signal_ns;
            d.end_ns = std::iter::once(&d.node).chain(&d.followers).filter(|n| start > 0 && n.finish_ns >= start).map(|n| n.finish_ns - start).max().unwrap_or(0);
            out.push(d);
        }
    }
    out
}

/// A param as text, the way pw-dump shows it: the object's keys by name (rate, channels,
/// channelVolumes…), enumerated values by name (audio, raw, F32P), one key a line.
fn pod_text(pod: &spa::pod::Pod) -> String {
    match spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()) {
        Ok((_, v)) => value_text(&v, std::ptr::null(), 0),
        Err(_) => "(cannot read)".into(),
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

/// `enums`: the table an Id value is named from (the property's own values).
fn value_text(v: &spa::pod::Value, enums: *const spa::sys::spa_type_info, depth: usize) -> String {
    use spa::pod::Value;
    let pad = "  ".repeat(depth + 1);
    match v {
        Value::Object(o) => {
            // SAFETY: libspa's static type tables
            let props_table = unsafe {
                let t = spa::sys::spa_debug_type_find(spa::sys::spa_types, o.type_);
                if t.is_null() { std::ptr::null() } else { (*t).values }
            };
            let mut s = String::from("{\n");
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
                s += &format!("{pad}{name}: {}\n", value_text(&p.value, values, depth + 1));
            }
            s + &"  ".repeat(depth) + "}"
        }
        Value::Struct(items) => format!("[ {} ]", items.iter().map(|i| value_text(i, std::ptr::null(), depth + 1)).collect::<Vec<_>>().join(", ")),
        Value::Id(id) => type_name(enums, id.0).unwrap_or_else(|| id.0.to_string()),
        Value::String(t) => format!("\"{t}\""),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Long(l) => l.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Double(f) => format!("{f}"),
        Value::Fraction(f) => format!("{}/{}", f.num, f.denom),
        Value::Rectangle(r) => format!("{}x{}", r.width, r.height),
        Value::ValueArray(spa::pod::ValueArray::Id(ids)) => format!("[ {} ]", ids.iter().map(|i| type_name(enums, i.0).unwrap_or_else(|| i.0.to_string())).collect::<Vec<_>>().join(", ")),
        Value::ValueArray(a) => format!("{a:?}").split_once('(').map_or_else(|| format!("{a:?}"), |(_, r)| r.trim_end_matches(')').to_owned()),
        Value::Choice(c) => choice_text(c, enums),
        other => format!("{other:?}"),
    }
}

/// A choice (what a format may be): its default, then the alternatives.
fn choice_text(c: &spa::pod::ChoiceValue, enums: *const spa::sys::spa_type_info) -> String {
    use spa::pod::ChoiceValue as C;
    use spa::utils::ChoiceEnum as E;
    fn list<T: std::fmt::Display + spa::pod::CanonicalFixedSizedPod>(e: &E<T>) -> String {
        match e {
            E::None(v) => v.to_string(),
            E::Range { default, min, max } => format!("{default} ({min}…{max})"),
            E::Step { default, min, max, step } => format!("{default} ({min}…{max} by {step})"),
            E::Enum { default, alternatives } => format!("{default} (one of {})", alternatives.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")),
            E::Flags { default, flags } => format!("{default} (flags {})", flags.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")),
        }
    }
    let id = |i: &spa::utils::Id| type_name(enums, i.0).unwrap_or_else(|| i.0.to_string());
    match c {
        C::Int(ch) => list(&ch.1),
        C::Long(ch) => list(&ch.1),
        C::Float(ch) => list(&ch.1),
        C::Double(ch) => list(&ch.1),
        C::Id(ch) => match &ch.1 {
            E::None(v) => id(v),
            E::Enum { default, alternatives } => format!("{} (one of {})", id(default), alternatives.iter().map(id).collect::<Vec<_>>().join(", ")),
            other => format!("{other:?}"),
        },
        C::Fraction(ch) => match &ch.1 {
            E::None(v) => format!("{}/{}", v.num, v.denom),
            E::Range { default, min, max } => format!("{}/{} ({}/{}…{}/{})", default.num, default.denom, min.num, min.denom, max.num, max.denom),
            other => format!("{other:?}"),
        },
        other => format!("{other:?}"),
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
                let size = d.chunk().size() as usize;
                let Some(bytes) = d.data() else { return };
                let mut pk = peaks.borrow_mut();
                for (i, s) in bytes[..size.min(bytes.len())].chunks_exact(4).enumerate() {
                    let v = f32::from_le_bytes([s[0], s[1], s[2], s[3]]).abs();
                    let c = i % n;
                    if v > pk[c] {
                        pk[c] = v;
                    }
                }
            }
        })
        .register()?;

    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_channels(n as u32);
    let mut pos = [0u32; 64];
    for (i, c) in channels.iter().enumerate().take(64) {
        // SPA channel positions by name (MONO 2, FL 3, FR 4…); AUX0 4096… for the rest
        pos[i] = channel_id(c).unwrap_or(4096 + i as u32);
    }
    info.set_position(pos);
    let obj = spa::pod::Object { type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(), id: spa::param::ParamType::EnumFormat.as_raw(), properties: info.into() };
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj)).map_err(|_| pw::Error::CreationFailed)?.0.into_inner();
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
        assert_eq!(mix_of(pod), Some((vec![0.6141, 0.1327], true, None)));
    }
}
