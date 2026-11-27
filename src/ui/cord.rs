//! A patch cord, as in Reason: a rope of points (Verlet integration) hanging between the two
//! jacks it is plugged into, its ends straight in the jacks. No plugs stand out of the panel to
//! turn, so a cord just hangs: it swings, sags and settles.
//!
//! Distances are in world units (the panel's pixels at zoom 1).

use super::ease;

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
/// putting a cord at rest: steps, and the velocity kept per step (much less than swinging)
const REST_STEPS: usize = 200;
const REST_DAMP: f32 = 0.9;
/// how far a jack moves on its device in one frame to count as a jump (its mult closed up or
/// opened, the desk became the rack); a device gliding along carries its jacks without a jump
const JUMP: f32 = 12.0;

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

/// Where a cord's end goes: its jack's slot, and the corner of the jack's device.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub at: (f32, f32),
    pub dev: (f32, f32),
}

/// One end of a cord: in a jack, in the hand, or dropped.
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
    /// where the cord ends: on the jack, in the hand, or where it fell
    pub x: f32,
    pub y: f32,
    /// placed on its jack at least once (after that it glides when the jack moves)
    placed: bool,
    /// where its jack sits on its device (the jack less the device's corner), to tell a jack that
    /// jumps from one carried along
    rel: (f32, f32),
    /// the jack it was pulled out of, while in the hand
    pub orig: Option<String>,
}

impl End {
    fn new(jack: Option<String>, slot: u64) -> Self {
        End { jack, held: false, free: false, slot, rank: 0, x: 0.0, y: 0.0, placed: false, rel: (0.0, 0.0), orig: None }
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
    /// how far apart its ends were last frame
    span: f32,
    /// the signal it carries (0…1, eased) and where its light is on the way across (0…1)
    pub flow: f32,
    pub flow_at: f32,
    /// to be put at rest where it hangs before it is next drawn (just plugged in, or its jacks
    /// jumped): it shows in its place, without swinging into it
    settle: bool,
}

/// The hand holding a cord's end: where.
#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub x: f32,
    pub y: f32,
}

/// How long a cord between two points (`dx` apart across, `dy` up or down) is: a third longer
/// than the distance, as in Reason, so it hangs in a deep, calm U, and drops out of its jacks
/// rather than running sideways over the jacks beside them; a little more between jacks side by
/// side.
pub fn target_len(dx: f32, dy: f32) -> f32 {
    let (across, d) = (dx.abs(), (dx * dx + dy * dy).sqrt());
    d * 1.3 + (30.0f32).min(across * 0.4)
}

/// The curve a cord `len` long hangs in between `p0` and `p1`, as N points: a curve h·sin(πt)
/// over a span d is about d + (πh)²/(4d) long; it bulges across the span, on its lower side (down
/// between points side by side, sideways between points one above the other, where bulging down
/// would fold it back along itself).
fn hang(p0: (f32, f32), p1: (f32, f32), len: f32) -> Vec<(f32, f32)> {
    let d = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
    let deep = ((len - d).max(0.0) * 4.0 * d).sqrt() / std::f32::consts::PI;
    let (nx, ny) = if d > 1.0 { ((p1.1 - p0.1) / d, -(p1.0 - p0.0) / d) } else { (0.0, 1.0) };
    let (nx, ny) = if ny < 0.0 || (ny == 0.0 && nx < 0.0) { (-nx, -ny) } else { (nx, ny) };
    (0..N)
        .map(|i| {
            let t = i as f32 / (N - 1) as f32;
            let out = (t * std::f32::consts::PI).sin() * deep;
            (p0.0 + (p1.0 - p0.0) * t + nx * out, p0.1 + (p1.1 - p0.1) * t + ny * out)
        })
        .collect()
}

impl Cord {
    /// A cord plugged in at both ends, starting in the curve it hangs in (see `hang`), so it swings
    /// a little and settles instead of dropping or buckling.
    pub fn new(seq: u64, slots: (u64, u64), a: (String, (f32, f32)), b: (String, (f32, f32)), color: [u8; 3]) -> Self {
        let len = target_len(b.1.0 - a.1.0, b.1.1 - a.1.1);
        let pts = hang(a.1, b.1, len).into_iter().map(|(x, y)| P::at(x, y)).collect();
        Cord {
            seq,
            a: End::new(Some(a.0), slots.0),
            b: End::new(Some(b.0), slots.1),
            color,
            pts,
            len,
            dying: None,
            alpha: 1.0,
            grab: None,
            strain: 1.0,
            pending: 0.0,
            span: 0.0,
            flow: 0.0,
            flow_at: 0.0,
            settle: true,
        }
    }

    /// A cord just pulled out of a jack: one end plugged in, the other in the hand. It starts with
    /// no length and quickly grows to its own as the hand moves away.
    pub fn fresh(seq: u64, slot: u64, jack: String, at: (f32, f32), color: [u8; 3], output: bool) -> Self {
        let mut c = Cord::new(seq, (slot, slot), (jack.clone(), at), (jack, at), color);
        // it comes out of the jack in the hand: that is to be seen
        c.settle = false;
        c.len = 0.0;
        let held = if output { &mut c.b } else { &mut c.a };
        held.jack = None;
        held.held = true;
        (held.x, held.y) = at;
        c
    }

    pub fn live(&self) -> bool {
        self.dying.is_none() && self.a.jack.is_some() && self.b.jack.is_some()
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
        self.dying.is_some() || self.grab.is_some() || self.a.held || self.b.held || self.pts.iter().any(|p| (p.x - p.px).abs() + (p.y - p.py).abs() > 0.02)
    }

    /// One frame: `dt` seconds. `slot_pos` finds where a jack's n-th slot is, and the corner of
    /// its device; `snap` puts ends in jacks straight in place (while a device is dragged) instead
    /// of gliding there.
    pub fn frame(&mut self, dt: f32, steps: usize, slot_pos: &impl Fn(&str, usize) -> Option<Slot>, hand: Option<(f32, f32)>, held: Option<Held>, snap: bool) {
        self.pending = (self.pending - dt).max(0.0);
        let dying = self.dying.is_some();
        // how far each end in a jack moved with its device this frame, and where the ends and the
        // length were before
        let mut carried = [(0.0f32, 0.0f32); 2];
        let mut jumped = false;
        let before = ((self.a.x, self.a.y), (self.b.x, self.b.y), self.len);
        for (end_b, e) in [(false, &mut self.a), (true, &mut self.b)] {
            if e.held
                && let Some(h) = held
            {
                (e.x, e.y) = (h.x, h.y);
            } else if e.free {
                // a dropped end falls with the cord
                let p = if end_b { self.pts[N - 1] } else { self.pts[0] };
                (e.x, e.y) = (p.x, p.y);
            } else if let Some(j) = &e.jack
                && !dying
                && let Some(Slot { at: (x, y), dev }) = slot_pos(j, e.rank)
            {
                // a device that moves (in the hand, or gliding) carries its cords along, swinging; a
                // jack that jumps on its device (its mult closed up or opened, the desk became the
                // rack) takes its cords to rest at once
                let was = (e.x, e.y, e.placed, e.rel);
                let rel = (x - dev.0, y - dev.1);
                (e.x, e.y, e.placed, e.rel) = (x, y, true, rel);
                if was.2 && !snap && (rel.0 - was.3.0).abs() + (rel.1 - was.3.1).abs() > JUMP {
                    jumped = true;
                } else if was.2 {
                    carried[end_b as usize] = (e.x - was.0, e.y - was.1);
                }
            }
        }
        if !self.a.free && !self.b.free {
            let d = ((self.a.x - self.b.x).powi(2) + (self.a.y - self.b.y).powi(2)).sqrt();
            let in_hand = self.a.held || self.b.held;
            // When its ends move apart or together (its devices gliding somewhere), its length
            // changes with them at once, keeping its slack: it isn't squeezed into an arch that
            // then drops, nor pulled taut. Then it eases to its own length.
            if !in_hand && self.span > 1.0 && d > 1.0 {
                self.len *= d / self.span;
            }
            self.span = d;
            // In the hand it is as long as it will be in the jack, so it doesn't grow when it goes
            // in. It follows its ends quickly both ways, so devices coming together don't squeeze
            // it into a loop.
            let t = target_len(self.a.x - self.b.x, self.a.y - self.b.y);
            self.len += (t - self.len) * ease(0.2, dt);
        }
        // just plugged in, or its jacks jumped: straight to rest, in its own length
        if (self.settle || jumped) && self.a.seated() && self.b.seated() && !dying {
            self.settle = false;
            self.len = target_len(self.a.x - self.b.x, self.a.y - self.b.y);
            self.span = ((self.a.x - self.b.x).powi(2) + (self.a.y - self.b.y).powi(2)).sqrt();
            self.rest();
            carried = [(0.0, 0.0); 2];
        }
        // A device that moves carries its cords along: each point moves as much as the curve the
        // cord hangs in moves there (see `hang`), its last position too. So a cord goes along in
        // its hanging shape, keeping its own swing, without being pulled at (nothing to fight
        // its physics and tremble), whipped into a loop, or left to drop from above.
        if carried.iter().any(|&(x, y)| x.abs() + y.abs() > 1e-3) {
            let (was, now) = (hang(before.0, before.1, before.2), hang((self.a.x, self.a.y), (self.b.x, self.b.y), self.len));
            for ((p, w), n) in self.pts.iter_mut().zip(&was).zip(&now) {
                let (dx, dy) = (n.0 - w.0, n.1 - w.1);
                (p.x, p.y, p.px, p.py) = (p.x + dx, p.y + dy, p.px + dx, p.py + dy);
            }
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
            self.len = target_len(b.0 - a.0, b.1 - a.1);
        }
        let len: f32 = self.pts.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).sum();
        self.strain = len / self.len.max(1e-3);
        if let Some(t) = &mut self.dying {
            *t += dt;
            self.alpha = (1.0 - (*t - 0.65) / 0.45).clamp(0.0, 1.0);
        }
    }

    /// Hangs the cord where it comes to rest between its ends: from the curve it is about to hang
    /// in (see `hang`), swung out heavily damped, then still.
    fn rest(&mut self) {
        let (a, b) = ((self.a.x, self.a.y), (self.b.x, self.b.y));
        for (p, (x, y)) in self.pts.iter_mut().zip(hang(a, b, self.len)) {
            *p = P::at(x, y);
        }
        for _ in 0..REST_STEPS {
            self.step_damped(1.0 / RATE, None, REST_DAMP);
        }
        for p in &mut self.pts {
            (p.px, p.py) = (p.x, p.y);
        }
    }

    fn step(&mut self, dt: f32, hand: Option<(f32, f32)>) {
        self.step_damped(dt, hand, DAMP);
    }

    fn step_damped(&mut self, dt: f32, hand: Option<(f32, f32)>, damp: f32) {
        let n = N;
        let g = GRAV * dt * dt;
        let seg = self.len / (n - 1) as f32;
        for p in &mut self.pts {
            let (vx, vy) = ((p.x - p.px) * damp, (p.y - p.py) * damp);
            (p.px, p.py) = (p.x, p.y);
            p.x += vx;
            p.y += vy + g;
        }
        // the ends are pinned where they are (in a jack or in the hand); a dropped end falls
        let (pin_a, pin_b, k) = (!self.a.free, !self.b.free, self.grab);
        let (ax, ay, bx, by) = (self.a.x, self.a.y, self.b.x, self.b.y);
        let pin = |pts: &mut [P]| {
            if pin_a {
                (pts[0].x, pts[0].y) = (ax, ay);
            }
            if pin_b {
                (pts[n - 1].x, pts[n - 1].y) = (bx, by);
            }
            if let (Some(k), Some((hx, hy))) = (k, hand) {
                pts[k].x += (hx - pts[k].x) * 0.35;
                pts[k].y += (hy - pts[k].y) * 0.35;
            }
        };
        let w = |i: usize| if (i == 0 && pin_a) || (i == n - 1 && pin_b) || Some(i) == k { 0.0 } else { 1.0 };
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

    /// The nearest point of the cord to (x, y): its distance, and the index of the point to grab.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(a: f32) -> f32 {
        a.sin().atan2(a.cos())
    }

    /// How many times a cord crosses itself: a loop or a knot crosses at least once.
    fn crossings(c: &Cord) -> usize {
        let s: Vec<(P, P)> = c.pts.windows(2).map(|w| (w[0], w[1])).collect();
        let cross = |(a, b): (P, P), (c, d): (P, P)| {
            let side = |p: P, q: P, r: P| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
            side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0
        };
        (0..s.len()).flat_map(|i| (i + 2..s.len()).map(move |j| (i, j))).filter(|&(i, j)| cross(s[i], s[j])).count()
    }

    /// How far a cord turns in all along its length, in degrees: a cord that hooks round (a J or a
    /// U) turns about half a circle, a loop or a coil a whole one or more.
    fn turning(c: &Cord) -> f32 {
        c.pts.windows(3).map(|w| wrap((w[2].y - w[1].y).atan2(w[2].x - w[1].x) - (w[1].y - w[0].y).atan2(w[1].x - w[0].x)).abs()).sum::<f32>().to_degrees()
    }

    /// Settles a cord between two jacks, one of them moving along `path` (as a device glides).
    fn settle(ja: (f32, f32), jb: (f32, f32), path: &[(f32, f32)]) -> Cord {
        let pos = std::cell::Cell::new(ja);
        // the jack is carried along by its device: the two move together
        let slot = |k: &str, _n: usize| {
            let at = if k == "a" { pos.get() } else { jb };
            Some(Slot { at, dev: at })
        };
        let mut c = Cord::new(1, (1, 2), ("a".into(), ja), ("b".into(), jb), [0, 200, 0]);
        for &to in path {
            for _ in 0..60 {
                let at = pos.get();
                pos.set((at.0 + (to.0 - at.0) * 0.18, at.1 + (to.1 - at.1) * 0.18));
                c.frame(1.0 / 60.0, 2, &slot, None, None, false);
            }
        }
        for _ in 0..300 {
            c.frame(1.0 / 60.0, 2, &slot, None, None, false);
        }
        c
    }

    /// A cord sags with its length, as real cords do: a short one a little, a long one in a deep U.
    #[test]
    fn a_long_cord_hangs_lower_than_a_short_one() {
        let sag = |dx: f32| settle((0.0, 300.0), (dx, 300.0), &[]).pts.iter().map(|p| p.y).fold(f32::MIN, f32::max) - 300.0;
        let (short, long) = (sag(100.0), sag(1000.0));
        assert!(short < 80.0, "a cord across 100 sags {short:.0}: short ones hang in a small U");
        assert!(long > 250.0 && long > 4.0 * short, "a cord across 1000 sags only {long:.0} (one across 100: {short:.0}): long ones hang in a deep U");
    }

    /// A cord between two jacks hangs in one smooth curve below them, wherever they are: no loop
    /// (a whole turn), and nowhere above the higher jack. (Between a jack and one right below it,
    /// it hangs down past the lower one and up into it: a U, half a turn.)
    #[test]
    fn a_cord_hangs_below_its_jacks() {
        let ja = (400.0, 300.0);
        for (dx, dy) in [(300.0, 0.0), (-500.0, 120.0), (0.0, 120.0), (40.0, 140.0), (-15.0, 180.0), (70.0, 0.0), (250.0, -300.0), (-900.0, -150.0)] {
            let jb = (ja.0 + dx, ja.1 + dy);
            let c = settle(ja, jb, &[]);
            let (deg, top) = (turning(&c), c.pts.iter().map(|p| p.y).fold(f32::MAX, f32::min));
            assert!(deg < 300.0, "a cord from {ja:?} to {jb:?} turns {deg:.0}° along its length: a loop");
            assert!(top > ja.1.min(jb.1) - 1.0, "a cord from {ja:?} to {jb:?} rises to {top:.0}, above its jacks");
        }
    }

    /// A cord to a jack (nearly) right below hangs in a U a little past it and comes up into it,
    /// as in Reason: not straight, and not in a deep loop.
    #[test]
    fn a_cord_to_a_jack_below_hangs_in_a_u_past_it() {
        let ja = (400.0, 300.0);
        for (dx, dy) in [(0.0, 120.0), (15.0, 180.0), (-30.0, 250.0), (40.0, 400.0), (-335.0, 1040.0)] {
            let jb = (ja.0 + dx, ja.1 + dy);
            let c = settle(ja, jb, &[]);
            let low = c.pts.iter().map(|p| p.y).fold(f32::MIN, f32::max) - jb.1;
            assert!(low > 5.0 && low < dy / 3.0 + 20.0, "a cord from {ja:?} to {jb:?} hangs {low:.0} past its lower jack");
        }
    }

    /// A cord leaves each jack going down, not sideways over the jacks beside it: between the
    /// jacks of a mult, to a jack far off to one side, and up to one above.
    #[test]
    fn a_cord_drops_out_of_its_jacks() {
        let ja = (400.0, 300.0);
        for (dx, dy) in [(48.0, 0.0), (-96.0, 0.0), (600.0, 0.0), (-400.0, 60.0), (300.0, -200.0)] {
            let jb = (ja.0 + dx, ja.1 + dy);
            let c = settle(ja, jb, &[]);
            for (end, next, at) in [("first", c.pts[2], ja), ("second", c.pts[N - 3], jb)] {
                let (sx, sy) = (next.x - at.0, next.y - at.1);
                assert!(sy > sx.abs(), "a cord from {ja:?} to {jb:?} leaves its {end} jack sideways ({sx:.0}, {sy:.0})");
            }
        }
    }

    /// When the devices glide to new places (switching between desk and rack), a cord comes to rest
    /// without a loop again.
    #[test]
    fn cords_settle_after_the_devices_move() {
        let jb = (500.0, 400.0);
        for path in [&[(200.0, 150.0)][..], &[(900.0, 100.0), (300.0, 600.0)], &[(500.0, 900.0)], &[(520.0, 530.0)], &[(1500.0, 100.0), (500.0, 280.0)]] {
            let c = settle((700.0, 400.0), jb, path);
            let deg = turning(&c);
            assert!(deg < 300.0, "after its device moved along {path:?} the cord turns {deg:.0}° along its length: a loop");
        }
    }

    /// A cord shows in its place at rest: plugged in, or after its jack jumped, it doesn't swing.
    #[test]
    fn a_cord_appears_at_rest() {
        let ja = std::cell::Cell::new((400.0f32, 300.0f32));
        let jb = (900.0, 420.0);
        // the jack jumps on a device that stays
        let slot = |k: &str, _n: usize| Some(Slot { at: if k == "a" { ja.get() } else { jb }, dev: (0.0, 0.0) });
        let mut c = Cord::new(1, (1, 2), ("a".into(), ja.get()), ("b".into(), jb), [0, 200, 0]);
        for jump in [None, Some((150.0, 500.0))] {
            if let Some(to) = jump {
                ja.set(to);
            }
            c.frame(1.0 / 60.0, 2, &slot, None, None, false);
            let first: Vec<(f32, f32)> = c.pts.iter().map(|p| (p.x, p.y)).collect();
            for _ in 0..30 {
                c.frame(1.0 / 60.0, 2, &slot, None, None, false);
            }
            let drift = c.pts.iter().zip(&first).map(|(p, f)| (p.x - f.0).abs() + (p.y - f.1).abs()).fold(0.0f32, f32::max);
            assert!(drift < 2.0, "a cord {} moves {drift:.1} in the half second after", if jump.is_some() { "whose jack jumped" } else { "just plugged in" });
            assert!(turning(&c) < 300.0, "and it rests without a loop");
            assert!(!c.moving(), "and counts as still, so it is drawn once, not every frame");
        }
    }

    /// A cord pulled out of a free jack comes out in a gentle curve, without a coil that has to
    /// unroll first: the hand moves away from the jack in several directions.
    #[test]
    fn a_new_cord_comes_out_without_a_loop() {
        let ja = (400.0, 300.0);
        let slot = move |_: &str, _n: usize| Some(Slot { at: ja, dev: (0.0, 0.0) });
        for (dx, dy) in [(300.0, 0.0), (250.0, 200.0), (-250.0, 150.0), (20.0, 300.0), (-20.0, 300.0), (150.0, -200.0)] {
            let mut c = Cord::fresh(1, 1, "a".into(), ja, [0, 200, 0], true);
            let mut worst = 0;
            for i in 0..90 {
                // the hand pulls the end away over a second, then holds still
                let t = (i as f32 / 60.0).min(1.0);
                let h = Held { x: ja.0 + dx * t, y: ja.1 + dy * t };
                c.frame(1.0 / 60.0, 2, &slot, None, Some(h), false);
                if i > 3 {
                    worst = worst.max(crossings(&c));
                }
            }
            assert!(worst == 0, "a new cord pulled towards ({dx}, {dy}) crosses itself while it comes out");
        }
    }
}

