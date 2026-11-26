//! Draws the cords with tiny-skia (ported from docs/design/mockup).
//!
//! Everything is given in world units and drawn through `ts`, the world → pixels transform.

use resvg::tiny_skia::{BlendMode, Color, FillRule, GradientStop, LineCap, LineJoin, Paint, RadialGradient, Path, PathBuilder, Pixmap, Point, SpreadMode, Stroke, Transform};

use super::cord::Cord;

type Rgb = [u8; 3];

/// The colour lighter (f > 0, towards white) or darker (f < 0, towards black).
pub(super) fn shade(c: Rgb, f: f32) -> Rgb {
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

/// A cord's thickness: every cord alike, stereo or mono (2026-11-26: they were 7.6 and 6.6, which
/// looked like macaroni); thinner as it is pulled taut.
pub(super) fn cord_w(c: &Cord) -> f32 {
    5.0 / c.strain.max(1.0).sqrt()
}

pub fn shadow(pm: &mut Pixmap, c: &Cord, alpha: f32, ts: Transform) {
    let Some(path) = trace(c, 7.0, 12.0) else { return };
    // (as wide round the cord as it was round the thicker one)
    for (w, a) in [(9.0, 0.07), (6.3, 0.1), (4.2, 0.12)] {
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

/// Which way each cord goes: a light runs inside it from the output to the input, one at a time, a
/// glow in the wire with a fading tail; as it reaches the jack the next leaves the plug. (`flow`
/// fades it in and out, `flow_at` is where it is: see Cord.)
pub fn flow(pm: &mut Pixmap, cords: &[Cord], alpha_of: impl Fn(&Cord) -> f32, ts: Transform) {
    for c in cords {
        let a = alpha_of(c);
        let lvl = c.flow.min(1.0);
        if lvl < 0.02 || a < 0.05 || c.pts.len() < 2 {
            continue;
        }
        // the cord's length point by point, from the output end
        let mut cum = vec![0.0f32];
        for w in c.pts.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + (w[1].x - w[0].x).hypot(w[1].y - w[0].y));
        }
        let len = *cum.last().unwrap_or(&0.0);
        if len < 20.0 {
            continue;
        }
        let at = |s: f32| {
            let i = cum.iter().position(|v| *v >= s).unwrap_or(cum.len() - 1).max(1);
            let k = ((s - cum[i - 1]) / (cum[i] - cum[i - 1]).max(1e-6)).clamp(0.0, 1.0);
            (c.pts[i - 1].x + (c.pts[i].x - c.pts[i - 1].x) * k, c.pts[i - 1].y + (c.pts[i].y - c.pts[i - 1].y) * k)
        };
        let head = c.flow_at * len;
        let tail = (len * 0.45).min(120.0);
        let w = cord_w(c);
        let (core, halo) = (shade(c.color, 0.55), shade(c.color, 0.3));
        // fading in leaving the plug, out reaching the jack
        let edge = (head / 30.0).min((len - head) / 30.0 + 0.15).clamp(0.0, 1.0);
        let bright = a * edge * 0.38 * lvl;
        // the tail: short pieces of the wire, dimmer the further back
        const N: usize = 14;
        for i in 0..N {
            let (s0, s1) = (head - tail * (i + 1) as f32 / N as f32, head - tail * i as f32 / N as f32);
            if s1 <= 0.0 {
                break;
            }
            let (p0, p1) = (at(s0.max(0.0)), at(s1));
            let f = (1.0 - i as f32 / N as f32).powf(1.6);
            let mut pb = PathBuilder::new();
            pb.move_to(p0.0, p0.1);
            pb.line_to(p1.0, p1.1);
            let Some(path) = pb.finish() else { continue };
            for (col, width, al) in [(halo, w * 2.2, bright * f * 0.25), (core, w * 0.7, bright * f)] {
                let mut p = solid(col, al);
                p.blend_mode = BlendMode::Plus;
                pm.stroke_path(&path, &p, &stroke(width), ts, None);
            }
        }
        // the head: a soft glow round it
        let (hx, hy) = at(head);
        let r = w * 1.9;
        if let Some(sh) = RadialGradient::new(Point::from_xy(hx, hy), Point::from_xy(hx, hy), r, vec![GradientStop::new(0.0, rgba(halo, 0.7 * bright)), GradientStop::new(1.0, rgba(halo, 0.0))], SpreadMode::Pad, Transform::identity())
            && let Some(circle) = PathBuilder::from_circle(hx, hy, r)
        {
            let p = Paint { shader: sh, anti_alias: true, blend_mode: BlendMode::Plus, ..Default::default() };
            pm.fill_path(&circle, &p, FillRule::Winding, ts, None);
        }
    }
}
