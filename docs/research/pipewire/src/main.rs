//! PipeWire spike: can Patchcord read the live graph and make and break links with the
//! `pipewire` crate?
//!
//! It is built the way the app would be: PipeWire runs its own loop on a thread of its own and
//! sends what it sees to the main thread over a channel; the main thread sends commands back.
//!
//!     pw-spike list                 print the graph once, grouped into devices like the panel
//!     pw-spike watch                print the graph, then every change as it happens
//!     pw-spike link OUT IN          connect two ports
//!     pw-spike unlink OUT IN        disconnect them
//!
//! A port is `NODE:PORT` (node name, description or nick, then port name) or a port id.

use pipewire as pw;
use pw::types::ObjectType;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    process::ExitCode,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

type Props = BTreeMap<String, String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Device,
    Node,
    Port,
    Link,
}

/// From the PipeWire thread to the main thread.
enum Event {
    Added { id: u32, kind: Kind, props: Props },
    Removed(u32),
    /// Everything that existed when we connected has been reported.
    Synced,
    Error(String),
}

/// From the main thread to the PipeWire thread.
enum Cmd {
    Link { out_node: u32, out_port: u32, in_node: u32, in_port: u32 },
    Unlink(u32),
    Quit,
}

// ---------------- the PipeWire thread ----------------

fn pw_thread(events: mpsc::Sender<Event>, cmds: pw::channel::Receiver<Cmd>) {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).expect("PipeWire main loop");
    let context = pw::context::ContextRc::new(&mainloop, None).expect("PipeWire context");
    let core = match context.connect_rc(None) {
        Ok(core) => core,
        Err(e) => {
            let _ = events.send(Event::Error(format!("cannot connect to PipeWire: {e}")));
            return;
        }
    };
    let registry = core.get_registry_rc().expect("PipeWire registry");

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let events = events.clone();
            move |g| {
                let kind = match g.type_ {
                    ObjectType::Device => Kind::Device,
                    ObjectType::Node => Kind::Node,
                    ObjectType::Port => Kind::Port,
                    ObjectType::Link => Kind::Link,
                    _ => return,
                };
                let props = g
                    .props
                    .map(|p| p.iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect())
                    .unwrap_or_default();
                let _ = events.send(Event::Added { id: g.id, kind, props });
            }
        })
        .global_remove({
            let events = events.clone();
            move |id| {
                let _ = events.send(Event::Removed(id));
            }
        })
        .register();

    // the answer to this sync arrives after every object that already exists
    let pending = core.sync(0).expect("PipeWire sync");
    let _core_listener = core
        .add_listener_local()
        .done({
            let events = events.clone();
            move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    let _ = events.send(Event::Synced);
                }
            }
        })
        .error({
            let events = events.clone();
            move |id, _seq, res, message| {
                let _ = events.send(Event::Error(format!("object {id}: {message} ({res})")));
            }
        })
        .register();

    // links we created; with object.linger they outlive us, but we keep the proxies while we run
    let made: RefCell<Vec<pw::link::Link>> = RefCell::new(Vec::new());
    let _commands = cmds.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        let core = core.clone();
        let registry = registry.clone();
        let events = events.clone();
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
                    Err(e) => {
                        let _ = events.send(Event::Error(format!("cannot create link: {e}")));
                    }
                }
            }
            Cmd::Unlink(id) => {
                registry.destroy_global(id);
            }
            Cmd::Quit => mainloop.quit(),
        }
    });

    mainloop.run();
}

// ---------------- the graph, on the main thread ----------------

#[derive(Default)]
struct Graph {
    objects: HashMap<u32, (Kind, Props)>,
}

impl Graph {
    fn apply(&mut self, ev: &Event) {
        match ev {
            Event::Added { id, kind, props } => {
                self.objects.insert(*id, (*kind, props.clone()));
            }
            Event::Removed(id) => {
                self.objects.remove(id);
            }
            _ => {}
        }
    }

    fn of(&self, kind: Kind) -> impl Iterator<Item = (u32, &Props)> {
        self.objects.iter().filter(move |(_, (k, _))| *k == kind).map(|(id, (_, p))| (*id, p))
    }

    fn prop(&self, id: u32, key: &str) -> Option<&str> {
        self.objects.get(&id).and_then(|(_, p)| p.get(key)).map(String::as_str)
    }

    fn node_title(&self, node: u32) -> String {
        ["node.description", "node.nick", "application.name", "node.name"]
            .iter()
            .find_map(|k| self.prop(node, k))
            .unwrap_or("?")
            .to_owned()
    }

    fn port_name(&self, port: u32) -> String {
        let node = self.prop(port, "node.id").and_then(|n| n.parse().ok()).unwrap_or(0);
        format!("{}:{}", self.node_title(node), self.prop(port, "port.name").unwrap_or("?"))
    }

    /// `NODE:PORT` (node name, description or nick) or a port id, in the given direction
    fn find_port(&self, spec: &str, dir: &str) -> Result<(u32, u32), String> {
        let pick = |id: u32| -> Option<(u32, u32)> {
            if self.prop(id, "port.direction") != Some(dir) {
                return None;
            }
            let node = self.prop(id, "node.id")?.parse().ok()?;
            Some((node, id))
        };
        if let Ok(id) = spec.parse::<u32>() {
            return pick(id).ok_or(format!("port {id} is not an {dir}put port"));
        }
        // port names can hold ':' too ("Midi Through: Port-0 (capture)"), so try every split
        let mut found: Vec<_> = spec
            .match_indices(':')
            .flat_map(|(i, _)| {
                let (node_spec, port_spec) = (&spec[..i], &spec[i + 1..]);
                self.of(Kind::Port)
                    .filter(move |(_, p)| p.get("port.name").map(String::as_str) == Some(port_spec))
                    .filter_map(|(id, _)| pick(id))
                    .filter(move |(node, _)| {
                        ["node.name", "node.description", "node.nick"].iter().any(|k| self.prop(*node, k) == Some(node_spec))
                    })
            })
            .collect();
        found.dedup();
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("no {dir}put port '{spec}'")),
            _ => Err(format!("'{spec}' matches {} {dir}put ports, use a port id", found.len())),
        }
    }

    fn link_between(&self, out_port: u32, in_port: u32) -> Option<u32> {
        self.of(Kind::Link)
            .find(|(_, p)| {
                p.get("link.output.port") == Some(&out_port.to_string())
                    && p.get("link.input.port") == Some(&in_port.to_string())
            })
            .map(|(id, _)| id)
    }

    /// Nodes grouped the way the panel would show them: one box per device (a card's sink, source
    /// and monitor together), and one box for each node that has no device (apps, streams, MIDI).
    fn print(&self) {
        let mut boxes: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (node, p) in self.of(Kind::Node) {
            let key = match p.get("device.id") {
                Some(dev) => format!("device {dev}"),
                None => format!("node {node}"),
            };
            boxes.entry(key).or_default().push(node);
        }
        for (key, mut nodes) in boxes {
            nodes.sort();
            let title = match key.strip_prefix("device ").and_then(|d| d.parse::<u32>().ok()) {
                Some(dev) => self
                    .prop(dev, "device.description")
                    .or(self.prop(dev, "device.nick"))
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.node_title(nodes[0])),
                None => self.node_title(nodes[0]),
            };
            println!("\n■ {title}  [{key}]");
            for node in nodes {
                println!(
                    "  node {node} · {} · {}{}",
                    self.prop(node, "media.class").unwrap_or("-"),
                    self.prop(node, "node.name").unwrap_or("-"),
                    self.prop(node, "node.nick").map(|n| format!(" · nick \"{n}\"")).unwrap_or_default(),
                );
                let mut ports: Vec<_> = self
                    .of(Kind::Port)
                    .filter(|(_, p)| p.get("node.id") == Some(&node.to_string()))
                    .map(|(id, p)| (p.get("port.direction").cloned().unwrap_or_default(), id))
                    .collect();
                ports.sort();
                for (dir, port) in ports {
                    let dsp = self.prop(port, "format.dsp").unwrap_or("");
                    let kind = if dsp.contains("midi") { "midi" } else if dsp.contains("audio") { "audio" } else { dsp };
                    let monitor = if self.prop(port, "port.monitor") == Some("true") { " · monitor" } else { "" };
                    println!(
                        "    {:<3} {:<28} port {port} · {kind}{monitor}",
                        dir,
                        self.prop(port, "port.name").unwrap_or("?")
                    );
                }
            }
        }
        let mut links: Vec<_> = self.of(Kind::Link).collect();
        links.sort_by_key(|(id, _)| *id);
        println!("\nlinks ({}):", links.len());
        for (id, p) in links {
            let port = |k: &str| p.get(k).and_then(|v| v.parse().ok()).map(|id| self.port_name(id)).unwrap_or("?".into());
            println!("  {} → {}  [link {id}]", port("link.output.port"), port("link.input.port"));
        }
    }

    fn describe(&self, id: u32) -> String {
        match self.objects.get(&id) {
            Some((Kind::Port, _)) => format!("port {}", self.port_name(id)),
            Some((Kind::Node, _)) => format!("node \"{}\"", self.node_title(id)),
            Some((Kind::Device, p)) => format!("device \"{}\"", p.get("device.description").map_or("?", |s| s)),
            Some((Kind::Link, p)) => {
                let port = |k: &str| p.get(k).and_then(|v| v.parse().ok()).map(|id| self.port_name(id)).unwrap_or("?".into());
                format!("link {} → {}", port("link.output.port"), port("link.input.port"))
            }
            None => format!("object {id}"),
        }
    }
}

// ---------------- main ----------------

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("list");
    if !matches!(cmd, "list" | "watch" | "link" | "unlink") || (matches!(cmd, "link" | "unlink") && args.len() != 3) {
        eprintln!("usage: pw-spike list | watch | link OUT IN | unlink OUT IN");
        return ExitCode::from(2);
    }

    let started = Instant::now();
    let (ev_tx, ev_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = pw::channel::channel();
    let pw = thread::spawn(move || pw_thread(ev_tx, cmd_rx));
    let quit = |code: u8| {
        let _ = cmd_tx.send(Cmd::Quit);
        let _ = pw.join();
        ExitCode::from(code)
    };

    // wait for the whole graph as it is now
    let mut graph = Graph::default();
    loop {
        match ev_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::Synced) => break,
            Ok(Event::Error(e)) => {
                eprintln!("error: {e}");
                return quit(1);
            }
            Ok(ev) => graph.apply(&ev),
            Err(_) => {
                eprintln!("error: PipeWire did not answer in 5 s");
                return quit(1);
            }
        }
    }
    let synced = started.elapsed();
    let count = |k| graph.of(k).count();
    eprintln!(
        "graph in {:.1} ms: {} devices, {} nodes, {} ports, {} links",
        synced.as_secs_f64() * 1000.0,
        count(Kind::Device),
        count(Kind::Node),
        count(Kind::Port),
        count(Kind::Link)
    );

    match cmd {
        "list" => {
            graph.print();
            quit(0)
        }
        "watch" => {
            graph.print();
            eprintln!("\nwatching, Ctrl+C to stop");
            for ev in ev_rx.iter() {
                match &ev {
                    Event::Added { id, .. } => {
                        graph.apply(&ev);
                        println!("+ {}", graph.describe(*id));
                    }
                    // clients and other objects we don't track also leave; skip those
                    Event::Removed(id) if graph.objects.contains_key(id) => {
                        println!("- {}", graph.describe(*id));
                        graph.apply(&ev);
                    }
                    Event::Removed(_) => {}
                    Event::Error(e) => eprintln!("error: {e}"),
                    Event::Synced => {}
                }
            }
            quit(0)
        }
        _ => {
            let ends = graph.find_port(&args[1], "out").and_then(|o| graph.find_port(&args[2], "in").map(|i| (o, i)));
            let ((out_node, out_port), (in_node, in_port)) = match ends {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("error: {e}");
                    return quit(1);
                }
            };
            let existing = graph.link_between(out_port, in_port);
            let t0 = Instant::now();
            let wanted = if cmd == "link" {
                if let Some(id) = existing {
                    eprintln!("already linked [link {id}]");
                    return quit(0);
                }
                let _ = cmd_tx.send(Cmd::Link { out_node, out_port, in_node, in_port });
                None
            } else {
                let Some(id) = existing else {
                    eprintln!("not linked");
                    return quit(1);
                };
                let _ = cmd_tx.send(Cmd::Unlink(id));
                Some(id)
            };
            // wait until PipeWire tells us the link appeared (or went away)
            loop {
                match ev_rx.recv_timeout(Duration::from_secs(3)) {
                    Ok(ev @ Event::Added { kind: Kind::Link, .. }) => {
                        graph.apply(&ev);
                        if wanted.is_none() && let Some(id) = graph.link_between(out_port, in_port) {
                            println!("linked {} → {}  [link {id}] in {:.1} ms", graph.port_name(out_port), graph.port_name(in_port), t0.elapsed().as_secs_f64() * 1000.0);
                            return quit(0);
                        }
                    }
                    Ok(Event::Removed(id)) if Some(id) == wanted => {
                        println!("unlinked {} → {} in {:.1} ms", graph.port_name(out_port), graph.port_name(in_port), t0.elapsed().as_secs_f64() * 1000.0);
                        return quit(0);
                    }
                    Ok(Event::Error(e)) => {
                        eprintln!("error: {e}");
                        return quit(1);
                    }
                    Ok(ev) => graph.apply(&ev),
                    Err(_) => {
                        eprintln!("error: no answer from PipeWire in 3 s");
                        return quit(1);
                    }
                }
            }
        }
    }
}
