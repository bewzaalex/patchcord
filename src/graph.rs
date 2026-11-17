//! The PipeWire graph as Patchcord shows it: raw objects in, panel devices out.
//!
//! A device on the panel is one card (all nodes sharing a `device.id`: sink, source and their
//! monitors) or one node without a device (an app, a stream, a MIDI bridge). Its ports are
//! sorted into labelled groups such as PLAYBACK, CAPTURE or MONITOR.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use crate::pw::{Avail, Event, Kind, METER_PREFIX, Profile, Props};

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
    /// its signal is measured for the device's meter (what a sink plays, what an app sends)
    pub meter: bool,
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
            Event::Removed(id) => {
                self.born.remove(id);
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
        for (node, p) in self.of(Kind::Node) {
            if !ports.contains_key(&node) || self.hidden(node, p) {
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
                // inputs, then outputs, then monitors: PLAYBACK, CAPTURE, MONITOR
                groups.sort_by_key(|g| (g.dir == Dir::Out, g.label == "MONITOR"));
                let mut h = DefaultHasher::new();
                key.hash(&mut h);
                Device {
                    model: model(dev, first),
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
            || p.get("node.name").is_some_and(|n| n.starts_with(METER_PREFIX))
            || (class.starts_with("Stream/") && (!self.info.contains(&node) || self.born.get(&node).is_some_and(|t| t.elapsed() < YOUNG)))
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
            // what a sink plays (its monitor) and what an app sends; never a microphone, which
            // recording would switch on (and a Bluetooth headset into its low-quality mode)
            let meter = ps[0].media == Media::Audio && ((mon && class.ends_with("Sink")) || (dir == Dir::Out && class.starts_with("Stream/Output/Audio")));
            groups.push(Group { label: label.into(), dir, ports: ps, stereo, meter });
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
