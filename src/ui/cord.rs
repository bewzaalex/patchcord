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
/// size of a plug in the hand
pub const PLUG: f32 = 1.7;
/// the boot runs this far back from the grip, tapering down to the cord (plug units)
pub const BOOT_TAIL: f32 = -12.0;
/// from where the cord leaves the boot to the middle of the grip
pub const HOLD: f32 = PLUG * (30.0 - BOOT_TAIL);

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
    /// 0 seated, 1 in the hand: the plug turns from showing its back to showing its side
    pub lift: f32,
    /// the way the tip points (radians)
    pub ang: f32,
    pub x: f32,
    pub y: f32,
    placed: bool,
    /// the jack it was pulled out of, while in the hand
    pub orig: Option<String>,
}

impl End {
    fn new(jack: Option<String>, kind: JackKind, slot: u64) -> Self {
        End { jack, held: false, free: false, slot, rank: 0, kind, ins: 0.0, lift: 0.0, ang: -std::f32::consts::FRAC_PI_2, x: 0.0, y: 0.0, placed: false, orig: None }
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
        let (p0, p1) = (a.2, b.2);
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
            if e.held
                && let Some(h) = held
            {
                // a plug in the hand turns its tip away from the cord, leaning with the movement
                let (p0, p3) = if end_b { (self.pts[N - 1], self.pts[N - 5]) } else { (self.pts[0], self.pts[4]) };
                let mut want = if (p0.x - p3.x).abs() + (p0.y - p3.y).abs() > 2.0 { (p0.y - p3.y).atan2(p0.x - p3.x) } else { e.ang };
                want += (h.vx * 0.0006).clamp(-0.5, 0.5);
                e.ang += wrap(want - e.ang) * 0.16;
                if let Some(k) = h.kind {
                    e.kind = k;
                }
                // the hand holds the grip; the cord leaves the boot behind it
                let l = HOLD * e.lift;
                e.x = h.x - e.ang.cos() * l;
                e.y = h.y - e.ang.sin() * l;
            } else if e.free {
                // a dropped plug turns its tip away from the cord
                let (p0, p3) = if end_b { (self.pts[N - 1], self.pts[N - 5]) } else { (self.pts[0], self.pts[4]) };
                let want = (p0.y - p3.y).atan2(p0.x - p3.x);
                e.ang += wrap(want - e.ang) * 0.16;
                e.x = p0.x;
                e.y = p0.y;
            } else if let Some(j) = &e.jack
                && !dying
                && let Some((x, y)) = slot_pos(j, e.rank)
            {
                // seated plugs glide when their mult closes up or opens
                if !e.placed || snap {
                    (e.x, e.y, e.placed) = (x, y, true);
                } else {
                    e.x += (x - e.x) * 0.3;
                    e.y += (y - e.y) * 0.3;
                }
            }
        }
        if !self.a.free && !self.b.free {
            let d = ((self.a.x - self.b.x).powi(2) + (self.a.y - self.b.y).powi(2)).sqrt();
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
            let (a, b) = if sane(self.a.x) && sane(self.a.y) && sane(self.b.x) && sane(self.b.y) { ((self.a.x, self.a.y), (self.b.x, self.b.y)) } else { ((0.0, 0.0), (0.0, 0.0)) };
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
        let (ax, ay, bx, by) = (self.a.x, self.a.y, self.b.x, self.b.y);
        // the cord leaves a plug in the hand straight out of its boot
        let ax_a = (self.a.held && self.a.lift > 0.3).then(|| (ax - self.a.ang.cos() * seg, ay - self.a.ang.sin() * seg));
        let ax_b = (self.b.held && self.b.lift > 0.3).then(|| (bx - self.b.ang.cos() * seg, by - self.b.ang.sin() * seg));
        let pin = |pts: &mut [P]| {
            if pin_a {
                (pts[0].x, pts[0].y) = (ax, ay);
            }
            if pin_b {
                (pts[n - 1].x, pts[n - 1].y) = (bx, by);
            }
            if let Some((x, y)) = ax_a {
                (pts[1].x, pts[1].y) = (x, y);
            }
            if let Some((x, y)) = ax_b {
                (pts[n - 2].x, pts[n - 2].y) = (x, y);
            }
            if let (Some(k), Some((hx, hy))) = (k, hand) {
                pts[k].x += (hx - pts[k].x) * 0.35;
                pts[k].y += (hy - pts[k].y) * 0.35;
            }
        };
        let w = |i: usize| if (i == 0 && pin_a) || (i == n - 1 && pin_b) || (i == 1 && ax_a.is_some()) || (i == n - 2 && ax_b.is_some()) || Some(i) == k { 0.0 } else { 1.0 };
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
            // a thick cord resists folding: keep every second point apart
            for i in 0..n - 2 {
                let (wa, wb) = (w(i), w(i + 2));
                if wa + wb == 0.0 {
                    continue;
                }
                let (p, q) = (self.pts[i], self.pts[i + 2]);
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let (d, min) = ((dx * dx + dy * dy).sqrt().max(1e-6), seg * 1.9);
                if d >= min {
                    continue;
                }
                let diff = (d - min) / d / (wa + wb) * STIFF;
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
