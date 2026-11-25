//! A report saved by ENGINE's SAVE REPORT, read back as the events PipeWire would have sent: the
//! objects and their properties, the nodes' and links' states, the metadata, and (in reports
//! with a STATE section) the volumes, mute, cards' profiles and ports. From them a scene draws
//! the reporter's rack and MIXER (`shot::from_report`), with no PipeWire: only the file is read.
//! What a report doesn't keep isn't there: levels, and the reporter's own choices on the desk
//! (hidden devices, places, presets).

use std::collections::HashMap;

use crate::pw::{Avail, Event, Kind, Profile, Props, Route};

/// The states a report writes, as the `&'static str` the graph keeps.
fn state(s: &str) -> Option<&'static str> {
    ["running", "idle", "suspended", "creating", "error", "active", "paused", "init", "negotiating", "allocating", "unlinked"].into_iter().find(|x| *x == s)
}

/// The lines of each section ("GRAPH", "OBJECTS"…), by its name's first word.
fn sections(text: &str) -> HashMap<&str, Vec<&str>> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut name = "HEAD";
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("==== ") {
            name = rest.split_whitespace().next().unwrap_or("");
            out.entry(name).or_default();
        } else {
            out.entry(name).or_default().push(line);
        }
    }
    out
}

fn avail(s: &str) -> Avail {
    match s {
        "yes" => Avail::Yes,
        "no" => Avail::No,
        _ => Avail::Unknown,
    }
}

/// "85% 85%" as the linear volumes PipeWire keeps (the cube of the knob's scale).
fn volumes(s: &str) -> Vec<f32> {
    s.split_whitespace().filter_map(|v| v.strip_suffix('%')?.parse::<f32>().ok()).map(|p| (p / 100.0).powi(3)).collect()
}

/// "[1, 3, 4]" as numbers.
fn list(s: &str) -> Vec<i32> {
    s.trim_matches(['[', ']']).split(',').filter_map(|v| v.trim().parse().ok()).collect()
}

/// The value after `key ` in " · "-separated fields.
fn field<'a>(fields: &[&'a str], key: &str) -> Option<&'a str> {
    fields.iter().find_map(|f| f.strip_prefix(key)?.strip_prefix(' '))
}

/// "*1 name \"description\" · rest": in use, the index, the name, the description and the
/// fields after it.
fn entry(s: &str) -> Option<(bool, u32, &str, &str, Vec<&str>)> {
    let used = s.starts_with('*');
    let s = s[1..].trim_start();
    let (index, s) = s.split_once(' ')?;
    let (head, rest) = s.split_once("\" · ").map_or((s, ""), |(h, r)| (h, r));
    let (name, desc) = head.split_once(" \"")?;
    Some((used, index.parse().ok()?, name.trim(), desc.trim_end_matches('"'), rest.split(" · ").collect()))
}

/// A card as STATE lists it: its id, profiles, the one in use, its ports, the ones in use.
type Card = (u32, Vec<Profile>, Option<u32>, Vec<Route>, Vec<Route>);

/// The events a report stands for, ending with `Synced`.
pub fn events(text: &str) -> Vec<Event> {
    let s = sections(text);
    let get = |n: &str| s.get(n).map(Vec::as_slice).unwrap_or(&[]);
    let mut ev = Vec::new();
    if let Some(v) = get("SYSTEM").iter().find_map(|l| l.strip_prefix("pipewire")) {
        ev.push(Event::Version(v.trim().to_owned()));
    }
    // the states of nodes and links, from GRAPH
    let mut states: HashMap<u32, &'static str> = HashMap::new();
    for l in get("GRAPH") {
        if let Some(rest) = l.strip_prefix("node ")
            && let Some((id, _)) = rest.split_once(' ')
            && let Some(i) = rest.rfind("\" [")
            && let Some((_, after)) = rest[i..].split_once("] ")
            && let (Ok(id), Some(st)) = (id.parse(), state(after.split([' ', '·']).next().unwrap_or("")))
        {
            states.insert(id, st);
        }
        for (i, _) in l.match_indices("(link ") {
            let mut w = l[i + 6..].trim_end_matches(')').split([' ', ')']);
            if let (Some(Ok(id)), Some(Some(st))) = (w.next().map(str::parse), w.next().map(state)) {
                states.insert(id, st);
            }
        }
    }
    // every object, with its properties
    let mut objects: Vec<(Kind, u32, Props)> = Vec::new();
    for l in get("OBJECTS") {
        if let Some(rest) = l.strip_prefix("    ") {
            if let (Some(o), Some((k, v))) = (objects.last_mut(), rest.split_once(" = ")) {
                o.2.insert(k.to_owned(), v.to_owned());
            }
        } else if let Some((ty, id)) = l.split_once(' ')
            && let Ok(id) = id.trim().parse()
        {
            let kind = match ty {
                "Node" => Kind::Node,
                "Port" => Kind::Port,
                "Link" => Kind::Link,
                "Device" => Kind::Device,
                "Client" => Kind::Client,
                _ => Kind::Other,
            };
            let mut p = Props::new();
            if kind == Kind::Other {
                p.insert("object.type".into(), format!("PipeWire:Interface:{ty}"));
            }
            objects.push((kind, id, p));
        }
    }
    let metadata: HashMap<String, u32> = objects.iter().filter_map(|(_, id, p)| Some((p.get("metadata.name")?.clone(), *id))).collect();
    for (kind, id, props) in &objects {
        ev.push(Event::Added { id: *id, kind: *kind, props: props.clone() });
    }
    for (kind, id, props) in &objects {
        if matches!(kind, Kind::Node | Kind::Device | Kind::Link) {
            ev.push(Event::Info { id: *id, props: props.clone(), state: states.get(id).copied() });
        }
    }
    // the metadata: "name [subject] key (type) = value", a value going on over the lines after
    let mut meta: Vec<(String, u32, String, String, String)> = Vec::new();
    for l in get("METADATA") {
        let parsed = (|| {
            let (name, rest) = l.split_once(" [")?;
            let (subject, rest) = rest.split_once("] ")?;
            let (key, rest) = rest.split_once(" (")?;
            let (ty, value) = rest.split_once(") = ")?;
            Some((name.to_owned(), subject.parse().ok()?, key.to_owned(), ty.to_owned(), value.to_owned()))
        })();
        match parsed {
            Some(m) if !l.starts_with(' ') => meta.push(m),
            _ => {
                if let Some(m) = meta.last_mut()
                    && !l.is_empty()
                {
                    m.4.push('\n');
                    m.4.push_str(l);
                }
            }
        }
    }
    for (name, subject, key, ty, value) in meta {
        if matches!(name.as_str(), "settings" | "default") {
            ev.push(Event::Setting { key: key.clone(), value: value.clone() });
        }
        if let Some(id) = metadata.get(&name) {
            ev.push(Event::Meta { id: *id, subject, key: Some(key), ty: (!ty.is_empty()).then_some(ty), value: Some(value) });
        }
    }
    // STATE: volumes and mute, cards' profiles and ports
    let mut card: Option<Card> = None;
    let flush = |card: &mut Option<Card>, ev: &mut Vec<Event>| {
        if let Some((id, list, current, all, active)) = card.take() {
            if !list.is_empty() {
                ev.push(Event::Profiles { id, list, current });
            }
            if !all.is_empty() {
                ev.push(Event::Routes { id, all, active });
            }
        }
    };
    for l in get("STATE") {
        if let Some(rest) = l.strip_prefix("node ") {
            let f: Vec<&str> = rest.split(" · ").collect();
            if let Some((id, vol)) = f[0].split_once(" volume ")
                && let Ok(id) = id.parse()
            {
                ev.push(Event::Mix { id, volumes: volumes(vol), mute: field(&f, "mute") == Some("yes") });
            }
        } else if let Some(id) = l.strip_prefix("device ").and_then(|v| v.trim().parse().ok()) {
            flush(&mut card, &mut ev);
            card = Some((id, Vec::new(), None, Vec::new(), Vec::new()));
        } else if let Some(rest) = l.strip_prefix("    profile ")
            && let Some(c) = card.as_mut()
            && let Some((used, index, name, desc, f)) = entry(rest)
        {
            if used {
                c.2 = Some(index);
            }
            c.1.push(Profile { index, name: name.into(), description: desc.into(), available: avail(field(&f, "available").unwrap_or("?")), priority: field(&f, "priority").and_then(|v| v.parse().ok()).unwrap_or(0) });
        } else if let Some(rest) = l.strip_prefix("    port ")
            && let Some(c) = card.as_mut()
            && let Some((used, index, head, desc, f)) = entry(rest)
        {
            let (dir, name) = head.split_once(' ').unwrap_or(("out", head));
            let r = Route {
                index,
                output: dir == "out",
                device: None,
                name: name.trim().into(),
                description: desc.into(),
                available: avail(field(&f, "available").unwrap_or("?")),
                devices: field(&f, "devices").map(list).unwrap_or_default(),
                profiles: field(&f, "profiles").map(list).unwrap_or_default(),
                volumes: None,
                mute: false,
                latency_ns: 0,
            };
            if used {
                let ms: f64 = field(&f, "latency").and_then(|v| v.trim_end_matches(" ms").parse().ok()).unwrap_or(0.0);
                c.4.push(Route {
                    device: field(&f, "in use for device").and_then(|v| v.parse().ok()),
                    volumes: field(&f, "volume").map(volumes),
                    mute: field(&f, "mute") == Some("yes"),
                    latency_ns: (ms * 1e6) as i64,
                    ..r.clone()
                });
            }
            c.3.push(r);
        }
    }
    flush(&mut card, &mut ev);
    ev.push(Event::Synced);
    ev
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;

    /// What SAVE REPORT writes reads back as the same graph and state.
    #[test]
    fn a_report_reads_back_as_its_graph() {
        let text = [
            "Patchcord report · 2026-11-25 15:47:51",
            "",
            "==== SYSTEM ====",
            "",
            "pipewire      1.6.9",
            "==== GRAPH ====",
            "",
            "node 51 alsa_output.x \"Speakers\" [Audio/Sink] suspended · desk: X PLAYBACK",
            "    in  56 playback_FL",
            "node 60 app \"App\" [Stream/Output/Audio] running",
            "    out 61 output_FL  → alsa_output.x:playback_FL (link 70 active passive)",
            "==== STATE (volumes) ====",
            "",
            "node 60 volume 50% 50% · mute yes",
            "device 9",
            "    profile  0 off \"Off\" · available yes · priority 0",
            "    profile *1 hifi \"Duplex\" · available yes · priority 65",
            "    port *2 out analog-output-speaker \"Speakers\" · available ? · profiles [1] · devices [3] · in use for device 3 · volume 0% 0% · mute no · latency 1.5 ms",
            "==== METADATA ====",
            "",
            "default [0] default.audio.sink (Spa:String:JSON) = { \"name\": \"alsa_output.x\" }",
            "==== OBJECTS ====",
            "",
            "Metadata 40",
            "    metadata.name = default",
            "Device 9",
            "    device.name = alsa_card.x",
            "Node 51",
            "    node.name = alsa_output.x",
            "    media.class = Audio/Sink",
            "    device.id = 9",
            "Port 56",
            "    node.id = 51",
            "    port.direction = in",
            "    port.name = playback_FL",
            "Link 70",
            "    link.output.port = 61",
        ]
        .join("\n");
        let ev = events(&text);
        let mut g = Graph::default();
        for e in &ev {
            g.apply(e);
        }
        assert!(g.synced);
        assert_eq!(g.state_of(51), Some("suspended"));
        assert_eq!(g.state_of(70), Some("active"));
        assert!(ev.iter().any(|e| matches!(e, Event::Version(v) if v == "1.6.9")));
        assert!(ev.iter().any(|e| matches!(e, Event::Setting { key, value } if key == "default.audio.sink" && value.contains("alsa_output.x"))));
        assert!(ev.iter().any(|e| matches!(e, Event::Meta { id: 40, .. })));
        // the state written back is the state read
        let t = g.state_text();
        assert!(t.contains("node 60 volume 50% 50% · mute yes"), "{t}");
        assert!(t.contains("profile *1 hifi \"Duplex\" · available yes · priority 65"), "{t}");
        assert!(t.contains("port *2 out analog-output-speaker \"Speakers\" · available ? · profiles [1] · devices [3] · in use for device 3 · volume 0% 0% · mute no · latency 1.5 ms"), "{t}");
    }
}
