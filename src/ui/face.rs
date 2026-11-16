//! The front panel of a device as an SVG document (ported from docs/design/mockup).
//!
//! The name and LCD block on the left, then the port groups: inputs on the left, outputs on the
//! right (in a rack the outputs sit against the signal meter, with a grille between). Each port
//! is a mult: one jack per cord plugged in, plus one free jack.

use std::fmt::Write;
use std::sync::OnceLock;

use super::text;
use crate::graph::{Device, Dir, Finish, Media};

pub const DEV_H: f64 = 128.0;
/// distance between jacks
pub const JP: f64 = 48.0;
/// a rack is at least this wide
pub const RACK_MIN: f64 = 1000.0;
/// room around a face for its shadow
pub const SHADOW: f64 = 30.0;
/// width of the name and LCD block
const IDW: f64 = 190.0;
const PAD: f64 = 16.0;
const EAR: f64 = 30.0;
const TITLE: f64 = 14.0;
/// characters the LCD shows on one line
pub const LCD_CH: usize = 27;
/// where the LCD's text area is, relative to the face (without rack ears)
pub const LCD_X: f64 = PAD + 1.5;
pub const LCD_Y: f64 = 67.5;
pub const LCD_W: f64 = IDW - 3.0;
pub const LCD_H: f64 = 37.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum JackKind {
    Mono,
    Stereo,
    Midi,
}

/// What you plug into: a stereo jack carries the L and R ports, a mono jack one port.
#[derive(Clone, Debug)]
pub struct JackDef {
    /// stays the same while the device and its split switches do
    pub key: String,
    pub group: usize,
    pub ports: Vec<u32>,
    pub kind: JackKind,
    pub dir: Dir,
    pub media: Media,
    pub label: String,
}

pub fn jacks(dev: &Device, split: &[bool]) -> Vec<JackDef> {
    let mut v = Vec::new();
    for (gi, g) in dev.groups.iter().enumerate() {
        if g.stereo && !split.get(gi).copied().unwrap_or(false) {
            let names = format!("{}+{}", g.ports[0].name, g.ports[1].name);
            v.push(JackDef {
                key: format!("{}|{:?}|{names}", dev.key, g.dir),
                group: gi,
                ports: vec![g.ports[0].id, g.ports[1].id],
                kind: JackKind::Stereo,
                dir: g.dir,
                media: Media::Audio,
                label: "STEREO".into(),
            });
        } else {
            for p in &g.ports {
                v.push(JackDef {
                    key: format!("{}|{:?}|{}", dev.key, g.dir, p.name),
                    group: gi,
                    ports: vec![p.id],
                    kind: if p.media == Media::Midi { JackKind::Midi } else { JackKind::Mono },
                    dir: g.dir,
                    media: p.media,
                    label: p.label.clone(),
                });
            }
        }
    }
    v
}

/// Where things are on a face, relative to its top-left corner.
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub w: f64,
    /// the first jack of each mult, by `JackDef` index
    pub jacks: Vec<(f64, f64)>,
    /// split/join switches: group and rectangle (x, y, w, h)
    pub switches: Vec<(usize, [f64; 4])>,
    /// left edge of the panel proper (after a rack ear)
    pub left: f64,
    /// each group's left edge and width
    pub groups: Vec<(f64, f64)>,
    /// left edge of the signal meter
    pub meter: f64,
    /// the free space between inputs and outputs in a rack, for the grille
    pub grille: Option<(f64, f64)>,
}

struct Fin {
    base: &'static str,
    paint: bool,
    noise: f64,
    print: &'static str,
    sub: &'static str,
    /// text on a filled OUT tag
    inv: &'static str,
}

fn fin(f: Finish) -> Fin {
    match f {
        Finish::Metal => Fin { base: "#8b8d90", paint: true, noise: 0.0, print: "#1d1e20", sub: "#2c2e31", inv: "#d9dbdd" },
        Finish::Black => Fin { base: "#26272a", paint: false, noise: 0.9, print: "#d5d7d9", sub: "#9a9da1", inv: "#222" },
        Finish::Cream => Fin { base: "#d9d3be", paint: false, noise: 0.6, print: "#2a2926", sub: "#5a574f", inv: "#d9dbdd" },
        Finish::Navy => Fin { base: "#2b3950", paint: false, noise: 0.8, print: "#dde5f0", sub: "#9aa8bd", inv: "#222" },
    }
}

fn slots(count: usize) -> usize {
    count + 1
}

fn label_w(t: &str) -> f64 {
    text::width(text::BOLD, t, 8.5, 0.8)
}

fn group_w(dev: &Device, gi: usize, jd: &[JackDef], counts: &[usize]) -> f64 {
    let g = &dev.groups[gi];
    let n: usize = jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| slots(counts[i])).sum();
    (n as f64 * JP + 14.0).max(if g.stereo { 100.0 } else { 0.0 }).max((label_w(&g.label) + 48.0).ceil())
}

/// Width the face needs; a rack is as wide as its widest device.
pub fn natural_w(dev: &Device, jd: &[JackDef], counts: &[usize], rack: bool) -> f64 {
    let groups: f64 = (0..dev.groups.len()).map(|gi| group_w(dev, gi, jd, counts) + 10.0).sum();
    2.0 * if rack { EAR } else { 0.0 } + 2.0 * PAD + IDW + 14.0 + groups - 10.0 + 40.0
}

/// Where everything goes, without drawing it.
pub fn face_geometry(dev: &Device, jd: &[JackDef], counts: &[usize], rack_w: Option<f64>) -> Geometry {
    let rack = rack_w.is_some();
    let ear = if rack { EAR } else { 0.0 };
    let natural = natural_w(dev, jd, counts, rack);
    let w = rack_w.map_or(natural, |r| r.max(natural));
    let mx = w - ear - PAD - 22.0;
    let mut geo = Geometry { w, jacks: vec![(0.0, 0.0); jd.len()], switches: Vec::new(), left: ear, groups: vec![(0.0, 0.0); dev.groups.len()], meter: mx, grille: None };
    let place = |gi: usize, x: f64, geo: &mut Geometry| -> f64 {
        let gw = group_w(dev, gi, jd, counts);
        let n: usize = jd.iter().enumerate().filter(|(_, j)| j.group == gi).map(|(i, _)| slots(counts[i])).sum();
        let mut jx = x + (gw - n as f64 * JP) / 2.0 + JP / 2.0;
        for (i, _) in jd.iter().enumerate().filter(|(_, j)| j.group == gi) {
            geo.jacks[i] = (jx, 62.0);
            jx += slots(counts[i]) as f64 * JP;
        }
        if dev.groups[gi].stereo {
            geo.switches.push((gi, [x + gw - 42.0, 95.0, 36.0, 14.0]));
        }
        geo.groups[gi] = (x, gw);
        x + gw + 10.0
    };
    // inputs on the left, outputs on the right; in a rack the outputs sit against the meter
    let ins: Vec<usize> = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::In).collect();
    let outs: Vec<usize> = (0..dev.groups.len()).filter(|g| dev.groups[*g].dir == Dir::Out).collect();
    let span: f64 = outs.iter().map(|g| group_w(dev, *g, jd, counts) + 10.0).sum();
    let mut x = ear + PAD + IDW + 14.0;
    for g in &ins {
        x = place(*g, x, &mut geo);
    }
    let gx0 = x;
    let mut ox = if rack { mx - 4.0 - span } else { x };
    if rack && ox - 10.0 - gx0 > 50.0 {
        geo.grille = Some((gx0, ox - 10.0));
    }
    for g in &outs {
        ox = place(*g, ox, &mut geo);
    }
    geo
}

/// The name in capitals at one size: one line, else two, else two ending in "…". Lines break
/// after a space, "-", "_" or ":", and inside a word only when nothing else fits.
fn fit_title(name: &str, max_w: f64) -> Vec<String> {
    let t = name.to_uppercase();
    let w = |s: &str| text::width(text::BLACK, s, TITLE, 0.0);
    if w(&t) <= max_w {
        return vec![t];
    }
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in t.chars() {
        cur.push(c);
        if matches!(c, ' ' | '-' | '_' | ':') {
            parts.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let mut l1 = String::new();
    let mut i = 0;
    while i < parts.len() && w(&format!("{l1}{}", parts[i])) <= max_w {
        l1.push_str(&parts[i]);
        i += 1;
    }
    let mut rest: String = parts[i..].concat();
    if l1.is_empty() {
        let chars: Vec<char> = t.chars().collect();
        let mut k = 0;
        while k < chars.len() && w(&chars[..=k].iter().collect::<String>()) <= max_w {
            k += 1;
        }
        l1 = chars[..k].iter().collect();
        rest = chars[k..].iter().collect();
    }
    if w(&rest) > max_w {
        while !rest.is_empty() && w(&format!("{rest}…")) > max_w {
            rest.pop();
        }
        rest = format!("{}…", rest.trim_end());
    }
    vec![l1.trim_end().to_owned(), rest]
}

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The face as an SVG document, `SHADOW` larger on every side than the face itself.
/// `rack_w`: in a rack, the rack's width (the face gets ears and fills it).
pub fn face(dev: &Device, jd: &[JackDef], counts: &[usize], split: &[bool], rack_w: Option<f64>) -> (String, Geometry) {
    let f = fin(dev.finish);
    let rack = rack_w.is_some();
    let ear = if rack { EAR } else { 0.0 };
    let h = DEV_H;
    let x0 = ear + PAD;
    let geo = face_geometry(dev, jd, counts, rack_w);
    let (w, mx) = (geo.w, geo.meter);
    let mut body = String::new();
    for (gi, g) in dev.groups.iter().enumerate() {
        let (x, gw) = geo.groups[gi];
        let _ = write!(
            body,
            r#"<rect x="{x}" y="18" width="{gw}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="rgba(0,0,0,.3)"/><path d="M{},110.6 h{}" stroke="rgba(255,255,255,.14)"/><text x="{}" y="31" font-size="8.5" fill="{}" letter-spacing=".8">{}</text>"#,
            x + 6.0,
            gw - 12.0,
            x + 8.0,
            f.print,
            esc(&g.label)
        );
        let tx = x + gw - 30.0;
        if g.dir == Dir::In {
            let _ = write!(body, r#"<rect x="{tx}" y="22.5" width="23" height="11" rx="2" fill="none" stroke="{p}" stroke-width=".9"/><text x="{}" y="31" font-size="7.5" fill="{p}" text-anchor="middle">IN</text>"#, tx + 11.5, p = f.print);
        } else {
            let _ = write!(body, r#"<rect x="{tx}" y="22.5" width="23" height="11" rx="2" fill="{}"/><text x="{}" y="31" font-size="7.5" fill="{}" text-anchor="middle">OUT</text>"#, f.print, tx + 11.5, f.inv);
        }
        for (i, j) in jd.iter().enumerate().filter(|(_, j)| j.group == gi) {
            let k = slots(counts[i]);
            let (jx, _) = geo.jacks[i];
            // parallel jacks of one port get a printed outline, like a mult on a patch bay
            if k > 1 {
                let _ = write!(body, r#"<rect x="{}" y="41" width="{}" height="42" rx="21" fill="rgba(0,0,0,.08)" stroke="{}" stroke-width=".9" opacity=".5"/>"#, jx - 21.0, (k - 1) as f64 * JP + 42.0, f.print);
            }
            let sym = match j.kind {
                JackKind::Mono => "jack-mono",
                JackKind::Stereo => "jack-stereo",
                JackKind::Midi => "jack-midi",
            };
            for s in 0..k {
                let _ = write!(body, r##"<use href="#{sym}" x="{}" y="62"/>"##, jx + s as f64 * JP);
            }
            let (size, ls) = if j.kind == JackKind::Stereo { (7.5, 0.6) } else { (9.0, 0.0) };
            let _ = write!(body, r#"<text x="{}" y="91" font-size="{size}" fill="{}" text-anchor="middle" letter-spacing="{ls}">{}</text>"#, jx + (k - 1) as f64 * JP / 2.0, f.print, esc(&j.label));
        }
    }
    for (gi, [bx, by, _, _]) in &geo.switches {
        // a little hardware switch: one stereo jack, or L and R apart
        let by = by + 2.0;
        let on = split.get(*gi).copied().unwrap_or(false);
        let _ = write!(body, r##"<rect x="{bx}" y="{by}" width="34" height="10" rx="2" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/><text x="{}" y="{}" font-size="6.8" fill="#e6e7e8" text-anchor="middle" letter-spacing=".5">{}</text>"##, bx + 17.0, by + 7.6, if on { "JOIN" } else { "SPLIT" });
    }
    let mut grille = String::new();
    if let Some((gx0, gx1)) = geo.grille {
        let mut y = 30.0;
        while y <= 98.0 {
            let _ = write!(grille, r##"<rect x="{gx0}" y="{y}" width="{}" height="4.2" rx="2.1" fill="#0b0c0d"/><path d="M{},{} h{}" stroke="rgba(255,255,255,.12)"/>"##, gx1 - gx0, gx0 + 2.0, y + 5.1, gx1 - gx0 - 4.0);
            y += 9.5;
        }
    }
    let mut leds = String::new();
    for c in 0..2 {
        for i in 0..10 {
            let _ = write!(leds, r#"<rect x="{}" y="{}" width="8" height="5.4" rx="1" fill="rgba(0,0,0,.5)"/>"#, mx + c as f64 * 11.0, 92.0 - i as f64 * 7.6);
        }
    }

    let r = if rack { 3.0 } else { 12.0 };
    let lines = fit_title(&dev.name, IDW - 14.0);
    let title = if lines.len() == 1 {
        format!(r#"<text x="{x0}" y="40" font-size="{TITLE}">{}</text>"#, esc(&lines[0]))
    } else {
        format!(r#"<text x="{x0}" y="{}" font-size="{TITLE}">{}</text><text x="{x0}" y="45" font-size="{TITLE}">{}</text>"#, 44.0 - TITLE - 2.0, esc(&lines[0]), esc(&lines[1]))
    };
    let screws: String = if rack {
        String::new()
    } else {
        [(11.0, 11.0), (w - 11.0, 11.0), (11.0, h - 11.0), (w - 11.0, h - 11.0)]
            .iter()
            .enumerate()
            .map(|(i, (sx, sy))| format!(r##"<use href="#screw" transform="translate({sx},{sy}) rotate({})"/>"##, (i * 37 + dev.name.len() * 11) % 90))
            .collect()
    };
    // paint: solid base, then the texture on top
    let skin = |sx: f64, sw: f64, rr: f64| {
        let mut s = format!(r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="{}"/>"#, f.base);
        if f.paint {
            let _ = write!(s, r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="url(#pt)"/>"#);
        }
        if f.noise > 0.0 {
            let _ = write!(s, r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="url(#nz)" opacity="{}"/>"#, f.noise);
        }
        let _ = write!(s, r#"<rect x="{sx}" y="0" width="{sw}" height="{h}" rx="{rr}" fill="url(#gSheen)"/>"#);
        s
    };
    let ears = if rack {
        [0.0, w - EAR - 6.0]
            .iter()
            .map(|&ex| {
                let cx = if ex == 0.0 { EAR / 2.0 } else { w - EAR / 2.0 };
                let mut s = skin(ex, EAR + 6.0, 3.0);
                let _ = write!(s, r#"<rect x="{ex}" y="0" width="{}" height="{h}" rx="3" fill="none" stroke="rgba(0,0,0,.6)"/>"#, EAR + 6.0);
                for sy in [24.0, h - 24.0] {
                    let _ = write!(s, r##"<rect x="{}" y="{}" width="11" height="18" rx="5.5" fill="#0a0a0b"/><use href="#screw" transform="translate({cx},{sy}) rotate({sy})"/>"##, cx - 5.5, sy - 9.0);
                }
                s
            })
            .collect::<String>()
    } else {
        String::new()
    };

    let (dw, dh) = (w + 2.0 * SHADOW, h + 2.0 * SHADOW);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}" font-family="Noto Sans" font-weight="bold">
{defs}
<g transform="translate({SHADOW},{SHADOW})">
<g filter="url(#sh)">
{ears}
{skin}
<rect x="{a}" y=".75" width="{b}" height="{c}" rx="{r}" fill="none" stroke="rgba(255,255,255,.2)"/>
<rect x="{ear}" y="0" width="{d}" height="{h}" rx="{r}" fill="none" stroke="rgba(0,0,0,.65)"/>
</g>
{screws}
<g font-weight="900" fill="{print}">{title}</g>
<text x="{x0}" y="57" font-size="7.8" fill="{sub}" letter-spacing="1">{model}</text>
<circle cx="{led}" cy="54" r="5" fill="{glow}" opacity=".35"/><circle cx="{led}" cy="54" r="3.2" fill="{lamp}" stroke="rgba(0,0,0,.6)" stroke-width=".8"/>
<rect x="{lx}" y="64.5" width="{lw}" height="43" rx="4" fill="#111214"/>
<rect x="{ix}" y="67.5" width="{iw}" height="37" rx="2" fill="url(#gLcd)"/>
<rect x="{ix}" y="67.5" width="{iw}" height="37" rx="2" fill="none" stroke="rgba(110,45,0,.35)" stroke-width="3"/>
{body}{grille}{leds}
<text x="{sig}" y="112" font-size="7.5" fill="{print}" text-anchor="middle" letter-spacing=".6">SIG</text>
</g>
</svg>"##,
        defs = defs(),
        skin = skin(ear, w - 2.0 * ear, r),
        a = ear + 0.75,
        b = w - 2.0 * ear - 1.5,
        c = h - 1.5,
        d = w - 2.0 * ear,
        print = f.print,
        sub = f.sub,
        model = esc(&dev.model),
        led = x0 + IDW - 5.0,
        lx = x0 - 1.5,
        lw = IDW + 3.0,
        ix = x0 + 1.5,
        iw = IDW - 3.0,
        sig = mx + 9.5,
        // the status lamp: green while running, amber when idle, dark when suspended
        lamp = match dev.state {
            "running" => "#4cff6a",
            "idle" => "#c48a1c",
            _ => "#2a1d0a",
        },
        glow = if dev.state == "running" { "#4cff6a" } else { "none" },
    );
    (svg, geo)
}

/// The LCD's two lines of text, as a document the size of the LCD's text area.
pub fn lcd(line1: &str, line2: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{LCD_W}" height="{LCD_H}" viewBox="0 0 {LCD_W} {LCD_H}" font-family="Noto Sans Mono" fill="#3a2408" xml:space="preserve"><text x="7.5" y="15.5" font-size="10.5" font-weight="bold">{}</text><text x="7.5" y="30.5" font-size="10.5">{}</text></svg>"##,
        esc(line1),
        esc(line2)
    )
}

/// The LCD's top line: the full name, scrolling when it is too long, pausing at the start.
pub fn marquee(name: &str, ms: u128) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= LCD_CH {
        return name.to_owned();
    }
    let looped: Vec<char> = format!("{name}   ·  {name}").chars().collect();
    let period = chars.len() + 6 + 8;
    let pos = ((ms / 280) as usize % period).saturating_sub(8);
    looped[pos..(pos + LCD_CH).min(looped.len())].iter().collect()
}

/// The painted-metal texture, as a data URI for SVG documents.
pub fn paint_uri() -> &'static str {
    static URI: OnceLock<String> = OnceLock::new();
    URI.get_or_init(|| format!("data:image/png;base64,{}", base64(include_bytes!("../../assets/paint.png"))))
}

fn defs() -> &'static str {
    static DEFS: OnceLock<String> = OnceLock::new();
    DEFS.get_or_init(|| {
        let paint = paint_uri();
        let noise = format!("data:image/png;base64,{}", base64(include_bytes!("../../assets/noise.png")));
        let hex: String = (0..6)
            .map(|k| {
                let a = (k as f64 * 60.0 + 30.0).to_radians();
                format!("{:.2},{:.2} ", 15.0 * a.cos(), 15.0 * a.sin())
            })
            .collect();
        format!(
            r##"<defs>
<pattern id="pt" width="80" height="80" patternUnits="userSpaceOnUse"><image href="{paint}" width="80" height="80"/></pattern>
<pattern id="nz" width="128" height="128" patternUnits="userSpaceOnUse"><image href="{noise}" width="128" height="128"/></pattern>
<filter id="sh" x="-10%" y="-25%" width="120%" height="160%"><feDropShadow dx="0" dy="10" stdDeviation="7" flood-color="#000" flood-opacity=".5"/></filter>
<linearGradient id="gSheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".24"/></linearGradient>
<linearGradient id="gNut" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f7f8"/><stop offset=".45" stop-color="#b6b9bd"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gWasher" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#5e6166"/><stop offset="1" stop-color="#141517"/></radialGradient>
<radialGradient id="gWasherMidi" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#c8483a"/><stop offset="1" stop-color="#4a120d"/></radialGradient>
<radialGradient id="gChrome" cx=".35" cy=".3" r=".85"><stop offset="0" stop-color="#fff"/><stop offset=".4" stop-color="#b9bcc1"/><stop offset=".75" stop-color="#65686d"/><stop offset="1" stop-color="#a6a9ad"/></radialGradient>
<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>
<linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e38c2a"/></linearGradient>
<polygon id="hex" points="{hex}" fill="url(#gNut)" stroke="#43454a" stroke-width=".8"/>
<g id="jack-mono"><circle r="16.5" cx="1.2" cy="2.4" fill="#000" opacity=".35"/><use href="#hex"/><circle r="11.4" fill="url(#gWasher)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
<g id="jack-stereo"><use href="#jack-mono"/><circle r="10.1" fill="none" stroke="#c9a24a" stroke-width="1.3"/></g>
<g id="jack-midi"><circle r="16.5" cx="1.2" cy="2.4" fill="#000" opacity=".35"/><use href="#hex"/><circle r="11.4" fill="url(#gWasherMidi)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
<g id="screw"><circle r="5.2" fill="url(#gChrome)" stroke="#35363a" stroke-width=".7"/><path d="M-3.2,0 H3.2 M0,-3.2 V3.2" stroke="#36373b" stroke-width="1.4" stroke-linecap="round"/></g>
</defs>"##
        )
    })
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}
