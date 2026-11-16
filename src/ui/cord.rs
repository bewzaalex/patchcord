//! A patch cord: a rope of points (Verlet integration) with a plug at each end.
//!
//! Distances are in world units (the panel's pixels at zoom 1). Ported from docs/design/mockup.

use super::face::JackKind;

/// points along a cord
pub const N: usize = 22;
const GRAV: f32 = 2000.0;
/// velocity kept per step: cords settle quickly instead of wobbling
const DAMP: f32 = 0.965;
const ITER: usize = 20;
/// how much a cord resists folding
const STIFF: f32 = 0.35;
/// physics steps per second
pub const RATE: f32 = 120.0;
/// A plug is drawn in millimetres: one millimetre is this many world units. A jack's hole on a
/// face is 11 units across and stands for the 6.3 mm opening, so the shaft fits it exactly.
pub const PLUG: f32 = 11.0 / 6.3;
/// A right-angle 6.3 mm plug, in millimetres: two barrels joined at a right angle. The metal
/// barrel runs along x from where the cord goes in (0) to the head; the head is the second barrel,
/// a little thicker, in the cord's colour, standing across it with its axis along y and a rounded
/// back; the shaft comes out of the middle of its front. Overall 46 from the head's back to the
/// tip, as a standard plug.
pub const BARREL_R: f32 = 5.0;
pub const HEAD_R: f32 = 6.5;
pub const HEAD_X0: f32 = 20.0;
pub const SHAFT_X: f32 = HEAD_X0 + HEAD_R;
pub const HEAD_X1: f32 = SHAFT_X + HEAD_R;
pub const HEAD_TOP: f32 = -8.0;
pub const HEAD_BOTTOM: f32 = 7.0;
/// the shaft: 6.3 thick, reaching 31.2 out of the head
pub const SHAFT_R: f32 = 3.15;
pub const SHAFT_LEN: f32 = 31.2;
/// the cord comes out of the barrel straight for this long before it bends, as from a strain relief
pub const STRAIN: f32 = 8.0;
const STRAIN_W: f32 = STRAIN * PLUG;
/// the hand holds the plug by its head, over the shaft: this far from where the cord leaves it
pub const HOLD: f32 = PLUG * SHAFT_X;

#[derive(Clone, Copy, Default, Debug)]
pub struct P {
    pub x: f32,
    pub y: f32,
    px: f32,
    py: f32,
}

impl P {
    fn at(x: f32, y: f32) -> Self {
        P { x, y, px: x, py: y }
    }
}

#[derive(Clone, Debug)]
pub struct End {
    /// the jack it is plugged into
    pub jack: Option<String>,
    /// in the hand
    pub held: bool,
    /// dropped: falls with the cord
    pub free: bool,
    /// order in the jack's mult: the earliest plugged in sits first
    pub slot: u64,
    pub rank: usize,
    pub kind: JackKind,
    /// a little bump when it goes in, fading
    pub ins: f32,
    /// 0 seated, 1 in the hand: how much of the shaft shows (in a jack it is inside the panel)
    pub lift: f32,
    /// the way the barrel points, from where the cord leaves it towards the head (radians)
    pub ang: f32,
    /// the head, over the shaft: on the jack when seated, in the hand when held
    pub x: f32,
    pub y: f32,
    /// where the cord leaves the barrel, and where it is free to bend, a little further out
    pub cx: f32,
    pub cy: f32,
    pub sx: f32,
    pub sy: f32,
    placed: bool,
    /// the jack it was pulled out of, while in the hand
    pub orig: Option<String>,
}

impl End {
    fn new(jack: Option<String>, kind: JackKind, slot: u64) -> Self {
        End { jack, held: false, free: false, slot, rank: 0, kind, ins: 0.0, lift: 0.0, ang: -std::f32::consts::FRAC_PI_2, x: 0.0, y: 0.0, cx: 0.0, cy: 0.0, sx: 0.0, sy: 0.0, placed: false, orig: None }
    }
    pub fn seated(&self) -> bool {
        self.jack.is_some() && !self.held && !self.free
    }
}

pub struct Cord {
    pub seq: u64,
    /// the output end
    pub a: End,
    /// the input end
    pub b: End,
    pub color: [u8; 3],
    pub pts: Vec<P>,
    /// rest length
    pub len: f32,
    /// seconds since it was pulled out (it falls and fades)
    pub dying: Option<f32>,
    pub alpha: f32,
    /// the point held by the hand when the cord itself is grabbed
    pub grab: Option<usize>,
    pub strain: f32,
    /// seconds to wait for PipeWire to confirm the links of a cord just plugged in
    pub pending: f32,
}

/// The hand holding a plug: where, how fast it moves sideways, and how far the plug has turned
/// to face the panel (1 in the air, 0 when it is about to go in).
#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub lift: f32,
    pub kind: Option<JackKind>,
}

pub fn target_len(d: f32, sag: f32) -> f32 {
    d * (1.03 + 0.2 * sag) + 30.0 + 140.0 * sag
}

impl Cord {
    /// A cord plugged in at both ends, starting on a gentle curve so it settles instead of dropping.
    pub fn new(seq: u64, slots: (u64, u64), a: (String, JackKind, (f32, f32)), b: (String, JackKind, (f32, f32)), color: [u8; 3], sag: f32) -> Self {
        // the barrels hang down from the jacks at first: the cord starts below them
        let (p0, p1) = ((a.2.0, a.2.1 + HOLD + STRAIN_W), (b.2.0, b.2.1 + HOLD + STRAIN_W));
        let d = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
        let pts = (0..N)
            .map(|i| {
                let t = i as f32 / (N - 1) as f32;
                let sag = (t * std::f32::consts::PI).sin() * (d * 0.25).min(120.0);
                P::at(p0.0 + (p1.0 - p0.0) * t, p0.1 + (p1.1 - p0.1) * t + sag)
            })
            .collect();
        Cord {
            seq,
            a: End::new(Some(a.0), a.1, slots.0),
            b: End::new(Some(b.0), b.1, slots.1),
            color,
            pts,
            len: target_len(d, sag),
            dying: None,
            alpha: 1.0,
            grab: None,
            strain: 1.0,
            pending: 0.0,
        }
    }

    /// A cord just pulled out of a jack: one end plugged in, the other in the hand.
    pub fn fresh(seq: u64, slot: u64, jack: String, kind: JackKind, at: (f32, f32), color: [u8; 3], output: bool) -> Self {
        let mut c = Cord::new(seq, (slot, slot), (jack.clone(), kind, at), (jack, kind, at), color, 0.0);
        c.len = 30.0;
        let held = if output { &mut c.b } else { &mut c.a };
        held.jack = None;
        held.held = true;
        (held.x, held.y) = at;
        c
    }

    pub fn live(&self) -> bool {
        self.dying.is_none() && self.a.jack.is_some() && self.b.jack.is_some()
    }

    pub fn stereo(&self) -> bool {
        self.a.kind == JackKind::Stereo || self.b.kind == JackKind::Stereo
    }

    /// Pulls the cord out: the given end drops and the cord fades away.
    pub fn kill(&mut self, b_end: bool) {
        self.kill_ends(!b_end, b_end);
    }

    /// The cord fades away with these ends dropping (both, when what they were in is gone).
    pub fn kill_ends(&mut self, a: bool, b: bool) {
        if self.dying.is_some() {
            return;
        }
        self.dying = Some(0.0);
        for (drop, e) in [(a, &mut self.a), (b, &mut self.b)] {
            if drop {
                e.free = true;
                e.held = false;
            }
        }
    }

    /// True once it has faded out.
    pub fn gone(&self) -> bool {
        self.dying.is_some_and(|t| t > 1.1)
    }

    /// Moving enough to need another frame.
    pub fn moving(&self) -> bool {
        self.dying.is_some() || self.grab.is_some() || self.a.held || self.b.held || self.a.ins > 0.01 || self.b.ins > 0.01 || self.pts.iter().any(|p| (p.x - p.px).abs() + (p.y - p.py).abs() > 0.02)
    }

    /// One frame: `dt` seconds. `slot_pos` finds where a jack's n-th slot is; `snap` puts seated
    /// plugs straight in place (while a device is dragged) instead of gliding there.
    pub fn frame(&mut self, dt: f32, steps: usize, slot_pos: &impl Fn(&str, usize) -> Option<(f32, f32)>, hand: Option<(f32, f32)>, held: Option<Held>, sag: f32, snap: bool) {
        self.pending = (self.pending - dt).max(0.0);
        let dying = self.dying.is_some();
        for (end_b, e) in [(false, &mut self.a), (true, &mut self.b)] {
            let lift = if e.held { held.map_or(1.0, |h| h.lift) } else if e.free { 1.0 } else { 0.0 };
            e.lift += (lift - e.lift) * 0.22;
            e.ins *= 0.82;
            // the cord near the plug: its end, and a point further in that shows where it pulls
            // (close to the plug when it is in a jack, where the cord turns the barrel directly)
            let k = if e.held || e.free { 6 } else { 2 };
            let (p0, p3) = if end_b { (self.pts[N - 1], self.pts[N - 1 - k]) } else { (self.pts[0], self.pts[k]) };
            // the barrel turns towards where the cord pulls, like a right-angle plug in a jack or a hand
            let toward = |hx: f32, hy: f32, ang: f32| {
                if (hx - p3.x).abs() + (hy - p3.y).abs() > 2.0 { (hy - p3.y).atan2(hx - p3.x) } else { ang }
            };
            if e.held
                && let Some(h) = held
            {
                // held by the head; the barrel trails towards the cord, leaning with the movement
                (e.x, e.y) = (h.x, h.y);
                let want = toward(e.x, e.y, e.ang) + (h.vx * 0.0006).clamp(-0.5, 0.5);
                e.ang += wrap(want - e.ang) * 0.25;
                if let Some(k) = h.kind {
                    e.kind = k;
                }
            } else if e.free {
                // a dropped plug falls with the cord's end, its barrel in line with the cord
                let want = (p0.y - p3.y).atan2(p0.x - p3.x);
                e.ang += wrap(want - e.ang) * 0.16;
                let (u, v) = (e.ang.cos(), e.ang.sin());
                (e.sx, e.sy) = (p0.x, p0.y);
                (e.cx, e.cy) = (p0.x + u * STRAIN_W, p0.y + v * STRAIN_W);
                (e.x, e.y) = (e.cx + u * HOLD, e.cy + v * HOLD);
                continue;
            } else if let Some(j) = &e.jack
                && !dying
                && let Some((x, y)) = slot_pos(j, e.rank)
            {
                // seated plugs glide when their mult closes up or opens, and turn in the jack
                if !e.placed || snap {
                    (e.x, e.y, e.placed) = (x, y, true);
                } else {
                    e.x += (x - e.x) * 0.3;
                    e.y += (y - e.y) * 0.3;
                }
                let want = toward(e.x, e.y, e.ang);
                e.ang += wrap(want - e.ang) * 0.35;
            }
            // the cord leaves the barrel's end, behind the head, and runs straight a little way
            let (u, v) = (e.ang.cos(), e.ang.sin());
            (e.cx, e.cy) = (e.x - u * HOLD, e.y - v * HOLD);
            (e.sx, e.sy) = (e.cx - u * STRAIN_W, e.cy - v * STRAIN_W);
        }
        if !self.a.free && !self.b.free {
            let d = ((self.a.sx - self.b.sx).powi(2) + (self.a.sy - self.b.sy).powi(2)).sqrt();
            let in_hand = self.a.held || self.b.held;
            // in the hand the cord pays out as it is pulled, a little behind: it stretches on a quick pull
            let t = if in_hand { (d + 60.0 + 60.0 * sag).max(90.0) } else { target_len(d, sag) };
            self.len += (t - self.len) * if t > self.len { if in_hand { 0.12 } else { 0.05 } } else { 0.03 };
        }
        for _ in 0..steps {
            self.step(1.0 / RATE, hand);
        }
        // a safety net: a cord whose numbers went wrong starts over as a straight line
        let sane = |v: f32| v.is_finite() && v.abs() < 1e6;
        if !self.pts.iter().all(|p| sane(p.x) && sane(p.y)) || !sane(self.len) {
            let (a, b) = if sane(self.a.sx) && sane(self.a.sy) && sane(self.b.sx) && sane(self.b.sy) { ((self.a.sx, self.a.sy), (self.b.sx, self.b.sy)) } else { ((0.0, 0.0), (0.0, 0.0)) };
            for (i, p) in self.pts.iter_mut().enumerate() {
                let t = i as f32 / (N - 1) as f32;
                *p = P::at(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            }
            self.len = target_len(((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt(), sag);
        }
        let len: f32 = self.pts.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).sum();
        self.strain = len / self.len;
        if let Some(t) = &mut self.dying {
            *t += dt;
            self.alpha = (1.0 - (*t - 0.65) / 0.45).clamp(0.0, 1.0);
        }
    }

    fn step(&mut self, dt: f32, hand: Option<(f32, f32)>) {
        let n = N;
        let g = GRAV * dt * dt;
        let seg = self.len / (n - 1) as f32;
        for p in &mut self.pts {
            let (vx, vy) = ((p.x - p.px) * DAMP, (p.y - p.py) * DAMP);
            (p.px, p.py) = (p.x, p.y);
            p.x += vx;
            p.y += vy + g;
        }
        let (pin_a, pin_b, k) = (!self.a.free, !self.b.free, self.grab);
        let (ax, ay, bx, by) = (self.a.sx, self.a.sy, self.b.sx, self.b.sy);
        // The cord goes on in line with the barrel. In the hand that is firm (the hand holds the
        // plug's angle); in a jack it is a gentle pull, since the plug turns there and the cord
        // turns it: neither can lock the other into a kink.
        let ax_a = pin_a.then(|| (ax - self.a.ang.cos() * seg, ay - self.a.ang.sin() * seg, self.a.held));
        let ax_b = pin_b.then(|| (bx - self.b.ang.cos() * seg, by - self.b.ang.sin() * seg, self.b.held));
        let pin = |pts: &mut [P]| {
            if pin_a {
                (pts[0].x, pts[0].y) = (ax, ay);
            }
            if pin_b {
                (pts[n - 1].x, pts[n - 1].y) = (bx, by);
            }
            for (i, axis) in [(1, ax_a), (n - 2, ax_b)] {
                if let Some((x, y, firm)) = axis {
                    let pull = if firm { 1.0 } else { 0.3 };
                    pts[i].x += (x - pts[i].x) * pull;
                    pts[i].y += (y - pts[i].y) * pull;
                }
            }
            if let (Some(k), Some((hx, hy))) = (k, hand) {
                pts[k].x += (hx - pts[k].x) * 0.35;
                pts[k].y += (hy - pts[k].y) * 0.35;
            }
        };
        let firm = |axis: Option<(f32, f32, bool)>| axis.is_some_and(|a| a.2);
        let w = |i: usize| if (i == 0 && pin_a) || (i == n - 1 && pin_b) || (i == 1 && firm(ax_a)) || (i == n - 2 && firm(ax_b)) || Some(i) == k { 0.0 } else { 1.0 };
        pin(&mut self.pts);
        for _ in 0..ITER {
            for i in 0..n - 1 {
                let (wa, wb) = (w(i), w(i + 1));
                if wa + wb == 0.0 {
                    continue;
                }
                let (p, q) = (self.pts[i], self.pts[i + 1]);
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let d = (dx * dx + dy * dy).sqrt().max(1e-6);
                let diff = (d - seg) / d / (wa + wb);
                self.pts[i].x += dx * diff * wa;
                self.pts[i].y += dy * diff * wa;
                self.pts[i + 1].x -= dx * diff * wb;
                self.pts[i + 1].y -= dy * diff * wb;
            }
            // a thick cord resists folding: keep every second point apart; near the plugs, where a
            // real cord is stiffer, almost straight
            for i in 0..n - 2 {
                let (wa, wb) = (w(i), w(i + 2));
                if wa + wb == 0.0 {
                    continue;
                }
                let near = i < 3 || i + 2 > n - 4;
                let (p, q) = (self.pts[i], self.pts[i + 2]);
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let (d, min) = ((dx * dx + dy * dy).sqrt().max(1e-6), seg * if near { 1.96 } else { 1.9 });
                if d >= min {
                    continue;
                }
                let diff = (d - min) / d / (wa + wb) * if near { 0.7 } else { STIFF };
                self.pts[i].x += dx * diff * wa;
                self.pts[i].y += dy * diff * wa;
                self.pts[i + 2].x -= dx * diff * wb;
                self.pts[i + 2].y -= dy * diff * wb;
            }
            pin(&mut self.pts);
        }
    }

    /// Distance from (x, y) to the cord, and the nearest inner point.
    pub fn hit(&self, x: f32, y: f32) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, w) in self.pts.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let l2 = (dx * dx + dy * dy).max(1e-6);
            let t = (((x - a.x) * dx + (y - a.y) * dy) / l2).clamp(0.0, 1.0);
            let d = ((x - a.x - dx * t).powi(2) + (y - a.y - dy * t).powi(2)).sqrt();
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, (i + t.round() as usize).clamp(1, N - 2)));
            }
        }
        best
    }
}

pub fn wrap(a: f32) -> f32 {
    a.sin().atan2(a.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How badly the cord bends back where it leaves a plug in a jack, after settling: the cosine
    /// between the barrel (pointing out, away from the head) and the cord's first stretch.
    fn settle(ja: (f32, f32), jb: (f32, f32), start: f32) -> f32 {
        let slot = move |k: &str, _n: usize| Some(if k == "a" { ja } else { jb });
        let mut c = Cord::new(1, (1, 2), ("a".into(), JackKind::Stereo, ja), ("b".into(), JackKind::Stereo, jb), [0, 200, 0], 0.45);
        c.a.ang = start;
        for _ in 0..180 {
            c.frame(1.0 / 60.0, 2, &slot, None, None, 0.45, false);
        }
        let (bx, by) = (c.a.ang.cos(), c.a.ang.sin());
        let (p0, p2) = (c.pts[0], c.pts[2]);
        let (dx, dy) = (p2.x - p0.x, p2.y - p0.y);
        (dx * -bx + dy * -by) / (dx * dx + dy * dy).sqrt()
    }

    /// How far the cord turns, in degrees, over its first stretch out of the plug after the
    /// plug's device was dragged along `path` and the cord let settle.
    fn turn_after_drag(path: &[(f32, f32)], jb: (f32, f32)) -> f32 {
        let pos = std::cell::Cell::new(path[0]);
        let slot = |k: &str, _n: usize| Some(if k == "a" { pos.get() } else { jb });
        let mut c = Cord::new(1, (1, 2), ("a".into(), JackKind::Stereo, path[0]), ("b".into(), JackKind::Stereo, jb), [0, 200, 0], 0.45);
        for w in path.windows(2) {
            for i in 0..20 {
                let t = i as f32 / 20.0;
                pos.set((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
                c.frame(1.0 / 60.0, 2, &slot, None, None, 0.45, true);
            }
        }
        for _ in 0..240 {
            c.frame(1.0 / 60.0, 2, &slot, None, None, 0.45, false);
        }
        let mut total = 0.0;
        for i in 0..8 {
            let (p, q, r) = (c.pts[i], c.pts[i + 1], c.pts[i + 2]);
            let (a1, a2) = ((q.y - p.y).atan2(q.x - p.x), (r.y - q.y).atan2(r.x - q.x));
            total += wrap(a2 - a1).abs();
        }
        // and the bend between the barrel and the cord's first stretch
        let (bx, by) = (-c.a.ang.cos(), -c.a.ang.sin());
        let (p0, p1) = (c.pts[0], c.pts[1]);
        total += wrap((p1.y - p0.y).atan2(p1.x - p0.x) - by.atan2(bx)).abs();
        total.to_degrees()
    }

    /// Dragging a device about, over and around its cord, never leaves a hairpin where the cord
    /// comes out of the plug.
    #[test]
    fn no_hairpin_after_dragging() {
        let jb = (100.0, 400.0);
        let paths: [&[(f32, f32)]; 4] = [
            &[(700.0, 400.0), (300.0, 150.0), (500.0, 100.0)],
            &[(700.0, 400.0), (200.0, 600.0), (600.0, 200.0)],
            &[(400.0, 300.0), (50.0, 200.0), (300.0, 250.0)],
            &[(700.0, 500.0), (700.0, 100.0), (400.0, 380.0)],
        ];
        for path in paths {
            let deg = turn_after_drag(path, jb);
            assert!(deg < 120.0, "the cord turns {deg:.0}° right out of the plug after dragging along {path:?}");
        }
    }

    /// A cord that leaves a plug in a jack with a tight U-turn (as after swinging it about)
    /// straightens out once it settles.
    #[test]
    fn a_loop_by_the_plug_comes_out() {
        let (ja, jb) = ((600.0, 300.0), (100.0, 420.0));
        let slot = move |k: &str, _n: usize| Some(if k == "a" { ja } else { jb });
        let mut c = Cord::new(1, (1, 2), ("a".into(), JackKind::Stereo, ja), ("b".into(), JackKind::Stereo, jb), [0, 200, 0], 0.45);
        for _ in 0..5 {
            c.frame(1.0 / 60.0, 2, &slot, None, None, 0.45, false);
        }
        // bend the cord's first stretch into a U: out to the right, round, and back to the left
        let (sx, sy) = (c.pts[0].x, c.pts[0].y);
        for i in 1..9 {
            let t = i as f32 / 8.0 * std::f32::consts::PI;
            let (x, y) = (sx + 40.0 * t.sin(), sy + 40.0 - 40.0 * t.cos());
            c.pts[i] = P { x, y, px: x, py: y };
        }
        for _ in 0..240 {
            c.frame(1.0 / 60.0, 2, &slot, None, None, 0.45, false);
        }
        let mut total = 0.0;
        for i in 0..8 {
            let (p, q, r) = (c.pts[i], c.pts[i + 1], c.pts[i + 2]);
            total += wrap((r.y - q.y).atan2(r.x - q.x) - (q.y - p.y).atan2(q.x - p.x)).abs();
        }
        assert!(total.to_degrees() < 120.0, "the cord still turns {:.0}° by the plug", total.to_degrees());
    }

    /// A plug in a jack, its barrel starting at any angle, with the cord's other end anywhere
    /// around it, ends up with the cord leaving its barrel without a sharp bend back.
    #[test]
    fn a_seated_plug_turns_towards_its_cord() {
        let ja = (600.0, 300.0);
        let mut worst = (2.0f32, (0.0, 0.0), 0.0);
        for (dx, dy) in [(-500.0, 0.0), (-500.0, -250.0), (-500.0, 250.0), (-150.0, -200.0), (-150.0, 0.0), (0.0, -300.0), (300.0, -100.0), (-900.0, -150.0)] {
            for k in 0..8 {
                let start = k as f32 * std::f32::consts::FRAC_PI_4;
                let cos = settle(ja, (ja.0 + dx, ja.1 + dy), start);
                if cos < worst.0 {
                    worst = (cos, (dx, dy), start);
                }
            }
        }
        assert!(worst.0 > 0.34, "cord bends back out of the barrel: cos {:.2} with the other end at {:?}, barrel starting at {:.2}", worst.0, worst.1, worst.2);
    }
}
