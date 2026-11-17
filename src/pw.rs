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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Device,
    Node,
    Port,
    Link,
    /// a program connected to PipeWire: which process its streams belong to
    Client,
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
    Mix { id: u32, volumes: Vec<f32>, mute: bool },
    /// Peak level of each meter's channels since the last report, 0…1.
    Levels(Vec<(u32, Vec<f32>)>),
    /// A value of PipeWire's "settings" metadata (clock.rate, clock.quantum…).
    Setting { key: String, value: String },
    /// The PipeWire version.
    Version(String),
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
    Some(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj)).ok()?.0.into_inner())
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
            move |g| {
                // the "settings" metadata (the clock rate and quantum) and the "default" one (the
                // default output and input)
                let md_name = g.props.and_then(|p| p.get("metadata.name"));
                if g.type_ == ObjectType::Metadata
                    && matches!(md_name, Some("settings" | "default"))
                    && let Ok(md) = registry.bind::<pw::metadata::Metadata, _>(g)
                {
                    if md_name == Some("default") {
                        default_md.set(Some(g.id));
                    }
                    let ev = events.clone();
                    let l = md
                        .add_listener_local()
                        .property(move |_, key, _, value| {
                            if let Some(key) = key {
                                ev.send(Event::Setting { key: key.to_owned(), value: value.unwrap_or("").to_owned() });
                            }
                            0
                        })
                        .register();
                    bound.borrow_mut().insert(g.id, (Box::new(md), Box::new(l)));
                    return;
                }
                let kind = match g.type_ {
                    ObjectType::Device => Kind::Device,
                    ObjectType::Node => Kind::Node,
                    ObjectType::Port => Kind::Port,
                    ObjectType::Link => Kind::Link,
                    ObjectType::Client => Kind::Client,
                    _ => return,
                };
                let props = g.props.map(props_of).unwrap_or_default();
                let id = g.id;
                match kind {
                    Kind::Node => {
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
                            let l = node
                                .add_listener_local()
                                .param(move |_, kind, _, _, pod| {
                                    if kind == spa::param::ParamType::Props
                                        && let Some((volumes, mute)) = pod.and_then(mix_of)
                                    {
                                        mev.send(Event::Mix { id, volumes, mute });
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
                    Kind::Link | Kind::Client => {}
                }
                events.send(Event::Added { id, kind, props });
            }
        })
        .global_remove({
            let events = events.clone();
            let bound = bound.clone();
            let meters = meters.clone();
            move |id| {
                bound.borrow_mut().remove(&id);
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
            move |id, _seq, res, message| events.send(Event::Error(format!("object {id}: {message} ({res})")))
        })
        .info({
            let events = events.clone();
            move |info| events.send(Event::Version(info.version().to_owned()))
        })
        .register();

    // meter levels go out 25 times a second, while there is something to show; devices whose
    // profiles changed are asked for them again
    let quiet = Rc::new(RefCell::new(true));
    let timer = mainloop.loop_().add_timer({
        let meters = meters.clone();
        let events = events.clone();
        let stale = stale.clone();
        let bound = bound.clone();
        move |_| {
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
            Cmd::Quit => mainloop.quit(),
        }
    });

    mainloop.run();
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
        // SPA channel positions: MONO 2, FL 3, FR 4, AUX0 4096…
        pos[i] = match c.as_str() {
            "MONO" => 2,
            "FL" => 3,
            "FR" => 4,
            _ => 4096 + i as u32,
        };
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
