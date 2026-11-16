//! Draws the cords and their plugs with tiny-skia (ported from docs/design/mockup).
//!
//! Everything is given in world units and drawn through `ts`, the world → pixels transform.

use resvg::tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, Path, PathBuilder, Pixmap, Point, RadialGradient, Rect, Shader, SpreadMode,
    Stroke, Transform,
};

use super::cord::{BOOT_TAIL, Cord, End, N, P, PLUG};
use super::face::JackKind;

type Rgb = [u8; 3];

/// The colour lighter (f > 0, towards white) or darker (f < 0, towards black).
fn shade(c: Rgb, f: f32) -> Rgb {
    c.map(|v| {
        let v = v as f32;
        (if f < 0.0 { v * (1.0 + f) } else { v + (255.0 - v) * f }) as u8
    })
}

fn rgba(c: Rgb, a: f32) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn solid(c: Rgb, a: f32) -> Paint<'static> {
    let mut p = Paint { anti_alias: true, ..Default::default() };
    p.set_color(rgba(c, a));
    p
}

fn shaded(shader: Option<Shader<'static>>, fallback: Rgb, a: f32) -> Paint<'static> {
    match shader {
        Some(s) => Paint { shader: s, anti_alias: true, ..Default::default() },
        None => solid(fallback, a),
    }
}

fn stops(list: &[(f32, Rgb)], a: f32) -> Vec<GradientStop> {
    list.iter().map(|(o, c)| GradientStop::new(*o, rgba(*c, a))).collect()
}

fn stroke(width: f32) -> Stroke {
    Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() }
}

/// The cord's centre line as a smooth curve through its points.
fn trace(pts: &[P], ox: f32, oy: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x + ox, pts[0].y + oy);
    for i in 1..pts.len() - 1 {
        let (p, q) = (pts[i], pts[i + 1]);
        pb.quad_to(p.x + ox, p.y + oy, (p.x + q.x) / 2.0 + ox, (p.y + q.y) / 2.0 + oy);
    }
    let l = pts[pts.len() - 1];
    pb.line_to(l.x + ox, l.y + oy);
    pb.finish()
}

fn cord_w(c: &Cord) -> f32 {
    (if c.stereo() { 7.6 } else { 6.6 }) / c.strain.max(1.0).sqrt()
}

pub fn shadow(pm: &mut Pixmap, c: &Cord, alpha: f32, ts: Transform) {
    let Some(path) = trace(&c.pts, 7.0, 12.0) else { return };
    for (w, a) in [(13.0, 0.07), (9.0, 0.1), (6.0, 0.12)] {
        pm.stroke_path(&path, &solid([0, 0, 0], a * alpha), &stroke(w), ts, None);
    }
}

pub fn body(pm: &mut Pixmap, c: &Cord, alpha: f32, ts: Transform) {
    let w = cord_w(c);
    let Some(path) = trace(&c.pts, 0.0, 0.0) else { return };
    pm.stroke_path(&path, &solid(shade(c.color, -0.62), alpha), &stroke(w + 2.2), ts, None);
    pm.stroke_path(&path, &solid(c.color, alpha), &stroke(w), ts, None);
    if let Some(hl) = trace(&c.pts, -w * 0.17, -w * 0.2) {
        let lite = shade(c.color, if c.strain > 1.04 { 0.7 } else { 0.45 });
        pm.stroke_path(&hl, &solid(lite, alpha * 0.75), &stroke(w * 0.3), ts, None);
    }
}

/// Points along the cord, `ds` world units from one end, with the cord's direction there.
fn along(c: &Cord, from_b: bool, ds: &[f32]) -> Vec<(f32, f32, f32)> {
    let at = |i: usize| if from_b { c.pts[N - 1 - i] } else { c.pts[i] };
    let mut out = Vec::with_capacity(ds.len());
    let (mut acc, mut i) = (0.0, 0);
    let (mut a, mut b) = (at(0), at(1));
    let mut l = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    for &d in ds {
        while acc + l < d && i < N - 2 {
            acc += l;
            i += 1;
            (a, b) = (at(i), at(i + 1));
            l = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        }
        let t = if l > 0.0 { ((d - acc) / l).clamp(0.0, 1.0) } else { 0.0 };
        out.push((a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, (b.y - a.y).atan2(b.x - a.x)));
    }
    out
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, ang: f32) -> Option<Path> {
    let p = PathBuilder::from_oval(Rect::from_xywh(-rx, -ry, rx * 2.0, ry * 2.0)?)?;
    p.transform(Transform::from_translate(cx, cy).pre_rotate(ang.to_degrees()))
}

/// Seen from the front, a seated plug is its ribbed boot: round where it meets the jack,
/// bending down into the cord.
pub fn seated(pm: &mut Pixmap, c: &Cord, from_b: bool, e: &End, alpha: f32, ts: Transform) {
    let col = c.color;
    let k = 1.0 + e.ins * 0.2;
    let r0 = 12.2 * k;
    let ds: Vec<f32> = (0..14).map(|i| i as f32 * 2.4).collect();
    let sm = along(c, from_b, &ds);
    let (hx, hy, _) = sm[0];
    if let Some(p) = PathBuilder::from_circle(hx + 3.0, hy + 6.0, r0 + 1.5) {
        pm.fill_path(&p, &solid([0, 0, 0], 0.32 * alpha), FillRule::Winding, ts, None);
    }
    let (dark, lite, rim) = (shade(col, -0.6), shade(col, 0.38), shade(col, -0.72));
    for j in (1..sm.len()).rev() {
        let (x, y, ang) = sm[j];
        let u = j as f32 / (sm.len() - 1) as f32;
        let r = r0 - (r0 - 4.6) * u.powf(0.75);
        let rx = (r * (1.0 - 0.55 * (u * 2.5).min(1.0))).max(1.8);
        // light from the top-left: shade across the boot towards that side
        let (mut nx, mut ny) = (-ang.sin(), ang.cos());
        if nx * -0.6 + ny * -0.8 < 0.0 {
            (nx, ny) = (-nx, -ny);
        }
        let g = LinearGradient::new(
            Point::from_xy(x + nx * r, y + ny * r),
            Point::from_xy(x - nx * r, y - ny * r),
            stops(&[(0.0, shade(col, 0.2)), (0.25, lite), (0.55, col), (1.0, dark)], alpha),
            SpreadMode::Pad,
            Transform::identity(),
        );
        if let Some(p) = ellipse(x, y, rx, r, ang) {
            pm.fill_path(&p, &shaded(g, col, alpha), FillRule::Winding, ts, None);
            pm.stroke_path(&p, &solid(rim, alpha * 0.55), &stroke(0.9), ts, None);
        }
    }
    // the rounded back of the plug, nearest to us
    let g = RadialGradient::new(Point::from_xy(hx - r0 * 0.35, hy - r0 * 0.4), Point::from_xy(hx, hy), r0, stops(&[(0.0, lite), (0.55, col), (1.0, dark)], alpha), SpreadMode::Pad, Transform::identity());
    if let Some(p) = PathBuilder::from_circle(hx, hy, r0) {
        pm.fill_path(&p, &shaded(g, col, alpha), FillRule::Winding, ts, None);
        pm.stroke_path(&p, &solid(rim, alpha), &stroke(0.9), ts, None);
    }
    for rr in [0.72, 0.45] {
        if let Some(p) = PathBuilder::from_circle(hx, hy, r0 * rr) {
            pm.stroke_path(&p, &solid([0, 0, 0], 0.18 * alpha), &stroke(0.9), ts, None);
        }
    }
    if let Some(p) = ellipse(hx - r0 * 0.38, hy - r0 * 0.42, r0 * 0.3, r0 * 0.14, -0.7) {
        pm.fill_path(&p, &solid([255, 255, 255], 0.45 * alpha), FillRule::Winding, ts, None);
    }
}

/// In the hand (or falling), the whole plug shows from the side: boot, ribbed grip, collar,
/// sleeve, contacts and tip. While it turns to face a jack it gets shorter.
pub fn side(pm: &mut Pixmap, c: &Cord, e: &End, alpha: f32, ts: Transform) {
    let col = c.color;
    let f = e.lift.max(0.05);
    let alpha = alpha * (e.lift * 2.2).clamp(0.0, 1.0);
    let rc = (cord_w(c) + 2.2) / 2.0 / PLUG;
    let boot_r = |x: f32| rc + (6.4 - rc) * ((x - BOOT_TAIL) / (16.0 - BOOT_TAIL)).powf(1.6);
    let t0 = if e.kind == JackKind::Stereo { 85.0 } else { 77.6 };
    let tip_end = t0 + 10.5;
    let place = |x: f32, y: f32| {
        let mut t = Transform::from_translate(x, y).pre_rotate(e.ang.to_degrees());
        if e.ang.cos() < 0.0 {
            t = t.pre_scale(1.0, -1.0); // keep the highlight on top
        }
        ts.pre_concat(t.pre_scale(PLUG * f, PLUG).pre_translate(-BOOT_TAIL, 0.0))
    };
    let rubber = |pb: &mut PathBuilder, grip: f32| {
        pb.move_to(BOOT_TAIL, -rc);
        let mut x = BOOT_TAIL + 2.0;
        while x <= 16.0 {
            pb.line_to(x, -boot_r(x));
            x += 2.0;
        }
        pb.line_to(16.5, -7.0);
        pb.line_to(grip, -7.0);
    };
    // shadow: the plug's outline, offset like the cord's shadow
    let mut pb = PathBuilder::new();
    rubber(&mut pb, 44.0);
    for (x, y) in [(44.0, -4.9), (47.5, -4.9), (47.5, -3.7), (t0 + 5.0, -3.7), (tip_end, 0.0), (t0 + 5.0, 3.7), (47.5, 3.7), (47.5, 4.9), (44.0, 4.9), (44.0, 7.0), (16.5, 7.0)] {
        pb.line_to(x, y);
    }
    let mut x = 16.0;
    while x >= BOOT_TAIL + 2.0 {
        pb.line_to(x, boot_r(x));
        x -= 2.0;
    }
    pb.line_to(BOOT_TAIL, rc);
    pb.close();
    if let Some(p) = pb.finish() {
        pm.fill_path(&p, &solid([0, 0, 0], 0.3 * alpha), FillRule::Winding, place(e.x + 7.0, e.y + 12.0), None);
    }

    let t = place(e.x, e.y);
    let across = |r: f32, list: &[(f32, Rgb)]| LinearGradient::new(Point::from_xy(0.0, -r), Point::from_xy(0.0, r), stops(list, alpha), SpreadMode::Pad, Transform::identity());
    let chrome = |r: f32| across(r, &[(0.0, [0x7a, 0x7d, 0x82]), (0.28, [255, 255, 255]), (0.5, [0xc3, 0xc6, 0xca]), (0.8, [0x5a, 0x5d, 0x62]), (1.0, [0x8e, 0x91, 0x95])]);
    let black = across(3.7, &[(0.0, [0x11; 3]), (0.3, [0x55; 3]), (1.0, [0x05; 3])]);
    let band = |pm: &mut Pixmap, x0: f32, x1: f32, r: f32, sh: Option<Shader<'static>>| {
        if let Some(rect) = Rect::from_xywh(x0 - 0.15, -r, x1 - x0 + 0.3, 2.0 * r) {
            pm.fill_rect(rect, &shaded(sh, [0xb0; 3], alpha), t, None);
        }
    };
    // boot and grip in one piece of rubber, then the ribs
    let mut pb = PathBuilder::new();
    rubber(&mut pb, 42.5);
    pb.quad_to(44.2, -7.0, 44.2, -5.5);
    pb.line_to(44.2, 5.5);
    pb.quad_to(44.2, 7.0, 42.5, 7.0);
    pb.line_to(16.5, 7.0);
    let mut x = 16.0;
    while x >= BOOT_TAIL + 2.0 {
        pb.line_to(x, boot_r(x));
        x -= 2.0;
    }
    pb.line_to(BOOT_TAIL, rc);
    pb.close();
    if let Some(p) = pb.finish() {
        let g = across(7.0, &[(0.0, shade(col, -0.35)), (0.28, shade(col, 0.4)), (0.55, col), (1.0, shade(col, -0.65))]);
        pm.fill_path(&p, &shaded(g, col, alpha), FillRule::Winding, t, None);
    }
    let mut x = 1.0;
    while x < 36.0 {
        let r = if x <= 16.0 { boot_r(x) } else { 7.0 } - 0.7;
        let mut pb = PathBuilder::new();
        pb.move_to(x, -r);
        pb.line_to(x, r);
        if let Some(p) = pb.finish() {
            pm.stroke_path(&p, &solid(shade(col, -0.6), alpha * 0.75), &Stroke { width: 1.1, ..Default::default() }, t, None);
        }
        x += 3.2;
    }
    // collar and sleeve
    band(pm, 44.0, 47.5, 4.9, chrome(4.9));
    band(pm, 47.5, 75.0, 3.7, chrome(3.7));
    // mono (TS): sleeve | insulator | tip. stereo (TRS): sleeve | insulator | ring | insulator | tip
    band(pm, 75.0, 77.6, 3.7, black.clone());
    if e.kind == JackKind::Stereo {
        band(pm, 77.6, 82.4, 3.7, chrome(3.7));
        band(pm, 82.4, 85.0, 3.7, black);
    }
    // tip: one piece, with a groove before the rounded point
    let mut pb = PathBuilder::new();
    pb.move_to(t0 - 0.15, -3.7);
    for (x, y) in [(t0 + 3.0, -3.7), (t0 + 4.0, -2.7), (t0 + 5.0, -3.6)] {
        pb.line_to(x, y);
    }
    pb.quad_to(tip_end - 0.5, -3.2, tip_end, 0.0);
    pb.quad_to(tip_end - 0.5, 3.2, t0 + 5.0, 3.6);
    for (x, y) in [(t0 + 4.0, 2.7), (t0 + 3.0, 3.7), (t0 - 0.15, 3.7)] {
        pb.line_to(x, y);
    }
    pb.close();
    if let Some(p) = pb.finish() {
        pm.fill_path(&p, &shaded(chrome(3.7), [0xb0; 3], alpha), FillRule::Winding, t, None);
    }
}

/// Rings around the jacks a plug in the hand may go into; the one it is pulled to glows gold.
pub fn rings(pm: &mut Pixmap, rings: &[((f32, f32), bool)], pulse: f32, ts: Transform) {
    for &((x, y), hot) in rings {
        let Some(p) = PathBuilder::from_circle(x, y, if hot { 19.0 } else { 17.5 }) else { continue };
        let (c, a, w) = if hot { ([255, 200, 90], 0.95, 3.5) } else { ([120, 230, 150], 0.3 + 0.3 * pulse, 2.0) };
        pm.stroke_path(&p, &solid(c, a), &stroke(w), ts, None);
    }
}

/// All cords: shadows, then cords, then the plugs (seated ones first, then those in the air).
/// `alpha_of` gives each cord's opacity (fading, focus).
pub fn cords(pm: &mut Pixmap, cords: &[Cord], alpha_of: impl Fn(&Cord) -> f32, ts: Transform) {
    for c in cords {
        let a = alpha_of(c);
        if a > 0.01 {
            shadow(pm, c, a, ts);
        }
    }
    for c in cords {
        let a = alpha_of(c);
        if a > 0.01 {
            body(pm, c, a, ts);
        }
    }
    for air in [false, true] {
        for c in cords {
            for (from_b, e) in [(false, &c.a), (true, &c.b)] {
                if (e.held || e.free) != air {
                    continue;
                }
                if e.lift < 0.6 {
                    seated(pm, c, from_b, e, c.alpha * (1.0 - e.lift * 1.8).clamp(0.0, 1.0), ts);
                }
                if e.lift > 0.05 {
                    side(pm, c, e, c.alpha, ts);
                }
            }
        }
    }
}
