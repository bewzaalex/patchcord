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

/// The jacks listened to (cue): a cyan ring with a little pair of headphones on a badge; a
/// pinned one also glows (it plays on when the pointer leaves); one that would feed back is red.
pub fn cue_marks(pm: &mut Pixmap, marks: &[((f32, f32), bool, bool)], pulse: f32, ts: Transform) {
    for &((x, y), pinned, bad) in marks {
        let c: Rgb = if bad { [255, 80, 60] } else { [95, 212, 255] };
        if pinned {
            let glow = RadialGradient::new(Point::from_xy(x, y), Point::from_xy(x, y), 34.0, vec![GradientStop::new(0.0, rgba(c, 0.0)), GradientStop::new(0.4, rgba(c, 0.45)), GradientStop::new(1.0, rgba(c, 0.0))], SpreadMode::Pad, Transform::identity());
            if let (Some(p), Some(sh)) = (PathBuilder::from_circle(x, y, 34.0), glow) {
                pm.fill_path(&p, &Paint { shader: sh, anti_alias: true, ..Default::default() }, FillRule::Winding, ts, None);
            }
        } else if let Some(p) = PathBuilder::from_circle(x, y, 26.0) {
            pm.stroke_path(&p, &solid(c, 0.18 * (0.6 + 0.4 * pulse)), &stroke(8.0), ts, None);
        }
        if let Some(p) = PathBuilder::from_circle(x, y, 20.0) {
            pm.stroke_path(&p, &solid(c, 0.95), &stroke(if pinned { 3.2 } else { 2.2 }), ts, None);
        }
        // the badge, top right of the jack, with the same headphones as the CUE key
        let (bx, by) = (x + 17.0, y - 17.0);
        if let Some(p) = PathBuilder::from_circle(bx, by, 8.5) {
            pm.fill_path(&p, &solid([18, 19, 21], 1.0), FillRule::Winding, ts, None);
            pm.stroke_path(&p, &solid(c, 1.0), &stroke(1.4), ts, None);
        }
        // 24 units of the icon to 11 world units, its shape's middle (12, 12.5) on the badge's centre
        let k = 11.0 / 24.0;
        let at = |ux: f32, uy: f32| (bx + (ux - 12.0) * k, by + (uy - 12.5) * k);
        let mut band = PathBuilder::new();
        let (sx, sy) = at(4.0, 15.0);
        band.move_to(sx, sy);
        let (ax, ay) = at(4.0, 12.0);
        band.line_to(ax, ay);
        // the headband: half a circle of radius 8 from (4, 12) to (20, 12) over (12, 4)
        let (c1x, c1y) = at(4.0, 7.6);
        let (m1x, m1y) = at(7.6, 4.0);
        let (mx, my) = at(12.0, 4.0);
        band.cubic_to(c1x, c1y, m1x, m1y, mx, my);
        let (m2x, m2y) = at(16.4, 4.0);
        let (c2x, c2y) = at(20.0, 7.6);
        let (bx2, by2) = at(20.0, 12.0);
        band.cubic_to(m2x, m2y, c2x, c2y, bx2, by2);
        let (ex, ey) = at(20.0, 15.0);
        band.line_to(ex, ey);
        if let Some(p) = band.finish() {
            pm.stroke_path(&p, &solid(c, 1.0), &stroke(2.4 * k), ts, None);
        }
        for ux in [3.0, 16.5] {
            let (cx, cy) = at(ux, 14.0);
            if let Some(r) = resvg::tiny_skia::Rect::from_xywh(cx, cy, 4.5 * k, 7.0 * k) {
                pm.fill_path(&PathBuilder::from_rect(r), &solid(c, 1.0), FillRule::Winding, ts, None);
            }
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
