//! Draws the cords with tiny-skia (ported from docs/design/mockup).
//!
//! Everything is given in world units and drawn through `ts`, the world → pixels transform.

use resvg::tiny_skia::{Color, FillRule, GradientStop, LineCap, LineJoin, Paint, RadialGradient, Path, PathBuilder, Pixmap, Point, SpreadMode, Stroke, Transform};

use super::cord::Cord;

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

fn stroke(width: f32) -> Stroke {
    Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() }
}

/// The cord's centre line as a smooth curve through its points.
fn trace(c: &Cord, ox: f32, oy: f32) -> Option<Path> {
    // from one end to the other, through the middles between its points with the points as
    // control points, so it bends smoothly
    let r: Vec<(f32, f32)> = c.pts.iter().map(|p| (p.x + ox, p.y + oy)).collect();
    let n = r.len();
    let mut pb = PathBuilder::new();
    pb.move_to(r[0].0, r[0].1);
    for i in 1..n - 1 {
        let (mx, my) = ((r[i].0 + r[i + 1].0) / 2.0, (r[i].1 + r[i + 1].1) / 2.0);
        if i + 2 == n {
            pb.quad_to(r[i].0, r[i].1, r[n - 1].0, r[n - 1].1);
        } else {
            pb.quad_to(r[i].0, r[i].1, mx, my);
        }
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

/// While a cord's end is in the hand, the jacks it may go into glow green and the one it is pulled to
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

/// All cords: their shadows, then the cords. A cord ends straight in its jack, as in Reason.
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
}
