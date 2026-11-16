//! The top panel (a strip of painted metal with hardware keys, the SAG knob and the LCD), the
//! status bar at the bottom, and the short notices. Sizes are in logical pixels.

use std::fmt::Write;

use super::face::paint_uri;
use super::scene::{Layer, next_id, rasterize};
use super::text;

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
    Cables,
    Sag,
    Arrange,
    Fit,
    Sound,
    Lcd,
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
            Key::Cables => "Cords: show · fade · hide (C)",
            Key::Sag => "How much the cords sag: drag or turn the wheel",
            Key::Arrange => "Arrange devices by signal flow (A)",
            Key::Fit => "Fit everything in the window (F)",
            Key::Sound => "Plug sounds",
            Key::Lcd => "Presets",
        }
    }
}

/// What the panel shows.
#[derive(Clone, PartialEq, Hash)]
pub struct State {
    pub rack: bool,
    pub locked: bool,
    /// 0 show, 1 fade, 2 hide
    pub cables: u8,
    /// 0…100
    pub sag: u8,
    pub sound: bool,
    pub lcd: [String; 2],
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
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuHit {
    Row(usize),
    Delete(usize),
    Save,
    /// somewhere else on the drop-down
    Inside,
}

const MENU_W: f64 = 300.0;

// icons on a 24-unit grid, drawn with round 2-unit strokes
const ICONS: &[(Key, &str)] = &[
    (Key::Undo, r#"<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>"#),
    (Key::Redo, r#"<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>"#),
    (Key::Save, r#"<path d="M5 3h11l3 3v15H5z"/><path d="M8 3v5h7V3"/><path d="M8 21v-7h8v7"/>"#),
    (Key::Load, r#"<path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h10"/>"#),
    (Key::Lock, r#"<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>"#),
    (Key::Arrange, r#"<rect x="3.5" y="4" width="7" height="5" rx="1"/><rect x="3.5" y="15" width="7" height="5" rx="1"/><rect x="13.5" y="9.5" width="7" height="5" rx="1"/><path d="M10.5 6.5h1.5v11h-1.5M12 12h1.5"/>"#),
    (Key::Fit, r#"<path d="M4 9V4h5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/><path d="M20 15v5h-5"/>"#),
    (Key::Sound, r#"<path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M16.5 8.5a5 5 0 0 1 0 7"/><path d="M19 6a8.5 8.5 0 0 1 0 12"/>"#),
];

const KEY_Y: f64 = 10.0;
const KEY_H: f64 = 30.0;
const LABEL: &str = "#26272a";

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Panel {
    pub fn new(state: State) -> Self {
        Panel { state, rects: Vec::new(), layer: None, menu: None, menu_rects: Vec::new(), menu_layer: None }
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
            let (x, y) = (width - 18.0 - MENU_W, H + 4.0);
            let (svg, mh, rects) = menu_svg(menu);
            self.menu_rects = std::iter::once((MenuHit::Inside, [x, y, MENU_W, mh])).chain(rects.into_iter().map(|(h, [rx, ry, rw, rh])| (h, [x + rx, y + ry, rw, rh]))).collect();
            let pm = rasterize(&text::svg_options(), &svg, dpi);
            self.menu_layer = Some((sig, Layer { id: next_id(), pm, x, y, w: MENU_W, h: mh }));
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
            r##"<g transform="translate({x},34) scale(1.06,1)" font-size="26" font-weight="900" letter-spacing="1"><text y="1" fill="#fff" opacity=".35">PATCHCORD</text><text fill="#2a2b2e">PATCHCORD</text></g><text x="{x}" y="50" font-size="9" letter-spacing="1.6" fill="#2e3033">PC-1 · PIPEWIRE PATCHBAY</text>"##
        );
        x += logo_w + 26.0;

        // a group of keys with its name printed under them
        let group = |o: &mut String, rects: &mut Vec<(Key, [f64; 4])>, x: &mut f64, keys: &[(Key, Option<&str>, Option<bool>)], label: &str| {
            let x0 = *x;
            for (i, (key, txt, led)) in keys.iter().enumerate() {
                if i > 0 {
                    *x += 8.0;
                }
                let kw = match txt {
                    Some(t) => (text::width(text::BOLD, t, 10.5, 0.4) + 20.0).max(62.0) + if *key == Key::Cables { 26.0 } else { 0.0 },
                    None => 38.0,
                };
                let pressed = st.pressed == Some(*key);
                o.push_str(&key_svg(*x, kw, pressed));
                let dy = if pressed { 1.0 } else { 0.0 };
                match txt {
                    Some(t) => {
                        let tx = *x + if *key == Key::Cables { (kw - 26.0) / 2.0 + 2.0 } else { kw / 2.0 + if led.is_some() { 4.0 } else { 0.0 } };
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
                if *key == Key::Cables {
                    for k in 0..3u8 {
                        o.push_str(&led_svg(*x + kw - 24.0 + k as f64 * 7.0, KEY_Y + 15.0 + dy, st.cables == k, "#ffb02e"));
                    }
                }
                rects.push((*key, [*x - 3.0, KEY_Y - 3.0, kw + 6.0, KEY_H + 6.0]));
                *x += kw;
            }
            let _ = write!(o, r#"<text x="{}" y="54" font-size="9" letter-spacing="1.2" fill="{LABEL}" text-anchor="middle">{label}</text>"#, (x0 + *x) / 2.0);
            *x += 22.0;
        };
        group(&mut o, &mut rects, &mut x, &[(Key::Undo, None, None), (Key::Redo, None, None), (Key::Save, None, None), (Key::Load, None, None)], "EDIT");
        group(&mut o, &mut rects, &mut x, &[(Key::Desk, Some("DESK"), Some(!st.rack)), (Key::Rack, Some("RACK"), Some(st.rack)), (Key::Lock, None, Some(st.locked))], "LAYOUT");
        group(&mut o, &mut rects, &mut x, &[(Key::Cables, Some("CABLES"), None)], "SHOW·FADE·HIDE");

        // the SAG knob
        let (kx, ky) = (x + 17.0, KEY_Y + 15.0);
        o.push_str(&knob_svg(kx, ky, st.sag as f64 / 100.0));
        let _ = write!(o, r#"<text x="{kx}" y="54" font-size="9" letter-spacing="1.2" fill="{LABEL}" text-anchor="middle">SAG</text>"#);
        rects.push((Key::Sag, [kx - 20.0, ky - 20.0, 40.0, 40.0]));
        x += 34.0 + 22.0;

        group(&mut o, &mut rects, &mut x, &[(Key::Arrange, None, None), (Key::Fit, None, None), (Key::Sound, None, Some(st.sound))], "VIEW");

        // the LCD, against the right edge, as wide as there is room for
        let lw = (w - 18.0 - x).min(340.0);
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
<linearGradient id="gKey" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#45464a"/><stop offset=".55" stop-color="#232427"/><stop offset="1" stop-color="#1b1c1e"/></linearGradient>
<linearGradient id="gKeyDown" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#36373a"/><stop offset="1" stop-color="#1d1e20"/></linearGradient>
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
    let w = MENU_W;
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

/// A dark hardware key with its bezel.
fn key_svg(x: f64, w: f64, pressed: bool) -> String {
    let (y, h) = (KEY_Y + if pressed { 1.0 } else { 0.0 }, KEY_H);
    format!(
        r##"<rect x="{}" y="{}" width="{}" height="{}" rx="7" fill="#000" opacity=".3"/><rect x="{}" y="{}" width="{}" height="{}" rx="7" fill="#fff" opacity=".18"/><rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="#19191b"/><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="5" fill="url(#{})"/><path d="M{},{} h{}" stroke="#fff" stroke-opacity=".18"/>"##,
        x - 3.0,
        KEY_Y - 1.0,
        w + 6.0,
        h + 7.0,
        x - 3.0,
        KEY_Y - 3.0,
        w + 6.0,
        h + 6.0,
        x - 2.0,
        KEY_Y - 2.0,
        w + 4.0,
        h + 4.0,
        if pressed { "gKeyDown" } else { "gKey" },
        x + 4.0,
        y + 0.75,
        w - 8.0
    )
}

fn led_svg(cx: f64, cy: f64, on: bool, color: &str) -> String {
    if on {
        format!(r#"<circle cx="{cx}" cy="{cy}" r="5" fill="{color}" opacity=".35"/><circle cx="{cx}" cy="{cy}" r="2.6" fill="{color}"/>"#)
    } else {
        format!(r##"<circle cx="{cx}" cy="{cy}" r="2.6" fill="#3a1512"/>"##)
    }
}

/// The SAG knob: a black knob with an amber pointer over a printed scale.
fn knob_svg(cx: f64, cy: f64, v: f64) -> String {
    let mut o = String::new();
    for i in 0..=10 {
        let t = (-135.0 + 27.0 * i as f64).to_radians();
        let _ = write!(o, r#"<path d="M{},{} L{},{}" stroke="{LABEL}" stroke-width="1.2"/>"#, cx + 17.0 * t.sin(), cy - 17.0 * t.cos(), cx + 19.5 * t.sin(), cy - 19.5 * t.cos());
    }
    let a = (-135.0 + 270.0 * v).to_radians();
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
