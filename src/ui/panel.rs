//! The top panel (a strip of painted metal with hardware keys, the CABLES switch and the LCD), the
//! status bar at the bottom, and the short notices. Sizes are in logical pixels.

use std::fmt::Write;

use super::face::paint_uri;
use super::print::{SECTION, SMALL};
use super::scene::{Layer, next_id, rasterize};
use super::text;
use crate::graph::{Card, PortChoice};

/// height of the top panel; its shadow falls a little below
pub const H: f64 = 64.0;
const SHADOW: f64 = 10.0;
pub const STATUS_H: f64 = 22.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Key {
    Undo,
    Redo,
    Save,
    Load,
    Desk,
    Rack,
    Lock,
    Align,
    /// the cords switch: the knob, or one of its printed positions
    Cables,
    CablesAt(u8),
    Arrange,
    Fit,
    Sound,
    Lcd,
    /// the screen of the default output and input
    Defaults,
}

impl Key {
    /// What it does, for the status bar.
    pub fn hint(self) -> &'static str {
        match self {
            Key::Undo => "Undo (Ctrl+Z)",
            Key::Redo => "Redo (Ctrl+Shift+Z)",
            Key::Save => "Save as a new preset (Ctrl+S)",
            Key::Load => "Load a preset (Ctrl+O)",
            Key::Desk => "Devices anywhere on a desk",
            Key::Rack => "Devices in a 19\" rack (R)",
            Key::Lock => "Lock devices in place (L)",
            Key::Align => "Line devices up while dragging them on the desk; off: place them freely (G)",
            Key::Cables => "Cords: show · fade · hide: drag, turn the wheel or click (C)",
            Key::CablesAt(0) => "Show the cords",
            Key::CablesAt(1) => "Fade the cords",
            Key::CablesAt(_) => "Hide the cords",
            Key::Arrange => "Arrange devices by signal flow (A)",
            Key::Fit => "Fit everything in the window (F)",
            Key::Sound => "Plug sounds",
            Key::Lcd => "Presets",
            Key::Defaults => "The default output and input, where new apps play and record: click to choose others or find them",
        }
    }
}

/// What the panel shows.
#[derive(Clone, PartialEq, Hash)]
pub struct State {
    pub rack: bool,
    pub locked: bool,
    /// devices dragged on the desk stick to each other's edges and centres
    pub align: bool,
    /// 0 show, 1 fade, 2 hide
    pub cables: u8,
    /// 0…100
    pub sound: bool,
    pub lcd: [String; 2],
    /// the defaults screen's lines: the default output's name and the default input's
    pub defaults: [String; 2],
    pub pressed: Option<Key>,
}

pub struct Panel {
    pub state: State,
    /// hit areas, in logical pixels
    rects: Vec<(Key, [f64; 4])>,
    layer: Option<(u64, Layer)>,
    /// the drop-down under the LCD, while open
    pub menu: Option<Menu>,
    menu_rects: Vec<(MenuHit, [f64; 4])>,
    menu_layer: Option<(u64, Layer)>,
}

/// The drop-down under the LCD: a name field to save a new preset, or the list of presets.
#[derive(Clone, PartialEq, Hash)]
pub enum Menu {
    Save { text: String, placeholder: String },
    /// `confirm`: the row whose delete button was pressed once and now asks "DEL?"
    Load { names: Vec<String>, current: Option<usize>, confirm: Option<usize> },
    /// the outputs and inputs that can be the default, by name, and which ones are
    Defaults { outs: Vec<String>, ins: Vec<String>, out: Option<usize>, inp: Option<usize> },
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuHit {
    Row(usize),
    Delete(usize),
    Save,
    /// an output (true) or input to make the default, or to find on the desk
    Default(bool, usize),
    Find(bool, usize),
    /// somewhere else on the drop-down
    Inside,
}

const MENU_W: f64 = 300.0;
/// the defaults' drop-down is wider: device names are long
const DEFAULTS_W: f64 = 360.0;
/// the defaults' screen on the panel
const DEFAULTS_LCD_W: f64 = 250.0;
/// width of a card's profile list
pub const PROFILE_W: f64 = 360.0;

/// What a click in a card's profile list hits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProfileHit {
    /// a family's row (by its place in `Card::families`)
    Row(usize),
    /// a codec of the profile in use: that profile's index
    Codec(u32),
    /// in a card's port list: a port (group, route index) and a latency step (group, ms)
    Port(usize, u32),
    Latency(usize, i64),
    Inside,
}

/// A card's profile list, amber like the LCDs: a row per family (the one in use dark), its codecs
/// as choices under it, profiles with nothing plugged in dimmed and marked. The layer is at (0, 0),
/// `PROFILE_W` wide; the hit areas are relative to it.
pub fn profile_menu(title: &str, card: &Card, dpi: f64) -> (Layer, Vec<(ProfileHit, [f64; 4])>) {
    const INK: &str = "#3a2408";
    let w = PROFILE_W;
    // the title, its name shortened so " · PROFILE" stays in
    let name = title.to_uppercase();
    let name = if name.chars().count() > 34 { format!("{}…", name.chars().take(33).collect::<String>().trim_end()) } else { name };
    let mut o = format!(r#"<text x="12" y="20" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".7">{} · PROFILE</text>"#, esc(&name));
    let mut hits = Vec::new();
    let mut y = 30.0;
    let current = card.profile().map(|p| Card::split(&p.description).0);
    for (fi, f) in card.families().iter().enumerate() {
        let cur = current.as_deref() == Some(f.name.as_str());
        let ink = if cur { "#f7bb62" } else { INK };
        if cur {
            let _ = write!(o, r#"<rect x="3" y="{y}" width="{}" height="24" fill="{INK}"/>"#, w - 6.0);
        }
        let room = if f.unplugged { 37 } else { 47 };
        let name: String = if f.name.chars().count() > room { format!("{}…", f.name.chars().take(room - 1).collect::<String>()) } else { f.name.clone() };
        let _ = write!(o, r#"<text x="12" y="{}" font-size="11.5" fill="{ink}" opacity="{}" xml:space="preserve">{}</text>"#, y + 16.0, if f.unplugged && !cur { 0.55 } else { 1.0 }, esc(&name));
        if f.unplugged {
            let tw = 70.0;
            let _ = write!(
                o,
                r#"<rect x="{}" y="{}" width="{tw}" height="14" rx="2" fill="none" stroke="{ink}" stroke-opacity=".7"/><text x="{}" y="{}" font-size="8.5" letter-spacing=".8" fill="{ink}" opacity=".8" text-anchor="middle">UNPLUGGED</text>"#,
                w - 12.0 - tw,
                y + 5.0,
                w - 12.0 - tw / 2.0,
                y + 15.0
            );
        }
        hits.push((ProfileHit::Row(fi), [0.0, y, w, 24.0]));
        y += 24.0;
        // the codecs of the family in use, as choices under it
        if cur && f.members.iter().any(|(_, c)| c.is_some()) {
            let mut x = 12.0;
            let now = card.current;
            for (index, codec) in &f.members {
                let Some(c) = codec else { continue };
                let cw = c.chars().count() as f64 * 6.6 + 14.0;
                let on = now == Some(*index);
                let _ = write!(
                    o,
                    r#"<rect x="{x}" y="{}" width="{cw}" height="17" rx="3" fill="{}" stroke="{INK}" stroke-width="1.5"/><text x="{}" y="{}" font-size="11" text-anchor="middle" fill="{}">{}</text>"#,
                    y + 4.0,
                    if on { INK } else { "none" },
                    x + cw / 2.0,
                    y + 16.5,
                    if on { "#f7bb62" } else { INK },
                    esc(c)
                );
                hits.push((ProfileHit::Codec(*index), [x, y + 4.0, cw, 17.0]));
                x += cw + 6.0;
            }
            y += 26.0;
        }
    }
    let h = y + 8.0;
    hits.push((ProfileHit::Inside, [0.0, 0.0, w, h]));
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#121214"/><rect x="3" y="3" width="{}" height="{}" rx="3" fill="url(#g)"/>{o}</svg>"##,
        w - 6.0,
        h - 6.0
    );
    (Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h }, hits)
}

/// A card's port list, under its PORT key, as amber as the profile list: for each of its outputs
/// and inputs with several ports, its name (when it has several), the ports (the one in use dark,
/// the unplugged dimmed and marked), and its latency offset with − and + (10 ms steps with Shift).
pub fn port_menu(title: &str, sections: &[(usize, String, Vec<PortChoice>, Option<u32>, i64)], dpi: f64) -> (Layer, Vec<(ProfileHit, [f64; 4])>) {
    const INK: &str = "#3a2408";
    let w = PROFILE_W;
    let name = title.to_uppercase();
    let name = if name.chars().count() > 37 { format!("{}…", name.chars().take(36).collect::<String>().trim_end()) } else { name };
    let mut o = format!(r#"<text x="12" y="20" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".7">{} · PORT</text>"#, esc(&name));
    let mut hits = Vec::new();
    let mut y = 30.0;
    for (gi, label, ports, current, latency) in sections {
        if sections.len() > 1 {
            let _ = write!(o, r#"<text x="12" y="{}" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".7">{}</text>"#, y + 12.0, esc(label));
            y += 18.0;
        }
        for p in ports {
            let cur = *current == Some(p.index);
            let ink = if cur { "#f7bb62" } else { INK };
            if cur {
                let _ = write!(o, r#"<rect x="3" y="{y}" width="{}" height="24" fill="{INK}"/>"#, w - 6.0);
            }
            let room = if p.unplugged { 37 } else { 47 };
            let shown: String = if p.description.chars().count() > room { format!("{}…", p.description.chars().take(room - 1).collect::<String>()) } else { p.description.clone() };
            let _ = write!(o, r#"<text x="12" y="{}" font-size="11.5" fill="{ink}" opacity="{}" xml:space="preserve">{}</text>"#, y + 16.0, if p.unplugged && !cur { 0.55 } else { 1.0 }, esc(&shown));
            if p.unplugged {
                let tw = 70.0;
                let _ = write!(
                    o,
                    r#"<rect x="{}" y="{}" width="{tw}" height="14" rx="2" fill="none" stroke="{ink}" stroke-opacity=".7"/><text x="{}" y="{}" font-size="8.5" letter-spacing=".8" fill="{ink}" opacity=".8" text-anchor="middle">UNPLUGGED</text>"#,
                    w - 12.0 - tw,
                    y + 5.0,
                    w - 12.0 - tw / 2.0,
                    y + 15.0
                );
            }
            hits.push((ProfileHit::Port(*gi, p.index), [0.0, y, w, 24.0]));
            y += 24.0;
        }
        // its latency offset: − value +
        y += 4.0;
        let _ = write!(o, r#"<path d="M12,{y} H{}" stroke="{INK}" stroke-opacity=".35"/>"#, w - 12.0);
        let ly = y + 6.0;
        let _ = write!(o, r#"<text x="12" y="{}" font-size="10" letter-spacing=".8" fill="{INK}" opacity=".75">LATENCY OFFSET</text>"#, ly + 13.0);
        let ms = *latency as f64 / 1e6;
        let value = if ms.fract().abs() < 1e-9 { format!("{ms:.0} ms") } else { format!("{ms:.1} ms") };
        let (bm, bp) = (w - 12.0 - 22.0 - 70.0 - 22.0, w - 12.0 - 22.0);
        for (bx, sign) in [(bm, "−"), (bp, "+")] {
            let _ = write!(o, r##"<rect x="{bx}" y="{ly}" width="22" height="18" rx="3" fill="{INK}"/><text x="{}" y="{}" font-size="12" fill="#f7bb62" text-anchor="middle">{sign}</text>"##, bx + 11.0, ly + 13.5);
        }
        let _ = write!(o, r#"<text x="{}" y="{}" font-size="11.5" fill="{INK}" text-anchor="middle">{value}</text>"#, bm + 22.0 + 35.0, ly + 13.5);
        hits.push((ProfileHit::Latency(*gi, -1), [bm, ly, 22.0, 18.0]));
        hits.push((ProfileHit::Latency(*gi, 1), [bp, ly, 22.0, 18.0]));
        y = ly + 26.0;
    }
    let h = y + 4.0;
    hits.push((ProfileHit::Inside, [0.0, 0.0, w, h]));
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#121214"/><rect x="3" y="3" width="{}" height="{}" rx="3" fill="url(#g)"/>{o}</svg>"##,
        w - 6.0,
        h - 6.0
    );
    (Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h }, hits)
}

// icons on a 24-unit grid, drawn with round 2-unit strokes
const ICONS: &[(Key, &str)] = &[
    (Key::Undo, r#"<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>"#),
    (Key::Redo, r#"<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>"#),
    (Key::Save, r#"<path d="M5 3h11l3 3v15H5z"/><path d="M8 3v5h7V3"/><path d="M8 21v-7h8v7"/>"#),
    (Key::Load, r#"<path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h10"/>"#),
    (Key::Lock, r#"<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>"#),
    (Key::Align, r#"<path d="M4 3v18"/><rect x="7.5" y="5" width="12" height="5" rx="1"/><rect x="7.5" y="14" width="7.5" height="5" rx="1"/>"#),
    (Key::Arrange, r#"<rect x="3.5" y="4" width="7" height="5" rx="1"/><rect x="3.5" y="15" width="7" height="5" rx="1"/><rect x="13.5" y="9.5" width="7" height="5" rx="1"/><path d="M10.5 6.5h1.5v11h-1.5M12 12h1.5"/>"#),
    (Key::Fit, r#"<path d="M4 9V4h5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/><path d="M20 15v5h-5"/>"#),
    (Key::Sound, r#"<path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M16.5 8.5a5 5 0 0 1 0 7"/><path d="M19 6a8.5 8.5 0 0 1 0 12"/>"#),
];

const KEY_Y: f64 = 10.0;
/// the knobs' centre line, a little low so the label above the cords switch clears the edge
const KNOB_Y: f64 = 30.0;
const KEY_H: f64 = 30.0;
const LABEL: &str = "#26272a";

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Panel {
    pub fn new(state: State) -> Self {
        Panel { state, rects: Vec::new(), layer: None, menu: None, menu_rects: Vec::new(), menu_layer: None }
    }

    /// Where a key is (x, y, w, h), once the panel has been drawn.
    pub fn rect(&self, key: Key) -> Option<[f64; 4]> {
        self.rects.iter().find(|(k, _)| *k == key).map(|(_, r)| *r)
    }

    /// The key under a point in logical pixels.
    pub fn hit(&self, x: f64, y: f64) -> Option<Key> {
        self.rects.iter().find(|(_, [rx, ry, rw, rh])| x >= *rx && x <= rx + rw && y >= *ry && y <= ry + rh).map(|(k, _)| *k)
    }

    /// The panel as a layer `width` logical pixels wide, drawn at `dpi`.
    pub fn layer(&mut self, width: f64, dpi: f64) -> &Layer {
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(&self.state, width.to_bits(), dpi.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if self.layer.as_ref().is_none_or(|(s, _)| *s != sig) {
            let (svg, rects) = self.svg(width);
            self.rects = rects;
            let pm = rasterize(&text::svg_options(), &svg, dpi);
            self.layer = Some((sig, Layer { id: next_id(), pm, x: 0.0, y: 0.0, w: width, h: H + SHADOW }));
        }
        &self.layer.as_ref().unwrap().1
    }

    /// The layer's pixels went to the GPU.
    pub fn uploaded(&mut self) {
        for (_, l) in [self.layer.as_mut(), self.menu_layer.as_mut()].into_iter().flatten() {
            l.pm = None;
        }
    }

    /// The panel and drop-down layers as last prepared.
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        [self.layer.as_ref(), self.menu_layer.as_ref()].into_iter().flatten().map(|(_, l)| l)
    }

    /// What of the drop-down is under a point in logical pixels; None when it is outside it.
    pub fn menu_hit(&self, x: f64, y: f64) -> Option<MenuHit> {
        self.menu.as_ref()?;
        let inside = |[rx, ry, rw, rh]: [f64; 4]| x >= rx && x <= rx + rw && y >= ry && y <= ry + rh;
        let hit = self.menu_rects.iter().skip(1).find(|(_, r)| inside(*r)).map(|(h, _)| *h);
        hit.or_else(|| self.menu_rects.first().filter(|(_, r)| inside(*r)).map(|_| MenuHit::Inside))
    }

    /// The drop-down as a layer, placed under the LCD of a panel `width` logical pixels wide.
    pub fn menu_layer(&mut self, width: f64, dpi: f64) -> Option<&Layer> {
        let Some(menu) = &self.menu else {
            self.menu_layer = None;
            return None;
        };
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(menu, width.to_bits(), dpi.to_bits()), &mut h);
        let sig = std::hash::Hasher::finish(&h);
        if self.menu_layer.as_ref().is_none_or(|(s, _)| *s != sig) {
            // under its screen: the defaults' under theirs, the presets' against the right edge
            let mw = if matches!(menu, Menu::Defaults { .. }) { DEFAULTS_W } else { MENU_W };
            let x = match (menu, self.rect(Key::Defaults)) {
                (Menu::Defaults { .. }, Some([dx, ..])) => dx.min(width - 6.0 - mw).max(6.0),
                _ => width - 18.0 - mw,
            };
            let y = H + 4.0;
            let (svg, mh, rects) = menu_svg(menu);
            self.menu_rects = std::iter::once((MenuHit::Inside, [x, y, mw, mh])).chain(rects.into_iter().map(|(h, [rx, ry, rw, rh])| (h, [x + rx, y + ry, rw, rh]))).collect();
            let pm = rasterize(&text::svg_options(), &svg, dpi);
            self.menu_layer = Some((sig, Layer { id: next_id(), pm, x, y, w: mw, h: mh }));
        }
        self.menu_layer.as_ref().map(|(_, l)| l)
    }

    fn svg(&self, w: f64) -> (String, Vec<(Key, [f64; 4])>) {
        let st = &self.state;
        let mut rects = Vec::new();
        let mut o = String::new();
        let mut x = 18.0;

        // the maker's name, pressed into the metal
        let logo_w = text::width(text::BLACK, "PATCHCORD", 26.0, 1.0) * 1.06;
        let _ = write!(
            o,
            r##"<g transform="translate({x},34) scale(1.06,1)" font-size="26" font-weight="900" letter-spacing="1"><text y="1" fill="#fff" opacity=".35">PATCHCORD</text><text fill="#2a2b2e">PATCHCORD</text></g>{sub}"##,
            sub = SECTION.text(x, 50.0, LABEL, "start", "PC-1 · PIPEWIRE PATCHBAY")
        );
        x += logo_w + 26.0;

        // a group of keys with its name printed under them
        let group = |o: &mut String, rects: &mut Vec<(Key, [f64; 4])>, x: &mut f64, keys: &[(Key, Option<&str>, Option<bool>)], label: &str| {
            let x0 = *x;
            for (i, (key, txt, led)) in keys.iter().enumerate() {
                if i > 0 {
                    *x += 13.0;
                }
                let kw = match txt {
                    Some(t) => (text::width(text::BOLD, t, 10.5, 0.4) + 20.0).max(62.0),
                    None => 38.0,
                };
                let pressed = st.pressed == Some(*key);
                o.push_str(&key_svg(*x, kw, pressed));
                let dy = 0.0;
                match txt {
                    Some(t) => {
                        let tx = *x + kw / 2.0 + if led.is_some() { 4.0 } else { 0.0 };
                        let _ = write!(o, r##"<text x="{tx}" y="{}" font-size="10.5" letter-spacing=".4" fill="#e6e7e8" text-anchor="middle">{t}</text>"##, KEY_Y + 19.0 + dy);
                    }
                    None => {
                        let icon = ICONS.iter().find(|(k, _)| k == key).map_or("", |(_, s)| s);
                        let _ = write!(
                            o,
                            r##"<g transform="translate({},{}) scale(.7083)" fill="none" stroke="#e6e7e8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{icon}</g>"##,
                            *x + kw / 2.0 - 8.5 + if led.is_some() { 2.0 } else { 0.0 },
                            KEY_Y + KEY_H / 2.0 - 8.5 + dy
                        );
                    }
                }
                if let Some(on) = led {
                    o.push_str(&led_svg(*x + 8.0, KEY_Y + 7.0 + dy, *on, "#ff4a32"));
                }
                rects.push((*key, [*x - 3.0, KEY_Y - 3.0, kw + 6.0, KEY_H + 6.0]));
                *x += kw;
            }
            o.push_str(&SECTION.text((x0 + *x) / 2.0, 54.0, LABEL, "middle", label));
            *x += 22.0;
        };
        group(&mut o, &mut rects, &mut x, &[(Key::Undo, None, None), (Key::Redo, None, None), (Key::Save, None, None), (Key::Load, None, None)], "EDIT");
        group(&mut o, &mut rects, &mut x, &[(Key::Desk, Some("DESK"), Some(!st.rack)), (Key::Rack, Some("RACK"), Some(st.rack)), (Key::Lock, None, Some(st.locked)), (Key::Align, None, Some(st.align))], "LAYOUT");
        // the cords switch: three click positions with their names printed around it
        let (kx, ky) = (x + 44.0, KNOB_Y);
        let mut ticks = String::new();
        for (i, (name, anchor, dx, dy)) in [("SHOW", "end", -19.0, 0.0), ("FADE", "middle", 0.0, -20.5), ("HIDE", "start", 19.0, 0.0)]
            .into_iter()
            .enumerate()
        {
            let a = SWITCH[i].to_radians();
            let _ = write!(ticks, r#"<path d="M{},{} L{},{}" stroke="{LABEL}" stroke-width="1.4"/>"#, kx + 15.0 * a.sin(), ky - 15.0 * a.cos(), kx + 16.8 * a.sin(), ky - 16.8 * a.cos());
            // the labels stay as printed: the pointer shows the position
            ticks.push_str(&SMALL.text(kx + dx * 1.15, ky + dy + 2.5, LABEL, anchor, name));
            let w = SMALL.width(name) + 4.0;
            let lx = match anchor {
                "end" => kx + dx * 1.15 - w,
                "middle" => kx - w / 2.0,
                _ => kx + dx * 1.15 - 2.0,
            };
            rects.push((Key::CablesAt(i as u8), [lx, ky + dy - 6.0, w, 11.0]));
        }
        o.push_str(&ticks);
        o.push_str(&knob_svg(kx, ky, SWITCH[st.cables.min(2) as usize], ""));
        o.push_str(&SECTION.text(kx, 54.0, LABEL, "middle", "CABLES"));
        rects.push((Key::Cables, [kx - 16.0, ky - 16.0, 32.0, 32.0]));
        x += 88.0 + 22.0;

        group(&mut o, &mut rects, &mut x, &[(Key::Arrange, None, None), (Key::Fit, None, None), (Key::Sound, None, Some(st.sound))], "VIEW");

        // the LCD, against the right edge, as wide as there is room for; left of it, when there is
        // room for both, the screen of the default output and input
        let screen = |o: &mut String, lx: f64, lw: f64, lines: [&str; 2], bold2: bool| {
            let chars = ((lw - 24.0) / 6.95) as usize;
            let line = |t: &str| esc(&t.chars().take(chars).collect::<String>());
            let _ = write!(
                o,
                r##"<rect x="{}" y="{}" width="{}" height="50" rx="6" fill="#fff" opacity=".2"/><rect x="{}" y="{}" width="{}" height="48" rx="5" fill="#121214"/>
<rect x="{lx}" y="9" width="{lw}" height="42" rx="4" fill="url(#gPanelLcd)"/><rect x="{lx}" y="9" width="{lw}" height="42" rx="4" fill="none" stroke="#783200" stroke-opacity=".35" stroke-width="3"/>
<g font-family="Noto Sans Mono" font-size="11.5" fill="#3a2408" xml:space="preserve"><text x="{}" y="26">{}</text><text x="{}" y="43" font-weight="{}">{}</text></g>"##,
                lx - 4.0,
                5.0,
                lw + 8.0,
                lx - 3.0,
                6.0,
                lw + 6.0,
                lx + 12.0,
                line(lines[0]),
                lx + 12.0,
                if bold2 { "bold" } else { "normal" },
                line(lines[1])
            );
        };
        let room = w - 18.0 - x;
        if room >= 150.0 + 14.0 + DEFAULTS_LCD_W {
            let lw = (room - 14.0 - DEFAULTS_LCD_W).min(340.0);
            let dx = w - 18.0 - lw - 14.0 - DEFAULTS_LCD_W;
            screen(&mut o, dx, DEFAULTS_LCD_W, [&format!("OUT ▸ {}", st.defaults[0]), &format!("IN  ▸ {}", st.defaults[1])], true);
            rects.push((Key::Defaults, [dx, 9.0, DEFAULTS_LCD_W, 42.0]));
        }
        let lw = (w - 18.0 - x).min(340.0);
        let lw = if rects.iter().any(|(k, _)| *k == Key::Defaults) { (room - 14.0 - DEFAULTS_LCD_W).min(340.0) } else { lw };
        if lw >= 150.0 {
            let lx = w - 18.0 - lw;
            let chars = ((lw - 24.0) / 6.95) as usize;
            let line = |t: &str| esc(&t.chars().take(chars).collect::<String>());
            let _ = write!(
                o,
                r##"<rect x="{}" y="{}" width="{}" height="50" rx="6" fill="#fff" opacity=".2"/><rect x="{}" y="{}" width="{}" height="48" rx="5" fill="#121214"/>
<rect x="{lx}" y="9" width="{lw}" height="42" rx="4" fill="url(#gPanelLcd)"/><rect x="{lx}" y="9" width="{lw}" height="42" rx="4" fill="none" stroke="#783200" stroke-opacity=".35" stroke-width="3"/>
<g font-family="Noto Sans Mono" font-size="11.5" fill="#3a2408" xml:space="preserve"><text x="{}" y="26">{}</text><text x="{}" y="43" font-weight="normal">{}</text></g>"##,
                lx - 4.0,
                5.0,
                lw + 8.0,
                lx - 3.0,
                6.0,
                lw + 6.0,
                lx + 12.0,
                line(&st.lcd[0]),
                lx + 12.0,
                line(&st.lcd[1])
            );
            rects.push((Key::Lcd, [lx, 9.0, lw, 42.0]));
        }

        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{th}" viewBox="0 0 {w} {th}" font-family="Noto Sans" font-weight="bold">
<defs>
<pattern id="pt" width="80" height="80" patternUnits="userSpaceOnUse"><image href="{paint}" width="80" height="80"/></pattern>
<linearGradient id="gStrip" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".10"/><stop offset="1" stop-color="#000" stop-opacity=".18"/></linearGradient>
<linearGradient id="gDrop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity=".55"/><stop offset="1" stop-color="#000" stop-opacity="0"/></linearGradient>
<linearGradient id="hole" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#050506"/><stop offset="1" stop-color="#232427"/></linearGradient>
<linearGradient id="edgeHole" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2a2c2e"/><stop offset="0.5" stop-color="#55585b"/><stop offset="1" stop-color="#a7aaad"/></linearGradient>
<linearGradient id="btnGrad" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#303134"/><stop offset="0.6" stop-color="#1d1e20"/><stop offset="1" stop-color="#131314"/></linearGradient>
<linearGradient id="capEdge" x1="0" y1="0" x2="0.3" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.28"/><stop offset="0.45" stop-color="#fff" stop-opacity="0.04"/><stop offset="1" stop-color="#000" stop-opacity="0.5"/></linearGradient>
<linearGradient id="capTop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.13"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>
<filter id="blur1" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="1"/></filter>
<radialGradient id="gKnob" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient>
<linearGradient id="gPanelLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient>
</defs>
<rect y="{H}" width="{w}" height="{SHADOW}" fill="url(#gDrop)"/>
<rect width="{w}" height="{H}" fill="#8c8e91"/><rect width="{w}" height="{H}" fill="url(#pt)"/><rect width="{w}" height="{H}" fill="url(#gStrip)"/>
<path d="M0,.5 H{w}" stroke="#fff" stroke-opacity=".35"/><path d="M0,{b} H{w}" stroke="#000" stroke-opacity=".5"/>
{o}
</svg>"##,
            th = H + SHADOW,
            paint = paint_uri(),
            b = H - 0.5,
        );
        (svg, rects)
    }
}

/// The drop-down, in the LCD's amber: its SVG, height and hit areas (relative to its corner).
fn menu_svg(menu: &Menu) -> (String, f64, Vec<(MenuHit, [f64; 4])>) {
    const INK: &str = "#3a2408";
    let mut o = String::new();
    let mut rects = Vec::new();
    let w = if matches!(menu, Menu::Defaults { .. }) { DEFAULTS_W } else { MENU_W };
    let h;
    let title = |o: &mut String, t: &str| {
        let _ = write!(o, r#"<text x="12" y="20" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".7">{t}</text>"#);
    };
    match menu {
        Menu::Save { text, placeholder } => {
            h = 70.0;
            title(&mut o, "SAVE AS NEW PRESET");
            let shown = if text.is_empty() { placeholder.clone() } else { text.to_uppercase() };
            let _ = write!(
                o,
                r##"<rect x="12" y="30" width="{}" height="26" rx="3" fill="#fff" fill-opacity=".35" stroke="{INK}" stroke-width="2"/><text x="20" y="48" font-size="12" fill="{INK}" opacity="{}" xml:space="preserve">{}</text>"##,
                w - 92.0,
                if text.is_empty() { 0.5 } else { 1.0 },
                esc(&shown)
            );
            if !text.is_empty() {
                // the caret, after the text
                let cx = 20.0 + text.chars().count() as f64 * 7.22 + 1.0;
                let _ = write!(o, r#"<rect x="{cx}" y="36" width="2" height="15" fill="{INK}"/>"#);
            }
            let _ = write!(o, r##"<rect x="{}" y="30" width="62" height="26" rx="3" fill="{INK}"/><text x="{}" y="48" font-size="12" fill="#f7bb62" text-anchor="middle">SAVE</text>"##, w - 74.0, w - 43.0);
            rects.push((MenuHit::Save, [w - 74.0, 30.0, 62.0, 26.0]));
        }
        Menu::Defaults { outs, ins, out, inp } => {
            let mut y = 8.0;
            for (output, names, cur, head) in [(true, outs, out, "DEFAULT OUTPUT · NEW APPS PLAY HERE"), (false, ins, inp, "DEFAULT INPUT · NEW APPS RECORD HERE")] {
                let _ = write!(o, r#"<text x="12" y="{}" font-size="10" letter-spacing="1.5" fill="{INK}" opacity=".7">{head}</text>"#, y + 12.0);
                y += 22.0;
                if names.is_empty() {
                    let _ = write!(o, r#"<text x="12" y="{}" font-size="12" font-weight="normal" fill="{INK}" opacity=".7">none</text>"#, y + 16.0);
                    y += 24.0;
                }
                for (i, name) in names.iter().enumerate() {
                    let on = *cur == Some(i);
                    let ink = if on { "#f7bb62" } else { INK };
                    if on {
                        let _ = write!(o, r#"<rect x="3" y="{y}" width="{}" height="24" fill="{INK}"/>"#, w - 6.0);
                    }
                    let shown: String = if name.chars().count() > 42 { format!("{}…", name.chars().take(41).collect::<String>()) } else { name.clone() };
                    let _ = write!(o, r#"<text x="12" y="{}" font-size="12" fill="{ink}" xml:space="preserve">{}</text>"#, y + 16.5, esc(&shown));
                    // find it on the desk: a crosshair
                    let (cx, cy) = (w - 20.0, y + 12.0);
                    let _ = write!(
                        o,
                        r#"<g stroke="{ink}" stroke-width="1.6" fill="none" opacity=".8"><circle cx="{cx}" cy="{cy}" r="5"/><path d="M{},{cy} h4 M{},{cy} h4 M{cx},{} v4 M{cx},{} v4"/></g>"#,
                        cx - 9.0,
                        cx + 5.0,
                        cy - 9.0,
                        cy + 5.0
                    );
                    rects.push((MenuHit::Find(output, i), [w - 40.0, y, 40.0, 24.0]));
                    rects.push((MenuHit::Default(output, i), [0.0, y, w - 40.0, 24.0]));
                    y += 24.0;
                }
                y += 6.0;
            }
            h = y + 2.0;
        }
        Menu::Load { names, current, confirm } => {
            title(&mut o, "PRESETS");
            let mut y = 30.0;
            if names.is_empty() {
                let _ = write!(o, r#"<text x="12" y="{}" font-size="12" font-weight="normal" fill="{INK}" opacity=".7">No presets yet. Save one first.</text>"#, y + 16.0);
                y += 24.0;
            }
            for (i, name) in names.iter().enumerate() {
                let cur = *current == Some(i);
                let ink = if cur { "#f7bb62" } else { INK };
                if cur {
                    let _ = write!(o, r#"<rect x="3" y="{y}" width="{}" height="24" fill="{INK}"/>"#, w - 6.0);
                }
                // long names end in "…" before the delete button
                let shown: String = if name.chars().count() > 26 { format!("{}…", name.chars().take(25).collect::<String>()) } else { name.clone() };
                let _ = write!(o, r#"<text x="12" y="{}" font-size="12" fill="{ink}" xml:space="preserve">{:02}  {}</text>"#, y + 16.5, i + 1, esc(&shown));
                if *confirm == Some(i) {
                    let _ = write!(o, r#"<text x="{}" y="{}" font-size="12" fill="{ink}" text-anchor="end">DEL?</text>"#, w - 12.0, y + 16.5);
                } else {
                    // a cross: the mono font has no ✕
                    let (cx, cy) = (w - 18.0, y + 12.0);
                    let _ = write!(o, r#"<path d="M{},{} l8,8 M{},{} l-8,8" stroke="{ink}" stroke-width="1.8" stroke-linecap="round" opacity=".75"/>"#, cx - 4.0, cy - 4.0, cx + 4.0, cy - 4.0);
                }
                rects.push((MenuHit::Delete(i), [w - 50.0, y, 50.0, 24.0]));
                rects.push((MenuHit::Row(i), [0.0, y, w - 50.0, 24.0]));
                y += 24.0;
            }
            h = y + 8.0;
        }
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans Mono" font-weight="bold"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs><rect width="{w}" height="{h}" rx="5" fill="#121214"/><rect x="3" y="3" width="{}" height="{}" rx="3" fill="url(#g)"/>{o}</svg>"##,
        w - 6.0,
        h - 6.0
    );
    (svg, h, rects)
}

/// A dark key as on Kadence: a cap in a hole in the panel. The hole's rim catches the light at
/// the bottom right; the cap casts a soft shadow into it and has a fine lit edge and a sheen.
/// Pressed, the cap sinks a little into the hole and darkens.
fn key_svg(x: f64, w: f64, pressed: bool) -> String {
    let (y, h) = (KEY_Y, KEY_H);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut o = format!(r#"<rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="url(#hole)" stroke="url(#edgeHole)" stroke-width="1.2"/>"#, x - 3.0, y - 3.0, w + 6.0, h + 6.0);
    let (tr, shadow, sheen) = if pressed {
        let s = 0.955;
        (format!(r#" transform="translate({},{}) scale({s})""#, cx * (1.0 - s), cy * (1.0 - s)), 0.15 * 0.75, 0.35)
    } else {
        (String::new(), 0.75, 1.0)
    };
    let _ = write!(
        o,
        r##"<g{tr}><rect x="{}" y="{}" width="{w}" height="{h}" rx="4" fill="#000" opacity="{shadow}" filter="url(#blur1)"/><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="url(#btnGrad)" stroke="#070708" stroke-width="1"/><g opacity="{sheen}"><rect x="{}" y="{}" width="{}" height="{}" rx="3.4" fill="none" stroke="url(#capEdge)" stroke-width="1.4"/><rect x="{}" y="{}" width="{}" height="{}" rx="2.5" fill="url(#capTop)"/></g>"##,
        x + 1.2,
        y + 2.4,
        x + 0.8,
        y + 0.8,
        w - 1.6,
        h - 1.6,
        x + 3.0,
        y + 1.6,
        w - 6.0,
        h * 0.4
    );
    if pressed {
        let _ = write!(o, r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="#000" opacity="0.2"/>"##);
    }
    o += "</g>";
    o
}

fn led_svg(cx: f64, cy: f64, on: bool, color: &str) -> String {
    if on {
        format!(r#"<circle cx="{cx}" cy="{cy}" r="5" fill="{color}" opacity=".35"/><circle cx="{cx}" cy="{cy}" r="2.6" fill="{color}"/>"#)
    } else {
        format!(r##"<circle cx="{cx}" cy="{cy}" r="2.6" fill="#3a1512"/>"##)
    }
}

/// The angles (degrees from straight up) of the cords switch's three positions.
pub const SWITCH: [f64; 3] = [-90.0, 0.0, 90.0];

/// A black knob with an amber pointer at `deg` (from straight up), over its printed `scale`.
fn knob_svg(cx: f64, cy: f64, deg: f64, scale: &str) -> String {
    let mut o = scale.to_owned();
    let a = deg.to_radians();
    let _ = write!(
        o,
        r##"<circle cx="{cx}" cy="{}" r="14.5" fill="#111" opacity=".5"/><circle cx="{cx}" cy="{cy}" r="14" fill="url(#gKnob)" stroke="#0c0c0d"/><circle cx="{cx}" cy="{cy}" r="10.5" fill="none" stroke="#fff" stroke-opacity=".08"/><path d="M{},{} L{},{}" stroke="#ffb02e" stroke-width="2.4" stroke-linecap="round"/>"##,
        cy + 1.5,
        cx + 4.0 * a.sin(),
        cy - 4.0 * a.cos(),
        cx + 12.0 * a.sin(),
        cy - 12.0 * a.cos()
    );
    o
}

/// The status bar: what PipeWire has on the left, a hint for what is under the pointer on the right.
pub fn status(w: f64, left: &str, right: &str, dpi: f64) -> Layer {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{STATUS_H}" viewBox="0 0 {w} {STATUS_H}" font-family="Noto Sans Mono" font-size="11.5"><rect width="{w}" height="{STATUS_H}" fill="#0e0f10"/><path d="M0,.5 H{w}" stroke="#000"/><text x="14" y="15" fill="#7d8085">{}</text><text x="{}" y="15" fill="#a8abae" text-anchor="end">{}</text></svg>"##,
        esc(left),
        w - 14.0,
        esc(right)
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h: STATUS_H }
}

/// A tooltip: a title in bold, then lines of PipeWire names in mono.
pub fn tooltip(lines: &[String], dpi: f64) -> Layer {
    let (title, rest) = lines.split_first().map_or(("", &[][..]), |(t, r)| (t.as_str(), r));
    let w = rest.iter().map(|l| text::width(text::MONO, l, 11.0, 0.0)).fold(text::width(text::BOLD, title, 12.0, 0.0), f64::max) + 24.0;
    let h = 30.0 + rest.len() as f64 * 16.0;
    let mut body = format!(r##"<text x="12" y="20" font-size="12" font-weight="bold" fill="#eceded">{}</text>"##, esc(title));
    for (i, l) in rest.iter().enumerate() {
        body += &format!(r##"<text x="12" y="{}" font-family="Noto Sans Mono" font-size="11" fill="#a9acb0">{}</text>"##, 37.0 + i as f64 * 16.0, esc(l));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans"><rect x=".5" y=".5" width="{}" height="{}" rx="7" fill="#141517" fill-opacity=".95" stroke="#000" stroke-opacity=".6"/>{body}</svg>"##,
        w - 1.0,
        h - 1.0
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h }
}

/// A short notice, a dark pill with a line of text.
pub fn notice(msg: &str, dpi: f64) -> Layer {
    let w = text::width(text::BOLD, msg, 13.0, 0.0) + 32.0;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="34" viewBox="0 0 {w} 34" font-family="Noto Sans" font-weight="bold"><rect width="{w}" height="34" rx="9" fill="#141517" fill-opacity=".94"/><text x="{}" y="22" font-size="13" fill="#e6e7e8" text-anchor="middle">{}</text></svg>"##,
        w / 2.0,
        esc(msg)
    );
    Layer { id: next_id(), pm: rasterize(&text::svg_options(), &svg, dpi), x: 0.0, y: 0.0, w, h: 34.0 }
}
