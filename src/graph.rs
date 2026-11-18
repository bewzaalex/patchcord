//! The PipeWire graph as Patchcord shows it: raw objects in, panel devices out.
//!
//! A device on the panel is one card (all nodes sharing a `device.id`: sink, source and their
//! monitors) or one node without a device (an app, a stream, a MIDI bridge). Its ports are
//! sorted into labelled groups such as PLAYBACK, CAPTURE or MONITOR.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use crate::pw::{Avail, CUE_NODE, Event, Kind, METER_PREFIX, Profile, Props, Route};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Dir {
    In,
    Out,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Media {
    Audio,
    Midi,
    Video,
    Other,
}

#[derive(Clone, Debug)]
pub struct Port {
    /// PipeWire's id of the port
    pub id: u32,
    pub node: u32,
    /// `port.name`, as PipeWire calls it
    pub name: String,
    /// short label printed under the jack
    pub label: String,
    pub dir: Dir,
    pub media: Media,
    /// `audio.channel`: FL, FR, MONO…
    pub channel: String,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub label: String,
    pub dir: Dir,
    pub ports: Vec<Port>,
    /// exactly a left and a right audio channel: shown as one stereo jack unless split
    pub stereo: bool,
    /// it has a meter in its frame: every audio group, but Bluetooth's internal capture stream's
    /// (WirePlumber watches what links to it, to switch a headset's profile)
    pub meter: bool,
    /// an app stream's volume (its output) or gain (its input), or a device's (its output or
    /// input), set beside its jacks
    pub mix: Option<Mix>,
    /// a card's output or input: the ports it can use (Speakers, Headphones…), in PipeWire's order
    pub ports_to: Vec<PortChoice>,
    /// one of an app's several streams of a kind (Teams plays on five): its number, from 1, in
    /// its label too (OUTPUT 1, OUTPUT 2…); 0 for an app's only one, and for everything else.
    /// Each is a frame of its own with its own controls and meter
    pub stream: usize,
}

/// One of the ports a card's output or input can use.
#[derive(Clone, Debug, PartialEq)]
pub struct PortChoice {
    /// the card's route index
    pub index: u32,
    /// PipeWire's name for it, the same in every language (`analog-output-speaker`)
    pub name: String,
    pub description: String,
    /// nothing is plugged into it
    pub unplugged: bool,
}

/// Where a card keeps an output's or input's volume: the port in use (its route) on the card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteRef {
    pub card: u32,
    pub index: u32,
    /// the card's sink or source the port is in use for (`card.profile.device`)
    pub device: i32,
    pub latency_ns: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Finish {
    Metal,
    Black,
    Cream,
    Navy,
}

#[derive(Clone, Debug)]
pub struct Device {
    /// stays the same across restarts (`device.name` or `node.name`), for layouts and presets
    pub key: String,
    /// the full name, as PipeWire shows it
    pub name: String,
    /// the small line under the name: what kind of device it is
    pub model: String,
    pub finish: Finish,
    /// colour of the cords that leave this device
    pub color: [u8; 3],
    pub groups: Vec<Group>,
    /// "running", "idle" or "suspended": the busiest of its nodes
    pub state: &'static str,
    /// a sound card's profiles, when it has some
    pub card: Option<Card>,
}

/// The highest volume a knob goes to: 153% (+11 dB), as in pavucontrol.
pub const VOL_MAX: f64 = 1.53;

/// An app stream's volume (or, recording, its gain): PipeWire keeps one linear gain per channel;
/// Patchcord shows them as pavucontrol does, as a volume (the cube root: 100% is 0 dB) and a
/// balance between L and R.
#[derive(Clone, Debug, PartialEq)]
pub struct Mix {
    pub node: u32,
    pub volumes: Vec<f32>,
    pub mute: bool,
    /// what comes in (an app that records, a microphone): its knob is a GAIN
    pub gain: bool,
    /// a card's output or input: its volume is on the port in use, not on the node
    pub route: Option<RouteRef>,
}

impl Mix {
    /// Each channel's volume on the knob's scale (1 is 100%).
    fn levels(&self) -> Vec<f64> {
        self.volumes.iter().map(|v| (*v as f64).max(0.0).cbrt()).collect()
    }

    /// The volume: the loudest channel.
    pub fn volume(&self) -> f64 {
        self.levels().into_iter().fold(0.0, f64::max)
    }

    /// The balance, -1 (only L) to 1 (only R), as PulseAudio works it out: the quieter side
    /// against the louder one. Only stereo has one.
    pub fn balance(&self) -> f64 {
        let l = self.levels();
        let [a, b] = l[..] else { return 0.0 };
        if a == b {
            0.0
        } else if a > b {
            b / a - 1.0
        } else {
            1.0 - a / b
        }
    }

    /// The channel volumes (linear) for a volume and balance: the far side at the volume, the
    /// near side lower.
    pub fn volumes_for(&self, volume: f64, balance: f64) -> Vec<f32> {
        let v = volume.clamp(0.0, VOL_MAX);
        let b = balance.clamp(-1.0, 1.0);
        let lin = |x: f64| x.powi(3) as f32;
        match self.volumes.len() {
            2 => vec![lin(if b > 0.0 { v * (1.0 - b) } else { v }), lin(if b < 0.0 { v * (1.0 + b) } else { v })],
            n => vec![lin(v); n.max(1)],
        }
    }
}

/// A volume as pavucontrol writes it: "85%" and "-4.24 dB".
pub fn volume_text(v: f64) -> (String, String) {
    let db = if v <= 0.0 { "-∞ dB".to_owned() } else { format!("{:.2} dB", 60.0 * v.log10() + 0.0) };
    (format!("{}%", (v * 100.0).round()), db.replace("-0.00", "0.00"))
}

/// A sound card: its PipeWire device and its profiles (see `pw::Profile`).
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: u32,
    pub profiles: Vec<Profile>,
    pub current: Option<u32>,
}

/// Profiles that differ only in their codec, together: one row of the PROFILE list, its codecs as
/// choices. Off, and the profiles of a sound card, are families of one.
#[derive(Clone, Debug, PartialEq)]
pub struct Family {
    pub name: String,
    /// (profile index, codec)
    pub members: Vec<(u32, Option<String>)>,
    /// nothing is plugged in for it (an HDMI output without a screen with speakers)
    pub unplugged: bool,
}

impl Card {
    pub fn profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| Some(p.index) == self.current)
    }

    /// A profile's family and codec: "High Fidelity Playback (A2DP Sink, codec AAC)" is
    /// "High Fidelity Playback (A2DP Sink)" with AAC.
    pub fn split(description: &str) -> (String, Option<String>) {
        if let Some(i) = description.rfind(", codec ")
            && description.ends_with(')')
        {
            return (format!("{})", &description[..i]), Some(description[i + 8..description.len() - 1].to_owned()));
        }
        (description.to_owned(), None)
    }

    /// The profiles by family, in PipeWire's order.
    pub fn families(&self) -> Vec<Family> {
        let mut out: Vec<Family> = Vec::new();
        for p in &self.profiles {
            let (name, codec) = Card::split(&p.description);
            let gone = p.available == Avail::No;
            match out.iter_mut().find(|f| f.name == name) {
                Some(f) => {
                    f.members.push((p.index, codec));
                    f.unplugged &= gone;
                }
                None => out.push(Family { name, members: vec![(p.index, codec)], unplugged: gone }),
            }
        }
        out
    }

    /// The profile a family's row stands for: the one with the codec in use now, if it has it,
    /// else the one PipeWire ranks first.
    pub fn pick(&self, family: &Family) -> Option<u32> {
        let codec = self.profile().and_then(|p| Card::split(&p.description).1);
        let rank = |i: &u32| self.profiles.iter().find(|p| p.index == *i).map_or(i32::MIN, |p| p.priority);
        family.members.iter().find(|(_, c)| c.is_some() && *c == codec).map(|(i, _)| *i).or_else(|| family.members.iter().map(|(i, _)| *i).max_by_key(rank))
    }

    /// What the LCD says: the profile in use, its codec, and whether anything is plugged in.
    pub fn summary(&self) -> String {
        let Some(p) = self.profile() else { return "PROFILE ?".into() };
        let (name, codec) = Card::split(&p.description);
        let mut s = name;
        if let Some(c) = codec {
            s = format!("{s} · {c}");
        }
        if p.available == Avail::No {
            s.push_str(" · UNPLUGGED");
        }
        s
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Link {
    pub id: u32,
    pub out_port: u32,
    pub in_port: u32,
}

#[derive(Default)]
pub struct Graph {
    objects: HashMap<u32, (Kind, Props)>,
    /// nodes and devices whose full properties have arrived
    info: HashSet<u32>,
    state: HashMap<u32, &'static str>,
    /// when each node appeared: streams show only once they have lived a little
    born: HashMap<u32, Instant>,
    /// each card's profiles and the one it is in, by device
    profiles: HashMap<u32, (Vec<Profile>, Option<u32>)>,
    /// each app stream's channel volumes and mute, by node
    mix: HashMap<u32, (Vec<f32>, bool)>,
    /// each card's ports, and the ones in use (with the card's real volumes), by device
    routes: HashMap<u32, (Vec<Route>, Vec<Route>)>,
    pub synced: bool,
}

/// A new stream shows after this long, so a short event sound (a click when changing the volume)
/// never pops up as a device.
const YOUNG: Duration = Duration::from_millis(500);

const PALETTE: [[u8; 3]; 9] = [
    [0xe8, 0x74, 0x2a],
    [0x5b, 0x7f, 0xe0],
    [0x3f, 0xae, 0x5a],
    [0x2b, 0xb0, 0xa8],
    [0xd2, 0x3b, 0x32],
    [0x9b, 0x5b, 0xd0],
    [0xe9, 0xc5, 0x3a],
    [0xc0, 0x5b, 0xd0],
    [0xd8, 0xdc, 0xdf],
];

impl Graph {
    /// Applies an event; true when what the panel shows may have changed.
    pub fn apply(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Added { id, kind, props } => {
                if *kind == Kind::Node {
                    // what was there before we connected is old enough already
                    let t = if self.synced { Instant::now() } else { Instant::now() - YOUNG };
                    self.born.insert(*id, t);
                }
                self.objects.insert(*id, (*kind, props.clone()));
                true
            }
            Event::Profiles { id, list, current } => {
                let new = (list.clone(), *current);
                self.profiles.insert(*id, new.clone()) != Some(new)
            }
            Event::Routes { id, all, active } => {
                let new = (all.clone(), active.clone());
                self.routes.insert(*id, new.clone()) != Some(new)
            }
            Event::Mix { id, volumes, mute } => {
                let new = (volumes.clone(), *mute);
                self.mix.insert(*id, new.clone()) != Some(new)
            }
            Event::Removed(id) => {
                self.born.remove(id);
                self.mix.remove(id);
                self.routes.remove(id);
                self.profiles.remove(id);
                self.info.remove(id);
                self.state.remove(id);
                self.objects.remove(id).is_some()
            }
            Event::Info { id, props, state } => {
                let Some((_, p)) = self.objects.get_mut(id) else { return false };
                let before = (p.get("media.class").cloned(), p.get("node.description").cloned(), p.get("stream.monitor").cloned());
                p.extend(props.iter().map(|(k, v)| (k.clone(), v.clone())));
                let new = self.info.insert(*id);
                let changed_state = state.is_some_and(|s| self.state.insert(*id, s) != Some(s));
                let after = (p.get("media.class").cloned(), p.get("node.description").cloned(), p.get("stream.monitor").cloned());
                new || changed_state || before != after
            }
            Event::Levels(_) | Event::Setting { .. } | Event::Version(_) => false,
            Event::Synced => {
                self.synced = true;
                true
            }
            Event::Error(_) => false,
        }
    }

    fn of(&self, kind: Kind) -> impl Iterator<Item = (u32, &Props)> {
        self.objects.iter().filter(move |(_, (k, _))| *k == kind).map(|(id, (_, p))| (*id, p))
    }

    fn props(&self, id: u32) -> Option<&Props> {
        self.objects.get(&id).map(|(_, p)| p)
    }

    /// The nodes a port's sound reaches, following the links and on through every node it enters
    /// (a sound card passes it on out of its monitor, an effect out of its outputs).
    pub fn reaches(&self, port: u32) -> HashSet<u32> {
        let links = self.links();
        let mut seen: HashSet<u32> = HashSet::new();
        let mut ports = vec![port];
        let mut done: HashSet<u32> = HashSet::new();
        while let Some(p) = ports.pop() {
            if !done.insert(p) {
                continue;
            }
            for l in links.iter().filter(|l| l.out_port == p) {
                let Some(n) = self.port_node(l.in_port) else { continue };
                if seen.insert(n) {
                    // on out of that node's outputs
                    ports.extend(self.of(Kind::Port).filter(|(_, pp)| pp.get("node.id").and_then(|v| v.parse::<u32>().ok()) == Some(n) && pp.get("port.direction").is_some_and(|d| d == "out")).map(|(id, _)| id));
                }
            }
        }
        seen
    }

    /// The nodes Patchcord always leaves out, by who they belong to: other programs' level
    /// meters (`stream.monitor`) and Patchcord's own meters and cue. (who, what, how many)
    pub fn system(&self) -> Vec<(String, &'static str, usize)> {
        let mut m: std::collections::BTreeMap<(String, &'static str), usize> = std::collections::BTreeMap::new();
        for (_, p) in self.of(Kind::Node) {
            let name = p.get("node.name").map_or("", |s| s.as_str());
            let who_what = if name.starts_with(METER_PREFIX) {
                ("Patchcord".to_owned(), "level meters")
            } else if name.starts_with(CUE_NODE) {
                ("Patchcord".to_owned(), "cue (listen)")
            } else if p.get("stream.monitor").is_some_and(|v| v == "true") {
                (some(p.get("application.name")).or(some(p.get("node.name"))).cloned().unwrap_or_else(|| "?".into()), "level meters")
            } else if p.get("media.class").is_some_and(|c| c.ends_with("/Internal")) {
                // by its headset's name
                let dev = p.get("device.id").and_then(|d| d.parse::<u32>().ok()).and_then(|d| self.props(d)).and_then(|d| some(d.get("device.description")).cloned());
                let desc = some(p.get("node.description")).map(|d| d.trim_start_matches("Bluetooth internal capture stream for ").to_owned());
                (dev.or(desc).unwrap_or_else(|| name.to_owned()), "the plumbing behind MIC")
            } else {
                continue;
            };
            *m.entry(who_what).or_default() += 1;
        }
        // an app's recording streams' monitors (a copy of what each records): left off its face
        let monitored: HashSet<u32> = self.of(Kind::Port).filter(|(_, pp)| pp.get("port.monitor").is_some_and(|v| v == "true")).filter_map(|(_, pp)| pp.get("node.id")?.parse().ok()).collect();
        for (id, np) in self.of(Kind::Node) {
            if monitored.contains(&id) && np.get("media.class").is_some_and(|c| c == "Stream/Input/Audio") && !self.hidden(id, np) {
                let who = some(np.get("application.name")).or(some(np.get("node.name"))).cloned().unwrap_or_else(|| "?".into());
                *m.entry((who, "monitors of its recording streams")).or_default() += 1;
            }
        }
        m.into_iter().map(|((who, what), n)| (who, what, n)).collect()
    }

    /// A node's `node.name`, as PipeWire's defaults name it.
    pub fn node_name(&self, node: u32) -> Option<&str> {
        self.props(node)?.get("node.name").map(|s| s.as_str())
    }

    pub fn links(&self) -> Vec<Link> {
        let num = |p: &Props, k: &str| p.get(k).and_then(|v| v.parse().ok());
        let mut v: Vec<Link> = self
            .of(Kind::Link)
            .filter_map(|(id, p)| Some(Link { id, out_port: num(p, "link.output.port")?, in_port: num(p, "link.input.port")? }))
            .collect();
        v.sort_by_key(|l| l.id);
        v
    }

    /// The node a port belongs to.
    pub fn port_node(&self, port: u32) -> Option<u32> {
        self.props(port)?.get("node.id")?.parse().ok()
    }

    /// A port by name: (node.name, port.name), as presets keep them.
    pub fn port_name(&self, port: u32) -> Option<(String, String)> {
        let node = self.props(self.port_node(port)?)?.get("node.name")?.clone();
        Some((node, self.props(port)?.get("port.name")?.clone()))
    }

    /// The port with these names (the first, if several nodes share the name).
    pub fn find_port(&self, node: &str, port: &str) -> Option<u32> {
        let mut found: Vec<u32> = self
            .of(Kind::Port)
            .filter(|(_, p)| p.get("port.name").is_some_and(|n| n == port))
            .filter(|(id, _)| self.port_node(*id).and_then(|n| self.props(n)).and_then(|p| p.get("node.name")).is_some_and(|n| n == node))
            .map(|(id, _)| id)
            .collect();
        found.sort();
        found.first().copied()
    }

    /// The names of the nodes shown, for presets.
    pub fn node_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.of(Kind::Node).filter(|(id, p)| !self.hidden(*id, p)).filter_map(|(_, p)| p.get("node.name").cloned()).collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn devices(&self) -> Vec<Device> {
        // ports by node
        let mut ports: HashMap<u32, Vec<(u32, &Props)>> = HashMap::new();
        for (id, p) in self.of(Kind::Port) {
            if let Some(node) = p.get("node.id").and_then(|n| n.parse().ok()) {
                ports.entry(node).or_default().push((id, p));
            }
        }
        // nodes into boxes: by device, or one box per node
        let mut boxes: HashMap<String, Vec<u32>> = HashMap::new();
        let mut names: HashMap<&str, usize> = HashMap::new();
        for (_, p) in self.of(Kind::Node) {
            *names.entry(p.get("node.name").map_or("", |s| s.as_str())).or_default() += 1;
        }
        // an app that both plays and records (one stream each, as Audacity does) is one device:
        // its streams' process, from the stream or else its client (`pipewire.sec.pid`), and its
        // name (apps that talk to PipeWire through its PulseAudio server may share a process)
        let audio_stream = |p: &Props| p.get("media.class").is_some_and(|c| c == "Stream/Output/Audio" || c == "Stream/Input/Audio");
        let process = |p: &Props| -> Option<String> {
            let client = p.get("client.id").and_then(|c| c.parse::<u32>().ok()).and_then(|c| self.objects.get(&c)).filter(|(k, _)| *k == Kind::Client).map(|(_, cp)| cp);
            let pid = p.get("application.process.id").or_else(|| client.and_then(|c| c.get("application.process.id").or(c.get("pipewire.sec.pid"))))?;
            Some(format!("{pid}|{}", p.get("application.name").map_or("", |s| s)))
        };
        let mut by_pid: HashMap<String, Vec<u32>> = HashMap::new();
        for (node, p) in self.of(Kind::Node) {
            if ports.contains_key(&node) && !self.hidden(node, p) && audio_stream(p)
                && let Some(pid) = process(p)
            {
                by_pid.entry(pid).or_default().push(node);
            }
        }
        let mut paired: HashMap<u32, String> = HashMap::new();
        let mut app_names: HashMap<String, usize> = HashMap::new();
        // (all of them: Teams plays on five streams and records on one)
        for nodes in by_pid.values() {
            if nodes.len() >= 2 {
                let p = self.props(nodes[0]).expect("node");
                let name = some(p.get("application.name")).or(some(p.get("node.name"))).map_or("?", |s| s.as_str()).to_owned();
                *app_names.entry(name.clone()).or_default() += 1;
                for n in nodes {
                    paired.insert(*n, name.clone());
                }
            }
        }
        for (node, p) in self.of(Kind::Node) {
            if !ports.contains_key(&node) || self.hidden(node, p) {
                continue;
            }
            if let Some(name) = paired.get(&node) {
                let pid = process(p).unwrap_or_default();
                let pid = pid.split('|').next().unwrap_or("");
                let key = if app_names.get(name).copied().unwrap_or(0) > 1 { format!("app:{name}#{pid}") } else { format!("app:{name}") };
                boxes.entry(key).or_default().push(node);
                continue;
            }
            let key = match p.get("device.id").and_then(|d| d.parse::<u32>().ok()).and_then(|d| self.props(d)) {
                Some(dp) => format!("device:{}", dp.get("device.name").map_or("?", |s| s)),
                None => {
                    // nodes of the same name stay apart (several streams of one app)
                    let name = p.get("node.name").map_or("?", |s| s.as_str());
                    if names.get(name).copied().unwrap_or(0) > 1 {
                        format!("node:{name}#{}", p.get("object.serial").map_or(node.to_string(), |s| s.clone()))
                    } else {
                        format!("node:{name}")
                    }
                }
            };
            boxes.entry(key).or_default().push(node);
        }
        // sound cards show even when their profile has no inputs or outputs (Off, or an HDMI
        // output nobody picked), so they can be switched on
        let card_of = |key: &str| self.of(Kind::Device).find(|(_, p)| key.strip_prefix("device:") == p.get("device.name").map(|s| s.as_str())).map(|(id, _)| id);
        for (id, p) in self.of(Kind::Device) {
            if self.profiles.get(&id).is_some_and(|(l, _)| !l.is_empty()) && p.get("media.class").is_some_and(|c| c == "Audio/Device") {
                boxes.entry(format!("device:{}", p.get("device.name").map_or("?", |s| s))).or_default();
            }
        }

        let mut out: Vec<Device> = boxes
            .into_iter()
            .map(|(key, mut nodes)| {
                // sinks first, then sources, then the rest, so groups read PLAYBACK, CAPTURE, MONITOR
                let rank = |n: &u32| {
                    let c = self.props(*n).and_then(|p| p.get("media.class")).map_or("", |s| s);
                    (if c.ends_with("Sink") { 0 } else if c.ends_with("Source") { 1 } else { 2 }, *n)
                };
                nodes.sort_by_key(rank);
                let card_id = card_of(&key);
                let none = Props::new();
                // a card without nodes (its profile has none) speaks for itself
                let first = nodes.first().and_then(|n| self.props(*n)).unwrap_or(&none);
                let dev = first.get("device.id").and_then(|d| d.parse::<u32>().ok()).or(card_id).and_then(|d| self.props(d));

                // apps: the program's name first; a stream's own names are often generic ("audio stream")
                let app = first.get("media.class").is_some_and(|c| c.starts_with("Stream/"));
                let name = dev
                    .and_then(|d| some(d.get("device.description")).or(some(d.get("device.nick"))))
                    .or(if app { some(first.get("application.name")) } else { None })
                    .or(some(first.get("node.description")))
                    .or(some(first.get("application.name")))
                    .or(some(first.get("node.name")))
                    .or(some(first.get("media.name")))
                    .cloned()
                    .map(|n| plain_name(&n))
                    .unwrap_or_else(|| "?".into());
                let state = nodes
                    .iter()
                    .filter_map(|n| self.state.get(n).copied())
                    .min_by_key(|s| match *s {
                        "running" => 0,
                        "idle" => 1,
                        _ => 2,
                    })
                    .unwrap_or("suspended");
                let class = first.get("media.class").map_or("", |s| s.as_str());
                let mut groups: Vec<Group> = nodes.iter().flat_map(|n| self.groups(*n, ports.get(n).map_or(&[][..], |v| v))).collect();
                // an app's monitors (a copy of what each of its recording streams records) stay off
                // its face: nobody patches from them (the HIDDEN list names them under SYSTEM)
                let app = key.starts_with("app:") || class.starts_with("Stream/");
                if app {
                    groups.retain(|g| g.label != "MONITOR");
                }
                // an app's several streams of one kind: numbered, in the order they came
                if key.starts_with("app:") {
                    number_streams(&mut groups);
                }
                // inputs, then outputs, then monitors: PLAYBACK, CAPTURE, MONITOR
                groups.sort_by_key(|g| (g.dir == Dir::Out, g.label == "MONITOR"));
                let mut h = DefaultHasher::new();
                key.hash(&mut h);
                Device {
                    model: if key.starts_with("app:") {
                        match (groups.iter().any(|g| g.dir == Dir::In), groups.iter().any(|g| g.dir == Dir::Out && g.label != "MONITOR")) {
                            (true, true) => "APP · RECORD & PLAY".into(),
                            (true, false) => "APP · RECORDING".into(),
                            _ => "APP · PLAYBACK".into(),
                        }
                    } else {
                        model(dev, first)
                    },
                    finish: finish(dev.is_some(), class),
                    color: PALETTE[(h.finish() % PALETTE.len() as u64) as usize],
                    key,
                    name,
                    groups,
                    state,
                    card: card_id.and_then(|id| {
                        let (profiles, current) = self.profiles.get(&id)?;
                        (!profiles.is_empty()).then(|| Card { id, profiles: profiles.clone(), current: *current })
                    }),
                }
            })
            .collect();
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

}

/// An app's groups of the same label and direction (one per stream), each numbered: OUTPUT 1,
/// OUTPUT 2… Each keeps its own jacks, controls and meter.
fn number_streams(groups: &mut [Group]) {
    let kinds: Vec<(String, Dir)> = groups.iter().map(|g| (g.label.clone(), g.dir)).collect();
    let mut seen: HashMap<(String, Dir), usize> = HashMap::new();
    for (g, k) in groups.iter_mut().zip(&kinds) {
        if kinds.iter().filter(|o| *o == k).count() < 2 {
            continue;
        }
        let n = seen.entry(k.clone()).or_insert(0);
        *n += 1;
        g.stream = *n;
        g.label = format!("{} {n}", k.0);
    }
}

/// A name without the wrapper PipeWire's ALSA plugin puts around the programs that use it:
/// "ALSA plug-in [soundcraft]" is "soundcraft".
fn plain_name(name: &str) -> String {
    name.strip_prefix("ALSA plug-in [").and_then(|n| n.strip_suffix(']')).filter(|n| !n.is_empty()).unwrap_or(name).to_owned()
}

/// The value, unless it is blank.
fn some(v: Option<&String>) -> Option<&String> {
    v.filter(|s| !s.trim().is_empty())
}

impl Graph {
    /// Nodes Patchcord doesn't show: other programs' peak meters (`stream.monitor`), its own
    /// meters, and streams whose full properties haven't arrived yet (to know which they are).
    fn hidden(&self, node: u32, p: &Props) -> bool {
        let class = p.get("media.class").map_or("", |s| s.as_str());
        p.get("stream.monitor").is_some_and(|m| m == "true")
            || p.get("node.name").is_some_and(|n| n.starts_with(METER_PREFIX) || n.starts_with(CUE_NODE))
            // a Bluetooth headset's real microphone and WirePlumber's internal capture stream:
            // inside its loopback the sound reaches the microphone apps see (MIC) without a link,
            // so a cord to them would lead nowhere
            || class.ends_with("/Internal")
            || (class.starts_with("Stream/") && (!self.info.contains(&node) || self.born.get(&node).is_some_and(|t| t.elapsed() < YOUNG)))
    }

    /// A card's output or input node: its port in use (route), if the card has one for it.
    fn route_of(&self, np: &Props, dir: Dir) -> Option<(RouteRef, Route)> {
        let card: u32 = np.get("device.id")?.parse().ok()?;
        let device: i32 = np.get("card.profile.device")?.parse().ok()?;
        let (_, active) = self.routes.get(&card)?;
        let r = active.iter().find(|r| r.device == Some(device) && r.output == (dir == Dir::In))?;
        Some((RouteRef { card, index: r.index, device, latency_ns: r.latency_ns }, r.clone()))
    }

    /// The ports a card's output or input can use now: of its direction, for its device, in the
    /// card's profile.
    fn ports_to(&self, r: RouteRef, dir: Dir) -> Vec<PortChoice> {
        let profile = self.profiles.get(&r.card).and_then(|(_, cur)| *cur).map(|p| p as i32);
        let Some((all, _)) = self.routes.get(&r.card) else { return Vec::new() };
        all.iter()
            .filter(|x| x.output == (dir == Dir::In) && x.devices.contains(&r.device) && profile.is_none_or(|p| x.profiles.contains(&p)))
            .map(|x| PortChoice { index: x.index, name: x.name.clone(), description: x.description.clone(), unplugged: x.available == Avail::No })
            .collect()
    }

    /// A card's ports for its output or input `np` when none is in use: of its direction, for its
    /// device.
    fn ports_of(&self, np: &Props, dir: Dir) -> Vec<PortChoice> {
        let (Some(card), Some(device)) = (np.get("device.id").and_then(|v| v.parse::<u32>().ok()), np.get("card.profile.device").and_then(|v| v.parse::<i32>().ok())) else { return Vec::new() };
        let Some((all, _)) = self.routes.get(&card) else { return Vec::new() };
        all.iter()
            .filter(|x| x.output == (dir == Dir::In) && x.devices.contains(&device))
            .map(|x| PortChoice { index: x.index, name: x.name.clone(), description: x.description.clone(), unplugged: x.available == Avail::No })
            .collect()
    }

    /// Nodes still too young to show.
    pub fn young(&self) -> usize {
        self.born.values().filter(|t| t.elapsed() < YOUNG).count()
    }

    /// One node's ports, as groups: inputs, outputs and monitor outputs apart.
    fn groups(&self, node: u32, ports: &[(u32, &Props)]) -> Vec<Group> {
        let np = self.props(node).expect("node");
        let class = np.get("media.class").map_or("", |s| s.as_str());
        let mut list: Vec<Port> = ports.iter().map(|(id, p)| port(*id, node, p)).collect();
        // PipeWire numbers a node's ports per direction in `port.id`
        let index = |id: u32| self.props(id).and_then(|p| p.get("port.id")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(id);
        list.sort_by_key(|p| (p.dir == Dir::Out, index(p.id)));
        let monitor = |p: &Port| self.props(p.id).and_then(|pp| pp.get("port.monitor")).is_some_and(|m| m == "true");

        let mut groups: Vec<Group> = Vec::new();
        for (dir, mon) in [(Dir::In, false), (Dir::Out, false), (Dir::Out, true)] {
            let ps: Vec<Port> = list.iter().filter(|p| p.dir == dir && monitor(p) == mon).cloned().collect();
            if ps.is_empty() {
                continue;
            }
            let label = if mon {
                "MONITOR"
            } else if ps[0].media == Media::Midi {
                "MIDI"
            } else if ps[0].media == Media::Video {
                "VIDEO"
            } else {
                match (class, dir) {
                    (c, Dir::In) if c.ends_with("Sink") => "PLAYBACK",
                    (c, Dir::Out) if c.ends_with("Source") => "CAPTURE",
                    (_, Dir::In) => "INPUT",
                    (_, Dir::Out) => "OUTPUT",
                }
            };
            let stereo = ps.len() == 2 && ps[0].media == Media::Audio && ps[0].channel == "FL" && ps[1].channel == "FR";
            // not Bluetooth's internal capture stream, either way: a meter on what it records links
            // to the headset's microphone, which keeps it open
            let meter = ps[0].media == Media::Audio && !class.ends_with("/Internal");
            // a volume goes with its own ports: what an app plays or records, what a device's output
            // plays (its PLAYBACK) or its input takes in (its CAPTURE)
            let audio = ps[0].media == Media::Audio && !mon;
            let stream = (dir == Dir::Out && class == "Stream/Output/Audio") || (dir == Dir::In && class == "Stream/Input/Audio");
            let device = (dir == Dir::In && class == "Audio/Sink") || (dir == Dir::Out && class == "Audio/Source");
            let gain = dir == Dir::In && stream || dir == Dir::Out && device;
            // a card keeps it on its port in use; a virtual device (and an app) on the node
            let route = if audio && device { self.route_of(np, dir) } else { None };
            let mix = if !audio || !(stream || device) {
                None
            } else if let Some((r, active)) = &route {
                active.volumes.clone().map(|volumes| Mix { node, volumes, mute: active.mute, gain, route: Some(*r) })
            } else {
                self.mix.get(&node).map(|(volumes, mute)| Mix { node, volumes: volumes.clone(), mute: *mute, gain, route: None })
            };
            // a card's ports for it; with none in use (HDMI with nothing plugged in), all it has
            let ports_to = match &route {
                Some((r, _)) => self.ports_to(*r, dir),
                None if audio && device => self.ports_of(np, dir),
                None => Vec::new(),
            };
            groups.push(Group { label: label.into(), dir, ports: ps, stereo, meter, mix, ports_to, stream: 0 });
        }
        groups
    }
}

fn port(id: u32, node: u32, p: &Props) -> Port {
    let name = p.get("port.name").cloned().unwrap_or_default();
    let dsp = p.get("format.dsp").map_or("", |s| s.as_str());
    let media = if dsp.contains("midi") {
        Media::Midi
    } else if dsp.contains("audio") {
        Media::Audio
    } else if dsp.contains("video") {
        Media::Video
    } else {
        Media::Other
    };
    let dir = if p.get("port.direction").is_some_and(|d| d == "out") { Dir::Out } else { Dir::In };
    let channel = p.get("audio.channel").cloned().unwrap_or_default();
    let label = match channel.as_str() {
        "FL" => "L".into(),
        "FR" => "R".into(),
        "" => short_label(&name, dir),
        c => c.into(),
    };
    Port { id, node, name, label, dir, media, channel }
}

/// "Midi Through: Port-0 (capture)" → "PORT-0"; short names stay as they are.
fn short_label(name: &str, dir: Dir) -> String {
    let base = name.trim_end_matches(" (capture)").trim_end_matches(" (playback)");
    let base = base.rsplit(": ").next().unwrap_or(base);
    let l = base.to_uppercase();
    if l.is_empty() {
        return if dir == Dir::In { "IN".into() } else { "OUT".into() };
    }
    l.chars().take(7).collect()
}

/// The small line under the device name.
fn model(dev: Option<&Props>, node: &Props) -> String {
    let class = node.get("media.class").map_or("", |s| s.as_str());
    if let Some(d) = dev {
        let api = d.get("device.api").map_or("", |s| s.as_str());
        let mut parts: Vec<String> = Vec::new();
        match api {
            "bluez5" => parts.push("BLUETOOTH".into()),
            "" => {}
            a => parts.push(a.to_uppercase()),
        }
        if api != "bluez5"
            && let Some(bus) = d.get("device.bus")
        {
            parts.push(bus.to_uppercase());
        }
        let line = parts.join(" · ");
        return match d.get("device.form-factor") {
            Some(f) if f != "internal" => format!("{line} {}", f.to_uppercase()),
            _ => line,
        };
    }
    match class {
        c if c.contains("Internal") => "INTERNAL STREAM".into(),
        c if c.starts_with("Stream/Output") => "APP · PLAYBACK".into(),
        c if c.starts_with("Stream/Input") => "APP · RECORDING".into(),
        c if c.starts_with("Midi") => "MIDI".into(),
        c if c.starts_with("Video") => "VIDEO".into(),
        c if c.starts_with("Audio") => "VIRTUAL DEVICE".into(),
        c => c.to_uppercase(),
    }
}

fn finish(hardware: bool, class: &str) -> Finish {
    if hardware {
        Finish::Metal
    } else if class.starts_with("Midi") {
        Finish::Navy
    } else if class.contains("Internal") || class.starts_with("Audio") {
        // internal streams and virtual devices (filters, effects)
        Finish::Cream
    } else {
        Finish::Black
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_and_balance_read_as_in_pavucontrol() {
        let m = Mix { node: 1, volumes: vec![0.85f32.powi(3), (0.85f32 * 0.6).powi(3)], mute: false, gain: false, route: None };
        assert!((m.volume() - 0.85).abs() < 1e-4);
        assert!((m.balance() + 0.4).abs() < 1e-4, "{}", m.balance());
        let v = m.volumes_for(m.volume(), m.balance());
        assert!(v.iter().zip(&m.volumes).all(|(a, b)| (a - b).abs() < 1e-5), "{v:?}");
        assert_eq!(m.volumes_for(1.0, 0.5), vec![0.125, 1.0]);
        assert_eq!(volume_text(1.0), ("100%".into(), "0.00 dB".into()));
        assert_eq!(volume_text(0.85).1, "-4.23 dB");
        assert_eq!(volume_text(0.0).1, "-∞ dB");
    }

    #[test]
    fn a_sound_is_followed_through_a_card_and_out_of_its_monitor() {
        // app (10) → card A (20) playback; A's monitor → card B (30) playback
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| -> Props { kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        for (id, node, dir) in [(11, 10, "out"), (21, 20, "in"), (22, 20, "out"), (31, 30, "in")] {
            g.apply(&Event::Added { id, kind: Kind::Port, props: props(&[("node.id", &node.to_string()), ("port.direction", dir)]) });
        }
        for (id, o, i) in [(40, 11, 21), (41, 22, 31)] {
            g.apply(&Event::Added { id, kind: Kind::Link, props: props(&[("link.output.port", &o.to_string()), ("link.input.port", &i.to_string())]) });
        }
        let r = g.reaches(11);
        assert!(r.contains(&20) && r.contains(&30), "{r:?}");
        assert!(g.reaches(31).is_empty());
    }

    #[test]
    fn an_app_with_many_streams_is_one_device_with_a_jack_for_each() {
        // Teams: five playback streams and one recording stream, all named alike, one process
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| -> Props { kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        for (port_id, (id, class, dir)) in (101..).zip([(10, "Stream/Input/Audio", "in"), (11, "Stream/Output/Audio", "out"), (12, "Stream/Output/Audio", "out"), (13, "Stream/Output/Audio", "out"), (14, "Stream/Output/Audio", "out"), (15, "Stream/Output/Audio", "out")]) {
            let p = props(&[("media.class", class), ("node.name", "Teams for Linux"), ("application.name", "Teams for Linux"), ("application.process.id", "3013308")]);
            g.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
            g.apply(&Event::Info { id, props: p, state: Some("running") });
            let name = if dir == "in" { "input_MONO" } else { "output_MONO" };
            g.apply(&Event::Added { id: port_id, kind: Kind::Port, props: props(&[("node.id", &id.to_string()), ("port.direction", dir), ("port.name", name), ("format.dsp", "32 bit float mono audio"), ("audio.channel", "MONO")]) });
            g.apply(&Event::Mix { id, volumes: vec![1.0], mute: false });
        }
        // the recording stream's monitor: a copy of what it records
        g.apply(&Event::Added { id: 199, kind: Kind::Port, props: props(&[("node.id", "10"), ("port.direction", "out"), ("port.name", "monitor_MONO"), ("port.monitor", "true"), ("format.dsp", "32 bit float mono audio"), ("audio.channel", "MONO")]) });
        let devs = g.devices();
        assert_eq!(devs.len(), 1, "{:?}", devs.iter().map(|d| &d.key).collect::<Vec<_>>());
        let d = &devs[0];
        assert_eq!(d.model, "APP · RECORD & PLAY");
        // each stream a frame of its own, numbered, with its own volume
        let outs: Vec<(&str, usize, bool)> = d.groups.iter().filter(|g| g.dir == Dir::Out).map(|g| (g.label.as_str(), g.stream, g.mix.is_some())).collect();
        assert_eq!(outs, [("OUTPUT 1", 1, true), ("OUTPUT 2", 2, true), ("OUTPUT 3", 3, true), ("OUTPUT 4", 4, true), ("OUTPUT 5", 5, true)]);
        assert!(d.groups.iter().all(|g| !g.label.starts_with("MONITOR")), "an app's monitors stay off its face");
        assert_eq!(g.system().iter().find(|s| s.1 == "monitors of its recording streams").map(|s| (s.0.as_str(), s.2)), Some(("Teams for Linux", 1)), "and are listed as system");
        let input = d.groups.iter().find(|g| g.dir == Dir::In).expect("an INPUT");
        assert_eq!((input.label.as_str(), input.stream), ("INPUT", 0));
        let jd = crate::ui::face_jacks(d);
        let labels: Vec<&str> = jd.iter().filter(|j| j.0 == Dir::Out).map(|j| j.1.as_str()).collect();
        assert_eq!(labels, ["1", "2", "3", "4", "5"]);
        // balanced: the six take two units of three, in columns; what makes sound first (the
        // outputs, from the top-left), what takes it (the INPUT) at the right end
        let ((_, h), at) = crate::ui::face_layout(d);
        assert_eq!(h, 256.0);
        assert_eq!(at.iter().map(|a| a.1).collect::<Vec<_>>(), [128.0, 0.0, 0.0, 0.0, 128.0, 128.0]);
        assert_eq!((at[1].0, at[2].0), (at[4].0, at[5].0));
        // (the INPUT in OUTPUT 3's column, against its right edge: it is a little narrower)
        assert!(at[0].0 >= at[3].0 && at[0].0 < at[3].0 + 40.0);
    }

    /// Teams in a call records on two streams and plays on six: eight frames, two units of four,
    /// the outputs first (they make sound), the inputs at the right, nothing left over.
    #[test]
    fn eight_frames_make_two_full_units() {
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| -> Props { kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        for id in 10..18u32 {
            let (class, dir) = if id < 12 { ("Stream/Input/Audio", "in") } else { ("Stream/Output/Audio", "out") };
            let p = props(&[("media.class", class), ("node.name", "Teams for Linux"), ("application.name", "Teams for Linux"), ("application.process.id", "7")]);
            g.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
            g.apply(&Event::Info { id, props: p, state: Some("running") });
            let name = if dir == "in" { "input_MONO" } else { "output_MONO" };
            g.apply(&Event::Added { id: 100 + id, kind: Kind::Port, props: props(&[("node.id", &id.to_string()), ("port.direction", dir), ("port.name", name), ("format.dsp", "32 bit float mono audio"), ("audio.channel", "MONO")]) });
            g.apply(&Event::Mix { id, volumes: vec![1.0], mute: false });
        }
        let d = &g.devices()[0];
        let ((_, h), at) = crate::ui::face_layout(d);
        assert_eq!(h, 256.0);
        let row = |u: f64| d.groups.iter().zip(&at).filter(|(_, a)| a.1 == u).map(|(g, _)| g.label.as_str()).collect::<Vec<_>>();
        assert_eq!(row(0.0), ["OUTPUT 1", "OUTPUT 2", "OUTPUT 3", "OUTPUT 4"]);
        assert_eq!(row(128.0), ["INPUT 1", "INPUT 2", "OUTPUT 5", "OUTPUT 6"]);
        // sound flows left to right: the outputs first, the inputs at the right
        let x = |l: &str| d.groups.iter().zip(&at).find(|(g, _)| g.label == l).map(|(_, a)| a.0).unwrap();
        assert!(x("OUTPUT 6") < x("INPUT 1") && x("INPUT 1") < x("INPUT 2"));
    }

    #[test]
    fn an_alsa_app_is_called_by_its_own_name() {
        assert_eq!(plain_name("ALSA plug-in [soundcraft]"), "soundcraft");
        assert_eq!(plain_name("Firefox"), "Firefox");
        assert_eq!(plain_name("ALSA plug-in []"), "ALSA plug-in []");
    }

    #[test]
    fn an_app_that_records_and_plays_is_one_device_with_two_sets_of_controls() {
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| -> Props { kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        let node = |g: &mut Graph, id: u32, class: &str| {
            let p = props(&[("media.class", class), ("node.name", "ALSA plug-in [audacity]"), ("application.name", "Audacity"), ("application.process.id", "4242")]);
            g.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
            g.apply(&Event::Info { id, props: p, state: Some("running") });
        };
        let port = |g: &mut Graph, id: u32, node: u32, dir: &str, name: &str, ch: &str| {
            let p = props(&[("node.id", &node.to_string()), ("port.direction", dir), ("port.name", name), ("format.dsp", "32 bit float mono audio"), ("audio.channel", ch)]);
            g.apply(&Event::Added { id, kind: Kind::Port, props: p });
        };
        node(&mut g, 10, "Stream/Output/Audio");
        node(&mut g, 20, "Stream/Input/Audio");
        port(&mut g, 11, 10, "out", "output_FL", "FL");
        port(&mut g, 12, 10, "out", "output_FR", "FR");
        port(&mut g, 21, 20, "in", "input_FL", "FL");
        port(&mut g, 22, 20, "in", "input_FR", "FR");
        g.apply(&Event::Mix { id: 10, volumes: vec![1.0, 1.0], mute: false });
        g.apply(&Event::Mix { id: 20, volumes: vec![0.343, 0.343], mute: true });
        let devs = g.devices();
        assert_eq!(devs.len(), 1, "{:?}", devs.iter().map(|d| &d.key).collect::<Vec<_>>());
        let d = &devs[0];
        assert_eq!((d.key.as_str(), d.name.as_str(), d.model.as_str()), ("app:Audacity", "Audacity", "APP · RECORD & PLAY"));
        let groups: Vec<(&str, Option<u32>, Option<bool>)> = d.groups.iter().map(|g| (g.label.as_str(), g.mix.as_ref().map(|m| m.node), g.mix.as_ref().map(|m| m.mute))).collect();
        assert_eq!(groups, vec![("INPUT", Some(20), Some(true)), ("OUTPUT", Some(10), Some(false))]);
        assert!((d.groups[0].mix.as_ref().unwrap().volume() - 0.7).abs() < 1e-3);
    }

    fn card(profiles: &[(u32, &str, Avail, i32)], current: u32) -> Card {
        Card {
            id: 1,
            profiles: profiles.iter().map(|&(index, d, available, priority)| Profile { index, name: String::new(), description: d.into(), available, priority }).collect(),
            current: Some(current),
        }
    }

    /// Bluetooth profiles that differ only in their codec are one family with codecs, picked by
    /// codec; the LCD says the family and the codec.
    #[test]
    fn codecs_are_choices_of_one_profile() {
        let c = card(
            &[
                (0, "Off", Avail::Yes, 0),
                (1, "High Fidelity Playback (A2DP Sink, codec SBC)", Avail::Yes, 132),
                (2, "High Fidelity Playback (A2DP Sink, codec AAC)", Avail::Yes, 133),
                (3, "Headset Head Unit (HSP/HFP, codec CVSD)", Avail::Yes, 5),
                (4, "Headset Head Unit (HSP/HFP, codec mSBC)", Avail::Yes, 6),
            ],
            1,
        );
        let f = c.families();
        assert_eq!(f.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Off", "High Fidelity Playback (A2DP Sink)", "Headset Head Unit (HSP/HFP)"]);
        assert_eq!(f[1].members, [(1, Some("SBC".into())), (2, Some("AAC".into()))]);
        assert_eq!(c.summary(), "High Fidelity Playback (A2DP Sink) · SBC");
        // the row in use keeps its codec; another row takes its best one
        assert_eq!(c.pick(&f[1]), Some(1));
        assert_eq!(c.pick(&f[2]), Some(4));
        assert_eq!(c.pick(&f[0]), Some(0));
    }

    /// A profile with nothing plugged in is marked so, in the list and on the LCD.
    #[test]
    fn an_unplugged_profile_says_so() {
        let c = card(&[(0, "Off", Avail::Yes, 0), (1, "Digital Stereo (HDMI) Output", Avail::No, 5900)], 1);
        let f = c.families();
        assert!(!f[0].unplugged && f[1].unplugged);
        assert_eq!(c.summary(), "Digital Stereo (HDMI) Output · UNPLUGGED");
    }
}
