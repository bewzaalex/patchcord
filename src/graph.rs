//! The PipeWire graph as Patchcord shows it: raw objects in, panel devices out.
//!
//! A device on the panel is one card (all nodes sharing a `device.id`: sink, source and their
//! monitors), one program (all the nodes it made: streams, its own sinks and sources, effects),
//! or one node of the system's (a MIDI bridge, a loopback). Its ports are sorted into labelled
//! groups such as PLAYBACK, CAPTURE or MONITOR.

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
    /// its frame's name: what it is (`role`), or what its program named its node
    /// (`node.description`)
    pub label: String,
    /// on a MIDI bridge, the ALSA client its ports belong to (MIDI THROUGH, HYDROGEN): printed
    /// under its jacks in the bridge's one frame
    pub client: Option<String>,
    /// its node's `node.nick`, PipeWire's short name for it (HDMI 1, Digital Microphone, a name
    /// given in KDE's settings; on a desktop card the codec's, ALC287 Analog): a card's frame's
    /// name when its port doesn't tell it from the others (`face::name`, ADR 0020)
    pub nick: Option<String>,
    /// what its ports are for, whatever its name: PLAYBACK, CAPTURE, MONITOR, INPUT, OUTPUT, MIDI,
    /// VIDEO (`port_role`): its jacks' colour
    pub role: &'static str,
    /// the kind of block its node is (`block_kind`): its frame's colour
    pub kind: &'static str,
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
/// A device's paint: who owns it (ADR 0014). Hardware (a node with a `device.id`) is aluminium, a
/// program black, a node of PipeWire's or WirePlumber's own olive.
pub enum Finish {
    Aluminium,
    Black,
    Olive,
}

#[derive(Clone, Debug)]
pub struct Device {
    /// stays the same across restarts (`device.name` or `node.name`), for layouts and presets
    pub key: String,
    /// the full name, as PipeWire shows it
    pub name: String,
    /// how it is made and connected (ALSA · USB, APP · PLAYBACK): the status bar, the HIDDEN list
    pub model: String,
    /// what it is, in a word or two (SOUND CARD, HEADSET, VIRTUAL SINK, APP): its screen's top line
    pub kind: &'static str,
    pub finish: Finish,
    /// colour of the cords that leave this device
    pub color: [u8; 3],
    pub groups: Vec<Group>,
    /// "running", "idle" or "suspended": the busiest of its nodes
    pub state: &'static str,
    /// a program's several nodes, in the order its sound goes through them, each its inputs then
    /// its outputs: the face keeps its frames in this order (not all inputs at the left)
    pub chain: bool,
    /// what it takes of PipeWire: its nodes, and all their ports
    pub nodes: usize,
    pub ports: usize,
    /// a sound card's profiles, when it has some
    pub card: Option<Card>,
}

/// The highest volume a knob or fader goes to: 150% (about +10.6 dB), as KDE's raised maximum; a
/// round number on the scale people see (pavucontrol's +11 dB is 153%).
pub const VOL_MAX: f64 = 1.5;

/// The knob's zero: -120 dB, not silence, so a meter can still show what plays at it (the knob
/// reads it, or less, as 0).
pub const VOL_FLOOR: f32 = 1e-6;

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
        self.volumes.iter().map(|v| if *v <= VOL_FLOOR * 1.01 { 0.0 } else { (*v as f64).cbrt() }).collect()
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
        // never quite silent: the meters undo the volume to show what plays, which a 0 would lose
        let lin = |x: f64| (x.powi(3) as f32).max(VOL_FLOOR);
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
    /// why a node or a link is in error, as PipeWire says it
    failed: HashMap<u32, String>,
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

/// The cords' colours, as patch cables come, all of them colourful: red, orange, yellow, green,
/// cyan, blue, purple, pink.
pub const PALETTE: [[u8; 3]; 8] = [
    [0xd4, 0x23, 0x2a],
    [0xec, 0x7a, 0x1c],
    [0xf0, 0xcc, 0x1c],
    [0x14, 0x96, 0x5a],
    [0x12, 0xa5, 0xb8],
    [0x17, 0x52, 0xc8],
    [0x7b, 0x4f, 0xb8],
    [0xd6, 0x3c, 0x8c],
];

impl Graph {
    /// Applies an event; true when what the panel shows may have changed.
    pub fn apply(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Added { id, kind, props } => {
                if *kind == Kind::Node {
                    // what was there before we connected is old enough already
                    let now = Instant::now();
                    let t = if self.synced { now } else { now.checked_sub(YOUNG).unwrap_or(now) };
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
                self.failed.remove(id);
                self.objects.remove(id).is_some()
            }
            Event::Info { id, props, state } => {
                let Some((_, p)) = self.objects.get_mut(id) else { return false };
                let before = (p.get("media.class").cloned(), p.get("node.description").cloned(), p.get("stream.monitor").cloned());
                p.extend(props.iter().map(|(k, v)| (k.clone(), v.clone())));
                let new = self.info.insert(*id);
                let changed_state = state.is_some_and(|s| self.state.insert(*id, s) != Some(s));
                if state.is_some_and(|s| s != "error") {
                    self.failed.remove(id);
                }
                let after = (p.get("media.class").cloned(), p.get("node.description").cloned(), p.get("stream.monitor").cloned());
                new || changed_state || before != after
            }
            Event::Failed { id, why } => self.failed.insert(*id, why.clone()).as_deref() != Some(why.as_str()),
            Event::Levels(_) | Event::Setting { .. } | Event::Version(_) | Event::Meta { .. } | Event::Profile(_) | Event::Param { .. } => false,
            Event::Synced => {
                self.synced = true;
                true
            }
            Event::Error(_) => false,
        }
    }

    /// What a report keeps of the state PipeWire gives as params, not properties: every node's
    /// volume and mute, each card's profiles (`*` the one it is in) and ports (`*` the ones in
    /// use, with the card's volume, mute and latency offset). Plain text, by id.
    pub fn state_text(&self) -> String {
        use std::fmt::Write;
        let pct = |vs: &[f32]| vs.iter().map(|v| volume_text(if *v <= VOL_FLOOR * 1.01 { 0.0 } else { (*v as f64).cbrt() }).0).collect::<Vec<_>>().join(" ");
        let yes = |b: bool| if b { "yes" } else { "no" };
        let avail = |a: Avail| match a {
            Avail::Yes => "yes",
            Avail::No => "no",
            Avail::Unknown => "?",
        };
        let mut o = String::new();
        let mut nodes: Vec<u32> = self.mix.keys().copied().collect();
        nodes.sort_unstable();
        for id in nodes {
            let (vs, mute) = &self.mix[&id];
            let _ = writeln!(o, "node {id} volume {} · mute {}", pct(vs), yes(*mute));
        }
        let mut cards: Vec<u32> = self.profiles.keys().chain(self.routes.keys()).copied().collect::<HashSet<_>>().into_iter().collect();
        cards.sort_unstable();
        for id in cards {
            let _ = writeln!(o, "device {id}");
            if let Some((list, current)) = self.profiles.get(&id) {
                for p in list {
                    let _ = writeln!(o, "    profile {}{} {} \"{}\" · available {} · priority {}", if Some(p.index) == *current { "*" } else { " " }, p.index, p.name, p.description, avail(p.available), p.priority);
                }
            }
            if let Some((all, active)) = self.routes.get(&id) {
                for r in all {
                    let used = active.iter().find(|a| a.index == r.index && a.output == r.output);
                    let _ = write!(
                        o,
                        "    port {}{} {} {} \"{}\" · available {} · profiles {:?} · devices {:?}",
                        if used.is_some() { "*" } else { " " },
                        r.index,
                        if r.output { "out" } else { "in " },
                        r.name,
                        r.description,
                        avail(r.available),
                        r.profiles,
                        r.devices
                    );
                    if let Some(a) = used {
                        let _ = write!(o, " · in use for device {} · volume {} · mute {} · latency {} ms", a.device.unwrap_or(-1), a.volumes.as_deref().map_or("?".into(), pct), yes(a.mute), a.latency_ns as f64 / 1e6);
                    }
                    o.push('\n');
                }
            }
        }
        o
    }

    /// Every object PipeWire listed, for ENGINE: (id, kind, properties).
    pub fn objects(&self) -> impl Iterator<Item = (u32, Kind, &Props)> {
        self.objects.iter().map(|(id, (k, p))| (*id, *k, p))
    }

    /// A node's state (running, idle, suspended…) or a link's (active, paused…).
    pub fn state_of(&self, id: u32) -> Option<&'static str> {
        self.state.get(&id).copied()
    }

    /// Why a node or a link is in error, as PipeWire says it.
    pub fn failed(&self, id: u32) -> Option<&str> {
        self.failed.get(&id).map(String::as_str)
    }

    /// An object's properties.
    pub fn props_of(&self, id: u32) -> Option<&Props> {
        self.props(id)
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
        m.into_iter().map(|((who, what), n)| (who, what, n)).collect()
    }

    /// A node's `node.name`, as PipeWire's defaults name it.
    pub fn node_name(&self, node: u32) -> Option<&str> {
        self.props(node)?.get("node.name").map(String::as_str)
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

    /// How many links there are (as `links`, without listing them).
    pub fn link_count(&self) -> usize {
        self.of(Kind::Link).filter(|(_, p)| p.contains_key("link.output.port") && p.contains_key("link.input.port")).count()
    }



    /// A node's monitor ports (a copy of what comes in, before its volume), with their channels.
    pub fn monitor_ports(&self, node: u32) -> Vec<(u32, String)> {
        let mut v: Vec<(u32, String)> = self
            .of(Kind::Port)
            .filter(|(_, p)| p.get("port.monitor").is_some_and(|m| m == "true") && p.get("node.id").and_then(|n| n.parse::<u32>().ok()) == Some(node))
            .map(|(id, p)| (id, p.get("audio.channel").cloned().unwrap_or_default()))
            .collect();
        v.sort();
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
        // a program's nodes, all of them, whatever they are (its streams, its own sinks and
        // sources, a JACK-style node, effects without a class): one device when it has several
        let mut progs: HashMap<String, Vec<u32>> = HashMap::new();
        for (node, p) in self.of(Kind::Node) {
            if ports.contains_key(&node)
                && !self.hidden(node, p)
                && let Some((who, _)) = self.program(p)
            {
                progs.entry(who).or_default().push(node);
            }
        }
        let mut paired: HashMap<u32, (String, String)> = HashMap::new();
        let mut app_names: HashMap<String, usize> = HashMap::new();
        for (who, nodes) in &progs {
            if nodes.len() >= 2 {
                let name = self.props(nodes[0]).and_then(|p| self.program(p)).map(|(_, n)| n).unwrap_or_default();
                *app_names.entry(name.clone()).or_default() += 1;
                for n in nodes {
                    paired.insert(*n, (name.clone(), who.rsplit('|').next().unwrap_or("").to_owned()));
                }
            }
        }
        for (node, p) in self.of(Kind::Node) {
            if !ports.contains_key(&node) || self.hidden(node, p) {
                continue;
            }
            // (two of one name apart by their process)
            if let Some((name, pid)) = paired.get(&node) {
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
                        format!("node:{name}#{}", p.get("object.serial").map_or(node.to_string(), Clone::clone))
                    } else {
                        format!("node:{name}")
                    }
                }
            };
            boxes.entry(key).or_default().push(node);
        }
        // sound cards show even when their profile has no inputs or outputs (Off, or an HDMI
        // output nobody picked), so they can be switched on
        let card_of = |key: &str| self.of(Kind::Device).find(|(_, p)| key.strip_prefix("device:") == p.get("device.name").map(String::as_str)).map(|(id, _)| id);
        for (id, p) in self.of(Kind::Device) {
            if self.profiles.get(&id).is_some_and(|(l, _)| !l.is_empty()) && p.get("media.class").is_some_and(|c| c == "Audio/Device") {
                boxes.entry(format!("device:{}", p.get("device.name").map_or("?", |s| s))).or_default();
            }
        }

        // each port's peers: the nodes at the other ends of its links
        let mut peers: HashMap<u32, Vec<u32>> = HashMap::new();
        for l in self.links() {
            if let (Some(a), Some(b)) = (self.port_node(l.out_port), self.port_node(l.in_port)) {
                peers.entry(l.out_port).or_default().push(b);
                peers.entry(l.in_port).or_default().push(a);
            }
        }
        let serial = |id: u32| self.props(id).and_then(|p| p.get("object.serial")).and_then(|v| v.parse::<u64>().ok()).unwrap_or(u64::MAX);
        let mut out: Vec<(Device, u64)> = boxes
            .into_iter()
            .map(|(key, mut nodes)| {
                let none = Props::new();
                let program = nodes.first().and_then(|n| self.props(*n)).and_then(|p| self.program(p)).map(|(_, name)| name);
                // its nodes linked among themselves: a chain the face follows
                let mut chained = false;
                if program.is_some() {
                    // a program's nodes in the order it made them
                    nodes.sort_by_key(|n| (serial(*n), *n));
                    if nodes.len() > 1 {
                        (nodes, chained) = chain_order(&nodes, |port| peers.get(&port).map_or(&[][..], Vec::as_slice), |n| ports.get(&n).map_or(&[][..], Vec::as_slice));
                    }
                } else {
                    // sinks first, then sources, then the rest, so groups read PLAYBACK, CAPTURE, MONITOR
                    nodes.sort_by_key(|n| {
                        let c = self.props(*n).and_then(|p| p.get("media.class")).map_or("", |s| s);
                        (if c.ends_with("Sink") { 0 } else if c.ends_with("Source") { 1 } else { 2 }, *n)
                    });
                }
                let card_id = card_of(&key);
                // a card without nodes (its profile has none) speaks for itself
                let first = nodes.first().and_then(|n| self.props(*n)).unwrap_or(&none);
                let dev = first.get("device.id").and_then(|d| d.parse::<u32>().ok()).or(card_id).and_then(|d| self.props(d));

                // a program by its own name; a stream's own names are often generic ("audio stream")
                let name = program.clone().unwrap_or_else(|| {
                    dev.and_then(|d| some(d.get("device.description")).or(some(d.get("device.nick"))))
                        .or(some(first.get("node.description")))
                        .or(some(first.get("application.name")))
                        .or(some(first.get("node.name")))
                        .or(some(first.get("media.name")))
                        .map(|n| plain_name(n))
                        .unwrap_or_else(|| "?".into())
                });
                let state = nodes
                    .iter()
                    .filter_map(|n| self.state.get(n).copied())
                    .min_by_key(|s| match *s {
                        "running" => 0,
                        "idle" => 1,
                        _ => 2,
                    })
                    .unwrap_or("suspended");
                let several = program.is_some() && nodes.len() > 1;
                let mut groups: Vec<Group> = Vec::new();
                for n in &nodes {
                    let mut gs = self.groups(*n, ports.get(n).map_or(&[][..], |v| v));
                    // one of a program's several nodes: named as the program named it, unless
                    // that is only the program's own name again
                    if several
                        && let Some(d) = self.props(*n).and_then(|p| some(p.get("node.description")))
                        && !d.eq_ignore_ascii_case(&name)
                    {
                        for g in gs.iter_mut().filter(|g| g.label == g.role) {
                            g.label = d.to_uppercase();
                        }
                    }
                    groups.extend(gs);
                }
                // (a stream's monitors are on its frame too: the rack patches whatever PipeWire
                // can link, ADR 0014)
                // a program's several streams of one kind: numbered, in the order they came
                if several {
                    number_streams(&mut groups);
                }
                if chained {
                    // node by node along the program's chains, each its inputs, then its outputs
                    let place: HashMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
                    groups.sort_by_key(|g| (g.ports.first().and_then(|p| place.get(&p.node)).copied().unwrap_or(usize::MAX), g.dir == Dir::Out, g.role == "MONITOR"));
                } else {
                    // inputs, then outputs, then monitors: PLAYBACK, CAPTURE, MONITOR
                    groups.sort_by_key(|g| (g.dir == Dir::Out, g.role == "MONITOR"));
                }
                // what it takes of PipeWire: its nodes and all their ports (monitors too)
                let port_count = nodes.iter().map(|n| ports.get(n).map_or(0, Vec::len)).sum();
                // who came first, among programs of one name: its client's, or its own, serial
                let born = nodes
                    .iter()
                    .map(|n| self.props(*n).and_then(|p| p.get("client.id")).and_then(|c| c.parse::<u32>().ok()).map_or(serial(*n), serial))
                    .min()
                    .unwrap_or(u64::MAX);
                let mut h = DefaultHasher::new();
                key.hash(&mut h);
                let dev = Device {
                    model: if program.is_some() {
                        match (groups.iter().any(|g| g.dir == Dir::In), groups.iter().any(|g| g.dir == Dir::Out && g.role != "MONITOR")) {
                            (true, true) => "APP · RECORD & PLAY".into(),
                            (true, false) => "APP · RECORDING".into(),
                            _ => "APP · PLAYBACK".into(),
                        }
                    } else {
                        model(dev, first)
                    },
                    kind: if program.is_some() { "APP" } else { kind(dev, card_id.and_then(|id| self.profiles.get(&id)).map(|(l, _)| l.as_slice()), first) },
                    finish: if program.is_some() {
                        Finish::Black
                    } else if dev.is_some() {
                        Finish::Aluminium
                    } else {
                        Finish::Olive
                    },
                    color: PALETTE[(h.finish() % PALETTE.len() as u64) as usize],
                    key,
                    name,
                    groups,
                    state,
                    chain: chained,
                    nodes: nodes.len(),
                    ports: port_count,
                    card: card_id.and_then(|id| {
                        let (profiles, current) = self.profiles.get(&id)?;
                        (!profiles.is_empty()).then(|| Card { id, profiles: profiles.clone(), current: *current })
                    }),
                };
                (dev, if program.is_some() { born } else { u64::MAX })
            })
            .collect();
        // programs of one name (two VLCs): the first keeps it, the next are numbered 2, 3…
        out.sort_by_key(|(d, born)| (*born, d.key.clone()));
        let mut seen: HashMap<String, usize> = HashMap::new();
        for (d, born) in &mut out {
            if *born == u64::MAX {
                continue;
            }
            let n = seen.entry(d.name.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                d.name = format!("{} {n}", d.name);
            }
        }
        let mut out: Vec<Device> = out.into_iter().map(|(d, _)| d).collect();
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

}

/// A program's nodes (in the order it made them) in the order its sound goes through them: each
/// chain from where it starts (a node none of the program's own feeds), on along the program's
/// own links, the chains in the order their first nodes were made; nodes in a loop after them,
/// and nodes with no links of the program's own last. `peers`: the nodes at a port's other ends;
/// `ports`: a node's ports. Also whether there are such links at all.
fn chain_order<'a>(nodes: &[u32], peers: impl Fn(u32) -> &'a [u32], ports: impl Fn(u32) -> &'a [(u32, &'a Props)]) -> (Vec<u32>, bool) {
    let place: HashMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let feeds: HashMap<u32, Vec<u32>> = nodes
        .iter()
        .map(|n| {
            let mut to: Vec<u32> = ports(*n)
                .iter()
                .filter(|(_, p)| p.get("port.direction").is_some_and(|d| d == "out"))
                .flat_map(|(id, _)| peers(*id).iter().copied())
                .filter(|m| m != n && place.contains_key(m))
                .collect();
            to.sort_by_key(|m| place[m]);
            to.dedup();
            (*n, to)
        })
        .collect();
    let fed: HashSet<u32> = feeds.values().flatten().copied().collect();
    let mut out: Vec<u32> = Vec::with_capacity(nodes.len());
    let mut seen: HashSet<u32> = HashSet::new();
    fn walk(n: u32, feeds: &HashMap<u32, Vec<u32>>, seen: &mut HashSet<u32>, out: &mut Vec<u32>) {
        if !seen.insert(n) {
            return;
        }
        out.push(n);
        for m in &feeds[&n] {
            walk(*m, feeds, seen, out);
        }
    }
    let alone = |n: &u32| !fed.contains(n) && feeds[n].is_empty();
    for n in nodes.iter().filter(|n| !fed.contains(n) && !alone(n)) {
        walk(*n, &feeds, &mut seen, &mut out);
    }
    for n in nodes.iter().filter(|n| !alone(n)) {
        walk(*n, &feeds, &mut seen, &mut out);
    }
    out.extend(nodes.iter().filter(|n| !seen.contains(n)));
    (out, !fed.is_empty())
}

/// An app's groups of the same label and direction (one per stream), each numbered: OUTPUT 1,
/// OUTPUT 2… Each keeps its own jacks, controls and meter.
/// Groups with names of their own (SPECTRUM, as Easy Effects names two of its nodes) keep them.
fn number_streams(groups: &mut [Group]) {
    let kinds: Vec<(String, Dir)> = groups.iter().map(|g| (g.label.clone(), g.dir)).collect();
    let mut seen: HashMap<(String, Dir), usize> = HashMap::new();
    for (g, k) in groups.iter_mut().zip(&kinds) {
        if g.label != g.role || kinds.iter().filter(|o| *o == k).count() < 2 {
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
    /// The program a node belongs to: (who it is, its name), from what PipeWire says alone. Who:
    /// its `application.name`, `application.process.binary` and `application.process.id` (the
    /// node's, else its client's), not the process alone: every Flatpak is process 2 in its
    /// sandbox, and an app talking PulseAudio comes through pipewire-pulse's (`pipewire.sec.pid`).
    /// Not a program: hardware (a node with a Device), a node a SPA plugin made inside PipeWire or
    /// WirePlumber (`factory.name` `api.*`: the MIDI bridge, BLE MIDI, a card's PCM, a camera),
    /// and PipeWire's own processes (the daemon, pipewire-pulse, a filter-chain: `pipewire`;
    /// WirePlumber): their nodes are the system's.
    fn program(&self, p: &Props) -> Option<(String, String)> {
        if p.contains_key("device.id") || p.get("factory.name").is_some_and(|f| f.starts_with("api.")) {
            return None;
        }
        let client = p.get("client.id").and_then(|c| c.parse::<u32>().ok()).and_then(|c| self.objects.get(&c)).filter(|(k, _)| *k == Kind::Client).map(|(_, cp)| cp);
        if client.is_some_and(|c| c.contains_key("wireplumber.daemon") || c.get("core.daemon").is_some_and(|v| v == "true") || c.get("application.process.binary").is_some_and(|b| b == "pipewire")) {
            return None;
        }
        let get = |k: &str| some(p.get(k)).or_else(|| client.and_then(|c| some(c.get(k)))).map_or("", |s| s.as_str());
        let name = plain_name(get("application.name"));
        (!name.trim().is_empty()).then(|| (format!("{name}|{}|{}", get("application.process.binary"), get("application.process.id")), name))
    }

    /// Nodes Patchcord doesn't show on the desk, in the rack or in MIXER (ENGINE shows them all):
    /// other programs' peak meters (`stream.monitor`), its own meters, video, and streams whose
    /// full properties haven't arrived yet (to know which they are).
    fn hidden(&self, node: u32, p: &Props) -> bool {
        let class = p.get("media.class").map_or("", |s| s.as_str());
        p.get("stream.monitor").is_some_and(|m| m == "true")
            || p.get("node.name").is_some_and(|n| n.starts_with(METER_PREFIX) || n.starts_with(CUE_NODE))
            // a Bluetooth headset's real microphone and WirePlumber's internal capture stream:
            // inside its loopback the sound reaches the microphone apps see (MIC) without a link,
            // so a cord to them would lead nowhere
            || class.ends_with("/Internal")
            // video (a camera, a screen being shared): only in ENGINE for now (ADR 0019)
            || block_kind(class) == "VIDEO"
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
        let mut list: Vec<Port> = ports.iter().map(|(id, p)| port(*id, node, p)).collect();
        // PipeWire numbers a node's ports per direction in `port.id`
        let index = |id: u32| self.props(id).and_then(|p| p.get("port.id")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(id);
        list.sort_by_key(|p| (p.dir == Dir::Out, index(p.id)));
        let monitor = |p: &Port| self.props(p.id).and_then(|pp| pp.get("port.monitor")).is_some_and(|m| m == "true");

        let props: HashMap<u32, &Props> = ports.iter().map(|(id, p)| (*id, *p)).collect();
        let mut groups: Vec<Group> = Vec::new();
        for (dir, mon) in [(Dir::In, false), (Dir::Out, false), (Dir::Out, true)] {
            let all: Vec<Port> = list.iter().filter(|p| p.dir == dir && monitor(p) == mon).cloned().collect();
            if all.is_empty() {
                continue;
            }
            for (client, ps) in by_client(all, &props) {
                groups.push(self.group(node, dir, mon, client, ps));
            }
        }
        groups
    }

    /// One frame's group: a node's ports one way (of one client, on a MIDI bridge).
    fn group(&self, node: u32, dir: Dir, mon: bool, client: Option<String>, ps: Vec<Port>) -> Group {
        let np = self.props(node).expect("node");
        let class = np.get("media.class").map_or("", |s| s.as_str());
        let role = port_role(class, dir, mon, ps[0].media);
        // a virtual source's outputs are what apps record from, though flagged as monitors
        let mon = role == "MONITOR";
        let label = role.to_owned();
        let stereo = ps.len() == 2 && ps[0].media == Media::Audio && ps[0].channel == "FL" && ps[1].channel == "FR";
        // not Bluetooth's internal capture stream, either way: a meter on what it records links
        // to the headset's microphone, which keeps it open
        let meter = ps[0].media == Media::Audio && !class.ends_with("/Internal");
        // a volume goes with its own ports: what an app plays or records, what a device's output
        // plays (its PLAYBACK) or its input takes in (its CAPTURE)
        let audio = ps[0].media == Media::Audio && !mon;
        let stream = (dir == Dir::Out && class == "Stream/Output/Audio") || (dir == Dir::In && class == "Stream/Input/Audio");
        // (a virtual source's too: Easy Effects Source has a volume of its own)
        let device = (dir == Dir::In && class == "Audio/Sink") || (dir == Dir::Out && class.starts_with("Audio/Source"));
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
        let nick = some(np.get("node.nick")).cloned();
        Group { label, client, nick, role, kind: block_kind(class), dir, ports: ps, stereo, meter, mix, ports_to, stream: 0 }
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

/// A node's ports one way, by the client each belongs to, when several share the node:
/// WirePlumber's MIDI bridge holds every ALSA MIDI client (Midi Through, a keyboard, Hydrogen).
/// Each client's ports have their own `port.group` (client_14) and their `port.alias` starts with
/// the client's name ("Hydrogen:Hydrogen Midi-In"): a set for each, named so, in ALSA's order (the
/// client's number). Ports that don't say so stay one set, unnamed: a JACK app's out1, out2 have a
/// `port.group` each, but one client in their aliases ("Bitwig:out1").
fn by_client(ps: Vec<Port>, props: &HashMap<u32, &Props>) -> Vec<(Option<String>, Vec<Port>)> {
    let who = |p: &Port| -> Option<(String, String)> {
        let pp = props.get(&p.id)?;
        let client = pp.get("port.alias")?.split_once(':')?.0.trim();
        let group = some(pp.get("port.group"))?;
        (!client.is_empty()).then(|| (group.clone(), client.to_owned()))
    };
    let whose: Vec<Option<(String, String)>> = ps.iter().map(who).collect();
    let mut sets: Vec<(String, String)> = whose.iter().flatten().cloned().collect();
    sets.sort();
    sets.dedup();
    let clients: HashSet<&String> = sets.iter().map(|s| &s.1).collect();
    let groups: HashSet<&String> = sets.iter().map(|s| &s.0).collect();
    // every port says whose it is, each group is one client's, and there are several
    if whose.iter().any(Option::is_none) || clients.len() < 2 || clients.len() != sets.len() || groups.len() != sets.len() {
        return vec![(None, ps)];
    }
    let number = |g: &str| g.rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(u32::MAX);
    sets.sort_by_key(|s| (number(&s.0), s.1.clone()));
    sets.into_iter()
        .map(|(g, client)| {
            let mine = ps.iter().zip(&whose).filter(|(_, w)| w.as_ref().is_some_and(|w| w.0 == g)).map(|(p, _)| p.clone()).collect();
            (Some(client.to_uppercase()), mine)
        })
        .collect()
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

/// What a device is, in a word or two, for its screen. Hardware by its form (`device.form-factor`),
/// HDMI by its profiles, else a sound card; without a Device, by what its node is (`media.class`)
/// and how it was made (`factory.name`, `node.link-group`).
fn kind(dev: Option<&Props>, profiles: Option<&[Profile]>, node: &Props) -> &'static str {
    let get = |p: &Props, k: &str| p.get(k).map_or("", |s| s.as_str()).to_owned();
    if let Some(d) = dev {
        if get(d, "media.class").starts_with("Video") {
            return "CAMERA";
        }
        match get(d, "device.form-factor").as_str() {
            "headset" | "hands-free" | "handset" => return "HEADSET",
            "headphone" => return "HEADPHONES",
            "speaker" | "portable" | "hifi" | "car" => return "SPEAKER",
            "microphone" => return "MICROPHONE",
            "webcam" => return "WEBCAM",
            "tv" => return "TV",
            _ => {}
        }
        // a card whose every sound profile is HDMI (Off and Pro Audio aside): a display's sound
        let sound: Vec<&Profile> = profiles.unwrap_or(&[]).iter().filter(|p| p.name != "off" && p.name != "pro-audio").collect();
        if !sound.is_empty() && sound.iter().all(|p| p.name.contains("hdmi")) {
            return "HDMI AUDIO";
        }
        return "SOUND CARD";
    }
    let class = get(node, "media.class");
    let made = format!("{} {}", get(node, "factory.name"), get(node, "node.name")).to_lowercase();
    let group = get(node, "node.link-group");
    if class.starts_with("Midi") {
        "MIDI"
    } else if class.starts_with("Video") {
        "CAMERA"
    } else if class.starts_with("Stream/") {
        if class.contains("Internal") { "INTERNAL" } else { "APP" }
    } else if ["tunnel", "rtp", "roc", "raop", "snapcast", "zeroconf"].iter().any(|w| made.contains(w)) {
        "NETWORK"
    } else if group.starts_with("loopback-") {
        "LOOPBACK"
    } else if group.starts_with("filter-chain") {
        "FILTER"
    } else if class.contains("Sink") {
        "VIRTUAL SINK"
    } else if class.contains("Source") {
        "VIRTUAL SOURCE"
    } else {
        "VIRTUAL DEVICE"
    }
}

/// What kind of block a node is, by its `media.class` (ADR 0014): its frame's colour on the rack
/// and its block's in ENGINE, whoever owns it (a program's virtual sink is a SINK).
pub fn block_kind(class: &str) -> &'static str {
    match class {
        c if c.starts_with("Midi") => "MIDI",
        c if c.starts_with("Video") || (c.starts_with("Stream/") && c.ends_with("/Video")) => "VIDEO",
        c if c.starts_with("Audio/Sink") => "SINK",
        c if c.starts_with("Audio/Source") => "SOURCE",
        c if c.starts_with("Audio/Duplex") => "DUPLEX",
        c if c.starts_with("Stream/Output") => "PLAYBACK STREAM",
        c if c.starts_with("Stream/Input") => "RECORDING STREAM",
        // none (Easy Effects' effects), or one Patchcord doesn't know: said as none, nothing made up
        _ => "NO CLASS",
    }
}

/// What a group of ports is for (ADR 0014): its jacks' colour on the rack and its ports' in
/// ENGINE. A sink's inputs are PLAYBACK, a source's outputs CAPTURE (a virtual source's too,
/// though PipeWire flags them `port.monitor`), other outputs flagged so MONITOR; anything else's
/// INPUT or OUTPUT; MIDI and video ports by their media; a port of a format Patchcord doesn't
/// know, or of none, OTHER.
pub fn port_role(class: &str, dir: Dir, monitor: bool, media: Media) -> &'static str {
    let source = class.starts_with("Audio/Source");
    match (class, dir) {
        _ if media == Media::Midi => "MIDI",
        _ if media == Media::Video => "VIDEO",
        // a format Patchcord doesn't know (or none): said so, NO CLASS's colour
        _ if media == Media::Other => "OTHER",
        _ if monitor && !(source && dir == Dir::Out) => "MONITOR",
        (c, Dir::In) if c.ends_with("Sink") => "PLAYBACK",
        (_, Dir::Out) if source => "CAPTURE",
        (_, Dir::In) => "INPUT",
        (_, Dir::Out) => "OUTPUT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A report keeps the state that comes as params: a stream's volume and mute, a card's
    /// profiles and ports, the ones in use marked, the port's volume with it.
    #[test]
    fn the_report_keeps_volumes_profiles_and_ports() {
        let mut g = Graph::default();
        g.apply(&Event::Mix { id: 70, volumes: vec![0.5f32.powi(3), 0.5f32.powi(3)], mute: true });
        let profile = |index, name: &str| Profile { index, name: name.into(), description: name.to_uppercase(), available: Avail::Yes, priority: 1 };
        g.apply(&Event::Profiles { id: 45, list: vec![profile(0, "off"), profile(1, "hifi")], current: Some(1) });
        let route = |index, name: &str, device| Route { index, output: true, device, name: name.into(), description: name.into(), available: Avail::No, devices: vec![1], profiles: vec![1], volumes: device.map(|_| vec![1.0, 1.0]), mute: false, latency_ns: 0 };
        g.apply(&Event::Routes { id: 45, all: vec![route(2, "headphones", None), route(3, "speaker", None)], active: vec![route(3, "speaker", Some(1))] });
        let t = g.state_text();
        assert!(t.contains("node 70 volume 50% 50% · mute yes"), "{t}");
        assert!(t.contains("profile *1 hifi \"HIFI\"") && t.contains("profile  0 off"), "{t}");
        assert!(t.contains("port  2 out headphones"), "{t}");
        assert!(t.contains("port *3 out speaker \"speaker\" · available no · profiles [1] · devices [1] · in use for device 1 · volume 100% 100% · mute no"), "{t}");
    }

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
    fn the_knobs_zero_is_minus_120_db_and_reads_as_zero() {
        let m = Mix { node: 1, volumes: vec![1.0, 1.0], mute: false, gain: false, route: None };
        // fully down, or one side by the balance: -120 dB, not silence
        assert_eq!(m.volumes_for(0.0, 0.0), vec![VOL_FLOOR; 2]);
        assert_eq!(m.volumes_for(1.0, 1.0), vec![VOL_FLOOR, 1.0]);
        let down = Mix { volumes: vec![VOL_FLOOR; 2], ..m };
        assert_eq!((down.volume(), down.balance()), (0.0, 0.0));
        assert_eq!(volume_text(down.volume()).0, "0%");
        // set to a true 0 elsewhere: also 0
        assert_eq!(Mix { volumes: vec![0.0], ..m }.volume(), 0.0);
    }

    /// Video (a camera, a screen shared by the compositor, an app watching it) is in ENGINE only:
    /// no device on the desk, in the rack or in MIXER, not even an empty camera.
    #[test]
    fn video_is_left_to_engine() {
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
        g.apply(&Event::Added { id: 1, kind: Kind::Device, props: props(&[("device.api", "v4l2"), ("media.class", "Video/Device"), ("device.description", "Camera")]) });
        for (node, class, dir) in [(10, "Video/Source", "out"), (20, "Stream/Output/Video", "out"), (30, "Stream/Input/Video", "in"), (40, "Stream/Output/Audio", "out")] {
            let mut p = vec![("media.class", class), ("node.name", "n"), ("application.name", "app")];
            if node == 10 {
                p.push(("device.id", "1"));
            }
            g.apply(&Event::Added { id: node, kind: Kind::Node, props: props(&p) });
            g.apply(&Event::Info { id: node, props: props(&p), state: Some("running") });
            let fmt = if class.ends_with("Audio") { "32 bit float mono audio" } else { "" };
            g.apply(&Event::Added { id: node + 1, kind: Kind::Port, props: props(&[("node.id", &node.to_string()), ("port.direction", dir), ("port.name", "p"), ("format.dsp", fmt)]) });
        }
        g.apply(&Event::Synced);
        let devs = g.devices();
        assert_eq!(devs.iter().flat_map(|d| d.groups.iter().map(|x| x.kind)).collect::<Vec<_>>(), ["PLAYBACK STREAM"], "{:?}", devs.iter().map(|d| &d.name).collect::<Vec<_>>());
        // ENGINE still has them all
        assert_eq!(g.objects().filter(|(_, k, _)| *k == Kind::Node).count(), 4);
    }

    #[test]
    fn a_nodes_monitor_ports_are_known() {
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Props>();
        g.apply(&Event::Added { id: 10, kind: Kind::Node, props: props(&[("media.class", "Stream/Input/Audio")]) });
        g.apply(&Event::Added { id: 12, kind: Kind::Port, props: props(&[("node.id", "10"), ("port.direction", "out"), ("port.monitor", "true"), ("audio.channel", "MONO")]) });
        g.apply(&Event::Added { id: 11, kind: Kind::Port, props: props(&[("node.id", "10"), ("port.direction", "in"), ("audio.channel", "MONO")]) });
        assert_eq!(g.monitor_ports(10), vec![(12, "MONO".to_owned())]);
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
        // and its recording stream's monitor, on its face: the rack patches whatever PipeWire can
        // link (ADR 0014), so it isn't listed as the system's
        assert_eq!(outs, [("OUTPUT 1", 1, true), ("OUTPUT 2", 2, true), ("OUTPUT 3", 3, true), ("OUTPUT 4", 4, true), ("OUTPUT 5", 5, true), ("MONITOR", 0, false)]);
        assert!(g.system().iter().all(|s| !s.1.contains("monitor")), "{:?}", g.system());
        let input = d.groups.iter().find(|g| g.dir == Dir::In).expect("an INPUT");
        assert_eq!((input.label.as_str(), input.stream), ("INPUT", 0));
        let jd = crate::ui::face_jacks(d);
        let labels: Vec<&str> = jd.iter().filter(|j| j.0 == Dir::Out).map(|j| j.1.as_str()).collect();
        // (the sixth: the INPUT's MONITOR)
        assert_eq!(labels, ["MONO"; 6], "a jack says what it carries; the number is in its frame's name");
        // balanced: the six take two units of three, in columns; what takes sound in first (the
        // INPUT, at the top-left), what sends it out (the outputs) after it, on to the next unit
        let ((_, h), at) = crate::ui::face_layout(d);
        let u = crate::ui::DEV_H;
        assert_eq!(h, 2.0 * u);
        // (the seventh group: the INPUT's MONITOR, inside its frame)
        assert_eq!(at.iter().map(|a| a.1).collect::<Vec<_>>(), [0.0, 0.0, 0.0, u, u, u, 0.0]);
        assert_eq!((at[1].0, at[2].0), (at[4].0, at[5].0));
        // (the INPUT in OUTPUT 3's column, against its left edge; OUTPUT 3 against its right: the
        // INPUT, with its MONITOR, is the wider)
        assert!(at[0].0 <= at[3].0 && at[3].0 < at[0].0 + 120.0, "{at:?}");
    }

    /// Teams in a call records on two streams and plays on six: eight frames, two units of four,
    /// the inputs first (sound comes in at the left), the outputs at the right, nothing left over.
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
        let unit = crate::ui::DEV_H;
        assert_eq!(h, 2.0 * unit);
        let row = |u: f64| d.groups.iter().zip(&at).filter(|(_, a)| a.1 == u).map(|(g, _)| g.label.as_str()).collect::<Vec<_>>();
        assert_eq!(row(0.0), ["INPUT 1", "INPUT 2", "OUTPUT 1", "OUTPUT 2"]);
        assert_eq!(row(unit), ["OUTPUT 3", "OUTPUT 4", "OUTPUT 5", "OUTPUT 6"]);
        // sound flows left to right: the inputs first, the outputs at the right
        let x = |l: &str| d.groups.iter().zip(&at).find(|(g, _)| g.label == l).map(|(_, a)| a.0).unwrap();
        assert!(x("INPUT 1") < x("INPUT 2") && x("INPUT 2") < x("OUTPUT 1"));
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

    /// A graph of programs as PipeWire has them: clients, nodes with ports.
    struct Programs(Graph);

    impl Programs {
        fn new() -> Self {
            Programs(Graph::default())
        }
        fn client(&mut self, id: u32, kv: &[(&str, &str)]) {
            let mut p: Props = kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            p.insert("object.serial".into(), id.to_string());
            self.0.apply(&Event::Added { id, kind: Kind::Client, props: p });
        }
        /// A node of `client` with stereo ports: (dir, monitor) each.
        fn node(&mut self, id: u32, client: u32, kv: &[(&str, &str)], ports: &[(&str, bool)]) {
            let mut p: Props = kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            p.insert("client.id".into(), client.to_string());
            p.insert("object.serial".into(), id.to_string());
            self.0.apply(&Event::Added { id, kind: Kind::Node, props: p.clone() });
            self.0.apply(&Event::Info { id, props: p, state: Some("running") });
            for (i, (dir, mon)) in ports.iter().enumerate() {
                for (j, ch) in ["FL", "FR"].iter().enumerate() {
                    let pid = id * 100 + (i * 2 + j) as u32;
                    let mut pp = vec![("node.id", id.to_string()), ("port.direction", dir.to_string()), ("port.name", format!("{dir}_{ch}")), ("format.dsp", "32 bit float mono audio".into()), ("audio.channel", ch.to_string())];
                    if *mon {
                        pp.push(("port.monitor", "true".into()));
                    }
                    self.0.apply(&Event::Added { id: pid, kind: Kind::Port, props: pp.into_iter().map(|(k, v)| (k.to_string(), v)).collect() });
                }
            }
        }
        fn devices(&self) -> Vec<Device> {
            self.0.devices()
        }
    }

    /// A node a SPA plugin made (`factory.name` `api.*`) is the system's, whatever its client
    /// says: PipeWire 1.6.11 (Fedora) gives WirePlumber's client only its name, no
    /// `wireplumber.daemon`, and the MIDI bridge and BLE MIDI came out as a program "WirePlumber".
    #[test]
    fn a_node_from_a_spa_plugin_is_never_a_program() {
        let mut g = Programs::new();
        // clients as 1.6.11 gives them: a name, nothing to tell WirePlumber by
        g.client(34, &[("application.name", "WirePlumber"), ("pipewire.access", "unrestricted")]);
        g.client(43, &[("application.name", "WirePlumber [client]"), ("pipewire.access", "unrestricted")]);
        g.client(173, &[("application.name", "Telegram"), ("pipewire.access", "unrestricted")]);
        g.node(46, 34, &[("media.class", "Midi/Bridge"), ("node.name", "Midi-Bridge"), ("factory.name", "api.alsa.seq.bridge"), ("device.api", "alsa")], &[("in", false)]);
        g.node(50, 43, &[("media.class", "Midi/Bridge"), ("node.name", "bluez_midi.server"), ("node.description", "BLE MIDI 1"), ("factory.name", "api.bluez5.midi.node"), ("device.api", "bluez5")], &[("in", false)]);
        g.node(60, 173, &[("media.class", "Stream/Output/Audio"), ("node.name", "Telegram"), ("factory.name", "adapter")], &[("out", false)]);
        g.0.apply(&Event::Synced);
        let devs = g.devices();
        let finish = |name: &str| devs.iter().find(|d| d.name == name).map(|d| (d.kind, d.finish));
        assert_eq!(finish("Midi-Bridge"), Some(("MIDI", Finish::Olive)), "{:?}", devs.iter().map(|d| &d.name).collect::<Vec<_>>());
        assert_eq!(finish("BLE MIDI 1"), Some(("MIDI", Finish::Olive)));
        assert!(!devs.iter().any(|d| d.name.starts_with("WirePlumber")));
        // a program made by a SPA-less factory stays a program
        assert_eq!(finish("Telegram"), Some(("APP", Finish::Black)));
    }

    /// A frame is a block (ADR 0014): a node's every port in its one frame, whatever ways they go,
    /// its frame coloured by its kind and its ports by what they are for.
    #[test]
    fn a_frame_is_a_block() {
        let mut g = Programs::new();
        g.client(50, &[("application.name", "easyeffects"), ("application.process.binary", "easyeffects"), ("application.process.id", "7")]);
        g.node(141, 50, &[("media.class", "Audio/Sink"), ("node.name", "easyeffects_sink"), ("node.description", "Easy Effects Sink")], &[("in", false), ("out", true)]);
        g.node(88, 50, &[("media.class", "Audio/Source/Virtual"), ("node.name", "easyeffects_source"), ("node.description", "Easy Effects Source")], &[("in", false), ("out", true)]);
        g.0.apply(&Event::Mix { id: 88, volumes: vec![1.0, 1.0], mute: false });
        g.client(60, &[("application.name", "Bitwig Studio"), ("application.process.binary", "bitwig"), ("application.process.id", "8")]);
        g.node(77, 60, &[("media.class", "Audio/Duplex"), ("node.name", "Bitwig Studio")], &[("in", false), ("out", false)]);
        g.client(70, &[("application.name", "Teams"), ("application.process.binary", "teams"), ("application.process.id", "9")]);
        g.node(119, 70, &[("media.class", "Stream/Input/Audio"), ("node.name", "teams")], &[("in", false), ("out", true)]);
        let devs = g.devices();
        let ee = devs.iter().find(|d| d.name == "easyeffects").unwrap();
        // a frame a node (in the order the program made them), the sink's MONITOR inside it, the
        // source's input and output together
        assert_eq!(crate::ui::face_frames(ee), [88, 141]);
        let roles: Vec<(&str, &str)> = ee.groups.iter().map(|g| (g.kind, g.role)).collect();
        assert!(roles.contains(&("SOURCE", "CAPTURE")), "a virtual source's outputs are what apps record from: {roles:?}");
        assert!(roles.contains(&("SINK", "MONITOR")) && roles.contains(&("SINK", "PLAYBACK")) && roles.contains(&("SOURCE", "INPUT")), "{roles:?}");
        assert_eq!(ee.finish, Finish::Black);
        let src = ee.groups.iter().find(|g| g.role == "CAPTURE").unwrap();
        assert!(src.mix.is_some(), "a virtual source has a volume of its own");
        // a duplex block: its inputs and outputs one frame, though no chain
        let bitwig = devs.iter().find(|d| d.name == "Bitwig Studio").unwrap();
        assert_eq!(crate::ui::face_frames(bitwig), [77]);
        assert!(bitwig.groups.iter().all(|g| g.kind == "DUPLEX"));
        // a recording stream: its MONITOR on its frame
        let teams = devs.iter().find(|d| d.name == "Teams").unwrap();
        assert_eq!(crate::ui::face_frames(teams), [119]);
        assert_eq!(teams.groups.iter().map(|g| (g.kind, g.role)).collect::<Vec<_>>(), [("RECORDING STREAM", "INPUT"), ("RECORDING STREAM", "MONITOR")]);
    }

    /// Each kind of block from its media.class, whoever owns it.
    #[test]
    fn a_block_is_of_its_class() {
        for (class, kind) in [
            ("Audio/Sink", "SINK"),
            ("Audio/Source", "SOURCE"),
            ("Audio/Source/Virtual", "SOURCE"),
            ("Stream/Output/Audio", "PLAYBACK STREAM"),
            ("Stream/Input/Audio", "RECORDING STREAM"),
            ("Stream/Input/Audio/Internal", "RECORDING STREAM"),
            ("Midi/Bridge", "MIDI"),
            ("Video/Source", "VIDEO"),
            ("Stream/Input/Video", "VIDEO"),
            ("Audio/Duplex", "DUPLEX"),
            ("", "NO CLASS"),
        ] {
            assert_eq!(block_kind(class), kind, "{class}");
        }
        // a port of a format Patchcord doesn't know: OTHER, whichever way it goes
        assert_eq!((port_role("Audio/Sink", Dir::In, false, Media::Other), port_role("", Dir::Out, false, Media::Other)), ("OTHER", "OTHER"));
    }

    /// Easy Effects: its own sink and its effects (no media.class), all of one client, are one
    /// device with every node on its face, named as it named them (two of a name kept so), in the
    /// order its sound goes through them: each chain from its start along the program's own links,
    /// each node's inputs then its outputs; a node with no links of its own last.
    #[test]
    fn a_program_is_one_device_its_nodes_along_its_chains() {
        let mut g = Programs::new();
        g.client(199, &[("application.name", "easyeffects"), ("application.process.binary", "easyeffects"), ("application.process.id", "2")]);
        g.client(50, &[("application.name", "Kadence"), ("application.process.binary", "kadence"), ("application.process.id", "77")]);
        // made in this order: the sink, a test signal, the two effects (the second first in its chain)
        g.node(281, 199, &[("media.class", "Audio/Sink"), ("node.name", "easyeffects_sink"), ("node.description", "Easy Effects Sink")], &[("in", false), ("out", true)]);
        g.node(282, 199, &[("node.name", "ee_test_signals"), ("node.description", "Easy Effects Filter")], &[("out", false)]);
        g.node(283, 199, &[("node.name", "ee_soe_spectrum"), ("node.description", "Spectrum")], &[("in", false), ("out", false)]);
        g.node(284, 199, &[("node.name", "ee_soe_equalizer"), ("node.description", "Equalizer")], &[("in", false), ("out", false)]);
        g.node(900, 50, &[("media.class", "Stream/Input/Audio"), ("node.name", "kadence")], &[("in", false)]);
        let link = |g: &mut Programs, id: u32, out: u32, inp: u32| {
            let p: Props = [("link.output.port", out.to_string()), ("link.input.port", inp.to_string())].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
            g.0.apply(&Event::Added { id, kind: Kind::Link, props: p });
        };
        // sink's monitor → equalizer → spectrum → Kadence
        link(&mut g, 1, 28102, 28400);
        link(&mut g, 2, 28402, 28300);
        link(&mut g, 3, 28302, 90000);
        let devs = g.devices();
        let d = devs.iter().find(|d| d.key == "app:easyeffects").expect("one device");
        assert_eq!((d.name.as_str(), d.kind, d.nodes, d.ports, d.chain), ("easyeffects", "APP", 4, 14, true));
        let frames: Vec<(&str, Dir)> = d.groups.iter().map(|g| (g.label.as_str(), g.dir)).collect();
        assert_eq!(
            frames,
            [
                ("EASY EFFECTS SINK", Dir::In),
                ("EASY EFFECTS SINK", Dir::Out),
                ("EQUALIZER", Dir::In),
                ("EQUALIZER", Dir::Out),
                ("SPECTRUM", Dir::In),
                ("SPECTRUM", Dir::Out),
                ("EASY EFFECTS FILTER", Dir::Out),
            ]
        );
        // an app's own streams are not a chain: inputs left, outputs right, as on every face
        assert!(devs.iter().all(|d| d.key == "app:easyeffects" || !d.chain));
        // every effect's ports are named alike (input_FL…): its jacks still each their own, so
        // every link of its own is a cord of its own
        let keys = crate::ui::face_jack_keys(d);
        let mut unique = keys.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "{keys:?}");
    }

    /// Flatpaks are all process 2: programs apart by name and binary; one program's two clients
    /// (Carla plays and records through two) are one.
    #[test]
    fn programs_are_known_by_name_binary_and_process() {
        let mut g = Programs::new();
        g.client(81, &[("application.name", "ALSA plug-in [kadence]"), ("application.process.binary", "kadence"), ("application.process.id", "2"), ("pipewire.sec.pid", "2389")]);
        g.client(330, &[("application.name", "Carla"), ("application.process.binary", "python3.13"), ("application.process.id", "2"), ("pipewire.sec.pid", "2389")]);
        g.client(138, &[("application.name", "Carla"), ("application.process.binary", "python3.13"), ("application.process.id", "2"), ("pipewire.sec.pid", "2389")]);
        g.node(112, 81, &[("media.class", "Stream/Output/Audio"), ("node.name", "ALSA plug-in [kadence]"), ("application.name", "ALSA plug-in [kadence]")], &[("out", false)]);
        g.node(216, 330, &[("media.class", "Stream/Output/Audio"), ("node.name", "Carla")], &[("out", false)]);
        g.node(217, 138, &[("media.class", "Stream/Input/Audio"), ("node.name", "Carla")], &[("in", false), ("out", true)]);
        let devs = g.devices();
        let names: Vec<(&str, &str, &str)> = devs.iter().map(|d| (d.key.as_str(), d.name.as_str(), d.kind)).collect();
        assert!(names.contains(&("app:Carla", "Carla", "APP")) && names.contains(&("node:ALSA plug-in [kadence]", "kadence", "APP")), "{names:?}");
        let carla = devs.iter().find(|d| d.name == "Carla").unwrap();
        // its recording stream's monitor on its face, as every port (ADR 0014)
        assert_eq!(carla.groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["INPUT", "OUTPUT", "MONITOR"]);
        assert_eq!((carla.nodes, carla.ports), (2, 6));
    }

    /// PipeWire's own processes (pipewire-pulse, a filter-chain) and WirePlumber are the system,
    /// not programs; a JACK-style node of one is a program: APP, by its name, one frame a way.
    #[test]
    fn the_system_is_no_program_and_a_jack_node_is_one() {
        let mut g = Programs::new();
        g.client(221, &[("application.name", "WirePlumber"), ("application.process.binary", "wireplumber"), ("wireplumber.daemon", "true")]);
        g.client(59, &[("application.name", "pipewire"), ("application.process.binary", "pipewire")]);
        g.client(92, &[("application.name", "Bitwig Studio"), ("application.process.binary", "BitwigAudioEngine-X64-AVX2"), ("application.process.id", "717")]);
        g.node(10, 221, &[("media.class", "Audio/Sink"), ("node.name", "loop-sink"), ("node.description", "Loop"), ("node.link-group", "loopback-1-2")], &[("in", false)]);
        g.node(20, 59, &[("media.class", "Audio/Sink"), ("node.name", "null-sink"), ("node.description", "Null Output"), ("factory.name", "support.null-audio-sink")], &[("in", false)]);
        g.node(86, 92, &[("media.class", "Audio/Duplex"), ("node.name", "Bitwig Studio"), ("node.description", "Bitwig Studio's audio engine")], &[("out", false)]);
        let devs = g.devices();
        let kinds: Vec<(&str, &str)> = devs.iter().map(|d| (d.name.as_str(), d.kind)).collect();
        assert!(kinds.contains(&("Loop", "LOOPBACK")) && kinds.contains(&("Null Output", "VIRTUAL SINK")), "{kinds:?}");
        let bitwig = devs.iter().find(|d| d.key == "node:Bitwig Studio").unwrap();
        assert_eq!((bitwig.name.as_str(), bitwig.kind, bitwig.finish), ("Bitwig Studio", "APP", Finish::Black));
        assert_eq!(bitwig.groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["OUTPUT"]);
    }

    /// Two programs of one name (two VLCs): the first keeps it, the next is numbered.
    #[test]
    fn two_programs_of_one_name_are_numbered() {
        let mut g = Programs::new();
        for (client, pid, node) in [(317, "1263044", 286), (400, "1300000", 500)] {
            g.client(client, &[("application.name", "VLC media player"), ("application.process.binary", "vlc"), ("application.process.id", pid)]);
            g.node(node, client, &[("media.class", "Stream/Output/Audio"), ("node.name", "VLC media player")], &[("out", false)]);
        }
        let mut names: Vec<String> = g.devices().into_iter().map(|d| d.name).collect();
        names.sort();
        assert_eq!(names, ["VLC media player", "VLC media player 2"]);
    }

    /// WirePlumber's MIDI bridge: a frame for each ALSA MIDI client, in ALSA's order; a JACK
    /// app's ports, a `port.group` each but one client, stay one frame.
    #[test]
    fn a_midi_bridge_has_a_frame_for_each_client() {
        let mut g = Graph::default();
        let props = |kv: &[(&str, &str)]| -> Props { kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        g.apply(&Event::Added { id: 42, kind: Kind::Node, props: props(&[("media.class", "Midi/Bridge"), ("node.name", "Midi-Bridge")]) });
        for (id, name, alias, group, dir) in [
            (50, "Hydrogen: Midi-In (playback)", "Hydrogen:Hydrogen Midi-In", "client_129", "in"),
            (51, "Midi Through: Port-0 (playback)", "Midi Through:Midi Through Port-0", "client_14", "in"),
            (52, "Midi Through: Port-0 (capture)", "Midi Through:Midi Through Port-0", "client_14", "out"),
            (53, "Hydrogen: Midi-Out (capture)", "Hydrogen:Hydrogen Midi-Out", "client_129", "out"),
        ] {
            g.apply(&Event::Added { id, kind: Kind::Port, props: props(&[("node.id", "42"), ("port.direction", dir), ("port.name", name), ("port.alias", alias), ("port.group", group), ("format.dsp", "8 bit raw midi")]) });
        }
        g.apply(&Event::Added { id: 86, kind: Kind::Node, props: props(&[("media.class", "Audio/Duplex"), ("node.name", "Bitwig Studio")]) });
        for (id, name, group) in [(60, "out1", "stream.0"), (61, "out2", "stream.1")] {
            g.apply(&Event::Added { id, kind: Kind::Port, props: props(&[("node.id", "86"), ("port.direction", "out"), ("port.name", name), ("port.alias", &format!("Bitwig:{name}")), ("port.group", group), ("format.dsp", "32 bit float mono audio")]) });
        }
        let devs = g.devices();
        let bridge = devs.iter().find(|d| d.name == "Midi-Bridge").unwrap();
        // a group a client, all of them in the bridge's one frame (ADR 0014), named by its client
        let frames: Vec<(Option<&str>, Dir, Vec<&str>)> = bridge.groups.iter().map(|g| (g.client.as_deref(), g.dir, g.ports.iter().map(|p| p.label.as_str()).collect())).collect();
        assert_eq!(frames, [(Some("MIDI THROUGH"), Dir::In, vec!["PORT-0"]), (Some("HYDROGEN"), Dir::In, vec!["MIDI-IN"]), (Some("MIDI THROUGH"), Dir::Out, vec!["PORT-0"]), (Some("HYDROGEN"), Dir::Out, vec!["MIDI-OU"])]);
        assert!(bridge.groups.iter().all(|g| g.role == "MIDI" && g.label == "MIDI" && g.kind == "MIDI"));
        assert_eq!(bridge.finish, Finish::Olive, "PipeWire's own: olive");
        assert_eq!(crate::ui::face_frames(bridge), [42], "one frame: the bridge is one node");
        let bitwig = devs.iter().find(|d| d.name == "Bitwig Studio").unwrap();
        assert_eq!(bitwig.groups.iter().map(|g| (g.label.as_str(), g.ports.len())).collect::<Vec<_>>(), [("OUTPUT", 2)]);
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

    /// The screen's type: hardware by its form or profiles, the rest by how its node was made.
    #[test]
    fn a_device_says_what_it_is() {
        let props = |kv: &[(&str, &str)]| -> Props { kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        let prof = |names: &[&str]| -> Vec<Profile> { names.iter().map(|n| Profile { index: 0, name: (*n).into(), description: String::new(), available: Avail::Yes, priority: 0 }).collect() };
        let sink = props(&[("media.class", "Audio/Sink")]);
        let card = props(&[("media.class", "Audio/Device")]);
        assert_eq!(kind(Some(&card), Some(&prof(&["off", "output:analog-stereo+input:analog-stereo", "pro-audio"])), &sink), "SOUND CARD");
        assert_eq!(kind(Some(&card), Some(&prof(&["off", "output:hdmi-stereo", "output:hdmi-surround", "pro-audio"])), &sink), "HDMI AUDIO");
        assert_eq!(kind(Some(&props(&[("media.class", "Audio/Device"), ("device.form-factor", "headset")])), None, &sink), "HEADSET");
        assert_eq!(kind(None, None, &props(&[("media.class", "Audio/Sink"), ("factory.name", "support.null-audio-sink")])), "VIRTUAL SINK");
        assert_eq!(kind(None, None, &props(&[("media.class", "Audio/Source/Virtual")])), "VIRTUAL SOURCE");
        assert_eq!(kind(None, None, &props(&[("media.class", "Audio/Sink"), ("node.link-group", "loopback-2005-20")])), "LOOPBACK");
        assert_eq!(kind(None, None, &props(&[("media.class", "Audio/Sink"), ("node.name", "tunnel-sink.studio")])), "NETWORK");
        assert_eq!(kind(None, None, &props(&[("media.class", "Midi/Bridge")])), "MIDI");
        assert_eq!(kind(None, None, &props(&[("media.class", "Stream/Output/Audio")])), "APP");
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
