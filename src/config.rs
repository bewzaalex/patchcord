//! Settings and presets, kept as small text files in the config folder
//! (`~/.config/patchcord/settings.txt` and `presets.txt`), easy to read and edit by hand.
//!
//! Lines are `key = value`; `#` starts a comment. In presets.txt each preset starts with its
//! name in brackets. Devices and ports are named as PipeWire names them (node.name, port.name),
//! never by number, since the numbers change every time PipeWire starts. Fields within a value
//! are separated by " | ".

use std::collections::BTreeMap;
use std::path::PathBuf;

/// What Patchcord remembers between runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// cords: 0 show, 1 fade, 2 hide
    pub cables: u8,
    pub lock: bool,
    /// devices dragged on the desk line up with each other
    pub align: bool,
    /// each group of jacks on a band of its kind's colour
    pub accents: bool,
    /// a light runs in the cords, showing which way they go
    pub flow: bool,
    pub rack: bool,
    /// MIXER shows (over the desk or the rack)
    pub mixer: bool,
    /// window size in logical pixels
    pub window: Option<(u32, u32)>,
    /// the preset last saved or loaded
    pub preset: Option<String>,
    /// where each device sits on the desk, by device key
    pub pos: BTreeMap<String, (f64, f64)>,
    /// the devices in the rack, top to bottom
    pub rack_order: Vec<String>,
    /// devices hidden by hand, by device key
    pub hidden: Vec<String>,
    /// the cue's LEVEL knob, 0…100
    pub cue_level: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { cables: 0, lock: false, align: true, accents: true, flow: true, rack: false, mixer: false, window: None, preset: None, pos: BTreeMap::new(), rack_order: Vec::new(), hidden: Vec::new(), cue_level: 100 }
    }
}

/// A saved patch: its links, which stereo jacks are split, where the devices sit, the apps'
/// volumes and the cards' profiles.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Preset {
    pub name: String,
    /// every node there was when it was saved: loading takes away other links only between these
    pub nodes: Vec<String>,
    /// (node, port) → (node, port)
    pub links: Vec<((String, String), (String, String))>,
    /// split stereo groups: device key, group label, "in" or "out"
    pub split: Vec<(String, String, String)>,
    pub pos: BTreeMap<String, (f64, f64)>,
    /// each app stream's volume (1 is 100%), balance (-1 L … 1 R) and mute, by device key,
    /// group label and "in" or "out"
    pub mix: BTreeMap<(String, String, String), (f64, f64, bool)>,
    /// each card's profile, by device key: its full name, as PipeWire describes it
    pub profiles: BTreeMap<String, String>,
    /// each card output's or input's port (its full name) and latency offset in ns, by device
    /// key, group label and "in" or "out"
    pub ports: BTreeMap<(String, String, String), (String, i64)>,
    /// the default output and input, by node.name
    pub default_out: Option<String>,
    pub default_in: Option<String>,
}

const CABLES: [&str; 3] = ["show", "fade", "hide"];

fn file(name: &str) -> Option<PathBuf> {
    Some(crate::dirs::config()?.join(name))
}

fn fields(v: &str) -> Vec<String> {
    v.split(" | ").map(|f| f.trim().to_owned()).collect()
}

fn on(v: &str) -> bool {
    matches!(v, "on" | "yes" | "true" | "1")
}

/// `key = value` lines; `[name]` lines start a new section (presets) and come out as an entry
/// with an empty key, so a section with nothing in it still counts.
fn parse(text: &str) -> Vec<(Option<String>, String, String)> {
    let mut out = Vec::new();
    let mut section = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = Some(name.trim().to_owned());
            out.push((section.clone(), String::new(), String::new()));
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.push((section.clone(), k.trim().to_owned(), v.trim().to_owned()));
        }
    }
    out
}

/// Writes a file whole: to a temporary file first, then renamed, so a crash never leaves half a file.
fn write(name: &str, text: &str) {
    let (Some(dir), Some(path)) = (crate::dirs::config(), file(name)) else { return };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return eprintln!("patchcord: {}: {e}", dir.display());
    }
    let tmp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)) {
        eprintln!("patchcord: {}: {e}", path.display());
        let _ = std::fs::remove_file(&tmp);
    }
}

fn pos_line(key: &str, (x, y): (f64, f64)) -> String {
    format!("pos = {key} | {} | {}\n", x.round(), y.round())
}

fn read_pos(v: &str) -> Option<(String, (f64, f64))> {
    let f = fields(v);
    let num = |i: usize| f.get(i)?.parse::<f64>().ok().filter(|v| v.is_finite());
    Some((f.first()?.clone(), (num(1)?, num(2)?)))
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Some(text) = file("settings.txt").and_then(|p| std::fs::read_to_string(p).ok()) else { return s };
        for (_, k, v) in parse(&text) {
            match k.as_str() {
                "cables" => s.cables = CABLES.iter().position(|c| *c == v).unwrap_or(0) as u8,
                "lock" => s.lock = on(&v),
                "align" => s.align = on(&v),
                "accents" => s.accents = on(&v),
                "flow" => s.flow = on(&v),
                "layout" => s.rack = v == "rack",
                "mixer" => s.mixer = on(&v),
                "window" => {
                    // (a window too small to use is forgotten)
                    s.window = v.split_once('x').and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?))).filter(|&(w, h): &(u32, u32)| w >= 200 && h >= 150);
                }
                "preset" => s.preset = Some(v).filter(|v| !v.is_empty()),
                "pos" => {
                    if let Some((key, p)) = read_pos(&v) {
                        s.pos.insert(key, p);
                    }
                }
                "rack" => s.rack_order.push(v),
                "hide" => s.hidden.push(v),
                "cue_level" => s.cue_level = v.parse::<u8>().map_or(100, |l| l.min(100)),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let mut t = String::from("# Patchcord settings. Written by Patchcord; edit while it is not running.\n");
        let onoff = |b: bool| if b { "on" } else { "off" };
        t += &format!("cables = {}\nlock = {}\nalign = {}\naccents = {}\nflow = {}\nlayout = {}\n", CABLES[self.cables.min(2) as usize], onoff(self.lock), onoff(self.align), onoff(self.accents), onoff(self.flow), if self.rack { "rack" } else { "desk" });
        t += &format!("mixer = {}\n", onoff(self.mixer));
        t += &format!("cue_level = {}\n", self.cue_level.min(100));
        if let Some((w, h)) = self.window {
            t += &format!("window = {w}x{h}\n");
        }
        if let Some(p) = &self.preset {
            t += &format!("preset = {p}\n");
        }
        if !self.pos.is_empty() {
            t += "\n# where the devices sit on the desk: device | x | y\n";
            for (k, p) in &self.pos {
                t += &pos_line(k, *p);
            }
        }
        if !self.rack_order.is_empty() {
            t += "\n# the devices in the rack, top to bottom\n";
            for k in &self.rack_order {
                t += &format!("rack = {k}\n");
            }
        }
        if !self.hidden.is_empty() {
            t += "\n# devices hidden by hand (the HIDDEN key shows them again)\n";
            for k in &self.hidden {
                t += &format!("hide = {k}\n");
            }
        }
        write("settings.txt", &t);
    }
}

pub fn load_presets() -> Vec<Preset> {
    let Some(text) = file("presets.txt").and_then(|p| std::fs::read_to_string(p).ok()) else { return Vec::new() };
    let mut out: Vec<Preset> = Vec::new();
    for (section, k, v) in parse(&text) {
        let Some(name) = section else { continue };
        if k.is_empty() {
            out.push(Preset { name, ..Default::default() });
            continue;
        }
        let Some(p) = out.last_mut() else { continue };
        match k.as_str() {
            "node" => p.nodes.push(v),
            "link" => {
                if let Some((a, b)) = v.split_once(" > ") {
                    let (a, b) = (fields(a), fields(b));
                    if a.len() == 2 && b.len() == 2 {
                        p.links.push(((a[0].clone(), a[1].clone()), (b[0].clone(), b[1].clone())));
                    }
                }
            }
            "split" => {
                let f = fields(&v);
                if f.len() == 3 {
                    p.split.push((f[0].clone(), f[1].clone(), f[2].clone()));
                }
            }
            "pos" => {
                if let Some((key, xy)) = read_pos(&v) {
                    p.pos.insert(key, xy);
                }
            }
            "mix" => {
                let f = fields(&v);
                if let [k, g, dir, vol, bal, m] = &f[..]
                    && let (Ok(vol), Ok(bal)) = (vol.parse::<f64>(), bal.parse::<f64>())
                    && vol.is_finite()
                    && bal.is_finite()
                {
                    p.mix.insert((k.clone(), g.clone(), dir.clone()), (vol / 100.0, bal / 100.0, on(m)));
                }
            }
            "profile" => {
                if let Some((k, name)) = v.split_once(" | ") {
                    p.profiles.insert(k.trim().to_owned(), name.trim().to_owned());
                }
            }
            "port" => {
                let f = fields(&v);
                if let [k, g, dir, name, ms] = &f[..]
                    && let Ok(ms) = ms.parse::<f64>()
                    && ms.is_finite()
                    && ms.abs() < 10_000.0
                {
                    p.ports.insert((k.clone(), g.clone(), dir.clone()), (name.clone(), (ms * 1e6).round() as i64));
                }
            }
            "default" => match v.split_once(" | ") {
                Some(("out", n)) => p.default_out = Some(n.trim().to_owned()),
                Some(("in", n)) => p.default_in = Some(n.trim().to_owned()),
                _ => {}
            },
            _ => {}
        }
    }
    out
}

pub fn save_presets(presets: &[Preset]) {
    let mut t = String::from(
        "# Patchcord presets. Written by Patchcord; edit while it is not running.\n\
         # node  = a PipeWire node that was there when the preset was saved\n\
         # link  = node | port > node | port\n\
         # split = device | group | in or out: a stereo jack shown as separate L and R jacks\n\
         # pos   = device | x | y: where a device sits on the desk\n\
         # mix   = device | group | in or out | volume % | balance % (-100 L … 100 R) | mute on or off:\n\
         #         an app stream's volume (or gain, recording)\n\
         # profile = device | profile: a card's profile, by its full name\n\
         # port  = device | group | in or out | port | latency offset in ms: a card's port\n\
         # default = out or in | node: the default output or input\n",
    );
    for p in presets {
        t += &format!("\n[{}]\n", p.name);
        for n in &p.nodes {
            t += &format!("node = {n}\n");
        }
        for ((on, op), (inn, ip)) in &p.links {
            t += &format!("link = {on} | {op} > {inn} | {ip}\n");
        }
        for (d, g, dir) in &p.split {
            t += &format!("split = {d} | {g} | {dir}\n");
        }
        for (k, xy) in &p.pos {
            t += &pos_line(k, *xy);
        }
        for ((k, g, dir), (vol, bal, mute)) in &p.mix {
            t += &format!("mix = {k} | {g} | {dir} | {} | {} | {}\n", (vol * 100.0).round(), (bal * 100.0).round(), if *mute { "on" } else { "off" });
        }
        for (k, name) in &p.profiles {
            t += &format!("profile = {k} | {name}\n");
        }
        for ((k, g, dir), (name, ns)) in &p.ports {
            t += &format!("port = {k} | {g} | {dir} | {name} | {}\n", *ns as f64 / 1e6);
        }
        if let Some(n) = &p.default_out {
            t += &format!("default = out | {n}\n");
        }
        if let Some(n) = &p.default_in {
            t += &format!("default = in | {n}\n");
        }
    }
    write("presets.txt", &t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_presets_survive_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("patchcord-test-{}", std::process::id()));
        // SAFETY: the only test that touches the environment
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };

        let mut s = Settings { cables: 2, lock: true, align: false, accents: false, flow: false, rack: true, mixer: true, window: Some((1600, 900)), preset: Some("HOME STUDIO".into()), pos: BTreeMap::new(), rack_order: vec!["node:Midi-Bridge".into(), "device:alsa_card.pci-0000_63_00.6".into()], hidden: vec!["node:BLE MIDI 1".into()], cue_level: 35 };
        s.pos.insert("device:bluez_card.C9_C1_60_6A_FD_E3".into(), (40.0, 214.0));
        s.pos.insert("node:VLC media player (LibVLC 4.0.0-dev)".into(), (-12.0, 388.0));
        s.save();
        assert_eq!(Settings::load(), s);

        let p = Preset {
            name: "HOME STUDIO".into(),
            nodes: vec!["bluez_input.C9:C1:60:6A:FD:E3".into(), "Midi-Bridge".into()],
            links: vec![
                (("VLC media player (LibVLC 4.0.0-dev)".into(), "output_FL".into()), ("bluez_output.C9_C1_60_6A_FD_E3.1".into(), "playback_FL".into())),
                (("Midi-Bridge".into(), "Midi Through: Port-0 (capture)".into()), ("bluez_midi.server".into(), "in".into())),
            ],
            split: vec![("device:alsa_card.pci-0000_63_00.6".into(), "PLAYBACK".into(), "in".into())],
            pos: s.pos.clone(),
            mix: BTreeMap::from([(("node:fooyin".into(), "OUTPUT".into(), "out".into()), (0.85, -0.3, false)), (("app:Audacity".into(), "INPUT".into(), "in".into()), (1.2, 0.0, true))]),
            profiles: BTreeMap::from([("device:bluez_card.C9_C1_60_6A_FD_E3".into(), "High Fidelity Playback (A2DP Sink, codec AAC)".into())]),
            ports: BTreeMap::from([(("device:alsa_card.pci-0000_63_00.6".into(), "PLAYBACK".into(), "in".into()), ("Headphones".into(), 20_500_000))]),
            default_out: Some("bluez_output.C9_C1_60_6A_FD_E3.1".into()),
            default_in: Some("alsa_input.pci-0000_63_00.6.analog-stereo".into()),
        };
        let q = Preset { name: "EMPTY".into(), ..Default::default() };
        save_presets(&[p.clone(), q.clone()]);
        assert_eq!(load_presets(), vec![p, q]);

        let _ = std::fs::remove_dir_all(dir);
    }
}
