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
use super::face::LCD_CH;
use super::gpu::{Gpu, Sprite};
use super::panel::{self, Key, Menu, MenuHit, Panel};
use crate::config::{self, Preset, Settings};
use super::scene::{Hover, Layer, Scene, View, next_id};
use crate::pw::Connection;

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
    /// a plug in the hand: a weight on a spring to the pointer, pulled into a jack nearby
    Plug { seq: u64, p: (f32, f32), v: (f32, f32), magnet: Option<String>, targets: Vec<String>, moved: f64, start: (f64, f64) },
    /// the SAG knob, turned by dragging up and down
    Knob { y0: f64, sag0: f32 },
    /// the cords switch: dragged, it steps between its positions; clicked, it moves on one
    Switch { from: (f64, f64), pos0: u8, moved: bool },
    /// a panel key held down
    Key,
}

/// How long a notice stays.
const NOTICE: Duration = Duration::from_millis(1800);
/// How long the pointer rests on something before its tooltip shows.
const TIP_DELAY: Duration = Duration::from_millis(450);

/// A second click this soon, this close, is a double-click.
const DOUBLE: Duration = Duration::from_millis(350);

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
        scene.sag = settings.sag as f32 / 100.0;
        scene.cables = settings.cables;
        scene.saved_pos = settings.pos.clone().into_iter().collect();
        scene.rack = settings.rack;
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
            last: now,
            start: now,
            animating: true,
            next_frame: now,
            last_press: (now, (0.0, 0.0)),
            panel: Panel::new(panel::State { rack: false, locked: settings.lock, cables: settings.cables, sag: settings.sag, sound: false, lcd: [String::new(), String::new()], pressed: None }),
            hint: String::new(),
            status: None,
            notice: None,
            hover_since: now,
            tip: None,
            settings,
            presets,
            current,
        }
    }

    /// Writes settings.txt when something in it changed.
    fn save_settings(&mut self) {
        let mut s = self.settings.clone();
        s.sag = (self.scene.sag * 100.0).round() as u8;
        s.cables = self.scene.cables;
        s.lock = self.panel.state.locked;
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
            Key::Sag => {}
            Key::Save => self.open_menu(true),
            Key::Load | Key::Lcd => self.open_menu(false),
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
            v.x += (t.x - v.x) * 0.15;
            v.y += (t.y - v.y) * 0.15;
            v.s += (t.s - v.s) * 0.15;
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
            // the plug is a weight on a spring to the pointer; near a jack it is pulled in
            let spot = magnet.as_ref().and_then(|m| self.scene.spare(m));
            let to = spot.unwrap_or((cursor.0 as f32, cursor.1 as f32));
            let k = if spot.is_some() { 1200.0 } else { 650.0 };
            let damp = 2.0 * f32::sqrt(k);
            // small fixed steps: one big step on a slow frame would fling the plug away
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
            // near its jack the plug turns to face the panel
            let lift = spot.map_or(1.0, |s| (((s.0 - p.0).powi(2) + (s.1 - p.1).powi(2)).sqrt() - 4.0).clamp(0.0, 70.0) / 70.0);
            let kind = magnet.as_ref().and_then(|m| self.scene.jack_kind(m));
            held = Some((*seq, Held { x: p.0, y: p.1, vx: v.0, lift, kind }));
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
        self.panel.state.sag = (self.scene.sag * 100.0).round() as u8;
        self.panel.state.cables = self.scene.cables;
        self.panel.state.rack = self.scene.rack;
        self.panel.state.lcd = self.lcd();
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

        let pulse = 0.5 + 0.5 * ((now - self.start).as_secs_f32() * 6.0).sin();
        let cords = self.scene.cord_layer(self.view, self.size, &rings, pulse);
        let cord_id = next_id();

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
            for l in [d.shade.as_ref().map(|(_, l)| l), d.face.as_ref(), d.lcd.as_ref().map(|(_, l)| l), d.meter.as_ref().map(|(_, l)| l)].into_iter().flatten() {
                sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (d.x + l.x) * v.s + v.x, y: (d.y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha: 1.0 });
            }
            // its jacks, over the face
            for (pic, x, y, alpha) in self.scene.jack_sprites(di, targets) {
                if let Some(l) = &self.scene.jack_pics[pic] {
                    sprites.push(Sprite { id: l.id, pm: l.pm.as_ref(), x: (x + l.x) * v.s + v.x, y: (y + l.y) * v.s + v.y, w: l.w * v.s, h: l.h * v.s, alpha });
                }
            }
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
            for l in [d.shade.as_mut().map(|(_, l)| l), d.face.as_mut(), d.lcd.as_mut().map(|(_, l)| l), d.meter.as_mut().map(|(_, l)| l)].into_iter().flatten() {
                if sf.gpu.has(l.id) {
                    l.pm = None;
                }
            }
        }
        self.animating = moving || self.view_to.is_some() || self.drag.is_some() || self.rescale_at.is_some() || self.notice.is_some() || faces_waiting || (self.scene.hover != Hover::None && self.tip.is_none() && now - self.hover_since < 2 * TIP_DELAY);
    }

    fn press(&mut self, button: MouseButton) {
        self.set_hover(Hover::None);
        let w = self.world(self.cursor);
        let (top, bottom) = self.field();
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
                Some(MenuHit::Inside) => {}
                None => {
                    // the keys that open it toggle it themselves
                    let key = self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi);
                    if !matches!(key, Some(Key::Save | Key::Load | Key::Lcd)) {
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
                    Key::Sag => Drag::Knob { y0: self.cursor.1, sag0: self.scene.sag },
                    Key::Cables => Drag::Switch { from: self.cursor, pos0: self.scene.cables, moved: false },
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
            self.drag = Some(Drag::Plug { seq, p, v: (0.0, 0.0), magnet, targets, moved: 0.0, start: self.cursor });
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
            Some(Drag::Plug { seq, magnet, moved, .. }) => {
                self.scene.drop_held(seq, magnet, moved > 6.0);
                self.after_patch();
            }
            Some(Drag::Device { .. }) => {
                self.scene.drop_device();
                self.save_settings();
            }
            Some(Drag::Knob { .. }) => self.save_settings(),
            Some(Drag::Switch { moved, .. }) => {
                if !moved {
                    self.panel_key(Key::Cables);
                }
                self.save_settings();
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
                self.scene.move_device(&key, x, y);
                self.redraw();
            }
            Some(Drag::Cord { .. }) => self.redraw(),
            Some(Drag::Knob { y0, sag0 }) => {
                let dpi = self.dpi();
                self.scene.sag = (sag0 + ((y0 - self.cursor.1) / (150.0 * dpi)) as f32).clamp(0.0, 1.0);
                self.redraw();
            }
            Some(Drag::Key) => {}
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
                let cursor = self.cursor;
                if let Some(Drag::Plug { magnet, targets, moved, start, .. }) = &mut self.drag {
                    *moved = moved.max((cursor.0 - start.0).abs() + (cursor.1 - start.1).abs());
                    *magnet = Self::magnet(&self.scene, targets, w);
                }
                self.redraw();
            }
            None => {
                let (top, bottom) = self.field();
                let dpi = self.dpi();
                let (icon, hint) = if self.cursor.1 < top {
                    match self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                        Some(Key::Sag) => (CursorIcon::NsResize, Key::Sag.hint().to_owned()),
                        Some(Key::Cables) => (CursorIcon::EwResize, Key::Cables.hint().to_owned()),
                        Some(k) => (CursorIcon::Pointer, k.hint().to_owned()),
                        None => (CursorIcon::Default, String::new()),
                    }
                } else if self.cursor.1 > bottom {
                    (CursorIcon::Default, String::new())
                } else {
                    (self.field_cursor(w), self.scene.hint_at(w.0, w.1, self.panel.state.locked))
                };
                let on_field = self.cursor.1 >= top && self.cursor.1 <= bottom && self.panel.menu.is_none();
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
        {
            {
                let icon = if self.scene.plug_at(w.0, w.1).is_some() || (self.scene.cables < 2 && self.scene.cord_at(w.0, w.1).is_some()) {
                    CursorIcon::Grab
                } else if self.scene.free_jack_at(w.0, w.1).is_some() {
                    CursorIcon::Crosshair
                } else if self.scene.switch_at(w.0, w.1).is_some() {
                    CursorIcon::Pointer
                } else if self.scene.device_at(w.0, w.1).is_some() && !self.panel.state.locked {
                    CursorIcon::Move
                } else {
                    CursorIcon::Default
                };
                icon
            }
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
            KeyCode::KeyC => self.panel_key(Key::Cables),
            KeyCode::KeyR => self.panel_key(if self.scene.rack { Key::Desk } else { Key::Rack }),
            KeyCode::Escape => {
                self.panel.menu = None;
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
                let f = match delta {
                    MouseScrollDelta::LineDelta(_, y) => 1.12f64.powf(y as f64),
                    MouseScrollDelta::PixelDelta(p) => (p.y * 0.0015).exp(),
                };
                let (top, bottom) = self.field();
                let dpi = self.dpi();
                if self.cursor.1 < top {
                    // the wheel turns the SAG knob
                    match self.panel.hit(self.cursor.0 / dpi, self.cursor.1 / dpi) {
                        Some(Key::Sag) => {
                            self.scene.sag = (self.scene.sag + (f.ln() * 0.6) as f32).clamp(0.0, 1.0);
                            self.redraw();
                        }
                        // the wheel steps the cords switch, one click at a time
                        Some(Key::Cables | Key::CablesAt(_)) if f != 1.0 => {
                            let step = if f > 1.0 { 1 } else { -1 };
                            self.scene.cables = (self.scene.cables as i32 + step).clamp(0, 2) as u8;
                            self.save_settings();
                            self.redraw();
                        }
                        _ => {}
                    }
                } else if self.cursor.1 <= bottom {
                    self.zoom(f);
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
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
