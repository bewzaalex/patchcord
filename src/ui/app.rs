//! The window: puts the field on screen and turns mouse, wheel and keys into actions.

use std::sync::Arc;
use std::time::{Duration, Instant};

use resvg::tiny_skia::{Color, GradientStop, Paint, Pixmap, Point, RadialGradient, Rect, SpreadMode, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorIcon, Window, WindowId};

use super::cord::Held;
use super::engine::{Action, Engine};
use super::mixer::{self, Mixer};
use super::ease;
use super::face::{BalKnob, LCD_CH};
use super::gpu::{Gpu, Sprite};
use super::panel::{self, Key, Menu, MenuHit, Panel, ProfileHit};
use crate::config::{self, Preset, Settings};
use super::scene::{self, CUE_PICS, Hover, Layer, Scene, TARGET_PICS, View, next_id};
use crate::graph::VOL_MAX;
use crate::pw::{Cmd, Connection};

/// After zooming, faces are drawn sharp again (on the painter's threads) once the zoom has been
/// still this long.
const RESCALE_DELAY: Duration = Duration::from_millis(120);
/// The window's width, for the rack (each new width draws every face again): a change after a
/// quiet moment (a maximize) is taken at once; while the window is being dragged wider or
/// narrower, the last width once it has kept still this long.
const ROOM_DELAY: Duration = Duration::from_millis(150);
/// How long after the wheel scrolled the rack it still scrolls it, over knobs too.
const SCROLL_LATCH: Duration = Duration::from_millis(400);

/// A picture of one pixel of a colour (opaque), stretched to whatever it fills.
fn pixel(rgb: [u8; 4]) -> Layer {
    let mut pm = resvg::tiny_skia::Pixmap::new(1, 1).expect("1×1");
    pm.data_mut().copy_from_slice(&rgb);
    Layer { id: next_id(), pm: Some(pm), x: 0.0, y: 0.0, w: 1.0, h: 1.0 }
}
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
    /// a press in ENGINE's graph (a click or a pan)
    Engine,
    /// a press in MIXER: a fader, or the console panned
    Mixer,
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

/// The list of devices hidden by hand, and of the nodes always left out.
pub fn hidden_menu(scene: &Scene) -> Menu {
    let mine = scene.hidden_devs.iter().map(|d| (d.name.clone(), d.model.clone())).collect();
    let system = scene.graph.system().into_iter().map(|(who, what, n)| (who, if n > 1 { format!("{what} ×{n}") } else { what.to_owned() })).collect();
    Menu::Hidden { mine, system }
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
    sent: Option<(Vec<crate::pw::CueSource>, u32)>,
    /// what that was worked out from: the jacks heard, the scene's `rev`, the level
    from: Option<(Vec<String>, u64, u32)>,
    badge: Option<(u64, Layer)>,
    /// in CUE mode the pointer is in a jack's hole: it patches there (the ring round it listens)
    in_hole: bool,
    /// the pointer is in the window (the headphones beside it only then)
    inside: bool,
}

/// A second click this soon, this close, is a double-click.
const DOUBLE: Duration = Duration::from_millis(350);

/// A card's profile list, open under its PROFILE key: drawn again when the card changes, placed
/// under the key wherever the view puts it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListOf {
    Profiles,
    Ports,
}

struct ProfileList {
    /// the device whose list it is
    key: String,
    /// which: its profiles (PROFILE), or its ports and latency offsets (PORT)
    what: ListOf,
    /// what it was drawn from
    sig: u64,
    layer: Option<Layer>,
    /// hit areas, relative to the list, in panel units
    hits: Vec<(ProfileHit, [f64; 4])>,
    /// its top left corner, in window pixels
    at: (f64, f64),
    /// opened from MIXER: under the key there (window pixels), not the face's
    anchor: Option<[f64; 4]>,
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
    /// the background being drawn for a new window size, on a thread of its own
    bg_job: Option<std::sync::mpsc::Receiver<Pixmap>>,
    /// one pixel of the alignment guides' colour, stretched into each line
    guide: (u64, Pixmap),
    last: Instant,
    start: Instant,
    animating: bool,
    /// the log of slow frames (PATCHCORD_FRAMES)
    prof: super::prof::Prof,
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
    /// a touchpad's scrolling, gathered into wheel clicks for what steps (knobs, switches)
    wheel_px: f64,
    /// when the wheel last scrolled the rack: a knob passing under the pointer meanwhile isn't
    /// turned (the scrolling goes on)
    scrolled: Option<Instant>,
    /// the window's width the rack is to take, and since when
    room_at: ((f64, f64), Instant),
    /// the rack's scroll bars: track, thumb
    bar_px: [Layer; 2],
    tip: Option<Layer>,
    /// a card's profile list, open under its PROFILE key
    profiles: Option<ProfileList>,
    cue: Cue,
    settings: Settings,
    presets: Vec<Preset>,
    /// the preset last saved or loaded
    current: Option<usize>,
    /// the ENGINE screen: PipeWire's real graph
    engine: Engine,
    /// the MIXER screen: every device and app as a console
    mixer: Mixer,
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
        scene.flow = settings.flow;
        scene.rack_order = settings.rack_order.clone();
        scene.hidden = settings.hidden.clone();
        let mixer_on = settings.mixer;
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
            bg_job: None,
            guide: (next_id(), {
                let mut pm = Pixmap::new(1, 1).unwrap();
                pm.fill(Color::from_rgba8(255, 200, 90, 255));
                pm
            }),
            last: now,
            start: now,
            animating: true,
            prof: super::prof::Prof::new(),
            next_frame: now,
            last_press: (now, (0.0, 0.0)),
            panel: Panel::new(panel::State { rack: false, engine: false, mixer: false, locked: settings.lock, align: settings.align, cables: settings.cables, lcd: [String::new(), String::new()], defaults: [String::new(), String::new()], cue_led: false, cue_level: settings.cue_level.min(100), hidden: 0, flow: true, pressed: None, more: false }),
            hint: String::new(),
            status: None,
            notice: None,
            hover_since: now,
            wheel_px: 0.0,
            scrolled: None,
            room_at: ((0.0, 0.0), Instant::now()),
            bar_px: [pixel([0x2a, 0x2b, 0x2e, 0xff]), pixel([0x9a, 0x9d, 0xa2, 0xff])],
            tip: None,
            profiles: None,
            // as it was left (at full the cue plays what it hears as loud as it is: the level
            // only takes away)
            cue: Cue { level: settings.cue_level.min(100) as f64 / 100.0, ..Default::default() },
            settings,
            presets,
            current,
            engine: Engine::new(),
            mixer: Mixer::new(mixer_on),
        }
    }

    /// Writes settings.txt when something in it changed.
    fn save_settings(&mut self) {
        let mut s = self.settings.clone();
        s.cables = self.scene.cables;
        s.lock = self.panel.state.locked;
        s.align = self.scene.align;
        s.accents = self.scene.accents;
        s.flow = self.scene.flow;
        s.rack = self.scene.rack;
        s.mixer = self.mixer.on;
        s.rack_order = self.scene.rack_order.clone();
        s.hidden = self.scene.hidden.clone();
        s.preset = self.current.map(|i| self.presets[i].name.clone());
        s.cue_level = (self.cue.level * 100.0).round().clamp(0.0, 100.0) as u8;
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
        self.panel.menu = Some(defaults_menu(&self.scene));
        self.redraw();
    }


    /// Opens the list of hidden devices (`toggle`: or closes it), or draws it again.
    fn open_hidden(&mut self, toggle: bool) {
        if toggle && matches!(self.panel.menu, Some(Menu::Hidden { .. })) {
            self.panel.menu = None;
            return self.redraw();
        }
        self.panel.menu = Some(hidden_menu(&self.scene));
        self.redraw();
    }

    /// Hides device `di` by hand, or says why it can't be.
    fn hide_device(&mut self, di: usize) {
        let name = self.scene.devs[di].dev.name.clone();
        if self.scene.hide(di) {
            self.say(&format!("{name} hidden · the HIDDEN key shows it again"));
            self.save_settings();
        } else {
            self.say(&format!("{name} has cords: pull them out first"));
        }
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

    /// The marks of the jacks heard: (device, position, picture in `jack_pics`, under the jack?).
    /// A jack kept on gets a disc under it, the half of it when only one of a stereo jack's two
    /// ports is kept on (from ENGINE); a jack heard gets a ring over it, red when it would feed
    /// back. A split pair (L over R) both marked alike gets one of each round the two.
    fn cue_marks(&self) -> Vec<(usize, (f32, f32), usize, bool)> {
        // per jack: its halves kept on (1 left, 2 right, 3 both), heard, would feed back
        let mut at: std::collections::BTreeMap<String, (u8, bool, bool)> = Default::default();
        for k in self.cue_heard() {
            // (a port heard from ENGINE shows on its jack)
            let Some(j) = self.scene.cue_jack(&k) else { continue };
            let bad = self.scene.cue_blocked(&k);
            let half = match scene::port_of_key(&k).and_then(|id| self.scene.port_side(j, id)) {
                Some(false) => 1,
                Some(true) => 2,
                None => 3,
            };
            let e = at.entry(j.to_owned()).or_default();
            e.1 = true;
            e.2 |= bad;
            if self.cue.pins.contains(&k) && !bad {
                e.0 |= half;
            }
        }
        let mut marks = Vec::new();
        let mut paired: (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        let place = |j: &str| Some((self.scene.jack_dev(j)?, self.scene.jack_pos(j)?));
        for (j, &(kept, heard, bad)) in &at {
            let Some((other, first)) = self.scene.split_pair(j) else { continue };
            let (Some(&(ok, oh, ob)), true) = (at.get(&other), first) else { continue };
            let (Some((di, a)), Some((_, b))) = (place(j), place(&other)) else { continue };
            // (only where the R sits right under the L, as the pair's pictures are drawn)
            if (a.0 - b.0).abs() > 0.5 || ((b.1 - a.1) as f64 - super::face::SPLIT_DY).abs() > 0.5 {
                continue;
            }
            if kept == 3 && ok == 3 {
                marks.push((di, a, scene::PAIR_PICS, true));
                paired.0.extend([j.clone(), other.clone()]);
            }
            if heard && oh && bad == ob {
                marks.push((di, a, scene::PAIR_PICS + 1 + bad as usize, false));
                paired.1.extend([j.clone(), other.clone()]);
            }
        }
        for (j, &(kept, heard, bad)) in &at {
            let Some((di, p)) = place(j) else { continue };
            if kept != 0 && !paired.0.contains(j) {
                marks.push((di, p, [0, scene::HALF_PICS, scene::HALF_PICS + 1, CUE_PICS][kept as usize], true));
            }
            if heard && !paired.1.contains(j) {
                marks.push((di, p, CUE_PICS + 1 + bad as usize, false));
            }
        }
        marks
    }

    /// Tells PipeWire what to hear, when that changed (the jacks, what is linked into an input
    /// jack, the default output, the level).
    fn cue_sync(&mut self) {
        let level = (self.cue.level.powi(3) as f32).to_bits();
        // (every frame: only worked out again when something it comes from moved on)
        if self.cue.from.as_ref().is_some_and(|f| f.1 != self.scene.rev) {
            // what kept on is gone (a device unplugged or hidden, a port gone): it goes too, so
            // that no stale name shows and no new port of the same number is heard unasked
            let scene = &self.scene;
            self.cue.pins.retain(|k| match scene::port_of_key(k) {
                Some(id) => scene.graph.props_of(id).is_some(),
                None => scene.jack(k).is_some(),
            });
        }
        let from = (self.cue_heard(), self.scene.rev, level);
        if self.cue.from.as_ref() == Some(&from) {
            return;
        }
        let heard: Vec<String> = from.0.iter().filter(|k| !self.scene.cue_blocked(k)).cloned().collect();
        self.cue.from = Some(from);
        // only what plays now (what sleeps has nothing to hear, and the cue's playing side would
        // keep the output awake for silence); with nothing to hear the cue goes, CUE mode or not
        let sources: Vec<crate::pw::CueSource> = self.scene.cue_mix(&heard).into_iter().filter(|s| self.scene.graph.state_of(s.0) == Some("running")).collect();
        let want = (sources, level);
        if self.cue.sent.as_ref() != Some(&want) {
            self.conn.send(Cmd::Cue { sources: want.0.clone(), level: f32::from_bits(level) });
            self.cue.sent = Some(want);
        }
    }

    /// When the next step of the scrolling of long names is due (counted from the start).
    fn next_step(&self, now: Instant) -> Option<Instant> {
        // (ENGINE shows no LCDs; what scrolls on them is a card's profile line)
        let marquee = !self.engine.on && self.scene.devs.iter().any(|d| d.dev.card.as_ref().is_some_and(|c| c.summary().chars().count() > LCD_CH));
        let ms = (now - self.start).as_millis() as u64;
        marquee.then(|| now + MARQUEE - Duration::from_millis(ms % MARQUEE.as_millis() as u64))
    }

    /// The status bar while listening.
    fn cue_hint(&self) -> String {
        if let Some(h) = &self.cue.hover
            && self.scene.cue_blocked(h)
        {
            return feedback(&self.scene.jack_title(h));
        }
        // (on the desk and the rack: ENGINE says what a click on a port does, `port_hint`)
        let what = self.cue_what().unwrap_or_else(|| "move over a jack to listen to it".to_owned());
        if self.cue.mode {
            let click = match &self.cue.hover {
                Some(h) if self.kept(h) => "  ·  click: stop it",
                Some(_) => "  ·  click: keep it on",
                None => "  ·  click a jack to keep it on; CUE or Esc stops all",
            };
            format!("{what}{click}")
        } else {
            what
        }
    }

    /// Whether a jack or a port is kept on, from the desk or from ENGINE: a jack with a port of it
    /// kept on, a port of a jack kept on.
    fn kept(&self, k: &str) -> bool {
        let jack = self.scene.cue_jack(k);
        self.cue.pins.iter().any(|p| p == k || (scene::port_of_key(k).is_none() && scene::port_of_key(p).is_some() && self.scene.cue_jack(p) == Some(k)) || (scene::port_of_key(k).is_some() && jack == Some(p.as_str())))
    }

    /// The status bar over a port in ENGINE in CUE mode: what a click does to it.
    fn port_hint(&self, k: &str) -> String {
        let title = self.scene.jack_title(k);
        if self.scene.cue_blocked(k) {
            feedback(&title)
        } else if self.kept(k) {
            format!("{title}  ·  click: stop it")
        } else {
            format!("{title}  ·  click: listen to it")
        }
    }

    /// What is heard, for the status bar: "CUE ▸ fooyin · OUTPUT · in the default output at 70%".
    fn cue_what(&self) -> Option<String> {
        let heard: Vec<String> = self
            .cue_heard()
            .iter()
            .filter(|k| !self.scene.cue_blocked(k))
            .map(|k| if self.scene.cue_already_heard(k) { format!("{} (already in the output you listen on)", self.scene.jack_title(k)) } else { self.scene.jack_title(k) })
            .collect();
        (!heard.is_empty()).then(|| format!("CUE ▸ {} · in the default output at {:.0}%", heard.join(" + "), self.cue.level * 100.0))
    }

    /// CUE mode on or off; either way it starts afresh: every jack heard stops.
    fn set_cue_mode(&mut self, on: bool) {
        self.cue.mode = on;
        self.engine.port_hover = None;
        self.cue.pins.clear();
        if !self.cue.probing {
            self.cue.hover = None;
        }
        self.say(match (on, self.engine.on) {
            (true, false) => "CUE: hover a jack to hear it, click to keep it on; its hole still patches",
            (true, true) => "CUE: click a port to listen to it, click again to stop",
            (false, _) => "Cue off",
        });
        self.redraw();
    }

    /// Glides the view to a device.
    fn find_device(&mut self, di: usize) {
        let dpi = self.dpi();
        let (top, bottom) = self.field();
        let Some(d) = self.scene.devs.get(di) else { return };
        let s = self.view.s.max(0.8 * dpi);
        let (cx, cy) = (d.x + d.geo.w / 2.0, d.y + d.h() / 2.0);
        self.view_to = Some(View { s, x: self.size.0 as f64 / 2.0 - cx * s, y: (top + bottom) / 2.0 - cy * s });
        self.redraw();
    }

    /// What a panel key does.
    fn panel_key(&mut self, key: Key) {
        if ((self.engine.on || self.mixer.on) && key.desk_only()) || (!self.engine.on && !self.mixer.on && self.scene.rack && key == Key::Align) {
            return;
        }
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
                // rows of unused devices shaped to the window
                let (top, bottom) = self.field();
                (self.scene.view_w, self.scene.view_h) = (self.size.0 as f64 / self.dpi(), (bottom - top) / self.dpi());
                self.scene.arrange();
                self.fit(true);
            }
            Key::Hidden => self.open_hidden(true),
            Key::Flow => {
                self.scene.flow = !self.scene.flow;
                self.say(if self.scene.flow { "Cord directions: on" } else { "Cord directions: off" });
                self.save_settings();
            }
            Key::Fit if self.engine.on => self.engine.fit(self.size, self.dpi()),
            Key::Fit => self.fit(true),
            // LAYOUT: each key shows its own screen, pressed again it stays there (nothing goes
            // back and forth); each has its own key on the keyboard (D, M, R, E)
            Key::Engine => self.set_engine(true),
            Key::Mixer => {
                self.set_engine(false);
                self.set_mixer(true);
            }
            Key::Desk | Key::Rack => {
                self.set_engine(false);
                self.set_mixer(false);
                self.scene.set_rack(key == Key::Rack);
                self.save_settings();
                // at once: a glide between screens only gets in the way
                self.view_to = None;
                self.fit(false);
            }
            Key::Save => self.open_menu(true),
            Key::Lcd => self.open_menu(false),
            Key::Defaults => self.open_defaults(true),
            Key::Cue => self.set_cue_mode(!self.cue.mode),
            Key::CueLevel => {}
            Key::More => {
                self.panel.state.more = !self.panel.state.more;
                if !self.panel.state.more {
                    self.panel.menu = None;
                }
            }
        }
        self.redraw();
    }

    /// Shows ENGINE (the real graph) or goes back to the desk or rack.
    fn set_engine(&mut self, on: bool) {
        if self.engine.on == on {
            return;
        }
        // what is in the hand is let go first, where the switch can't hide it: a plug falls
        // without going into a jack nearby
        if let Some(Drag::Plug { magnet, .. }) = &mut self.drag {
            *magnet = None;
        }
        // (a key, a knob or the cords switch held is just let go: not clicked)
        if matches!(self.drag, Some(Drag::Plug { .. } | Drag::Device { .. } | Drag::Cord { .. } | Drag::Engine)) {
            self.release();
        }
        // (the panel's key that switched it stays down until it is let go)
        if !matches!(self.drag, Some(Drag::Key)) {
            self.drag = None;
            self.panel.state.pressed = None;
        }
        self.engine.set_on(on);
        // what was under the pointer is on the other screen now (and Alt doesn't probe in ENGINE)
        self.cue.hover = None;
        self.engine.port_hover = None;
        if on {
            self.cue.probing = false;
        }
        self.cue.in_hole = false;
        self.profiles = None;
        self.panel.menu = None;
        self.set_hover(Hover::None);
        self.tip = None;
        self.hint.clear();
        self.send_engine();
        self.redraw();
    }

    /// MIXER is what shows (not ENGINE over it).
    fn mixer_shown(&self) -> bool {
        self.mixer.on && !self.engine.on
    }

    /// Shows MIXER, or goes back to the desk or rack.
    fn set_mixer(&mut self, on: bool) {
        if self.mixer.on == on {
            return;
        }
        if let Some(Drag::Plug { magnet, .. }) = &mut self.drag {
            *magnet = None;
        }
        if matches!(self.drag, Some(Drag::Plug { .. } | Drag::Device { .. } | Drag::Cord { .. } | Drag::Engine | Drag::Mixer)) {
            self.release();
        }
        // (the panel's key that switched it stays down until it is let go)
        if !matches!(self.drag, Some(Drag::Key)) {
            self.drag = None;
            self.panel.state.pressed = None;
        }
        self.mixer.on = on;
        self.mixer.let_go(&mut self.scene);
        self.cue.hover = None;
        self.cue.in_hole = false;
        if on {
            self.cue.probing = false;
        }
        self.profiles = None;
        self.panel.menu = None;
        self.set_hover(Hover::None);
        self.tip = None;
        self.hint.clear();
        self.save_settings();
        self.redraw();
    }

    /// What a key of MIXER asks for.
    fn mixer_act(&mut self, a: Option<mixer::Action>) {
        match a {
            Some(mixer::Action::Graph(node)) => {
                self.set_engine(true);
                self.engine.show_node(node);
                self.send_engine();
            }
            Some(mixer::Action::Profile(di, r)) => self.open_list(di, ListOf::Profiles, r),
            Some(mixer::Action::Port(di, r)) => self.open_list(di, ListOf::Ports, r),
            Some(mixer::Action::Cue(keys)) => self.cue_group(keys),
            Some(mixer::Action::Changed) => self.after_patch(),
            None => {}
        }
    }

    /// A card's PROFILE or PORT list, opened from MIXER under the key at `at` (window pixels).
    fn open_list(&mut self, di: usize, what: ListOf, at: [f64; 4]) {
        let Some(key) = self.scene.devs.get(di).map(|d| d.dev.key.clone()) else { return };
        // its own key again closes it
        if self.profiles.as_ref().is_some_and(|p| p.key == key && p.what == what) {
            self.profiles = None;
            return;
        }
        self.profiles = Some(ProfileList { key, what, sig: 0, layer: None, hits: Vec::new(), at: (0.0, 0.0), anchor: Some(at) });
    }

    /// MIXER's CUE key: a group's jacks heard (in CUE mode), or no longer.
    fn cue_group(&mut self, keys: Vec<String>) {
        if keys.is_empty() {
            return;
        }
        if keys.iter().all(|k| self.cue.pins.contains(k)) {
            self.cue.pins.retain(|p| !keys.contains(p));
            if self.cue.pins.is_empty() && self.cue.mode {
                self.set_cue_mode(false);
            }
            return;
        }
        if keys.iter().any(|k| self.scene.cue_blocked(k)) {
            return self.say("That is the output you listen on: it would feed back");
        }
        if !self.cue.mode {
            self.set_cue_mode(true);
        }
        for k in keys {
            if !self.cue.pins.contains(&k) {
                self.cue.pins.push(k);
            }
        }
    }

    /// ENGINE's report for a bug ticket, into the home folder.
    fn save_report(&mut self) {
        let text = self.engine.report(self.scene.version.as_deref().unwrap_or("?"));
        let name = format!("patchcord-report-{}.txt", super::engine::local_stamp().1);
        match crate::dirs::home().map(|d| d.join(name)) {
            Some(path) => match std::fs::write(&path, text) {
                Ok(()) => {
                    let shown = path.to_string_lossy().replace(&std::env::var("HOME").unwrap_or_default(), "~");
                    self.say(&format!("Report saved: {shown}"));
                }
                Err(e) => self.say(&format!("Cannot save the report: {e}")),
            },
            None => self.say("Cannot save the report: no home folder"),
        }
    }

    fn send_engine(&mut self) {
        for c in self.engine.outbox.drain(..) {
            self.conn.send(c);
        }
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
        let v = self.scene.fit(self.size.0 as f64, bottom - top, top, 2.0 * dpi, super::scene::EDGE * dpi);
        if glide {
            self.view_to = Some(v);
        } else {
            self.view = v;
        }
        self.redraw();
    }

    /// The rack is shown (not ENGINE over it).
    fn in_rack(&self) -> bool {
        self.scene.rack && !self.engine.on && self.size.0 > 0
    }

    /// A view kept to the rack: always at 100% (it doesn't zoom: it is as wide as the window, its
    /// rails at the window's edges); not above its first device, not below what it has (the last
    /// device, or a cord hanging lower) unless the window is taller. Only a window narrower than
    /// the rack's widest frame lets it move sideways. Off the rack any view goes.
    fn clamp_view(&self, v: View) -> View {
        let Some((l, r, b)) = self.scene.rack_extent().filter(|_| self.in_rack()) else { return v };
        let (top, bottom) = self.field();
        let (w, s) = (self.size.0 as f64, self.dpi());
        let x = if (r - l) * s <= w + 0.5 { -l * s } else { v.x.min(-l * s).max(w - r * s) };
        View { s, x, y: v.y.min(top).max((bottom - b * s).min(top)) }
    }

    /// The wheel in the rack: up and down (sideways with Shift, or a touchpad's sideways, in a
    /// window narrower than the rack); a wheel's click glides a little, a touchpad's pixels go as
    /// they come.
    fn scroll_rack(&mut self, delta: MouseScrollDelta) {
        let dpi = self.dpi();
        let (mut dx, mut dy, glide) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x as f64 * 90.0 * dpi, y as f64 * 90.0 * dpi, true),
            MouseScrollDelta::PixelDelta(p) => (p.x, p.y, false),
        };
        if self.mods.shift_key() && dx == 0.0 {
            (dx, dy) = (dy, 0.0);
        }
        if glide {
            let base = self.view_to.unwrap_or(self.view);
            self.view_to = Some(self.clamp_view(View { x: base.x + dx, y: base.y + dy, ..base }));
        } else {
            self.view = self.clamp_view(View { x: self.view.x + dx, y: self.view.y + dy, ..self.view });
            self.view_to = None;
        }
        self.scrolled = Some(Instant::now());
        self.redraw();
    }

    /// The rack's scroll bars, where it goes on beyond the window: down the right edge, along the
    /// bottom one. Track and thumb, in window pixels.
    fn rack_bars(&self) -> Vec<(usize, [f64; 4])> {
        let Some((l, r, b)) = self.scene.rack_extent().filter(|_| self.in_rack()) else { return Vec::new() };
        let (top, bottom) = self.field();
        let (v, w, dpi) = (self.view, self.size.0 as f64, self.dpi());
        let (fh, t) = (bottom - top, 4.0 * dpi);
        let mut out = Vec::new();
        // (the rack's height: what it has, or the window's if that is more)
        let ch = (b * v.s).max(fh);
        if ch > fh + 1.0 {
            let th = (fh * fh / ch).max(24.0 * dpi);
            let at = ((top - v.y) / (ch - fh)).clamp(0.0, 1.0);
            out.push((0, [w - t - 2.0, top + 2.0, t, fh - 4.0]));
            out.push((1, [w - t - 2.0, top + 2.0 + (fh - 4.0 - th) * at, t, th]));
        }
        let cw = (r - l) * v.s;
        if cw > w + 1.0 {
            let tw = (w * w / cw).max(24.0 * dpi);
            let at = ((-l * v.s - v.x) / (cw - w)).clamp(0.0, 1.0);
            out.push((0, [2.0, bottom - t - 2.0, w - 4.0, t]));
            out.push((1, [2.0 + (w - 4.0 - tw) * at, bottom - t - 2.0, tw, t]));
        }
        out
    }

    /// While a plug or a device is carried in the rack, the pointer near the field's top or bottom
    /// scrolls the rack that way, the faster the nearer the edge: to patch to a device out of
    /// sight, or to take one there.
    fn edge_scroll(&mut self, dt: f64) {
        if !matches!(self.drag, Some(Drag::Plug { .. } | Drag::Device { .. })) || !self.in_rack() {
            return;
        }
        let (top, bottom) = self.field();
        let band = ((bottom - top) * 0.1).min(70.0 * self.dpi());
        let y = self.cursor.1;
        let k = if y < top + band { (top + band - y) / band } else if y > bottom - band { -(y - (bottom - band)) / band } else { 0.0 };
        if k != 0.0 {
            self.view.y += k.clamp(-1.0, 1.0) * 1400.0 * self.dpi() * dt;
            self.view_to = None;
            // a carried device goes with the hand, the rack moving under it
            if let Some(Drag::Device { key, off }) = &self.drag {
                self.view = self.clamp_view(self.view);
                let w = self.world(self.cursor);
                let (key, x, y) = (key.clone(), w.0 - off.0, w.1 - off.1);
                let px = self.dpi() / self.view.s;
                self.scene.drag_device(&key, x, y, px);
            }
        }
    }

    fn frame(&mut self) {
        self.prof.begin();
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.05);
        self.last = now;
        // the rack is as wide as the window, and goes side by side by its height (see ROOM_DELAY)
        let (top, bottom) = self.field();
        let room = (self.size.0 as f64 / self.dpi(), ((bottom - top) / self.dpi()).max(0.0));
        let quiet = self.room_at.1.elapsed() >= ROOM_DELAY;
        let changed = !same_room(room, self.room_at.0);
        if changed {
            self.room_at = (room, now);
        }
        if self.scene.rack_room().0 <= 0.0 || self.room_at.1.elapsed() >= ROOM_DELAY || (changed && quiet) {
            self.scene.set_rack_room(room.0, room.1);
        }
        self.edge_scroll(dt as f64);
        // the rack's ends hold whatever moved the view: a pan, a zoom, a glide, the window's size
        self.view_to = self.view_to.map(|t| self.clamp_view(t));
        self.view = self.clamp_view(self.view);
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
        self.prof.mark("view");
        let moving = self.scene.tick(dt, hand, held, matches!(self.drag, Some(Drag::Device { .. })));
        self.prof.mark("tick");
        for (what, d) in &self.scene.spent {
            self.prof.add(what, *d);
        }

        // faces are drawn at one scale; after zooming, sharp ones replace the stretched ones. Not
        // below half the screen's: zoomed out far they are only made smaller, so zooming back in
        // stretches a picture of some size meanwhile, not a tiny one
        let target = self.view.s.max(0.5 * self.dpi());
        if self.raster == 0.0 {
            self.raster = target;
        } else if (target - self.raster).abs() > 1e-3 {
            match self.rescale_at {
                Some(t) if now >= t && self.view_to.is_none() => {
                    self.raster = target;
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
        self.scene.rail_numbers((self.field().1 - self.view.y) / self.view.s);
        self.prof.mark("faces");
        for (what, d) in &self.scene.prep_spent {
            self.prof.add(what, *d);
        }
        // the background for a new window size: drawn on a thread (the old one stretches meanwhile),
        // the first one here
        if let Some(rx) = &self.bg_job
            && let Ok(pm) = rx.try_recv()
        {
            self.bg = Some((next_id(), pm));
            self.bg_job = None;
        }
        if self.bg.is_none() {
            self.bg = Some((next_id(), background(self.size.0, self.size.1)));
            self.prof.mark("background");
        } else if self.bg.as_ref().is_some_and(|(_, pm)| (pm.width(), pm.height()) != self.size) && self.bg_job.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let (w, h) = self.size;
            let _ = std::thread::Builder::new().name("patchcord-background".into()).spawn(move || {
                let _ = tx.send(background(w, h));
            });
            self.bg_job = Some(rx);
            self.prof.mark("background");
        }
        // the panel, the status bar and a notice, over the field
        let dpi = self.dpi();
        let width = self.size.0 as f64 / dpi;
        self.panel.state.cables = self.scene.cables;
        self.panel.state.align = self.scene.align;
        self.panel.state.rack = self.scene.rack;
        self.panel.state.engine = self.engine.on;
        self.panel.state.mixer = self.mixer.on;
        self.panel.state.lcd = self.lcd();
        self.panel.state.defaults = [true, false].map(|output| self.scene.default_shown(output));
        // the CUE LED: lit in CUE mode
        self.panel.state.cue_led = self.cue.mode;
        self.panel.state.cue_level = (self.cue.level * 100.0).round() as u8;
        self.panel.state.hidden = self.scene.hidden_devs.len();
        self.panel.state.flow = self.scene.flow;
        // hidden devices that got a cord came back by themselves
        if !self.scene.shown_again.is_empty() {
            let names = std::mem::take(&mut self.scene.shown_again).join(", ");
            self.say(&format!("{names} shown again: it got a cord"));
            self.save_settings();
        }
        if let Some(Menu::Hidden { .. }) = &self.panel.menu {
            let m = hidden_menu(&self.scene);
            if self.panel.menu.as_ref() != Some(&m) {
                self.panel.menu = Some(m);
            }
        }
        self.prof.mark("panel.state");
        self.cue_sync();
        self.prof.mark("cue");
        // the open defaults list follows what is the default
        if let Some(Menu::Defaults { .. }) = &self.panel.menu {
            let m = defaults_menu(&self.scene);
            if self.panel.menu.as_ref() != Some(&m) {
                self.panel.menu = Some(m);
            }
        }
        self.prof.mark("menus");
        self.panel.layer(width, dpi);
        self.prof.mark("panel");
        self.panel.menu_layer(width, dpi);
        self.prof.mark("menu");
        if self.engine.on && self.engine.due() {
            let devices: Vec<&crate::graph::Device> = self.scene.devs.iter().map(|d| &d.dev).chain(self.scene.hidden_devs.iter()).collect();
            let hidden: std::collections::HashSet<String> = self.scene.hidden_devs.iter().map(|d| d.key.clone()).collect();
            self.engine.build(&self.scene.graph, &devices, &hidden, &self.scene.settings);
            self.prof.mark("engine.build");
        }
        if self.engine.on {
            // the ports of the jacks kept on, all of them: also those the cue leaves out (heard
            // in the output already, or asleep)
            // (and a port kept on from ENGINE itself, an input too)
            let mut kept: Vec<u32> = self.cue_heard().iter().filter(|k| !self.scene.cue_blocked(k)).flat_map(|k| self.scene.cue_sources(k).into_iter().map(|s| s.1).chain(scene::port_of_key(k))).collect();
            kept.sort_unstable();
            kept.dedup();
            self.engine.cue_kept = kept;
        }
        let left = if self.engine.on {
            self.engine.tab_hint_at(self.cursor, dpi).map_or_else(|| self.engine.summary(), str::to_owned)
        } else {
            format!(
                "PipeWire {} · {} devices · {} links · {} cords",
                self.scene.version.as_deref().unwrap_or("?"),
                self.scene.devs.len(),
                self.scene.graph.link_count(),
                self.scene.cords.iter().filter(|c| c.live()).count()
            )
        };
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

        self.prof.mark("status");
        self.place_profiles(dpi);
        self.prof.mark("profiles");
        let pulse = 0.5 + 0.5 * ((now - self.start).as_secs_f32() * 6.0).sin();
        // the jacks heard: pictures made once, a disc under a jack kept on and a ring over it
        let mixer_on = self.mixer_shown();
        let cue = if self.engine.on || mixer_on { Vec::new() } else { self.cue_marks() };
        // the cords are triangles for the GPU
        let mesh = (!self.engine.on && !mixer_on).then(|| super::mesh::cords(&self.scene.cords, |c| self.scene.cord_alpha(c), self.view));
        self.prof.mark("cords");

        // the badge beside the pointer while listening, over the field
        let (ftop, fbottom) = self.field();
        // (in ENGINE over the graph itself)
        // (not over a menu or a list, nor outside the window)
        let over = self.cue.inside && self.on_field() && !mixer_on && if self.engine.on { self.engine.over_graph(self.cursor, self.size, self.dpi()) } else { !self.cue.in_hole };
        let show_badge = (self.cue.probing || self.cue.mode) && over && self.cursor.1 >= ftop && self.cursor.1 <= fbottom && self.drag.is_none();
        if show_badge {
            let raster = (self.dpi() * 1e6).round() as u64;
            if self.cue.badge.as_ref().is_none_or(|(s, _)| *s != raster) {
                self.cue.badge = Some((raster, panel::cue_badge(self.dpi())));
            }
        }
        let version = self.scene.version.clone().unwrap_or_else(|| "?".into());
        let field = self.field();
        let bars = if mixer_on { self.mixer.bars(self.size, dpi) } else { self.rack_bars() };
        let pins = self.cue.pins.clone();
        // (the GPU keeps MIXER's modules while another screen shows)
        let mixer_ids: Vec<u64> = self.mixer.ids().collect();
        let Some(sf) = &mut self.win else { return };
        let v = self.view;
        let mut sprites: Vec<Sprite> = Vec::new();
        if let Some((id, pm)) = &self.bg {
            // stretched over the window while one for its new size is drawn
            sprites.push(Sprite { id: *id, pm: Some(pm), x: 0.0, y: 0.0, w: self.size.0 as f64, h: self.size.1 as f64, alpha: 1.0, clip: None });
        }
        let engine_on = self.engine.on;
        let graph_on = engine_on;
        if mixer_on {
            for (l, x, y, w, h, clip, alpha) in self.mixer.sprites(&self.scene, &pins, self.size, dpi) {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x, y, w, h, alpha, clip });
            }
            self.prof.mark("mixer.pictures");
        }
        // (MIXER hides the desk or the rack as ENGINE does)
        let engine_on = engine_on || mixer_on;
        if self.engine.on {
            self.prof.mark("badge");
            for (l, x, y, w, h, clip, alpha) in self.engine.sprites(self.size, dpi, &version) {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: x.round(), y: y.round(), w, h, alpha, clip });
            }
            self.prof.mark("engine.pictures");
        }
        if !engine_on && self.scene.rack {
            // the rails, unit by unit down the field: on whole pixels, so no seam shows between
            let (top, bottom) = field;
            for (l, [x, y, w, h]) in self.scene.rails((top - v.y) / v.s, (bottom - v.y) / v.s) {
                let (x0, y0) = ((x * v.s + v.x).round(), (y * v.s + v.y).round());
                let (x1, y1) = (((x + w) * v.s + v.x).round(), ((y + h) * v.s + v.y).round());
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: x0, y: y0, w: x1 - x0, h: y1 - y0, alpha: 1.0, clip: None });
            }
        }
        let targets = match &self.drag {
            Some(Drag::Plug { targets, .. }) => Some(targets.as_slice()),
            _ => None,
        };
        for (di, d) in self.scene.devs.iter().enumerate().filter(|_| !engine_on) {
            for l in d.layers() {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (d.x + l.x) * v.s + v.x, y: (d.y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0, clip: None });
            }
            // the jacks of it kept on: their discs, under the jacks
            for &(_, (x, y), pic, _) in cue.iter().filter(|m| m.0 == di && m.3) {
                if let Some(l) = &self.scene.jack_pics[pic] {
                    sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (x as f64 + l.x) * v.s + v.x, y: (y as f64 + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0, clip: None });
                }
            }
            // its jacks, over the face
            for (pic, x, y, alpha) in self.scene.jack_sprites(di, targets) {
                if let Some(l) = &self.scene.jack_pics[pic] {
                    sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (x + l.x) * v.s + v.x, y: (y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha, clip: None });
                }
            }
        }
        // alignment guides, over the faces and under the cords
        let lw = 1.5 * dpi;
        for &[x0, y0, x1, y1] in &self.scene.guides {
            let (x, y) = (x0 * v.s + v.x, y0 * v.s + v.y);
            let (w, h) = (((x1 - x0) * v.s).max(lw), ((y1 - y0) * v.s).max(lw));
            let (x, y) = if x0 == x1 { (x - lw / 2.0, y) } else { (x, y - lw / 2.0) };
            sprites.push(Sprite { id: self.guide.0, pm: Some(&self.guide.1), x, y, w, h, alpha: 0.85, clip: None });
        }
        // the rings round the jacks a cord's end in the hand may go into
        for ((x, y), hot) in &rings {
            if let Some(l) = &self.scene.jack_pics[TARGET_PICS + *hot as usize] {
                let alpha = if *hot { 1.0 } else { 0.6 + 0.4 * pulse };
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (*x as f64 + l.x) * v.s + v.x, y: (*y as f64 + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha, clip: None });
            }
        }
        // the rings of the jacks heard, with their headphones
        for (_, (x, y), pic, _) in cue.iter().filter(|m| !m.3) {
            if let Some(l) = &self.scene.jack_pics[*pic] {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (*x as f64 + l.x) * v.s + v.x, y: (*y as f64 + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0, clip: None });
            }
        }
        // the cords, over the faces and the rings, under the notices and the panel
        let cords_at = sprites.len();
        let (fw, fh) = (self.size.0 as f64, self.size.1 as f64);
        // the rack's scroll bars, over the cords
        for (i, [x, y, w, h]) in bars {
            let l = &self.bar_px[i];
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: x.round(), y: y.round(), w: w.round(), h: h.round(), alpha: if i == 0 { 0.55 } else { 0.8 }, clip: None });
        }
        if let Some((_, l)) = &self.notice {
            // under the panel, or under MORE's drop-down while it is open (else it hides it)
            let under = self.panel.more_bottom().unwrap_or(0.0).max(panel::H);
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: ((fw - l.w * dpi) / 2.0).round(), y: ((under + 14.0) * dpi).round(), w: l.w * dpi, h: l.h * dpi, alpha: 1.0, clip: None });
        }
        if let Some((_, l)) = &self.status {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: 0.0, y: fh - l.h * dpi, w: l.w * dpi, h: l.h * dpi, alpha: 1.0, clip: None });
        }
        // the bare panel where the window has grown past its picture, until the new one comes
        if let (Some(t), Some(pl)) = (self.panel.tile(), self.panel.layers().next()) {
            let mut x = (pl.w / panel::TILE_W).floor() * panel::TILE_W;
            while x < fw / dpi {
                let (x0, x1) = ((x * dpi).round(), ((x + panel::TILE_W) * dpi).round());
                sprites.push(Sprite { id: t.id, pm: t.pm.as_ref(), x: x0, y: 0.0, w: x1 - x0, h: t.h * dpi, alpha: 1.0, clip: None });
                x += panel::TILE_W;
            }
        }
        for l in self.panel.layers() {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (l.x * dpi).round(), y: (l.y * dpi).round(), w: l.w * dpi, h: l.h * dpi, alpha: 1.0, clip: None });
        }
        if let Some(pl) = &self.profiles
            && !graph_on
            && let Some(l) = &pl.layer
        {
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: pl.at.0, y: pl.at.1, w: l.w * dpi, h: l.h * dpi, alpha: 1.0, clip: None });
        }
        if show_badge && let Some((_, l)) = &self.cue.badge {
            // below and right of the pointer's tip, clear of the arrow
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (self.cursor.0 + 12.0 * dpi).round(), y: (self.cursor.1 + 14.0 * dpi).round(), w: l.w * dpi, h: l.h * dpi, alpha: 1.0, clip: None });
        }
        if let Some(l) = &self.tip {
            // below and right of the pointer; kept inside the window, above the pointer near the bottom
            let (w, h) = (l.w * dpi, l.h * dpi);
            let x = (self.cursor.0 + 14.0 * dpi).min(fw - w - 4.0 * dpi).max(4.0 * dpi);
            let below = self.cursor.1 + 22.0 * dpi;
            let y = if below + h > fh - panel::STATUS_H * dpi { self.cursor.1 - h - 10.0 * dpi } else { below };
            sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: x.round(), y: y.round(), w, h, alpha: 1.0, clip: None });
        }
        self.prof.mark("sprites");
        // while ENGINE shows, the GPU keeps the desk's pictures: back on the desk they are there;
        // the devices' faces for the other screen (desk or rack) and the jacks' pictures always
        let keep: std::collections::HashSet<u64> = if engine_on { self.scene.layer_ids().chain(mixer_ids).collect() } else { self.scene.pic_ids().chain(mixer_ids).collect() };
        let [wait, upload, draw] = sf.gpu.draw(&sprites, mesh.as_ref().map(|m| (cords_at, m)), &keep);
        self.prof.mark("gpu");
        self.prof.add("gpu.wait", wait);
        self.prof.add("gpu.upload", upload);
        self.prof.add("gpu.draw", draw);
        drop(sprites);
        if self.engine.on {
            for (what, d) in &self.engine.spent {
                self.prof.add(what, *d);
            }
        }
        self.panel.uploaded();
        for l in [self.status.as_mut().map(|(_, l)| l), self.notice.as_mut().map(|(_, l)| l), self.tip.as_mut()].into_iter().flatten() {
            if sf.gpu.has(l.id) {
                l.pm = None;
            }
        }
        // the GPU has the pixels now (the jack pictures are kept: they are small, and the GPU lets
        // a texture go in a frame that doesn't draw it)
        // (big pictures let go: their memory back to the system, see `mem`)
        let mut freed = self.engine.uploaded(|id| sf.gpu.has(id)) + self.mixer.uploaded(|id| sf.gpu.has(id));
        for d in &mut self.scene.devs {
            for l in d.layers_mut() {
                if sf.gpu.has(l.id) {
                    freed += l.pm.take().map_or(0, |pm| pm.data().len());
                }
            }
        }
        if freed > 4 << 20 {
            crate::mem::give_back();
            self.prof.mark("give_back");
        }
        self.send_engine();
        // (a new size the rack is still to take)
        let room_waits = self.scene.rack && !same_room(self.room_at.0, self.scene.rack_room());
        self.animating = room_waits || (self.engine.on && self.engine.settling()) || (mixer_on && (self.mixer.settling() || self.mixer.dragging())) || moving || self.view_to.is_some() || self.drag.is_some() || self.rescale_at.is_some() || self.notice.is_some() || faces_waiting || self.panel.busy() || self.bg_job.is_some() || self.scene.noting() || (self.scene.hover != Hover::None && self.tip.is_none() && now - self.hover_since < 2 * TIP_DELAY);
        self.prof.mark("rest");
        let (engine, rack, s, cords, devs) = (self.engine.on, self.scene.rack, self.view.s, self.scene.cords.len(), self.scene.devs.len());
        let (drag, zoom) = (self.drag.is_some(), self.rescale_at.is_some() || faces_waiting);
        self.prof.end(self.animating, || format!("{} scale {s:.2} · {devs} devices · {cords} cords{}{}", if engine { "ENGINE" } else if mixer_on { "MIXER" } else if rack { "RACK" } else { "DESK" }, if drag { " · dragging" } else { "" }, if zoom { " · faces at a new scale" } else { "" }));
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
        let field = self.field();
        let Some(pl) = &mut self.profiles else { return };
        let Some(d) = self.scene.devs.iter().find(|d| d.dev.key == pl.key) else {
            self.profiles = None;
            return;
        };
        let key = match (pl.anchor, pl.what) {
            // from MIXER: the key's rectangle in window pixels, made face units of a view at 1
            (Some([ax, ay, aw, ah]), _) => Some([ax, ay, aw, ah]),
            (None, ListOf::Profiles) => d.geo.profile,
            (None, ListOf::Ports) => d.geo.port,
        };
        let (Some(card), Some([kx, ky, _, kh])) = (&d.dev.card, key) else {
            self.profiles = None;
            return;
        };
        let sections = if pl.what == ListOf::Ports { self.scene.port_sections(self.scene.devs.iter().position(|x| x.dev.key == pl.key).unwrap_or(0)) } else { Vec::new() };
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(format!("{card:?}{sections:?}"), &d.dev.name, dpi.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if pl.sig != sig || pl.layer.is_none() {
            let (layer, hits) = match pl.what {
                ListOf::Ports => panel::port_menu(&sections, dpi),
                ListOf::Profiles => panel::profile_menu(card, dpi),
            };
            (pl.sig, pl.layer, pl.hits) = (sig, Some(layer), hits);
        }
        // under the key, its left edge at the key's, kept inside the field: over the key when
        // there is no room under it, else as low as fits
        let (v, (ox, oy)) = if pl.anchor.is_some() { (View { x: 0.0, y: 0.0, s: 1.0 }, (0.0, 0.0)) } else { (self.view, (d.x, d.y)) };
        let x = (ox + kx) * v.s + v.x;
        let y = (oy + ky + kh) * v.s + v.y + 4.0 * dpi;
        let w = panel::PROFILE_W * dpi;
        let h = pl.layer.as_ref().and_then(|l| l.pm.as_ref()).map_or(0.0, |pm| pm.height() as f64);
        let (top, bottom) = (field.0 + 6.0 * dpi, field.1 - 6.0 * dpi);
        let above = (oy + ky) * v.s + v.y - 4.0 * dpi - h;
        let y = if y + h <= bottom { y } else if above >= top { above } else { (bottom - h).max(top) };
        pl.at = (x.min(self.size.0 as f64 - w - 6.0 * dpi).max(6.0 * dpi).round(), y.round());
    }

    fn press(&mut self, button: MouseButton) {
        self.set_hover(Hover::None);
        let w = self.world(self.cursor);
        let (top, bottom) = self.field();
        let (px, py) = (self.cursor.0 / self.dpi(), self.cursor.1 / self.dpi());
        let in_more = self.panel.in_more(px, py);
        // a click off MORE's drop-down closes it, and does what it does there too (on the panel, or
        // on the field: a jack heard in CUE mode at once, a plug taken)
        if self.panel.state.more && !in_more && self.panel.hit(px, py) != Some(Key::More) && self.panel.menu_hit(px, py).is_none() {
            // and a list opened from it
            self.panel.state.more = false;
            self.panel.menu = None;
            self.redraw();
        }
        // ENGINE takes the clicks between the panel and the status bar
        if self.engine.on && self.cursor.1 >= top && self.cursor.1 <= bottom && self.panel.menu.is_none() && !in_more {
            // in CUE mode a click on a port keeps it playing, or stops it
            if button == MouseButton::Left
                && self.cue.mode
                && let Some(k) = self.engine_probe()
            {
                self.toggle_pin(k);
                return self.redraw();
            }
            if button == MouseButton::Left {
                match self.engine.press(self.cursor, self.size, self.dpi()) {
                    Some(Action::Report) => self.save_report(),
                    Some(Action::Desk(key)) => {
                        self.set_engine(false);
                        if let Some(di) = self.scene.dev_index(&key) {
                            self.find_device(di);
                        }
                    }
                    None => self.drag = Some(Drag::Engine),
                }
                self.send_engine();
            }
            self.redraw();
            return;
        }
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
                    if let Some(di) = self.scene.dev_index(&pl.key) {
                        self.scene.set_port(di, gi, index);
                    }
                    return self.after_patch();
                }
                Some(ProfileHit::Latency(gi, step)) => {
                    // 1 ms a click, 10 with Shift
                    if let Some(di) = self.scene.dev_index(&pl.key) {
                        let now = self.scene.port_sections(di).into_iter().find(|s| s.group == gi).map_or(0, |s| s.latency_ns);
                        let ms = if self.mods.shift_key() { 10 } else { 1 };
                        self.scene.set_latency(di, gi, now + step * ms * 1_000_000);
                    }
                    return self.after_patch();
                }
                Some(ProfileHit::Inside) => return,
                None => {
                    let key_at = if self.mixer_shown() {
                        self.mixer.list_key_at(self.cursor, pl.what == ListOf::Ports, self.dpi())
                    } else {
                        match pl.what {
                            ListOf::Ports => self.scene.port_key_at(w.0, w.1),
                            ListOf::Profiles => self.scene.profile_key_at(w.0, w.1),
                        }
                    };
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
                        // a hidden one is shown again first
                        let n = self.scene.devs.len();
                        let di = if e.0 < n { Some(e.0) } else { self.scene.hidden_devs.get(e.0 - n).map(|d| d.key.clone()).and_then(|k| self.scene.show(&k)) };
                        if e.0 >= n {
                            self.save_settings();
                        }
                        if let Some(di) = di {
                            self.find_device(di);
                        }
                    }
                }
                Some(MenuHit::Show(i)) => {
                    if let Some(key) = self.scene.hidden_devs.get(i).map(|d| d.key.clone())
                        && let Some(di) = self.scene.show(&key)
                    {
                        self.save_settings();
                        self.find_device(di);
                    }
                }
                Some(MenuHit::Inside) => {}
                None => {
                    // the keys that open it toggle it themselves
                    let key = self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi);
                    if !matches!(key, Some(Key::Save | Key::Lcd | Key::Defaults | Key::Hidden)) {
                        self.panel.menu = None;
                        // MORE closes with the list it holds open
                        if key == Some(Key::More) {
                            self.panel.state.more = false;
                        }
                        self.redraw();
                        return;
                    }
                }
            }
            // the keys that open it, on the panel or in MORE, go on to toggle it
            if self.panel.menu_hit(self.cursor.0 / dpi, self.cursor.1 / dpi).is_some() || (self.cursor.1 >= top && !in_more) {
                return;
            }
        }
        if self.cursor.1 < top || in_more {
            // the top panel, and MORE's drop-down
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
        // MIXER takes the clicks on the field
        if self.mixer_shown() {
            if button == MouseButton::Left {
                let dpi = self.dpi();
                let a = self.mixer.press(&mut self.scene, self.cursor, dpi);
                self.mixer_act(a);
                if self.mixer.dragging() {
                    self.drag = Some(Drag::Mixer);
                }
            }
            return self.redraw();
        }
        if button != MouseButton::Left {
            self.drag = Some(Drag::Pan { from: self.cursor, view: self.view });
            return;
        }
        // in CUE mode a click on a jack keeps it playing, or stops it, instead of pulling a cord;
        // its hole still patches (unless Alt is held: then the whole jack listens)
        if self.cue.mode
            && (self.cue.probing || self.hole_at(w).is_none())
            && let Some(k) = self.scene.jack_near(w.0, w.1, scene::RING)
        {
            self.toggle_pin(k);
            return self.redraw();
        }
        // with Alt held the desk and the rack only listen: no cord is touched (nor the device
        // under a jack moved); let Alt go to patch
        let wires = !self.cue.probing;
        if !wires && self.scene.jack_near(w.0, w.1, scene::RING).is_some() {
            return;
        }
        let now = Instant::now();
        let double = now - self.last_press.0 < DOUBLE && (self.cursor.0 - self.last_press.1.0).abs() + (self.cursor.1 - self.last_press.1.1).abs() < 8.0;
        self.last_press = (now, self.cursor);
        if double && wires {
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
        // a device's HIDE key
        if let Some(di) = self.scene.hide_key_at(w.0, w.1) {
            return self.hide_device(di);
        }
        // a frame's GRAPH key: its block in ENGINE, chosen and in view (a frame is a block)
        if let Some(node) = self.scene.graph_key_at(w.0, w.1) {
            self.set_engine(true);
            self.engine.show_node(node);
            return self.redraw();
        }
        // a card's PROFILE and PORT keys open their lists under them
        if let Some((di, what)) = self
            .scene
            .port_key_at(w.0, w.1)
            .map(|di| (di, ListOf::Ports))
            .or(self.scene.profile_key_at(w.0, w.1).map(|di| (di, ListOf::Profiles)))
        {
            self.profiles = Some(ProfileList { key: self.scene.devs[di].dev.key.clone(), what, sig: 0, layer: None, hits: Vec::new(), at: (0.0, 0.0), anchor: None });
            self.redraw();
            return;
        }
        let plug = |scene: &mut Scene| -> Option<u64> {
            if let Some((ci, b)) = scene.plug_at(w.0, w.1) {
                return Some(scene.unplug(ci, b));
            }
            scene.free_jack_at(w.0, w.1).and_then(|k| scene.pull_new(&k))
        };
        // (only when patching: taking a plug pulls it out at once)
        if let Some(seq) = if wires { plug(&mut self.scene) } else { None } {
            let targets = self.scene.targets(seq);
            let c = self.scene.cords.iter().find(|c| c.seq == seq).expect("cord");
            let e = if c.b.held { &c.b } else { &c.a };
            let p = (e.x, e.y);
            let magnet = Self::magnet(&self.scene, &targets, w);
            self.drag = Some(Drag::Plug { seq, p, v: (0.0, 0.0), magnet, targets });
        } else if let Some((di, g)) = self.scene.switch_at(w.0, w.1) {
            if wires {
                self.split(di, g);
            }
        } else if let Some((ci, k)) = self.scene.cord_at(w.0, w.1).filter(|_| wires && self.scene.cables < 2) {
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
            Some(Drag::Engine) => {
                self.engine.release();
                self.send_engine();
            }
            Some(Drag::Mixer) => {
                let dpi = self.dpi();
                let a = self.mixer.release(&mut self.scene, self.cursor, dpi);
                self.mixer_act(a);
                self.redraw();
            }
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
                // (clicked: the key saves; dragged: saved here)
                if moved {
                    self.save_settings();
                } else {
                    self.panel_key(Key::Cables);
                }
            }
            // a press without a turn: it comes back up, out if it was in, in if it was out
            Some(Drag::Bal { key, gi, was, turned, .. }) => {
                if let Some(d) = self.scene.devs.iter_mut().find(|d| d.dev.key == key) {
                    d.bal.insert(gi, if turned || was == BalKnob::In { BalKnob::Out } else { BalKnob::In });
                }
            }
            // the cue's level, remembered
            Some(Drag::CueLevel { .. }) => self.save_settings(),
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
        // the headphones beside the pointer follow it
        if self.cue.probing || self.cue.mode {
            self.redraw();
        }
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
            Some(Drag::Engine) => {
                if self.engine.drag(self.cursor, self.dpi()) {
                    self.redraw();
                }
            }
            Some(Drag::Mixer) => {
                let (size, dpi) = (self.size, self.dpi());
                if self.mixer.drag(&mut self.scene, self.cursor, size, dpi) {
                    self.after_patch();
                    self.redraw();
                }
            }
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
                if let Some(di) = self.scene.dev_index(&key) {
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
                let Some(di) = self.scene.dev_index(&key) else { return };
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
                let in_more = self.panel.in_more(self.cursor.0 / dpi, self.cursor.1 / dpi);
                // MIXER: what is under the pointer, in the status bar
                if self.mixer_shown() && self.cursor.1 >= top && !in_more {
                    let hint = if let Some(h) = self.mixer.hover(&self.scene, self.cursor, dpi) {
                        h
                    } else if self.cursor.1 <= bottom {
                        "drag: scroll · wheel over a fader: volume · D: desk · R: rack · E: ENGINE".to_owned()
                    } else {
                        String::new()
                    };
                    if hint != self.hint {
                        self.hint = hint;
                        self.redraw();
                    }
                    let over = self.mixer.over_key(self.cursor, dpi);
                    if let Some(sf) = &self.win {
                        sf.window.set_cursor(if over { CursorIcon::Pointer } else { CursorIcon::Default });
                    }
                    return;
                }
                // ENGINE: no hovering over the desk behind it
                if self.engine.on && self.cursor.1 >= top && !in_more {
                    // CUE mode: a click on a port keeps it on (nothing is heard by hovering: each
                    // port heard and let go would wake the output and move the graph); the port a
                    // click would take is framed
                    let probe = if self.cue.mode { self.engine_probe() } else { None };
                    let framed = probe.as_deref().and_then(|k| scene::port_of_key(k).map(|id| (id, self.scene.cue_blocked(k))));
                    if framed != self.engine.port_hover {
                        self.engine.port_hover = framed;
                        self.redraw();
                    }
                    if self.cue.mode {
                        let hint = if let Some(k) = probe {
                            Some(self.port_hint(&k))
                        } else if self.on_field() && self.engine.port_at(self.cursor, self.size, dpi).is_some() {
                            Some("nothing to hear on this port: MIDI, video or Patchcord's own".to_owned())
                        } else {
                            None
                        };
                        if let Some(hint) = hint {
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
                    let hint = if let Some(h) = self.engine.hover(self.cursor, dpi) {
                        h
                    } else if self.cursor.1 <= bottom
                        && let Some(what) = self.cue_what()
                    {
                        // ENGINE shows the cue; it is played with on the desk
                        format!("{what}  ·  the CUE key stops it")
                    } else if self.cursor.1 <= bottom { "click: inspect · drag: pan · wheel: zoom (scroll over the lists) · F: fit · ?: legend · D: desk · M: mixer · R: rack".to_owned() } else { String::new() };
                    if hint != self.hint {
                        self.hint = hint;
                        self.redraw();
                    }
                    if let Some(sf) = &self.win {
                        sf.window.set_cursor(CursorIcon::Default);
                    }
                    return;
                }
                // listening: the jack under the pointer is heard while it is there (and nothing
                // once the pointer leaves the field)
                // in CUE mode a jack's hole patches (what is heard stays as it is); with Alt held
                // the whole jack listens
                let hole = self.cue.mode && !self.cue.probing && self.on_field() && self.hole_at(w).is_some();
                if hole != self.cue.in_hole {
                    self.cue.in_hole = hole;
                    self.redraw();
                }
                if (self.cue.probing || self.cue.mode) && !hole {
                    let k = self.probe_at();
                    if k != self.cue.hover {
                        self.cue.hover = k;
                        self.redraw();
                    }
                    if self.on_field() && (self.cue.hover.is_some() || self.cue.probing) {
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
                } else if self.cursor.1 < top || in_more {
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
                // in a jack's hole in CUE mode: patching, as outside CUE mode, with a crosshair
                let (icon, hint) = if hole {
                    let what = if self.scene.plug_at(w.0, w.1).is_some() { "drag: pull this cord out and plug it elsewhere" } else { "drag: a new cord from this jack" };
                    (CursorIcon::Crosshair, format!("CUE ▸ {what}  ·  the ring around it: listen"))
                } else {
                    (icon, hint)
                };
                let on_field = self.cursor.1 >= top && self.cursor.1 <= bottom && self.panel.menu.is_none() && in_list.is_none() && !in_more;
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

    /// SPLIT or JOIN on device `di`'s frame `g`. The jacks get new names: what of them was kept on
    /// stays on, on the jacks its ports are on now (one channel of a stereo jack, as that port).
    fn split(&mut self, di: usize, g: usize) {
        let kept: Vec<Vec<u32>> = self
            .cue
            .pins
            .iter()
            .filter_map(|k| self.scene.jack(k).filter(|(d, j)| d.dev.key == self.scene.devs[di].dev.key && j.group == g).map(|(_, j)| j.ports.clone()))
            .collect();
        let gone: Vec<String> = self.cue.pins.iter().filter(|k| self.scene.jack(k).is_some_and(|(d, j)| d.dev.key == self.scene.devs[di].dev.key && j.group == g)).cloned().collect();
        self.scene.toggle_split(di, g);
        self.cue.pins.retain(|k| !gone.contains(k));
        for ports in kept {
            for p in &ports {
                let Some(j) = self.scene.cue_jack(&scene::port_key(*p)).map(str::to_owned) else { continue };
                let all = self.scene.jack(&j).is_some_and(|(_, d)| d.ports.iter().all(|q| ports.contains(q)));
                let k = if all { j } else { scene::port_key(*p) };
                if !self.cue.pins.contains(&k) {
                    self.cue.pins.push(k);
                }
            }
        }
    }

    /// A jack (or a port) kept on in the cue, or not any more; the output listened on can't be.
    ///
    /// A jack and its ports are the same sound, kept on from the desk or from ENGINE: a click on
    /// a jack stops all of it that is kept on (its ports kept on from ENGINE too); a click in
    /// ENGINE on a port of a jack kept on from the desk stops that port, its other one stays on.
    fn toggle_pin(&mut self, k: String) {
        if self.scene.cue_blocked(&k) {
            return self.say("That is the output you listen on: it would feed back");
        }
        if let Some(i) = self.cue.pins.iter().position(|p| *p == k) {
            self.cue.pins.remove(i);
            return;
        }
        let jack = self.scene.cue_jack(&k).map(str::to_owned);
        match (scene::port_of_key(&k), jack) {
            // a jack: what of it is kept on from ENGINE stops, else it is kept on
            (None, _) => {
                let before = self.cue.pins.len();
                self.cue.pins.retain(|p| scene::port_of_key(p).is_none() || self.scene.cue_jack(p) != Some(k.as_str()));
                if self.cue.pins.len() == before {
                    self.cue.pins.push(k);
                }
            }
            // a port of a jack kept on: the jack's other ports stay on, each on its own
            (Some(id), Some(j)) if self.cue.pins.contains(&j) => {
                self.cue.pins.retain(|p| *p != j);
                let others: Vec<String> = self.scene.jack(&j).map(|(_, d)| d.ports.iter().filter(|p| **p != id).map(|p| scene::port_key(*p)).collect()).unwrap_or_default();
                self.cue.pins.extend(others);
            }
            _ => self.cue.pins.push(k),
        }
    }

    /// The jack whose black hole is at world point `w` (listening comes first: no wider).
    fn hole_at(&self, w: (f64, f64)) -> Option<String> {
        self.scene.jack_near(w.0, w.1, scene::HOLE)
    }

    /// The port under the pointer in ENGINE that can be listened to, as a key among the jacks
    /// heard.
    fn engine_probe(&self) -> Option<String> {
        if !self.on_field() {
            return None;
        }
        self.engine.port_at(self.cursor, self.size, self.dpi()).filter(|id| self.scene.listenable(*id)).map(scene::port_key)
    }

    /// Whether the pointer is on the patch field itself: not on the panel, the status line, MORE,
    /// a menu or the open profile list.
    fn on_field(&self) -> bool {
        let (top, bottom) = self.field();
        let dpi = self.dpi();
        self.cursor.1 >= top && self.cursor.1 <= bottom && !self.panel.in_more(self.cursor.0 / dpi, self.cursor.1 / dpi) && self.panel.menu.is_none() && self.profile_hit().is_none()
    }

    /// The jack listened to under the pointer: only on the field (a jack behind the panel or a
    /// menu isn't heard).
    fn probe_at(&self) -> Option<String> {
        if self.engine.on || !self.on_field() {
            return None;
        }
        // (all of the ring round a jack listens)
        let w = self.world(self.cursor);
        self.scene.jack_near(w.0, w.1, scene::RING)
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
        } else if self.scene.switch_at(w.0, w.1).is_some() || self.scene.hide_key_at(w.0, w.1).is_some() || self.scene.graph_key_at(w.0, w.1).is_some() || self.scene.profile_key_at(w.0, w.1).is_some() || self.scene.port_key_at(w.0, w.1).is_some() || self.scene.mix_control_at(w.0, w.1) {
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
        // (the rack doesn't zoom)
        if self.in_rack() {
            return;
        }
        let s = (self.view.s * factor).clamp(0.25 * dpi, 2.5 * dpi);
        self.view = self.clamp_view(View { s, x: self.cursor.0 - w.0 * s, y: self.cursor.1 - w.1 * s });
        self.view_to = None;
        // the faces are drawn sharp again once the zooming stops, not while it goes on
        self.rescale_at = Some(Instant::now() + RESCALE_DELAY);
        self.redraw();
    }

    fn key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        // ENGINE has keys of its own; the desk's don't reach the desk behind it
        if self.engine.on && !self.mods.control_key() && !self.mods.alt_key() {
            match code {
                KeyCode::KeyD => self.panel_key(Key::Desk),
                KeyCode::KeyM => self.panel_key(Key::Mixer),
                KeyCode::KeyR => self.panel_key(Key::Rack),
                KeyCode::KeyF | KeyCode::Digit0 => self.panel_key(Key::Fit),
                // ? (and / without Shift): the legend
                KeyCode::Slash => {
                    self.engine.toggle_legend();
                    self.redraw();
                }
                KeyCode::Escape => {
                    self.panel.menu = None;
                    self.panel.state.more = false;
                    self.engine.deselect();
                    if self.cue.mode {
                        self.set_cue_mode(false);
                    }
                    self.redraw();
                }
                _ => {}
            }
            return;
        }
        // MIXER too: the desk's keys don't reach the desk behind it
        if self.mixer_shown() && !self.mods.control_key() && !self.mods.alt_key() {
            match code {
                KeyCode::KeyD => self.panel_key(Key::Desk),
                KeyCode::KeyR => self.panel_key(Key::Rack),
                KeyCode::KeyE => self.panel_key(Key::Engine),
                KeyCode::Escape => {
                    self.panel.menu = None;
                    self.panel.state.more = false;
                    self.profiles = None;
                    if self.cue.mode {
                        self.set_cue_mode(false);
                    }
                    self.redraw();
                }
                _ => {}
            }
            return;
        }
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
            KeyCode::KeyO if self.mods.control_key() => self.panel_key(Key::Lcd),
            _ if self.mods.control_key() || self.mods.alt_key() => {}
            KeyCode::KeyF | KeyCode::Digit0 => self.panel_key(Key::Fit),
            KeyCode::KeyA => self.panel_key(Key::Arrange),
            // the light running in the cords that carry sound, on and off
            KeyCode::KeyW => self.panel_key(Key::Flow),
            // H: hide the device under the pointer, or (over nothing) the list of hidden ones
            KeyCode::KeyH => {
                let w = self.world(self.cursor);
                match self.scene.device_at(w.0, w.1) {
                    Some(di) => self.hide_device(di),
                    None => self.open_hidden(true),
                }
            }
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
            KeyCode::KeyD => self.panel_key(Key::Desk),
            KeyCode::KeyM => self.panel_key(Key::Mixer),
            KeyCode::KeyR => self.panel_key(Key::Rack),
            KeyCode::KeyE => self.panel_key(Key::Engine),
            KeyCode::Escape => {
                self.panel.menu = None;
                self.panel.state.more = false;
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

impl App {
    /// PipeWire has news: what it changed, into the scene and ENGINE.
    fn news(&mut self) {
        let events: Vec<_> = self.conn.events.try_iter().collect();
        let mut engine = false;
        for e in &events {
            engine |= self.engine.apply(e);
        }
        let changed = self.scene.apply(events);
        if changed {
            self.engine.dirty = true;
        }
        if engine || (changed && self.engine.on) {
            self.redraw();
        }
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

    /// A window event (all but drawing, which `frame` times itself).
    fn window_event_now(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                self.cue.inside = true;
                self.moved();
            }
            WindowEvent::CursorLeft { .. } => {
                self.set_hover(Hover::None);
                // nothing under the pointer any more: not heard, not framed, no badge
                self.engine.port_hover = None;
                self.cue.hover = None;
                (self.cue.in_hole, self.cue.inside) = (false, false);
                self.hint.clear();
                self.redraw();
            }
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
                // what steps takes a click of the wheel, or a touchpad's worth of one: a touchpad
                // sends many small events, each of them a step would run to the end at once
                let clicks = match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        self.wheel_px = 0.0;
                        if y > 0.0 { 1 } else if y < 0.0 { -1 } else { 0 }
                    }
                    MouseScrollDelta::PixelDelta(p) => {
                        const CLICK: f64 = 40.0;
                        self.wheel_px += p.y;
                        let n = (self.wheel_px / CLICK).trunc();
                        self.wheel_px -= n * CLICK;
                        n as i32
                    }
                };
                let up = clicks.signum() as f64;
                let (top, bottom) = self.field();
                let dpi = self.dpi();
                if self.cursor.1 < top || self.panel.in_more(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                    match self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                        // the wheel steps the cords switch, one click at a time
                        Some(Key::CueLevel) if clicks != 0 => {
                            self.cue.level = (self.cue.level + clicks as f64 * 0.05).clamp(0.0, 1.0);
                            self.hint = format!("cue level {:.0}%", self.cue.level * 100.0);
                            self.save_settings();
                            self.redraw();
                        }
                        Some(Key::Cables | Key::CablesAt(_)) if clicks != 0 => {
                            self.scene.cables = (self.scene.cables as i32 + clicks.signum()).clamp(0, 2) as u8;
                            self.save_settings();
                            self.redraw();
                        }
                        _ => {}
                    }
                } else if self.cursor.1 <= bottom && self.engine.on {
                    self.engine.wheel(self.cursor, f, self.size, dpi);
                    self.redraw();
                } else if self.cursor.1 <= bottom && self.mixer_shown() {
                    let d = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (x as f64 * 60.0, y as f64 * 60.0),
                        MouseScrollDelta::PixelDelta(p) => (p.x / dpi, p.y / dpi),
                    };
                    let (size, fine) = (self.size, self.mods.shift_key());
                    if self.mixer.wheel(&mut self.scene, self.cursor, mixer::Wheel { clicks, d, fine }, size, dpi) {
                        self.after_patch();
                        self.redraw();
                    }
                } else if self.cursor.1 <= bottom {
                    // over an app's VOLUME knob, or its BAL knob when out, the wheel turns it
                    let w = self.world(self.cursor);
                    let fine = self.mods.shift_key();
                    // scrolling the rack goes on over a knob that comes under the pointer
                    let scrolling = self.in_rack() && self.scrolled.is_some_and(|t| t.elapsed() < SCROLL_LATCH);
                    if scrolling {
                        self.scroll_rack(delta);
                    } else if let Some((di, gi)) = self.scene.knob_at(w.0, w.1) {
                        if clicks != 0 {
                            let v = self.scene.devs[di].mix(gi).map_or(1.0, |m| m.0);
                            self.scene.set_volume((di, gi), v + up * if fine { 0.01 } else { 0.05 });
                            self.after_patch();
                        }
                    } else if let Some((di, gi)) = self.scene.bal_at(w.0, w.1) {
                        if clicks != 0 && self.scene.devs[di].bal(gi) == BalKnob::Out {
                            let b = self.scene.devs[di].mix(gi).map_or(0.0, |m| m.1);
                            self.scene.set_balance((di, gi), b + up * if fine { 0.02 } else { 0.1 });
                            self.after_patch();
                        }
                    } else if self.in_rack() {
                        self.scroll_rack(delta);
                    } else if f != 1.0 {
                        self.zoom(f);
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                // holding Alt probes: the jack under the pointer is heard while Alt is held (on
                // the desk and the rack: in ENGINE a click chooses, in CUE mode)
                // (not with something in the hand: a plug held over a jack isn't heard)
                let alt = self.mods.alt_key() && !self.engine.on && self.drag.is_none();
                if alt != self.cue.probing {
                    self.cue.probing = alt;
                    if !alt && !self.cue.mode {
                        self.cue.hover = None;
                    }
                    // what is under the pointer, heard or not, and the status bar, again
                    self.hint.clear();
                    self.moved();
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
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        let (w, h) = self.settings.window.unwrap_or((1400, 880));
        let attrs = with_app_id(Window::default_attributes().with_title("Patchcord").with_inner_size(LogicalSize::new(w as f64, h as f64)).with_min_inner_size(LogicalSize::new(self.panel.min_w().ceil(), 320.0)));
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
        let t = Instant::now();
        self.news();
        self.prof.between("pipewire", t.elapsed());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            return self.window_event_now(event_loop, id, event);
        }
        let t = Instant::now();
        self.window_event_now(event_loop, id, event);
        self.prof.between("input", t.elapsed());
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
        } else if let Some(next) = self.next_step(now) {
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

/// Desktop app ID (also the .desktop file and icon name). Wayland has no window icons:
/// the compositor finds the icon through the installed `<APP_ID>.desktop` file.
const APP_ID: &str = "io.github.bewzaalex.patchcord";

fn with_app_id(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    use winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};
    let attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
    WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID)
}

/// The status bar over what would feed back if it were heard.
fn feedback(title: &str) -> String {
    format!("can't listen to {title}: it is the output you listen on (it would feed back)")
}

/// The patch field: dark, a little lighter towards the top middle, with a fine dot grid.
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
    // the dot grid: a dot of radius 1 at (11 + 22i, 11 + 22j) covers the four pixels round that
    // corner, most of each; white at 7/255 over them, put in straight (a path of thousands of
    // circles took 100 ms and more on a big window)
    let a = 7.0 / 255.0 * 0.78;
    let data = pm.data_mut();
    for y in (11..h).step_by(22) {
        for x in (11..w).step_by(22) {
            for (px, py) in [(x - 1, y - 1), (x, y - 1), (x - 1, y), (x, y)] {
                let i = ((py * w + px) * 4) as usize;
                for c in &mut data[i..i + 3] {
                    *c = (*c as f32 + (255.0 - *c as f32) * a).round() as u8;
                }
            }
        }
    }
    pm
}

/// Two sizes of the rack's room alike (to half a unit).
fn same_room(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}
