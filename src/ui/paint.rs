//! Draws the cords and their plugs with tiny-skia (ported from docs/design/mockup).
//!
//! Everything is given in world units and drawn through `ts`, the world → pixels transform.

use resvg::tiny_skia::{Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, RadialGradient, Path, PathBuilder, Pixmap, Point, Rect, Shader, SpreadMode, Stroke, Transform};

use super::cord::{BARREL_R, Cord, End, HEAD_BOTTOM, HEAD_R, HEAD_TOP, HEAD_X0, HEAD_X1, PLUG, SHAFT_LEN, SHAFT_R, SHAFT_X};
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

/// Half an ellipse around (cx, cy), from its left end over the top to its right end (`top`), or
/// from its right end under the bottom to its left end; the path must already be at its start.
fn half(pb: &mut PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32, top: bool) {
    const K: f32 = 0.5523;
    let (rx, ry) = if top { (rx, -ry) } else { (-rx, ry) };
    pb.cubic_to(cx - rx, cy + ry * K, cx - rx * K, cy + ry, cx, cy + ry);
    pb.cubic_to(cx + rx * K, cy + ry, cx + rx, cy + ry * K, cx + rx, cy);
}

/// The cord's centre line as a smooth curve through its points.
fn trace(c: &Cord, ox: f32, oy: f32) -> Option<Path> {
    // from where it leaves one plug's barrel, straight to where it starts to bend, along its
    // points, and straight again into the other plug
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(c.pts.len() + 2);
    pts.push((c.a.cx, c.a.cy));
    pts.extend(c.pts.iter().map(|p| (p.x, p.y)));
    pts.push((c.b.cx, c.b.cy));
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0 + ox, pts[0].1 + oy);
    pb.line_to(pts[1].0 + ox, pts[1].1 + oy);
    for i in 2..pts.len() - 2 {
        let (p, q) = (pts[i], pts[i + 1]);
        pb.quad_to(p.0 + ox, p.1 + oy, (p.0 + q.0) / 2.0 + ox, (p.1 + q.1) / 2.0 + oy);
    }
    for p in &pts[pts.len() - 2..] {
        pb.line_to(p.0 + ox, p.1 + oy);
    }
    pb.finish()
}

fn cord_w(c: &Cord) -> f32 {
    (if c.stereo() { 7.6 } else { 6.6 }) / c.strain.max(1.0).sqrt()
}

pub fn shadow(pm: &mut Pixmap, c: &Cord, alpha: f32, ts: Transform) {
    let Some(path) = trace(c, 7.0, 12.0) else { return };
    for (w, a) in [(13.0, 0.07), (9.0, 0.1), (6.0, 0.12)] {
        pm.stroke_path(&path, &solid([0, 0, 0], a * alpha), &stroke(w), ts, None);
    }
}

pub fn body(pm: &mut Pixmap, c: &Cord, alpha: f32, ts: Transform) {
    let w = cord_w(c);
    let Some(path) = trace(c, 0.0, 0.0) else { return };
    pm.stroke_path(&path, &solid(shade(c.color, -0.62), alpha), &stroke(w + 2.2), ts, None);
    pm.stroke_path(&path, &solid(c.color, alpha), &stroke(w), ts, None);
    if let Some(hl) = trace(c, -w * 0.17, -w * 0.2) {
        let lite = shade(c.color, if c.strain > 1.04 { 0.7 } else { 0.45 });
        pm.stroke_path(&hl, &solid(lite, alpha * 0.75), &stroke(w * 0.3), ts, None);
    }
}

/// A right-angle 6.3 mm plug, drawn to scale in millimetres: two barrels of the same diameter
/// joined at a right angle. The cord goes into the end of the matte metal barrel; the head is the
/// second barrel, in the cord's colour, with a rounded back; the shaft (6.3 thick) comes out of
/// its front with its insulating ring(s) and pointed tip. In the hand the plug shows from the
/// side; in a jack the head points into the panel, so it shows as its round end with the metal
/// barrel lying along the panel. On the way in, the head and the shaft shorten as the plug turns.
/// Local units: x along the metal barrel (0 where the cord goes in), +y along the shaft.
/// `shadow`: draw only its shadow (all plug shadows go down before the cords, so a plug never
/// shades its own cord), else only the plug.
pub fn side(pm: &mut Pixmap, c: &Cord, e: &End, alpha: f32, ts: Transform, shadow: bool) {
    let col = c.color;
    let f = e.lift;
    let shaft = f > 0.06;
    let (r, hr) = (BARREL_R, HEAD_R);
    // The head is a second barrel standing on the shaft, its top a low dome. The plug turns about
    // the metal barrel's axis from side on (in the hand, f = 1) to end on (in a jack, f = 0):
    // `f` is the cosine of that turn, so lengths along the head and shaft shrink with it.
    let (c, s) = (f.clamp(0.0, 1.0), (1.0 - f.clamp(0.0, 1.0).powi(2)).sqrt());
    // the dome starts level with the top of the metal barrel, so end on the head is a whole circle
    let (yc, yb) = (-r, HEAD_BOTTOM);
    let hd = yc - HEAD_TOP;
    // the dome's outline and the head's rims, seen at this turn
    let (dome_ry, rim_ry) = ((hd * hd * c * c + hr * hr * s * s).sqrt(), hr * s);
    let hy1 = yb * c;
    // along the shaft, measured from its tip; the shaft shortens as it turns into the panel
    let at = |from_tip: f32| hy1 + (SHAFT_LEN - from_tip) * f;
    let stereo = e.kind == JackKind::Stereo;
    let place = |x: f32, y: f32| ts.pre_concat(Transform::from_translate(x, y).pre_rotate(e.ang.to_degrees()).pre_scale(PLUG, PLUG));
    // The light comes from the top left of the screen and stays there while the plug turns: the
    // brightest line across a round part moves smoothly towards the side that faces it.
    let (sin, cos) = e.ang.sin_cos();
    let facing = |vx: f32, vy: f32| (vx * -0.6 + vy * -0.8).clamp(-1.0, 1.0);
    // how much the local +y side (across the barrel) and +x side (across the head and shaft) face the light
    let (ky, kx) = (facing(-sin, cos), facing(cos, sin));
    let lit = |k: f32| (0.5 + 0.24 * k).clamp(0.2, 0.8);
    let shine = |h: f32, dark: Rgb, mid: Rgb, top: Rgb, edge: Rgb| vec![(0.0, dark), ((h - 0.2).max(0.02), mid), (h, top), ((h + 0.27).min(0.97), mid), (1.0, edge)];
    // matte metal: soft light across a round part, no sharp highlight
    let metal = |k: f32| shine(lit(k), [0x55, 0x58, 0x5c], [0x9a, 0x9d, 0xa1], [0xc6, 0xc9, 0xcd], [0x50, 0x53, 0x57]);
    // glossy plastic in the cord's colour, as on the mockup: a bright line on the lit side
    let gloss = |k: f32| {
        let h = lit(k);
        vec![(0.0, shade(col, -0.5)), ((h - 0.3).max(0.04), shade(col, -0.08)), (h, shade(col, 0.34)), ((h + 0.14).min(0.96), shade(col, 0.05)), (1.0, shade(col, -0.55))]
    };
    let grad = |from: Point, to: Point, list: Vec<(f32, Rgb)>| LinearGradient::new(from, to, stops(&list, alpha), SpreadMode::Pad, Transform::identity());
    let across_x = |rr: f32, list: Vec<(f32, Rgb)>| grad(Point::from_xy(0.0, -rr), Point::from_xy(0.0, rr), list);
    let across_y = |cx: f32, rr: f32, list: Vec<(f32, Rgb)>| grad(Point::from_xy(cx - rr, 0.0), Point::from_xy(cx + rr, 0.0), list);
    let rect = |pm: &mut Pixmap, t: Transform, x: f32, y: f32, w: f32, h: f32, sh: Option<Shader<'static>>| {
        if let Some(rr) = Rect::from_xywh(x, y, w, h) {
            pm.fill_rect(rr, &shaded(sh, [0xa0; 3], alpha), t, None);
        }
    };
    // the whole head: the dome's upper outline, a side, the bottom rim's near half, the other side
    let head = || {
        let mut pb = PathBuilder::new();
        pb.move_to(HEAD_X0, yc * c);
        half(&mut pb, SHAFT_X, yc * c, hr, dome_ry, true);
        pb.line_to(HEAD_X1, yb * c);
        half(&mut pb, SHAFT_X, yb * c, hr, rim_ry, false);
        pb.close();
        pb.finish()
    };
    // the part of the dome that shows above the near half of the top rim
    let dome = || {
        let mut pb = PathBuilder::new();
        pb.move_to(HEAD_X0, yc * c);
        half(&mut pb, SHAFT_X, yc * c, hr, dome_ry, true);
        half(&mut pb, SHAFT_X, yc * c, hr, rim_ry, false);
        pb.close();
        pb.finish()
    };
    // How far towards us the head's surface is at (x, y), or -inf off it. We look along
    // (0, -s, c) in the plug's own 3D frame (x along the metal barrel, y along the shaft, z out of
    // the screen when side on), so the ray through (x, y) is Y = y·c − t·s, Z = y·s + t·c.
    let near = |x: f32, y: f32| -> f32 {
        let a2 = hr * hr - (x - SHAFT_X).powi(2);
        if a2 < 0.0 {
            return f32::NEG_INFINITY;
        }
        let (p, q) = (y * c, y * s);
        let mut best = f32::NEG_INFINITY;
        // the barrel part: |Z| ≤ a, yc ≤ Y ≤ yb
        let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
        let a = a2.sqrt();
        if c > 1e-4 {
            (lo, hi) = (lo.max((-a - q) / c), hi.min((a - q) / c));
        } else if q.abs() > a {
            return f32::NEG_INFINITY;
        }
        if s > 1e-4 {
            (lo, hi) = (lo.max((p - yb) / s), hi.min((p - yc) / s));
        } else if p < yc || p > yb {
            lo = f32::INFINITY;
        }
        if lo <= hi {
            best = hi;
        }
        // the dome: half an ellipsoid, hr across and hd high, on top of it
        let (k2, pp) = ((hr / hd).powi(2), p - yc);
        let (qa, qb, qc) = (c * c + k2 * s * s, 2.0 * (q * c - k2 * pp * s), q * q + k2 * pp * pp - a2);
        let disc = qb * qb - 4.0 * qa * qc;
        if disc >= 0.0 {
            best = best.max((-qb + disc.sqrt()) / (2.0 * qa));
        }
        best
    };
    // The metal barrel ends where it meets the head: along each line of its surface, at the first
    // point the head comes nearer to us than the barrel. Side on that is the saddle cut into the
    // head; end on it is the head's circle, so the two always touch without a gap.
    let barrel = || {
        let mut pb = PathBuilder::new();
        let k = 1.6 * 0.5523;
        pb.move_to(0.0, -r + 1.6);
        pb.cubic_to(0.0, -r + 1.6 - k, 1.6 - k, -r, 1.6, -r);
        for i in 0..=24 {
            let psi = std::f32::consts::PI * (i as f32 / 24.0 - 0.5);
            let (y, depth) = (r * psi.sin(), r * psi.cos());
            let (mut lo, mut hi) = (HEAD_X0, SHAFT_X);
            if near(lo, y) >= depth {
                hi = lo;
            }
            for _ in 0..18 {
                let m = (lo + hi) / 2.0;
                if near(m, y) >= depth { hi = m } else { lo = m }
            }
            pb.line_to(hi, y);
        }
        pb.line_to(1.6, r);
        pb.cubic_to(1.6 - k, r, 0.0, r - 1.6 + k, 0.0, r - 1.6);
        pb.close();
        pb.finish()
    };

    if shadow {
        // the shadow on the panel: the plug's outline
        let mut pb = PathBuilder::new();
        if shaft && let Some(rr) = Rect::from_xywh(SHAFT_X - SHAFT_R, hy1, 2.0 * SHAFT_R, at(0.0) - hy1) {
            pb.push_rect(rr);
        }
        let mut paths: Vec<Path> = pb.finish().into_iter().collect();
        paths.extend(head());
        paths.extend(barrel());
        // lying on the panel the shadow is close; lifted, it falls further away
        let t = place(e.cx + 3.0 + 4.0 * f, e.cy + 5.0 + 7.0 * f);
        for p in &paths {
            pm.fill_path(p, &solid([0, 0, 0], 0.3 * alpha), FillRule::Winding, t, None);
        }
        return;
    }

    let t = place(e.cx, e.cy);
    // the shaft (drawn first: the head sits over its top): sleeve, insulators, ring, tip
    if shaft {
        let (cx, sr) = (SHAFT_X, SHAFT_R);
        let black = |rr: f32| across_y(cx, rr, shine(lit(kx), [0x05; 3], [0x22; 3], [0x50; 3], [0x05; 3]));
        let sleeve_end = if stereo { 16.0 } else { 9.5 };
        rect(pm, t, cx - sr, hy1 - 0.5, 2.0 * sr, at(sleeve_end) - hy1 + 0.5, across_y(cx, sr, metal(kx)));
        if stereo {
            rect(pm, t, cx - sr, at(16.0), 2.0 * sr, at(14.0) - at(16.0), black(sr));
            rect(pm, t, cx - sr, at(14.0), 2.0 * sr, at(9.5) - at(14.0), across_y(cx, sr, metal(kx)));
        }
        rect(pm, t, cx - sr, at(9.5), 2.0 * sr, at(7.5) - at(9.5), black(sr));
        // the tip: its body, a groove, then the point
        let mut pb = PathBuilder::new();
        pb.move_to(cx - sr, at(7.5));
        for (dx, d) in [(-sr, 4.5), (-2.3, 4.2), (-2.3, 3.6), (-2.95, 3.3), (0.0, 0.0), (2.95, 3.3), (2.3, 3.6), (2.3, 4.2), (sr, 4.5), (sr, 7.5)] {
            pb.line_to(cx + dx, at(d));
        }
        pb.close();
        if let Some(p) = pb.finish() {
            pm.fill_path(&p, &shaded(across_y(cx, sr, metal(kx)), [0xa0; 3], alpha), FillRule::Winding, t, None);
        }
    }

    // the head, in the cord's colour, lit across its own axis
    if let Some(p) = head() {
        pm.fill_path(&p, &shaded(across_y(SHAFT_X, hr, gloss(kx)), col, alpha), FillRule::Winding, t, None);
        pm.stroke_path(&p, &solid(shade(col, -0.6), 0.5 * alpha), &stroke(0.3), t, None);
    }
    // its dome, shaded like the mockup's bulb: a light spot towards the light, two moulded rings
    // and a gloss highlight, which show as the plug turns to face us
    let unit = Transform::from_row(hr, 0.0, 0.0, dome_ry, SHAFT_X, yc * c);
    if let Some(p) = dome() {
        let spot = Point::from_xy(0.55 * kx, 0.55 * ky);
        let sh = RadialGradient::new(spot, Point::from_xy(0.0, 0.0), 1.0, stops(&[(0.0, shade(col, 0.38)), (0.55, col), (1.0, shade(col, -0.6))], alpha), SpreadMode::Pad, unit);
        pm.fill_path(&p, &shaded(sh, col, alpha), FillRule::Winding, t, None);
        pm.stroke_path(&p, &solid(shade(col, -0.72), 0.5 * alpha * s), &stroke(0.3), t, None);
    }
    for rr in [0.72f32, 0.45] {
        let cy = (yc - hd * (1.0 - rr * rr).sqrt()) * c;
        if let Some(p) = Rect::from_xywh(SHAFT_X - hr * rr, cy - rim_ry * rr, 2.0 * hr * rr, 2.0 * rim_ry * rr).and_then(PathBuilder::from_oval) {
            pm.stroke_path(&p, &solid([0, 0, 0], 0.18 * alpha * s * s), &stroke(0.3), t, None);
        }
    }
    if let Some(p) = Rect::from_xywh(-0.3, -0.14, 0.6, 0.28).and_then(PathBuilder::from_oval) {
        let at = unit.pre_concat(Transform::from_translate(0.38 * kx, 0.42 * ky)).pre_rotate(ky.atan2(kx).to_degrees() + 90.0);
        pm.fill_path(&p, &solid([255; 3], 0.45 * alpha * s), FillRule::Winding, t.pre_concat(at), None);
    }
    // the metal barrel over it, its end cut to fit against the head
    if let Some(p) = barrel() {
        pm.fill_path(&p, &shaded(across_x(r, metal(ky)), [0xa0; 3], alpha), FillRule::Winding, t, None);
    }
    // the dark rim of its opening, where the cord goes in
    let mut pb = PathBuilder::new();
    pb.move_to(0.35, -r * 0.8);
    pb.line_to(0.35, r * 0.8);
    if let Some(p) = pb.finish() {
        pm.stroke_path(&p, &solid([0x20, 0x22, 0x25], 0.6 * alpha), &Stroke { width: 0.5, ..Default::default() }, t, None);
    }
}

/// While a plug is in the hand, the jacks it may go into glow green and the one it is pulled to
/// gold (the others fade: see `Scene::jack_sprites`).
pub fn rings(pm: &mut Pixmap, rings: &[((f32, f32), bool)], pulse: f32, ts: Transform) {
    for &((x, y), hot) in rings {
        let (c, glow, r, w) = if hot { ([255, 200, 90], 0.75, 19.0, 3.5) } else { ([120, 235, 150], 0.3 + 0.25 * pulse, 17.5, 2.5) };
        // a soft halo around the nut (the jack itself stays clear), then a bright ring
        let halo = RadialGradient::new(Point::from_xy(x, y), Point::from_xy(x, y), r + 14.0, vec![GradientStop::new(0.0, rgba(c, 0.0)), GradientStop::new(0.48, rgba(c, 0.0)), GradientStop::new(0.62, rgba(c, glow)), GradientStop::new(1.0, rgba(c, 0.0))], SpreadMode::Pad, Transform::identity());
        if let (Some(p), Some(sh)) = (PathBuilder::from_circle(x, y, r + 14.0), halo) {
            pm.fill_path(&p, &Paint { shader: sh, anti_alias: true, ..Default::default() }, FillRule::Winding, ts, None);
        }
        if let Some(p) = PathBuilder::from_circle(x, y, r) {
            pm.stroke_path(&p, &solid(c, if hot { 0.95 } else { 0.55 + 0.35 * pulse }), &stroke(w), ts, None);
        }
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
        for e in [&c.a, &c.b] {
            side(pm, c, e, c.alpha, ts, true);
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
            for e in [&c.a, &c.b] {
                if (e.held || e.free) == air {
                    side(pm, c, e, c.alpha, ts, false);
                }
            }
        }
    }
}
