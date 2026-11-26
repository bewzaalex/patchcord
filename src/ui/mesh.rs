//! The cords as triangles, for the GPU to draw every frame at any zoom (see `gpu::Gpu::draw`):
//! nothing to rasterize on the CPU, so cords that swing, follow a device or carry light cost
//! next to nothing. They look as `paint` draws them (for pictures without a GPU): a soft shadow,
//! a dark edge, the cord, a highlight along it, and the light running in it.
//!
//! Each vertex holds its place in window pixels, where it is from the stroke's centre line
//! (`e`), the stroke's half width and how soft its edge is, and its colour (premultiplied). The
//! fragment shader turns the distance into coverage, so edges are smooth without multisampling.

use super::cord::Cord;
use super::paint::{cord_w, shade};
use super::scene::View;

/// Floats per vertex: position (px), offset from the centre line (px), colour (premultiplied
/// RGBA), half width (px), softness (px).
pub const VERTEX: usize = 10;

/// What to draw: `normal` blended over what is below, `glow` added to it (the lights).
#[derive(Default)]
pub struct Mesh {
    pub normal: Vec<f32>,
    pub glow: Vec<f32>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.glow.is_empty()
    }
}

type Rgba = [f32; 4];

fn rgba(c: [u8; 3], a: f32) -> Rgba {
    let a = a.clamp(0.0, 1.0);
    [c[0] as f32 / 255.0 * a, c[1] as f32 / 255.0 * a, c[2] as f32 / 255.0 * a, a]
}

fn vertex(out: &mut Vec<f32>, p: (f32, f32), e: (f32, f32), c: Rgba, half: f32, soft: f32) {
    out.extend_from_slice(&[p.0, p.1, e.0, e.1, c[0], c[1], c[2], c[3], half, soft]);
}

/// A line through `pts` (px), `half` wide each side of it, its edge `soft` px wide, with round
/// ends: a strip of quads along it and a half disc beyond each end.
fn stroke(out: &mut Vec<f32>, pts: &[(f32, f32)], half: f32, soft: f32, c: Rgba) {
    if pts.len() < 2 || c[3] < 0.004 {
        return;
    }
    // how far the geometry reaches: where the coverage falls to nothing
    let ext = half + soft / 2.0 + 0.5;
    let dir = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt();
        if l > 1e-4 { (dx / l, dy / l) } else { (1.0, 0.0) }
    };
    let n = pts.len();
    // at each point, the normal to the line there (between its two pieces) and how much longer
    // the offset has to be for the edges to stay `ext` from the line
    let side: Vec<((f32, f32), f32)> = (0..n)
        .map(|i| {
            let (a, b) = (dir(pts[i.saturating_sub(1)], pts[i]), dir(pts[i], pts[(i + 1).min(n - 1)]));
            let (a, b) = if i == 0 { (b, b) } else if i == n - 1 { (a, a) } else { (a, b) };
            let (tx, ty) = (a.0 + b.0, a.1 + b.1);
            let l = (tx * tx + ty * ty).sqrt();
            let t = if l > 1e-4 { (tx / l, ty / l) } else { b };
            let cos = (t.0 * b.0 + t.1 * b.1).max(0.5);
            ((-t.1, t.0), 1.0 / cos)
        })
        .collect();
    for i in 0..n - 1 {
        let (p, q) = (pts[i], pts[i + 1]);
        let ((pn, pm), (qn, qm)) = (side[i], side[i + 1]);
        let pl = (p.0 + pn.0 * ext * pm, p.1 + pn.1 * ext * pm);
        let pr = (p.0 - pn.0 * ext * pm, p.1 - pn.1 * ext * pm);
        let ql = (q.0 + qn.0 * ext * qm, q.1 + qn.1 * ext * qm);
        let qr = (q.0 - qn.0 * ext * qm, q.1 - qn.1 * ext * qm);
        for (v, e) in [(pl, ext), (pr, -ext), (ql, ext), (pr, -ext), (qr, -ext), (ql, ext)] {
            vertex(out, v, (0.0, e), c, half, soft);
        }
    }
    // the round ends: half discs out beyond the first and last points
    for (at, d) in [(pts[0], dir(pts[1], pts[0])), (pts[n - 1], dir(pts[n - 2], pts[n - 1]))] {
        let nrm = (-d.1, d.0);
        let corner = |along: f32, across: f32| (at.0 + d.0 * along + nrm.0 * across, at.1 + d.1 * along + nrm.1 * across);
        let quad = [(0.0, ext), (0.0, -ext), (ext, ext), (0.0, -ext), (ext, -ext), (ext, ext)];
        for (along, across) in quad {
            vertex(out, corner(along, across), (along, across), c, half, soft);
        }
    }
}

/// A plain line: its points (window pixels), its colour and opacity.
pub type Line = (Vec<(f32, f32)>, [u8; 3], f32);

/// Plain lines, each `half` px wide each side: ENGINE's link in the hand (`Engine::carried`).
pub fn lines(lines: &[Line], half: f32) -> Mesh {
    let mut m = Mesh::default();
    for (pts, c, a) in lines {
        stroke(&mut m.normal, pts, half, 1.0, rgba(*c, *a));
    }
    m
}

/// A round spot of light at `at` (px), `r` across, fading from the middle out.
fn spot(out: &mut Vec<f32>, at: (f32, f32), r: f32, c: Rgba) {
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        vertex(out, (at.0 + x * r, at.1 + y * r), (x * r, y * r), c, r / 2.0, r);
    }
}

/// The cord's centre line as `paint::trace` draws it (through the middles between its points,
/// with the points as control points), shifted by (ox, oy) world units, in window pixels.
fn line(c: &Cord, ox: f32, oy: f32, v: View) -> Vec<(f32, f32)> {
    let s = v.s as f32;
    let px = |x: f32, y: f32| ((x + ox) * s + v.x as f32, (y + oy) * s + v.y as f32);
    let r: Vec<(f32, f32)> = c.pts.iter().map(|p| px(p.x, p.y)).collect();
    let n = r.len();
    if n < 2 {
        return r;
    }
    // each quadratic piece in steps short enough to look curved at any zoom
    let steps = ((s * 2.0).ceil() as usize).clamp(3, 8);
    let mut out = vec![r[0]];
    let mut from = r[0];
    for i in 1..n - 1 {
        let ctl = r[i];
        let to = if i + 2 == n { r[n - 1] } else { ((r[i].0 + r[i + 1].0) / 2.0, (r[i].1 + r[i + 1].1) / 2.0) };
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let u = 1.0 - t;
            out.push((u * u * from.0 + 2.0 * u * t * ctl.0 + t * t * to.0, u * u * from.1 + 2.0 * u * t * ctl.1 + t * t * to.1));
        }
        from = to;
    }
    out
}

/// The cords, as `paint::cords` and `paint::flow` draw them: shadows under every cord, then the
/// cords, then the lights. `alpha_of`: each cord's opacity (fading, focus).
pub fn cords(cords: &[Cord], alpha_of: impl Fn(&Cord) -> f32, v: View) -> Mesh {
    let mut m = Mesh::default();
    let s = v.s as f32;
    let shown: Vec<(&Cord, f32)> = cords.iter().map(|c| (c, alpha_of(c))).filter(|(_, a)| *a > 0.01).collect();
    // the shadow: three soft strokes in `paint`, one softer one here
    for &(c, a) in &shown {
        stroke(&mut m.normal, &line(c, 7.0, 12.0, v), 2.8 * s, 3.5 * s, rgba([0, 0, 0], 0.26 * a));
    }
    for &(c, a) in &shown {
        let w = cord_w(c);
        let mid = line(c, 0.0, 0.0, v);
        stroke(&mut m.normal, &mid, (w + 2.2) / 2.0 * s, 1.0, rgba(shade(c.color, -0.62), a));
        stroke(&mut m.normal, &mid, w / 2.0 * s, 1.0, rgba(c.color, a));
        let lite = shade(c.color, if c.strain > 1.04 { 0.7 } else { 0.45 });
        stroke(&mut m.normal, &line(c, -w * 0.17, -w * 0.2, v), w * 0.15 * s, 1.0, rgba(lite, a * 0.75));
    }
    for &(c, a) in &shown {
        flow(&mut m.glow, c, a, v);
    }
    m
}

/// The light running in a cord from its output to its input (see `paint::flow`).
fn flow(out: &mut Vec<f32>, c: &Cord, a: f32, v: View) {
    let lvl = c.flow.min(1.0);
    if lvl < 0.02 || a < 0.05 || c.pts.len() < 2 {
        return;
    }
    let s = v.s as f32;
    let mut cum = vec![0.0f32];
    for w in c.pts.windows(2) {
        cum.push(cum.last().copied().unwrap_or(0.0) + (w[1].x - w[0].x).hypot(w[1].y - w[0].y));
    }
    let len = *cum.last().unwrap_or(&0.0);
    if len < 20.0 {
        return;
    }
    let at = |d: f32| {
        let i = cum.iter().position(|v| *v >= d).unwrap_or(cum.len() - 1).max(1);
        let k = ((d - cum[i - 1]) / (cum[i] - cum[i - 1]).max(1e-6)).clamp(0.0, 1.0);
        let (x, y) = (c.pts[i - 1].x + (c.pts[i].x - c.pts[i - 1].x) * k, c.pts[i - 1].y + (c.pts[i].y - c.pts[i - 1].y) * k);
        (x * s + v.x as f32, y * s + v.y as f32)
    };
    let head = c.flow_at * len;
    let tail = (len * 0.45).min(120.0);
    let w = cord_w(c);
    let (core, halo) = (shade(c.color, 0.55), shade(c.color, 0.3));
    let edge = (head / 30.0).min((len - head) / 30.0 + 0.15).clamp(0.0, 1.0);
    let bright = a * edge * 0.38 * lvl;
    // the tail: short pieces of the wire, dimmer the further back
    const N: usize = 14;
    for i in 0..N {
        let (s0, s1) = (head - tail * (i + 1) as f32 / N as f32, head - tail * i as f32 / N as f32);
        if s1 <= 0.0 {
            break;
        }
        let piece = [at(s0.max(0.0)), at(s1)];
        let f = (1.0 - i as f32 / N as f32).powf(1.6);
        stroke(out, &piece, w * 1.1 * s, 1.0, rgba(halo, bright * f * 0.25));
        stroke(out, &piece, w * 0.35 * s, 1.0, rgba(core, bright * f));
    }
    // the head: a soft glow round it
    spot(out, at(head), w * 1.9 * s, rgba(halo, 0.7 * bright));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight stroke covers its length and width, and its round ends reach past its points.
    #[test]
    fn a_stroke_covers_its_line_and_ends() {
        let mut out = Vec::new();
        stroke(&mut out, &[(10.0, 50.0), (110.0, 50.0)], 5.0, 1.0, [1.0, 0.0, 0.0, 1.0]);
        let xs: Vec<f32> = out.chunks(VERTEX).map(|v| v[0]).collect();
        let ys: Vec<f32> = out.chunks(VERTEX).map(|v| v[1]).collect();
        let (x0, x1) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
        let (y0, y1) = (ys.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max));
        assert!(x0 <= 5.0 && x1 >= 115.0, "the ends reach {x0}…{x1}");
        assert!(y0 <= 45.0 && y1 >= 55.0 && y0 > 40.0 && y1 < 60.0, "the width is {y0}…{y1}");
        assert_eq!(out.len() % (3 * VERTEX), 0, "whole triangles");
    }

    /// How long the cords' triangles take to make on the main thread (done every frame while
    /// anything moves), next to one step of their physics: 40 stereo cords carrying sound, at
    /// rest across a desk. Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn mesh_time() {
        use super::super::cord::Slot;
        use super::super::face::JackKind;
        let ends = |i: usize| ((80.0 + (i % 8) as f32 * 150.0, 200.0 + (i / 8) as f32 * 40.0), (200.0 + (i * 7 % 8) as f32 * 140.0, 700.0 + (i / 8) as f32 * 30.0));
        let mut cs: Vec<Cord> = (0..40)
            .map(|i| {
                let (a, b) = ends(i);
                let mut c = Cord::new(i as u64, (2 * i as u64, 2 * i as u64 + 1), (format!("a{i}"), JackKind::Stereo, a), (format!("b{i}"), JackKind::Stereo, b), [40, 200, 90]);
                (c.flow, c.flow_at) = (1.0, 0.4);
                c
            })
            .collect();
        let slot = |k: &str, _n: usize| {
            let i: usize = k[1..].parse().ok()?;
            let at = if k.starts_with('a') { ends(i).0 } else { ends(i).1 };
            Some(Slot { at, dev: at })
        };
        for _ in 0..300 {
            for c in &mut cs {
                c.frame(1.0 / 60.0, 2, &slot, None, None, false);
            }
        }
        let n = 200;
        let v = View { x: 0.0, y: 0.0, s: 1.0 };
        let t = std::time::Instant::now();
        let mut verts = 0;
        for _ in 0..n {
            let m = cords(&cs, |_| 1.0, v);
            verts = (m.normal.len() + m.glow.len()) / VERTEX;
        }
        let mesh = t.elapsed() / n;
        let t = std::time::Instant::now();
        for _ in 0..n {
            for c in &mut cs {
                c.frame(1.0 / 60.0, 2, &slot, None, None, false);
            }
        }
        let physics = t.elapsed() / n;
        println!("40 cords: mesh {mesh:?} a frame ({verts} vertices, {} KB), physics {physics:?} a frame", verts * VERTEX * 4 / 1024);
    }
}
