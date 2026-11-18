//! The window: puts the field on screen and turns mouse, wheel and keys into actions.

use std::sync::Arc;
use std::time::{Duration, Instant};

use resvg::tiny_skia::{Color, FillRule, GradientStop, Paint, PathBuilder, Pixmap, Point, RadialGradient, Rect, SpreadMode, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorIcon, Window, WindowId};

use super::cord::Held;
use super::ease;
use super::face::{BalKnob, LCD_CH};
use super::gpu::{Gpu, Sprite};
use super::panel::{self, Key, Menu, MenuHit, Panel, ProfileHit};
use crate::config::{self, Preset, Settings};
use super::scene::{Hover, Layer, Scene, View, next_id};
use crate::graph::VOL_MAX;
use crate::pw::{Cmd, Connection};

/// After zooming, faces are drawn sharp again once the zoom has been still this long.
const RESCALE_DELAY: Duration = Duration::from_millis(250);
/// How often a long name on an LCD moves on.
const MARQUEE: Duration = Duration::from_millis(280);

struct Surface {
    window: Arc<Window>,
    gpu: Gpu,
}

enum Drag {
    Pan { from: (f64, f64), view: View },
    Device { key: String, off: (f64, f64) },
    Cord { seq: u64 },
    /// a cord's end in the hand: a weight on a spring to the pointer, pulled into a jack nearby
    Plug { seq: u64, p: (f32, f32), v: (f32, f32), magnet: Option<String>, targets: Vec<String> },
    /// the cords switch: dragged, it steps between its positions; clicked, it moves on one
    Switch { from: (f64, f64), pos0: u8, moved: bool },
    /// a panel key held down
    Key,
    /// the cue's LEVEL knob, turned by dragging up and down
    CueLevel { from: f64, v0: f64 },
    /// an app's VOLUME knob, turned by dragging up and down: the pointer's last height and the
    /// volume it has turned to (before resting at 100%)
    Knob { key: String, gi: usize, last: f64, raw: f64 },
    /// an app's BAL knob, pressed: it turns once the pointer moves (if it was out), else the
    /// release pops it out or pushes it in
    Bal { key: String, gi: usize, y0: f64, last: f64, raw: f64, was: BalKnob, turned: bool },
}

/// How far the pointer moves (logical pixels) to turn a knob all the way.
const KNOB_TRAVEL: f64 = 200.0;

/// How long a notice stays.
const NOTICE: Duration = Duration::from_millis(1800);
/// How long the pointer rests on something before its tooltip shows.
const TIP_DELAY: Duration = Duration::from_millis(450);

/// The list of the outputs and inputs that can be the default, and which ones are.
pub fn defaults_menu(scene: &Scene) -> Menu {
    let (outs, ins) = (scene.ends(true), scene.ends(false));
    let (dout, din) = (scene.default_name(true), scene.default_name(false));
    let at = |v: &[(usize, usize, String, String)], n: &Option<String>| v.iter().position(|e| Some(&e.2) == n.as_ref());
    Menu::Defaults { out: at(&outs, &dout), inp: at(&ins, &din), outs: outs.into_iter().map(|e| e.3).collect(), ins: ins.into_iter().map(|e| e.3).collect() }
}

/// Listening (cue): jacks heard in the default output through a little node of Patchcord's.
/// Holding Alt probes the jack under the pointer; CUE mode (the CUE key) hears the hovered jack,
/// and a click pins one to keep playing, as many as wanted.
#[derive(Default)]
struct Cue {
    mode: bool,
    probing: bool,
    hover: Option<String>,
    pins: Vec<String>,
    /// 0…1, as the knob shows it (the volume is its cube)
    level: f64,
    /// what PipeWire was last told
    sent: Option<(Vec<(u32, u32, Option<usize>)>, u32, bool)>,
    badge: Option<(u64, Layer)>,
}

/// How long a device found from the defaults list glows.
const FOUND: Duration = Duration::from_millis(1600);

/// A second click this soon, this close, is a double-click.
const DOUBLE: Duration = Duration::from_millis(350);

/// A card's profile list, open under its PROFILE key: drawn again when the card changes, placed
/// under the key wherever the view puts it.
struct ProfileList {
    /// the device whose list it is
    key: String,
    /// its ports and latency offsets (PORT) instead of its profiles
    port: bool,
    /// what it was drawn from
    sig: u64,
    layer: Option<Layer>,
    /// hit areas, relative to the list, in panel units
    hits: Vec<(ProfileHit, [f64; 4])>,
    /// its top left corner, in window pixels
    at: (f64, f64),
}

pub struct App {
    conn: Connection,
    scene: Scene,
    win: Option<Surface>,
    size: (u32, u32),
    view: View,
    /// where the view is gliding to (fit, arrange)
    view_to: Option<View>,
    fitted: bool,
    cursor: (f64, f64),
    mods: ModifiersState,
    drag: Option<Drag>,
    /// the scale the faces are drawn at, and when to draw them at the view's scale
    raster: f64,
    rescale_at: Option<Instant>,
    bg: Option<(u64, Pixmap)>,
    /// one pixel of the alignment guides' colour, stretched into each line
    guide: (u64, Pixmap),
    last: Instant,
    start: Instant,
    animating: bool,
    next_frame: Instant,
    /// the last press, for double-clicks
    last_press: (Instant, (f64, f64)),
    panel: Panel,
    /// what is under the pointer, for the status bar
    hint: String,
    status: Option<(String, Layer)>,
    notice: Option<(Instant, Layer)>,
    /// since when the pointer is over what it is over, and that thing's tooltip once shown
    hover_since: Instant,
    tip: Option<Layer>,
    /// a card's profile list, open under its PROFILE key
    profiles: Option<ProfileList>,
    /// the glow around the device under the pointer: whose, and what it was drawn for
    glow: Option<(String, u64, Layer)>,
    /// a device found from the defaults list: it glows a moment, fading
    found: Option<(String, Instant, Layer)>,
    cue: Cue,
    settings: Settings,
    presets: Vec<Preset>,
    /// the preset last saved or loaded
    current: Option<usize>,
}

impl App {
    pub fn new(conn: Connection) -> Self {
        let now = Instant::now();
        let settings = Settings::load();
        let presets = config::load_presets();
        let current = settings.preset.as_ref().and_then(|n| presets.iter().position(|p| p.name == *n));
        let mut scene = Scene::new();
        scene.cables = settings.cables;
        scene.saved_pos = settings.pos.clone().into_iter().collect();
        scene.rack = settings.rack;
        scene.align = settings.align;
        scene.accents = settings.accents;
        scene.rack_order = settings.rack_order.clone();
        App {
            conn,
            scene,
            win: None,
            size: (1, 1),
            view: View { x: 0.0, y: 0.0, s: 1.0 },
            view_to: None,
            fitted: false,
            cursor: (0.0, 0.0),
            mods: ModifiersState::empty(),
            drag: None,
            raster: 0.0,
            rescale_at: None,
            bg: None,
            guide: (next_id(), {
                let mut pm = Pixmap::new(1, 1).unwrap();
                pm.fill(Color::from_rgba8(255, 200, 90, 255));
                pm
            }),
            last: now,
            start: now,
            animating: true,
            next_frame: now,
            last_press: (now, (0.0, 0.0)),
            panel: Panel::new(panel::State { rack: false, locked: settings.lock, align: settings.align, cables: settings.cables, sound: false, lcd: [String::new(), String::new()], defaults: [String::new(), String::new()], cue_led: false, cue_level: 70, pressed: None }),
            hint: String::new(),
            status: None,
            notice: None,
            hover_since: now,
            tip: None,
            profiles: None,
            found: None,
            cue: Cue { level: 0.7, ..Default::default() },
            glow: None,
            settings,
            presets,
            current,
        }
    }

    /// Writes settings.txt when something in it changed.
    fn save_settings(&mut self) {
        let mut s = self.settings.clone();
        s.cables = self.scene.cables;
        s.lock = self.panel.state.locked;
        s.align = self.scene.align;
        s.accents = self.scene.accents;
        s.rack = self.scene.rack;
        s.rack_order = self.scene.rack_order.clone();
        s.preset = self.current.map(|i| self.presets[i].name.clone());
        if let Some(sf) = &self.win {
            let size = sf.window.inner_size().to_logical::<f64>(sf.window.scale_factor());
            s.window = Some((size.width.round() as u32, size.height.round() as u32));
        }
        // devices not here now keep their remembered place
        for (k, p) in self.scene.positions() {
            s.pos.insert(k.clone(), p);
            self.scene.saved_pos.insert(k, p);
        }
        if s != self.settings {
            s.save();
            self.settings = s;
        }
    }

    /// Opens the save field, or the preset list (or closes it when it is open).
    fn open_menu(&mut self, save: bool) {
        self.panel.menu = match (&self.panel.menu, save) {
            (Some(Menu::Save { .. }), true) | (Some(Menu::Load { .. }), false) => None,
            (_, true) => Some(Menu::Save { text: String::new(), placeholder: format!("PRESET {}", self.presets.len() + 1) }),
            (_, false) => Some(Menu::Load { names: self.presets.iter().map(|p| p.name.clone()).collect(), current: self.current, confirm: None }),
        };
        self.redraw();
    }

    /// Saves the patch as a new preset, named as typed (or "PRESET n").
    fn save_preset(&mut self, typed: &str) {
        let base = match typed.trim() {
            "" => format!("PRESET {}", self.presets.len() + 1),
            t => t.to_uppercase(),
        };
        // names stay unique: they head the sections of presets.txt
        let mut name = base.clone();
        let mut n = 2;
        while self.presets.iter().any(|p| p.name == name) {
            name = format!("{base} {n}");
            n += 1;
        }
        self.presets.push(self.scene.snapshot(&name));
        config::save_presets(&self.presets);
        self.current = Some(self.presets.len() - 1);
        self.scene.edited = false;
        self.panel.menu = None;
        self.save_settings();
        self.say(&format!("Saved preset {} · {name}", self.presets.len()));
    }

    fn load_preset(&mut self, i: usize) {
        let p = self.presets[i].clone();
        let (links, missing) = self.scene.apply_preset(&p);
        self.after_patch();
        self.current = Some(i);
        self.panel.menu = None;
        self.save_settings();
        let skipped = if missing > 0 { format!(", {missing} skipped (device not here)") } else { String::new() };
        self.say(&format!("Loaded {} · {links} links{skipped}", p.name));
    }

    /// The delete button asks once ("DEL?"), then deletes.
    fn delete_preset(&mut self, i: usize) {
        if let Some(Menu::Load { confirm, .. }) = &mut self.panel.menu
            && *confirm != Some(i)
        {
            *confirm = Some(i);
            return self.redraw();
        }
        let name = self.presets.remove(i).name;
        config::save_presets(&self.presets);
        self.current = match self.current {
            Some(c) if c == i => None,
            Some(c) if c > i => Some(c - 1),
            c => c,
        };
        self.save_settings();
        self.panel.menu = None;
        self.open_menu(false);
        self.say(&format!("Deleted preset {name}"));
    }

    /// Typing into the save field. True when the key was used.
    fn type_key(&mut self, code: KeyCode, text: Option<&str>) -> bool {
        let Some(Menu::Save { text: t, .. }) = &mut self.panel.menu else { return false };
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let typed = t.clone();
                self.save_preset(&typed);
            }
            KeyCode::Escape => self.panel.menu = None,
            KeyCode::Backspace => {
                t.pop();
            }
            _ => {
                for c in text.unwrap_or("").chars().filter(|c| !c.is_control() && *c != '[' && *c != ']') {
                    if t.chars().count() < 24 {
                        t.push(c);
                    }
                }
            }
        }
        self.redraw();
        true
    }

    fn dpi(&self) -> f64 {
        self.win.as_ref().map_or(1.0, |sf| sf.window.scale_factor())
    }

    /// The patch field's top and bottom edges, in window pixels (between the panel and the status bar).
    fn field(&self) -> (f64, f64) {
        let dpi = self.dpi();
        (panel::H * dpi, self.size.1 as f64 - panel::STATUS_H * dpi)
    }

    /// Shows a short notice under the panel.
    fn say(&mut self, msg: &str) {
        self.notice = Some((Instant::now() + NOTICE, panel::notice(msg, self.dpi())));
        self.redraw();
    }

    /// Opens the list of the outputs and inputs that can be the default (`toggle`: or closes it),
    /// or draws it again with what is the default now.
    fn open_defaults(&mut self, toggle: bool) {
        if toggle && matches!(self.panel.menu, Some(Menu::Defaults { .. })) {
            self.panel.menu = None;
            return self.redraw();
        }
        self.panel.menu = Some(self.defaults_menu());
        self.redraw();
    }

    fn defaults_menu(&self) -> Menu {
        defaults_menu(&self.scene)
    }

    /// The jacks heard: the pinned ones and the hovered one.
    fn cue_heard(&self) -> Vec<String> {
        let mut v = self.cue.pins.clone();
        if let Some(h) = &self.cue.hover
            && !v.contains(h)
        {
            v.push(h.clone());
        }
        v
    }

    /// Where to draw the rings of the jacks heard: (position, pinned, would feed back).
    fn cue_marks(&self) -> Vec<((f32, f32), bool, bool)> {
        self.cue_heard().iter().filter_map(|k| Some((self.scene.jack_pos(k)?, self.cue.pins.contains(k), self.scene.cue_blocked(k)))).collect()
    }

    /// Tells PipeWire what to hear, when that changed (the jacks, what is linked into an input
    /// jack, the default output, the level).
    fn cue_sync(&mut self) {
        let heard: Vec<String> = self.cue_heard().into_iter().filter(|k| !self.scene.cue_blocked(k)).collect();
        let sources = self.scene.cue_mix(&heard);
        // in CUE mode the cue stays, between one jack and the next
        let keep = self.cue.mode;
        let level = (self.cue.level.powi(3) as f32).to_bits();
        let want = (sources, level, keep);
        if self.cue.sent.as_ref() != Some(&want) {
            self.conn.send(Cmd::Cue { sources: want.0.clone(), level: f32::from_bits(level), keep });
            self.cue.sent = Some(want);
        }
    }

    /// The status bar while listening.
    fn cue_hint(&self) -> String {
        if let Some(h) = &self.cue.hover
            && self.scene.cue_blocked(h)
        {
            return format!("can't listen to {}: it is the output you listen on (it would feed back)", self.scene.jack_title(h));
        }
        let heard: Vec<String> = self
            .cue_heard()
            .iter()
            .filter(|k| !self.scene.cue_blocked(k))
            .map(|k| if self.scene.cue_already_heard(k) { format!("{} (already in the output you listen on)", self.scene.jack_title(k)) } else { self.scene.jack_title(k) })
            .collect();
        let what = if heard.is_empty() { "move over a jack to listen to it".to_owned() } else { format!("CUE ▸ {} · in the default output at {:.0}%", heard.join(" + "), self.cue.level * 100.0) };
        if self.cue.mode {
            let click = match &self.cue.hover {
                Some(h) if self.cue.pins.contains(h) => "  ·  click: stop it",
                Some(_) => "  ·  click: keep it on",
                None => "  ·  click a jack to keep it on; CUE or Esc stops all",
            };
            format!("{what}{click}")
        } else {
            what
        }
    }

    /// CUE mode on or off; off, every jack heard stops.
    fn set_cue_mode(&mut self, on: bool) {
        self.cue.mode = on;
        if !on {
            self.cue.pins.clear();
            if !self.cue.probing {
                self.cue.hover = None;
            }
        }
        self.say(if on { "CUE: hover a jack to hear it, click to keep it on" } else { "Cue off" });
        self.redraw();
    }

    /// Glides the view to a device and makes it glow for a moment.
    fn find_device(&mut self, di: usize) {
        let dpi = self.dpi();
        let (top, bottom) = self.field();
        let Some(d) = self.scene.devs.get(di) else { return };
        let s = self.view.s.max(0.8 * dpi);
        let (cx, cy) = (d.x + d.geo.w / 2.0, d.y + d.h() / 2.0);
        self.view_to = Some(View { s, x: self.size.0 as f64 / 2.0 - cx * s, y: (top + bottom) / 2.0 - cy * s });
        self.found = Some((d.dev.key.clone(), Instant::now(), glow_layer(d.geo.w, d.h(), self.scene.rack, self.raster)));
        self.redraw();
    }

    /// What a panel key does.
    fn panel_key(&mut self, key: Key) {
        match key {
            Key::Undo => {
                if self.scene.undo() {
                    self.after_patch();
                }
            }
            Key::Redo => {
                if self.scene.redo() {
                    self.after_patch();
                }
            }
            Key::Lock => {
                self.panel.state.locked = !self.panel.state.locked;
                self.save_settings();
            }
            Key::Align => {
                self.scene.align = !self.scene.align;
                self.save_settings();
            }
            Key::Cables => {
                self.scene.cables = (self.scene.cables + 1) % 3;
                self.save_settings();
            }
            Key::CablesAt(n) => {
                self.scene.cables = n.min(2);
                self.save_settings();
            }
            Key::Arrange => {
                self.scene.arrange();
                self.fit(true);
            }
            Key::Fit => self.fit(true),
            Key::Desk | Key::Rack => {
                self.scene.set_rack(key == Key::Rack);
                self.save_settings();
                self.fit(true);
            }
            Key::Save => self.open_menu(true),
            Key::Load | Key::Lcd => self.open_menu(false),
            Key::Defaults => self.open_defaults(true),
            Key::Cue => self.set_cue_mode(!self.cue.mode),
            Key::CueLevel => {}
            Key::Sound => self.say("Plug sounds are not implemented yet"),
        }
        self.redraw();
    }

    /// The panel LCD: the preset, and PipeWire's clock (rate, quantum and the latency they make).
    fn lcd(&self) -> [String; 2] {
        let num = |k: &str| self.scene.settings.get(k).and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0);
        let rate = num("clock.force-rate").or(num("clock.rate"));
        let quantum = num("clock.force-quantum").or(num("clock.quantum"));
        let clock = match (rate, quantum) {
            (Some(r), Some(q)) => format!("{} kHz · {q} · {:.1} ms", r as f64 / 1000.0, q as f64 * 1000.0 / r as f64),
            _ => "PIPEWIRE".to_owned(),
        };
        let preset = match self.current {
            Some(i) => format!("PRESET {:02} · {}{}", i + 1, self.presets[i].name, if self.scene.edited { " *" } else { "" }),
            None => "NO PRESET".to_owned(),
        };
        [preset, clock]
    }

    fn world(&self, p: (f64, f64)) -> (f64, f64) {
        ((p.0 - self.view.x) / self.view.s, (p.1 - self.view.y) / self.view.s)
    }

    fn redraw(&mut self) {
        self.animating = true;
        if let Some(sf) = &self.win {
            sf.window.request_redraw();
        }
    }

    fn fit(&mut self, glide: bool) {
        let dpi = self.dpi();
        let (top, bottom) = self.field();
        let v = self.scene.fit(self.size.0 as f64, bottom - top, top, 1.3 * dpi);
        if glide {
            self.view_to = Some(v);
        } else {
            self.view = v;
        }
        self.redraw();
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.05);
        self.last = now;
        if let Some(t) = self.view_to {
            let v = &mut self.view;
            let k = ease(0.15, dt) as f64;
            v.x += (t.x - v.x) * k;
            v.y += (t.y - v.y) * k;
            v.s += (t.s - v.s) * k;
            if (t.s - v.s).abs() < 0.001 && (t.x - v.x).abs() < 0.5 && (t.y - v.y).abs() < 0.5 {
                self.view = t;
                self.view_to = None;
            }
        }
        let hand = match &self.drag {
            Some(Drag::Cord { .. }) => {
                let w = self.world(self.cursor);
                Some((w.0 as f32, w.1 as f32))
            }
            _ => None,
        };
        let cursor = self.world(self.cursor);
        let mut held = None;
        let mut rings = Vec::new();
        if let Some(Drag::Plug { seq, p, v, magnet, targets, .. }) = &mut self.drag {
            // the end is a weight on a spring to the pointer; near a jack it is pulled in
            let spot = magnet.as_ref().and_then(|m| self.scene.spare(m));
            let to = spot.unwrap_or((cursor.0 as f32, cursor.1 as f32));
            let k = if spot.is_some() { 1200.0 } else { 650.0 };
            let damp = 2.0 * f32::sqrt(k);
            // small fixed steps: one big step on a slow frame would fling the end away
            let n = (dt / 0.004).ceil().max(1.0);
            let h = dt / n;
            for _ in 0..n as usize {
                v.0 += ((to.0 - p.0) * k - v.0 * damp) * h;
                v.1 += ((to.1 - p.1) * k - v.1 * damp) * h;
                p.0 += v.0 * h;
                p.1 += v.1 * h;
            }
            if !(p.0.is_finite() && p.1.is_finite()) {
                (*p, *v) = (to, (0.0, 0.0));
            }
            let kind = magnet.as_ref().and_then(|m| self.scene.jack_kind(m));
            held = Some((*seq, Held { x: p.0, y: p.1, kind }));
            for t in targets.iter() {
                if let Some(pos) = self.scene.spare(t) {
                    rings.push((pos, magnet.as_ref() == Some(t)));
                }
            }
        }
        let moving = self.scene.tick(dt, hand, held, matches!(self.drag, Some(Drag::Device { .. })));

        // faces are drawn at one scale; after zooming, sharp ones replace the stretched ones
        if self.raster == 0.0 {
            self.raster = self.view.s;
        } else if (self.view.s - self.raster).abs() > 1e-3 {
            match self.rescale_at {
                Some(t) if now >= t && self.view_to.is_none() => {
                    self.raster = self.view.s;
                    self.rescale_at = None;
                }
                Some(_) => {}
                None => self.rescale_at = Some(now + RESCALE_DELAY),
            }
        } else {
            // zoomed back to the scale they are drawn at
            self.rescale_at = None;
        }
        let faces_waiting = self.scene.prepare(self.raster, (now - self.start).as_millis());
        if self.bg.as_ref().is_none_or(|(_, pm)| (pm.width(), pm.height()) != self.size) {
            self.bg = Some((next_id(), background(self.size.0, self.size.1)));
        }
        // the panel, the status bar and a notice, over the field
        let dpi = self.dpi();
        let width = self.size.0 as f64 / dpi;
        self.panel.state.cables = self.scene.cables;
        self.panel.state.align = self.scene.align;
        self.panel.state.rack = self.scene.rack;
        self.panel.state.lcd = self.lcd();
        self.panel.state.defaults = [true, false].map(|output| self.scene.default_shown(output));
        // the CUE LED: lit in CUE mode, blinking while only probing with Alt
        let heard = self.cue_heard().iter().any(|k| !self.scene.cue_blocked(k));
        self.panel.state.cue_led = self.cue.mode || (heard && (now - self.start).as_millis() / 250 % 2 == 0);
        self.panel.state.cue_level = (self.cue.level * 100.0).round() as u8;
        self.cue_sync();
        // the open defaults list follows what is the default
        if let Some(Menu::Defaults { .. }) = &self.panel.menu {
            let m = self.defaults_menu();
            if self.panel.menu.as_ref() != Some(&m) {
                self.panel.menu = Some(m);
            }
        }
        self.panel.layer(width, dpi);
        self.panel.menu_layer(width, dpi);
        let left = format!(
            "PipeWire {} · {} devices · {} links · {} cords",
            self.scene.version.as_deref().unwrap_or("?"),
            self.scene.devs.len(),
            self.scene.graph.links().len(),
            self.scene.cords.iter().filter(|c| c.live()).count()
        );
        let line = format!("{left}\n{}\n{width}", self.hint);
        if self.status.as_ref().is_none_or(|(l, _)| *l != line) {
            self.status = Some((line, panel::status(width, &left, &self.hint, dpi)));
        }
        if self.notice.as_ref().is_some_and(|(t, _)| now >= *t) {
            self.notice = None;
        }
        if self.tip.is_none() && self.drag.is_none() && self.scene.hover != Hover::None && now - self.hover_since >= TIP_DELAY {
            let lines = self.scene.tooltip(&self.scene.hover);
            if !lines.is_empty() {
                self.tip = Some(panel::tooltip(&lines, dpi));
            }
        }

        self.place_profiles(dpi);
        self.update_glow();
        let pulse = 0.5 + 0.5 * ((now - self.start).as_secs_f32() * 6.0).sin();
        let cue = self.cue_marks();
        let cords = self.scene.cord_layer(self.view, self.size, &rings, &cue, pulse);
        let cord_id = next_id();

        // the badge beside the pointer while listening, over the field
        let (ftop, fbottom) = self.field();
        let show_badge = (self.cue.probing || self.cue.mode) && self.cursor.1 >= ftop && self.cursor.1 <= fbottom && self.drag.is_none();
        if show_badge {
            let raster = (self.dpi() * 1e6).round() as u64;
            if self.cue.badge.as_ref().is_none_or(|(s, _)| *s != raster) {
                self.cue.badge = Some((raster, panel::cue_badge(self.dpi())));
            }
        }
        let Some(sf) = &mut self.win else { return };
        let v = self.view;
        let mut sprites: Vec<Sprite> = Vec::new();
        if let Some((id, pm)) = &self.bg {
            sprites.push(Sprite { id: *id, pm: Some(pm), x: 0.0, y: 0.0, w: pm.width() as f64, h: pm.height() as f64, alpha: 1.0 });
        }
        if let Some((_, l)) = &self.scene.rack_bg {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: l.x * v.s + v.x, y: l.y * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0 });
        }
        let targets = match &self.drag {
            Some(Drag::Plug { targets, .. }) => Some(targets.as_slice()),
            _ => None,
        };
        for (di, d) in self.scene.devs.iter().enumerate() {
            // the hovered device glows around its edges, under its face
            if let Some((k, _, l)) = &self.glow
                && *k == d.dev.key
            {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (d.x + l.x) * v.s + v.x, y: (d.y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0 });
            }
            if let Some((k, at, l)) = &self.found
                && *k == d.dev.key
            {
                let alpha = (1.0 - at.elapsed().as_secs_f32() / FOUND.as_secs_f32()).clamp(0.0, 1.0);
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (d.x + l.x) * v.s + v.x, y: (d.y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha });
            }
            for l in d.layers() {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (d.x + l.x) * v.s + v.x, y: (d.y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0 });
            }
            // its jacks, over the face
            for (pic, x, y, alpha) in self.scene.jack_sprites(di, targets) {
                if let Some(l) = &self.scene.jack_pics[pic] {
                    sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (x + l.x) * v.s + v.x, y: (y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha });
                }
            }
        }
        // alignment guides, over the faces and under the cords
        let lw = 1.5 * dpi;
        for &[x0, y0, x1, y1] in &self.scene.guides {
            let (x, y) = (x0 * v.s + v.x, y0 * v.s + v.y);
            let (w, h) = (((x1 - x0) * v.s).max(lw), ((y1 - y0) * v.s).max(lw));
            let (x, y) = if x0 == x1 { (x - lw / 2.0, y) } else { (x, y - lw / 2.0) };
            sprites.push(Sprite { id: self.guide.0, pm: Some(&self.guide.1), x, y, w, h, alpha: 0.85 });
        }
        if let Some((pm, x, y)) = &cords {
            sprites.push(Sprite { id: cord_id, pm: Some(pm), x: *x as f64, y: *y as f64, w: pm.width() as f64, h: pm.height() as f64, alpha: 1.0 });
        }
        let (fw, fh) = (self.size.0 as f64, self.size.1 as f64);
        if let Some((_, l)) = &self.notice {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: ((fw - l.w * dpi) / 2.0).round(), y: ((panel::H + 14.0) * dpi).round(), w: l.w * dpi, h: l.h * dpi, alpha: 1.0 });
        }
        if let Some((_, l)) = &self.status {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: 0.0, y: fh - l.h * dpi, w: l.w * dpi, h: l.h * dpi, alpha: 1.0 });
        }
        for l in self.panel.layers() {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (l.x * dpi).round(), y: (l.y * dpi).round(), w: l.w * dpi, h: l.h * dpi, alpha: 1.0 });
        }
        if let Some(pl) = &self.profiles
            && let Some(l) = &pl.layer
        {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: pl.at.0, y: pl.at.1, w: l.w * dpi, h: l.h * dpi, alpha: 1.0 });
        }
        if show_badge && let Some((_, l)) = &self.cue.badge {
            // below and right of the pointer's tip, clear of the arrow
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (self.cursor.0 + 12.0 * dpi).round(), y: (self.cursor.1 + 14.0 * dpi).round(), w: l.w * dpi, h: l.h * dpi, alpha: 1.0 });
        }
        if let Some(l) = &self.tip {
            // below and right of the pointer; kept inside the window, above the pointer near the bottom
            let (w, h) = (l.w * dpi, l.h * dpi);
            let x = (self.cursor.0 + 14.0 * dpi).min(fw - w - 4.0 * dpi).max(4.0 * dpi);
            let below = self.cursor.1 + 22.0 * dpi;
            let y = if below + h > fh - panel::STATUS_H * dpi { self.cursor.1 - h - 10.0 * dpi } else { below };
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: x.round(), y: y.round(), w, h, alpha: 1.0 });
        }
        sf.gpu.draw(&sprites);
        self.panel.uploaded();
        if let Some((_, l)) = &mut self.scene.rack_bg
            && sf.gpu.has(l.id)
        {
            l.pm = None;
        }
        for l in [self.status.as_mut().map(|(_, l)| l), self.notice.as_mut().map(|(_, l)| l), self.tip.as_mut()].into_iter().flatten() {
            if sf.gpu.has(l.id) {
                l.pm = None;
            }
        }
        // the GPU has the pixels now (the jack pictures are kept: they are small, and the GPU lets
        // a texture go in a frame that doesn't draw it)
        for d in &mut self.scene.devs {
            for l in d.layers_mut() {
                if sf.gpu.has(l.id) {
                    l.pm = None;
                }
            }
        }
        if self.found.as_ref().is_some_and(|(_, at, _)| at.elapsed() > FOUND) {
            self.found = None;
        }
        self.animating = !self.cue_heard().is_empty() || self.found.is_some() || moving || self.view_to.is_some() || self.drag.is_some() || self.rescale_at.is_some() || self.notice.is_some() || faces_waiting || self.scene.noting() || (self.scene.hover != Hover::None && self.tip.is_none() && now - self.hover_since < 2 * TIP_DELAY);
    }

    /// The glow around the device under the pointer (not while dragging, nor over its switches).
    fn update_glow(&mut self) {
        let w = self.world(self.cursor);
        let key = match &self.scene.hover {
            Hover::Device(k) if self.drag.is_none() && self.scene.switch_at(w.0, w.1).is_none() && self.scene.profile_key_at(w.0, w.1).is_none() && self.scene.port_key_at(w.0, w.1).is_none() && !self.scene.mix_control_at(w.0, w.1) => k.clone(),
            _ => {
                self.glow = None;
                return;
            }
        };
        let Some(d) = self.scene.devs.iter().find(|d| d.dev.key == key) else { return };
        let rack = self.scene.rack;
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(&key, d.geo.w.to_bits(), d.h().to_bits(), rack, self.raster.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if self.glow.as_ref().is_none_or(|(k, s, _)| *k != key || *s != sig) {
            self.glow = Some((key, sig, glow_layer(d.geo.w, d.h(), rack, self.raster)));
        }
    }

    /// What the pointer is over in the open profile list.
    fn profile_hit(&self) -> Option<ProfileHit> {
        let pl = self.profiles.as_ref()?;
        let dpi = self.dpi();
        let (x, y) = ((self.cursor.0 - pl.at.0) / dpi, (self.cursor.1 - pl.at.1) / dpi);
        pl.hits.iter().find(|(_, [rx, ry, rw, rh])| x >= *rx && x <= rx + rw && y >= *ry && y <= ry + rh).map(|(h, _)| *h)
    }

    /// Switches the card whose list is open to its profile `index`.
    fn set_profile(&mut self, index: u32) {
        let Some(pl) = &self.profiles else { return };
        if let Some(card) = self.scene.devs.iter().find(|d| d.dev.key == pl.key).and_then(|d| d.dev.card.as_ref()) {
            self.conn.send(Cmd::SetProfile { device: card.id, index });
        }
    }

    /// Draws the open profile list again when its card changed, and puts it under the key.
    fn place_profiles(&mut self, dpi: f64) {
        let Some(pl) = &mut self.profiles else { return };
        let Some(d) = self.scene.devs.iter().find(|d| d.dev.key == pl.key) else {
            self.profiles = None;
            return;
        };
        let (Some(card), Some([kx, ky, kw, kh])) = (&d.dev.card, if pl.port { d.geo.port } else { d.geo.profile }) else {
            self.profiles = None;
            return;
        };
        let sections = if pl.port { self.scene.port_sections(self.scene.devs.iter().position(|x| x.dev.key == pl.key).unwrap_or(0)) } else { Vec::new() };
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(format!("{card:?}{sections:?}"), &d.dev.name, dpi.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if pl.sig != sig || pl.layer.is_none() {
            let (layer, hits) = if pl.port { panel::port_menu(&d.dev.name, &sections, dpi) } else { panel::profile_menu(&d.dev.name, card, dpi) };
            (pl.sig, pl.layer, pl.hits) = (sig, Some(layer), hits);
        }
        // under the key, its left edge at the key's, kept inside the window
        let v = self.view;
        let x = (d.x + kx) * v.s + v.x;
        let y = (d.y + ky + kh) * v.s + v.y + 4.0 * dpi;
        let _ = kw;
        let w = panel::PROFILE_W * dpi;
        pl.at = (x.min(self.size.0 as f64 - w - 6.0 * dpi).max(6.0 * dpi).round(), y.round());
    }

    fn press(&mut self, button: MouseButton) {
        self.set_hover(Hover::None);
        let w = self.world(self.cursor);
        let (top, bottom) = self.field();
        // a card's profile list takes the click; it stays open after a choice. A click anywhere
        // else closes it (and does what it does); one on its own PROFILE key only closes it.
        if let Some(pl) = &self.profiles {
            match self.profile_hit() {
                Some(ProfileHit::Codec(index)) => return self.set_profile(index),
                Some(ProfileHit::Row(f)) => {
                    let card = self.scene.devs.iter().find(|d| d.dev.key == pl.key).and_then(|d| d.dev.card.clone());
                    if let Some(index) = card.and_then(|c| c.families().get(f).and_then(|fam| c.pick(fam))) {
                        self.set_profile(index);
                    }
                    return;
                }
                Some(ProfileHit::Port(gi, index)) => {
                    if let Some(di) = self.scene.devs.iter().position(|d| d.dev.key == pl.key) {
                        self.scene.set_port(di, gi, index);
                    }
                    return self.after_patch();
                }
                Some(ProfileHit::Latency(gi, step)) => {
                    // 1 ms a click, 10 with Shift
                    if let Some(di) = self.scene.devs.iter().position(|d| d.dev.key == pl.key) {
                        let now = self.scene.port_sections(di).into_iter().find(|s| s.0 == gi).map_or(0, |s| s.4);
                        let ms = if self.mods.shift_key() { 10 } else { 1 };
                        self.scene.set_latency(di, gi, now + step * ms * 1_000_000);
                    }
                    return self.after_patch();
                }
                Some(ProfileHit::Inside) => return,
                None => {
                    let key_at = if pl.port { self.scene.port_key_at(w.0, w.1) } else { self.scene.profile_key_at(w.0, w.1) };
                    let own = key_at.is_some_and(|i| self.scene.devs[i].dev.key == pl.key);
                    self.profiles = None;
                    self.redraw();
                    if own {
                        return;
                    }
                }
            }
        }
        // the preset drop-down takes the click; a click elsewhere closes it
        if self.panel.menu.is_some() {
            let dpi = self.dpi();
            match self.panel.menu_hit(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                Some(MenuHit::Row(i)) => self.load_preset(i),
                Some(MenuHit::Delete(i)) => self.delete_preset(i),
                Some(MenuHit::Save) => {
                    if let Some(Menu::Save { text, .. }) = &self.panel.menu {
                        let typed = text.clone();
                        self.save_preset(&typed);
                    }
                }
                Some(MenuHit::Default(output, i)) => {
                    if let Some(e) = self.scene.ends(output).get(i).cloned() {
                        self.scene.set_default(output, &e.2);
                        self.after_patch();
                        // shown at once; PipeWire's answer follows
                        if let Some(Menu::Defaults { out, inp, .. }) = &mut self.panel.menu {
                            *(if output { out } else { inp }) = Some(i);
                        }
                    }
                }
                Some(MenuHit::Find(output, i)) => {
                    if let Some(e) = self.scene.ends(output).get(i).cloned() {
                        self.find_device(e.0);
                    }
                }
                Some(MenuHit::Inside) => {}
                None => {
                    // the keys that open it toggle it themselves
                    let key = self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi);
                    if !matches!(key, Some(Key::Save | Key::Load | Key::Lcd | Key::Defaults)) {
                        self.panel.menu = None;
                        self.redraw();
                        return;
                    }
                }
            }
            if self.panel.menu_hit(self.cursor.0 / dpi, self.cursor.1 / dpi).is_some() || self.cursor.1 >= top {
                return;
            }
        }
        if self.cursor.1 < top {
            // the top panel
            let dpi = self.dpi();
            if button == MouseButton::Left
                && let Some(key) = self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi)
            {
                self.panel.state.pressed = Some(key);
                self.drag = Some(match key {
                    Key::Cables => Drag::Switch { from: self.cursor, pos0: self.scene.cables, moved: false },
                    Key::CueLevel => Drag::CueLevel { from: self.cursor.1, v0: self.cue.level },
                    _ => Drag::Key,
                });
                if key != Key::Cables {
                    self.panel_key(key);
                }
            }
            return;
        }
        if self.cursor.1 > bottom {
            return;
        }
        if button != MouseButton::Left {
            self.drag = Some(Drag::Pan { from: self.cursor, view: self.view });
            return;
        }
        // in CUE mode a click on a jack keeps it playing, or stops it, instead of pulling a cord
        if self.cue.mode
            && let Some(k) = self.scene.jack_key_at(w.0, w.1)
        {
            if self.scene.cue_blocked(&k) {
                self.say("That is the output you listen on: it would feed back");
            } else if let Some(i) = self.cue.pins.iter().position(|p| *p == k) {
                self.cue.pins.remove(i);
            } else {
                self.cue.pins.push(k);
            }
            return self.redraw();
        }
        let now = Instant::now();
        let double = now - self.last_press.0 < DOUBLE && (self.cursor.0 - self.last_press.1.0).abs() + (self.cursor.1 - self.last_press.1.1).abs() < 8.0;
        self.last_press = (now, self.cursor);
        if double {
            // double-click on a cord or plug: pull it out
            if let Some((ci, b)) = self.scene.plug_at(w.0, w.1) {
                self.scene.pull_out(ci, b);
                return self.after_patch();
            }
            if let Some((ci, _)) = self.scene.cord_at(w.0, w.1) {
                self.scene.pull_out(ci, true);
                return self.after_patch();
            }
        }
        // an app's VOLUME controls (they work while devices are locked too)
        if let Some((di, gi)) = self.scene.knob_at(w.0, w.1) {
            if double {
                self.scene.set_volume((di, gi), 1.0);
                return self.after_patch();
            }
            let raw = self.scene.devs[di].mix(gi).map_or(1.0, |m| m.0);
            self.drag = Some(Drag::Knob { key: self.scene.devs[di].dev.key.clone(), gi, last: self.cursor.1, raw });
            return self.redraw();
        }
        if let Some((di, gi)) = self.scene.bal_at(w.0, w.1) {
            let d = &mut self.scene.devs[di];
            let raw = d.mix(gi).map_or(0.0, |m| m.1);
            self.drag = Some(Drag::Bal { key: d.dev.key.clone(), gi, y0: self.cursor.1, last: self.cursor.1, raw, was: d.bal(gi), turned: false });
            // down under the finger, below flush
            d.bal.insert(gi, BalKnob::Held);
            return self.redraw();
        }
        if let Some(at) = self.scene.mute_at(w.0, w.1) {
            self.scene.toggle_mute(at);
            return self.after_patch();
        }
        // a card's PROFILE and PORT keys open their lists under them
        if let Some((di, port)) = self.scene.port_key_at(w.0, w.1).map(|di| (di, true)).or(self.scene.profile_key_at(w.0, w.1).map(|di| (di, false))) {
            self.profiles = Some(ProfileList { key: self.scene.devs[di].dev.key.clone(), port, sig: 0, layer: None, hits: Vec::new(), at: (0.0, 0.0) });
            self.redraw();
            return;
        }
        let plug = |scene: &mut Scene| -> Option<u64> {
            if let Some((ci, b)) = scene.plug_at(w.0, w.1) {
                return Some(scene.unplug(ci, b));
            }
            scene.free_jack_at(w.0, w.1).and_then(|k| scene.pull_new(&k))
        };
        if let Some(seq) = plug(&mut self.scene) {
            let targets = self.scene.targets(seq);
            let c = self.scene.cords.iter().find(|c| c.seq == seq).expect("cord");
            let e = if c.b.held { &c.b } else { &c.a };
            let p = (e.x, e.y);
            let magnet = Self::magnet(&self.scene, &targets, w);
            self.drag = Some(Drag::Plug { seq, p, v: (0.0, 0.0), magnet, targets });
        } else if let Some((di, g)) = self.scene.switch_at(w.0, w.1) {
            self.scene.toggle_split(di, g);
        } else if let Some((ci, k)) = self.scene.cord_at(w.0, w.1).filter(|_| self.scene.cables < 2) {
            let c = &mut self.scene.cords[ci];
            c.grab = Some(k);
            self.drag = Some(Drag::Cord { seq: c.seq });
        } else if let Some(i) = self.scene.device_at(w.0, w.1).filter(|_| !self.panel.state.locked) {
            let i = self.scene.raise(i);
            let d = &self.scene.devs[i];
            self.drag = Some(Drag::Device { key: d.dev.key.clone(), off: (w.0 - d.x, w.1 - d.y) });
        } else {
            self.drag = Some(Drag::Pan { from: self.cursor, view: self.view });
        }
        self.view_to = None;
        self.redraw();
    }

    fn release(&mut self) {
        match self.drag.take() {
            Some(Drag::Cord { seq }) => {
                for c in self.scene.cords.iter_mut().filter(|c| c.seq == seq) {
                    c.grab = None;
                }
            }
            Some(Drag::Plug { seq, magnet, .. }) => {
                self.scene.drop_held(seq, magnet);
                self.after_patch();
            }
            Some(Drag::Device { .. }) => {
                self.scene.drop_device();
                self.save_settings();
            }
            Some(Drag::Switch { moved, .. }) => {
                if !moved {
                    self.panel_key(Key::Cables);
                }
                self.save_settings();
            }
            // a press without a turn: it comes back up, out if it was in, in if it was out
            Some(Drag::Bal { key, gi, was, turned, .. }) => {
                if let Some(d) = self.scene.devs.iter_mut().find(|d| d.dev.key == key) {
                    d.bal.insert(gi, if turned || was == BalKnob::In { BalKnob::Out } else { BalKnob::In });
                }
            }
            _ => {}
        }
        self.panel.state.pressed = None;
        self.redraw();
    }

    /// The jack a plug in the hand is pulled to: the nearest fitting free slot close by.
    fn magnet(scene: &Scene, targets: &[String], w: (f64, f64)) -> Option<String> {
        targets
            .iter()
            .filter_map(|t| scene.spare(t).map(|(x, y)| (t, ((x as f64 - w.0).powi(2) + (y as f64 - w.1).powi(2)).sqrt())))
            .filter(|(_, d)| *d < 34.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(t, _)| t.clone())
    }

    /// Sends the link changes the scene asked for.
    fn after_patch(&mut self) {
        for cmd in self.scene.outbox.drain(..) {
            self.conn.send(cmd);
        }
        self.redraw();
    }

    fn moved(&mut self) {
        let w = self.world(self.cursor);
        match &self.drag {
            Some(Drag::Pan { from, view }) => {
                self.view.x = view.x + self.cursor.0 - from.0;
                self.view.y = view.y + self.cursor.1 - from.1;
                self.redraw();
            }
            Some(Drag::Device { key, off }) => {
                let (key, x, y) = (key.clone(), w.0 - off.0, w.1 - off.1);
                let px = self.dpi() / self.view.s;
                self.scene.drag_device(&key, x, y, px);
                self.redraw();
            }
            Some(Drag::Cord { .. }) => self.redraw(),
            Some(Drag::Key) => {}
            Some(Drag::CueLevel { from, v0 }) => {
                let v = (v0 + (from - self.cursor.1) / (120.0 * self.dpi())).clamp(0.0, 1.0);
                self.cue.level = (v * 100.0).round() / 100.0;
                self.hint = format!("cue level {:.0}%", self.cue.level * 100.0);
                self.redraw();
            }
            Some(Drag::Knob { key, gi, last, raw }) => {
                // up is louder; Shift turns it finely; it rests at 100% for a moment on the way
                let fine = if self.mods.shift_key() { 0.2 } else { 1.0 };
                let raw = (raw + (last - self.cursor.1) / self.dpi() * VOL_MAX / KNOB_TRAVEL * fine).clamp(0.0, VOL_MAX);
                let (key, gi) = (key.clone(), *gi);
                self.drag = Some(Drag::Knob { key: key.clone(), gi, last: self.cursor.1, raw });
                if let Some(di) = self.scene.devs.iter().position(|d| d.dev.key == key) {
                    self.scene.set_volume((di, gi), if (raw - 1.0).abs() < 0.04 { 1.0 } else { raw });
                }
                self.after_patch();
            }
            Some(Drag::Bal { key, gi, y0, last, raw, was, turned }) => {
                let (key, gi, y0, last, raw, was, mut turned) = (key.clone(), *gi, *y0, *last, *raw, *was, *turned);
                let dpi = self.dpi();
                // pushed in, it can't turn; out, it turns once the pointer really moves
                if was != BalKnob::Out || (!turned && (self.cursor.1 - y0).abs() / dpi < 3.0) {
                    return;
                }
                let Some(di) = self.scene.devs.iter().position(|d| d.dev.key == key) else { return };
                if !turned {
                    // turning, not pressing: it comes back up without a click
                    turned = true;
                    self.scene.devs[di].bal.insert(gi, BalKnob::Out);
                }
                let fine = if self.mods.shift_key() { 0.2 } else { 1.0 };
                let raw = (raw + (last - self.cursor.1) / dpi * 2.0 / KNOB_TRAVEL * fine).clamp(-1.0, 1.0);
                self.drag = Some(Drag::Bal { key, gi, y0, last: self.cursor.1, raw, was, turned });
                self.scene.set_balance((di, gi), if raw.abs() < 0.05 { 0.0 } else { raw });
                self.after_patch();
            }
            Some(Drag::Switch { from, pos0, .. }) => {
                let (from, pos0) = (*from, *pos0);
                // right or up steps on, left or down steps back: one position per 30 pixels
                let dpi = self.dpi();
                let d = ((self.cursor.0 - from.0) - (self.cursor.1 - from.1)) / (30.0 * dpi);
                let pos = (pos0 as f64 + d.round()).clamp(0.0, 2.0) as u8;
                if d.abs() > 0.3
                    && let Some(Drag::Switch { moved, .. }) = &mut self.drag
                {
                    *moved = true;
                }
                if pos != self.scene.cables {
                    self.scene.cables = pos;
                    self.redraw();
                }
            }
            Some(Drag::Plug { .. }) => {
                if let Some(Drag::Plug { magnet, targets, .. }) = &mut self.drag {
                    *magnet = Self::magnet(&self.scene, targets, w);
                }
                self.redraw();
            }
            None => {
                let (top, bottom) = self.field();
                let dpi = self.dpi();
                // listening: the jack under the pointer is heard while it is there
                if (self.cue.probing || self.cue.mode) && self.cursor.1 >= top && self.cursor.1 <= bottom && self.profile_hit().is_none() && self.panel.menu.is_none() {
                    let k = self.scene.jack_key_at(w.0, w.1);
                    if k != self.cue.hover {
                        self.cue.hover = k;
                        self.redraw();
                    }
                    if self.cue.hover.is_some() || self.cue.probing {
                        let hint = self.cue_hint();
                        if hint != self.hint {
                            self.hint = hint;
                            self.redraw();
                        }
                        if let Some(sf) = &self.win {
                            sf.window.set_cursor(CursorIcon::Default);
                        }
                        return;
                    }
                }
                // the open profile list takes the pointer: nothing under it reacts
                let in_list = self.profile_hit();
                let (icon, hint) = if let Some(h) = in_list {
                    match h {
                        ProfileHit::Row(_) => (CursorIcon::Pointer, "click: switch to this profile".to_owned()),
                        ProfileHit::Codec(_) => (CursorIcon::Pointer, "click: switch to this codec".to_owned()),
                        ProfileHit::Port(..) => (CursorIcon::Pointer, "click: use this port".to_owned()),
                        ProfileHit::Latency(..) => (CursorIcon::Pointer, "click: 1 ms earlier or later (Shift: 10 ms); this port's sound is delayed by its latency offset".to_owned()),
                        ProfileHit::Inside => (CursorIcon::Default, String::new()),
                    }
                } else if self.cursor.1 < top {
                    match self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                        Some(Key::Cables) => (CursorIcon::EwResize, Key::Cables.hint().to_owned()),
                        Some(k) => (CursorIcon::Pointer, k.hint().to_owned()),
                        None => (CursorIcon::Default, String::new()),
                    }
                } else if self.cursor.1 > bottom {
                    (CursorIcon::Default, String::new())
                } else {
                    (self.field_cursor(w), self.scene.hint_at(w.0, w.1, self.panel.state.locked))
                };
                let on_field = self.cursor.1 >= top && self.cursor.1 <= bottom && self.panel.menu.is_none() && in_list.is_none();
                self.set_hover(if on_field { self.scene.hover_at(w.0, w.1) } else { Hover::None });
                if hint != self.hint {
                    self.hint = hint;
                    self.redraw();
                }
                if let Some(sf) = &self.win {
                    sf.window.set_cursor(icon);
                }
            }
        }
    }

    /// What the pointer is over changed: its cords light up, and its tooltip waits a moment.
    fn set_hover(&mut self, h: Hover) {
        if h != self.scene.hover {
            self.scene.hover = h;
            self.hover_since = Instant::now();
            self.tip = None;
            self.redraw();
        }
    }

    fn field_cursor(&self, w: (f64, f64)) -> CursorIcon {
        if self.scene.plug_at(w.0, w.1).is_some() || (self.scene.cables < 2 && self.scene.cord_at(w.0, w.1).is_some()) {
            CursorIcon::Grab
        } else if self.scene.free_jack_at(w.0, w.1).is_some() {
            CursorIcon::Crosshair
        } else if self.scene.knob_at(w.0, w.1).is_some() || self.scene.bal_at(w.0, w.1).is_some_and(|(di, gi)| self.scene.devs[di].bal(gi) == BalKnob::Out) {
            CursorIcon::NsResize
        } else if self.scene.switch_at(w.0, w.1).is_some() || self.scene.profile_key_at(w.0, w.1).is_some() || self.scene.port_key_at(w.0, w.1).is_some() || self.scene.mix_control_at(w.0, w.1) {
            CursorIcon::Pointer
        } else if self.scene.device_at(w.0, w.1).is_some() && !self.panel.state.locked {
            CursorIcon::Move
        } else {
            CursorIcon::Default
        }
    }

    fn zoom(&mut self, factor: f64) {
        let dpi = self.win.as_ref().map_or(1.0, |sf| sf.window.scale_factor());
        let w = self.world(self.cursor);
        let s = (self.view.s * factor).clamp(0.25 * dpi, 2.5 * dpi);
        self.view = View { s, x: self.cursor.0 - w.0 * s, y: self.cursor.1 - w.1 * s };
        self.view_to = None;
        // the faces are drawn sharp again once the zooming stops, not while it goes on
        self.rescale_at = Some(Instant::now() + RESCALE_DELAY);
        self.redraw();
    }

    fn key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        match code {
            KeyCode::KeyQ if self.mods.control_key() => {
                self.save_settings();
                event_loop.exit();
            }
            KeyCode::KeyZ if self.mods.control_key() => {
                if if self.mods.shift_key() { self.scene.redo() } else { self.scene.undo() } {
                    self.after_patch();
                }
            }
            KeyCode::KeyS if self.mods.control_key() => self.panel_key(Key::Save),
            KeyCode::KeyO if self.mods.control_key() => self.panel_key(Key::Load),
            _ if self.mods.control_key() || self.mods.alt_key() => {}
            KeyCode::KeyF | KeyCode::Digit0 => self.panel_key(Key::Fit),
            KeyCode::KeyA => self.panel_key(Key::Arrange),
            KeyCode::KeyL => self.panel_key(Key::Lock),
            KeyCode::KeyG => self.panel_key(Key::Align),
            // the groups' colours, on and off
            KeyCode::KeyK => {
                self.scene.accents = !self.scene.accents;
                self.say(if self.scene.accents { "Group colours on" } else { "Group colours off" });
                self.save_settings();
                self.redraw();
            }
            KeyCode::KeyC => self.panel_key(Key::Cables),
            KeyCode::KeyR => self.panel_key(if self.scene.rack { Key::Desk } else { Key::Rack }),
            KeyCode::Escape => {
                self.panel.menu = None;
                self.profiles = None;
                if self.cue.mode {
                    self.set_cue_mode(false);
                }
                self.redraw();
            }
            _ => {}
        }
    }
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        let (w, h) = self.settings.window.unwrap_or((1400, 880));
        let attrs = with_app_id(Window::default_attributes().with_title("Patchcord").with_inner_size(LogicalSize::new(w as f64, h as f64)).with_min_inner_size(LogicalSize::new(480.0, 320.0)));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let gpu = match Gpu::new(window.clone(), event_loop.owned_display_handle()) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("patchcord: {e}");
                event_loop.exit();
                return;
            }
        };
        if std::env::var_os("PATCHCORD_DEBUG").is_some() {
            eprintln!("patchcord: {}", gpu.info);
        }
        let s = window.inner_size();
        self.size = (s.width.max(1), s.height.max(1));
        self.win = Some(Surface { window, gpu });
        self.redraw();
    }

    /// PipeWire has news.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _: ()) {
        let events: Vec<_> = self.conn.events.try_iter().collect();
        let changed = self.scene.apply(events);
        for cmd in self.scene.outbox.drain(..) {
            self.conn.send(cmd);
        }
        if changed {
            if self.scene.laid_out && !self.fitted && self.win.is_some() {
                self.fitted = true;
                self.fit(false);
            }
            self.redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_settings();
                event_loop.exit();
            }
            WindowEvent::Resized(s) => {
                self.size = (s.width.max(1), s.height.max(1));
                if let Some(sf) = &mut self.win {
                    sf.gpu.resize(self.size.0, self.size.1);
                }
                self.redraw();
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                self.next_frame = Instant::now() + Duration::from_millis(16);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.moved();
            }
            WindowEvent::CursorLeft { .. } => self.set_hover(Hover::None),
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => self.press(button),
                ElementState::Released => self.release(),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                // the profile list stays under its key: zooming closes it
                if self.profiles.take().is_some() {
                    self.redraw();
                }
                let f = match delta {
                    MouseScrollDelta::LineDelta(_, y) => 1.12f64.powf(y as f64),
                    MouseScrollDelta::PixelDelta(p) => (p.y * 0.0015).exp(),
                };
                let (top, bottom) = self.field();
                let dpi = self.dpi();
                if self.cursor.1 < top {
                    match self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                        // the wheel steps the cords switch, one click at a time
                        Some(Key::CueLevel) if f != 1.0 => {
                            self.cue.level = (self.cue.level + if f > 1.0 { 0.05 } else { -0.05 }).clamp(0.0, 1.0);
                            self.hint = format!("cue level {:.0}%", self.cue.level * 100.0);
                            self.redraw();
                        }
                        Some(Key::Cables | Key::CablesAt(_)) if f != 1.0 => {
                            let step = if f > 1.0 { 1 } else { -1 };
                            self.scene.cables = (self.scene.cables as i32 + step).clamp(0, 2) as u8;
                            self.save_settings();
                            self.redraw();
                        }
                        _ => {}
                    }
                } else if self.cursor.1 <= bottom {
                    // over an app's VOLUME knob, or its BAL knob when out, the wheel turns it
                    let w = self.world(self.cursor);
                    let up = if f > 1.0 { 1.0 } else { -1.0 };
                    let fine = self.mods.shift_key();
                    if f == 1.0 {
                    } else if let Some((di, gi)) = self.scene.knob_at(w.0, w.1) {
                        let v = self.scene.devs[di].mix(gi).map_or(1.0, |m| m.0);
                        self.scene.set_volume((di, gi), v + up * if fine { 0.01 } else { 0.05 });
                        self.after_patch();
                    } else if let Some((di, gi)) = self.scene.bal_at(w.0, w.1) {
                        if self.scene.devs[di].bal(gi) == BalKnob::Out {
                            let b = self.scene.devs[di].mix(gi).map_or(0.0, |m| m.1);
                            self.scene.set_balance((di, gi), b + up * if fine { 0.02 } else { 0.1 });
                            self.after_patch();
                        }
                    } else {
                        self.zoom(f);
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                // holding Alt probes: the jack under the pointer is heard while Alt is held
                let alt = self.mods.alt_key();
                if alt != self.cue.probing {
                    self.cue.probing = alt;
                    if alt {
                        let w = self.world(self.cursor);
                        self.cue.hover = self.scene.jack_key_at(w.0, w.1);
                        self.hint = self.cue_hint();
                    } else if !self.cue.mode {
                        self.cue.hover = None;
                        self.hint.clear();
                    }
                    self.redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let PhysicalKey::Code(code) = event.physical_key
                    && !self.type_key(code, event.text.as_deref())
                    && !event.repeat
                {
                    self.key(code, event_loop);
                }
            }
            _ => {}
        }
    }

    /// Paces the frames: about 60 a second while something moves, else only to scroll long
    /// names on the LCDs, else nothing at all.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.animating {
            if now >= self.next_frame {
                self.redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            }
        } else if self.scene.devs.iter().any(|d| d.dev.name.chars().count() > LCD_CH) {
            let ms = (now - self.start).as_millis() as u64;
            let next = now + MARQUEE - Duration::from_millis(ms % MARQUEE.as_millis() as u64);
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            if let Some(sf) = &self.win
                && next - now < Duration::from_millis(5)
            {
                sf.window.request_redraw();
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        // the marquee's timer ran out: draw the next step of the scrolling names
        if matches!(cause, winit::event::StartCause::ResumeTimeReached { .. })
            && !self.animating
            && let Some(sf) = &self.win
        {
            sf.window.request_redraw();
        }
    }
}

/// The patch field: dark, a little lighter towards the top middle, with a fine dot grid.
/// Desktop app ID (also the .desktop file and icon name). Wayland has no window icons:
/// the compositor finds the icon through the installed `<APP_ID>.desktop` file.
const APP_ID: &str = "io.github.bewzaalex.patchcord";

fn with_app_id(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    use winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};
    let attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
    WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID)
}

/// The glow around a hovered device `w` wide and `h` high: a thin amber ring along its edge and
/// a soft halo, as in the mockup.
fn glow_layer(w: f64, h: f64, rack: bool, scale: f64) -> Layer {
    let m = 26.0;
    let (gw, gh) = (w + 2.0 * m, h + 2.0 * m);
    let r = if rack { 4.0 } else { 13.0 };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{gw}" height="{gh}" viewBox="0 0 {gw} {gh}"><defs><filter id="b" x="-20%" y="-50%" width="140%" height="200%"><feGaussianBlur stdDeviation="8"/></filter></defs><rect x="{m}" y="{m}" width="{w}" height="{h}" rx="{r}" fill="none" stroke="#ffaa3c" stroke-opacity=".3" stroke-width="12" filter="url(#b)"/><rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="none" stroke="#ffbe5a" stroke-opacity=".6" stroke-width="2"/></svg>"##,
        m - 1.0,
        m - 1.0,
        w + 2.0,
        h + 2.0,
        r + 1.0
    );
    Layer { id: next_id(), pm: super::scene::rasterize(&super::text::svg_options(), &svg, scale), x: -m, y: -m, w: gw, h: gh }
}

pub fn background(w: u32, h: u32) -> Pixmap {
    let mut pm = Pixmap::new(w.max(1), h.max(1)).expect("background");
    pm.fill(Color::from_rgba8(0x15, 0x16, 0x18, 255));
    let (fw, fh) = (w as f32, h as f32);
    let r = (fw * fw + fh * fh).sqrt() * 0.62;
    let g = RadialGradient::new(
        Point::from_xy(fw / 2.0, fh * 0.3),
        Point::from_xy(fw / 2.0, fh * 0.3),
        r,
        vec![GradientStop::new(0.0, Color::from_rgba8(0x2a, 0x2c, 0x2f, 255)), GradientStop::new(1.0, Color::from_rgba8(0x15, 0x16, 0x18, 255))],
        SpreadMode::Pad,
        Transform::identity(),
    );
    if let (Some(shader), Some(rect)) = (g, Rect::from_xywh(0.0, 0.0, fw, fh)) {
        pm.fill_rect(rect, &Paint { shader, ..Default::default() }, Transform::identity(), None);
    }
    let mut pb = PathBuilder::new();
    let mut y = 11.0;
    while y < fh {
        let mut x = 11.0;
        while x < fw {
            pb.push_circle(x, y, 1.0);
            x += 22.0;
        }
        y += 22.0;
    }
    if let Some(p) = pb.finish() {
        let mut paint = Paint { anti_alias: true, ..Default::default() };
        paint.set_color_rgba8(255, 255, 255, 7);
        pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
    }
    pm
}
