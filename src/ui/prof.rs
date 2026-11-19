//! A log of slow frames, to find what stops the picture: set `PATCHCORD_FRAMES=1` to write it to
//! stderr, or `PATCHCORD_FRAMES=/some/file` to append it to that file.
//!
//! Each frame is timed part by part (`mark`); a frame that takes longer than `SLOW`, or a frame
//! that comes long after the one before while something moves, is written down with where the
//! time went, also between the frames (PipeWire's news, the mouse and keys).

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::{Duration, Instant};

/// A frame this long is written down (a 60 Hz frame is 16.7 ms).
const SLOW: Duration = Duration::from_millis(25);
/// So is a wait this long between two frames while something moves.
const GAP: Duration = Duration::from_millis(50);

enum Out {
    Off,
    Stderr,
    File(std::fs::File),
}

pub struct Prof {
    out: Out,
    began: Instant,
    /// where the frame is: since the last mark
    at: Instant,
    parts: Vec<(&'static str, Duration)>,
    /// the frame before: when it ended, and whether something was moving
    ended: Option<Instant>,
    moving: bool,
    /// time spent since the frame before on news and input: (what, how many, how long)
    between: Vec<(&'static str, u32, Duration)>,
    started: Instant,
}

impl Prof {
    pub fn new() -> Self {
        let out = match std::env::var("PATCHCORD_FRAMES") {
            Ok(v) if v == "1" => Out::Stderr,
            Ok(v) if !v.is_empty() => match std::fs::OpenOptions::new().create(true).append(true).open(&v) {
                Ok(f) => Out::File(f),
                Err(e) => {
                    eprintln!("patchcord: PATCHCORD_FRAMES: {v}: {e}");
                    Out::Off
                }
            },
            _ => Out::Off,
        };
        let now = Instant::now();
        let p = Prof { out, began: now, at: now, parts: Vec::new(), ended: None, moving: false, between: Vec::new(), started: now };
        p.write("patchcord: slow frames: frame ms = parts; gap = since the frame before, and what ran in it");
        p
    }

    fn on(&self) -> bool {
        !matches!(self.out, Out::Off)
    }

    fn write(&self, line: &str) {
        match &self.out {
            Out::Off => {}
            Out::Stderr => eprintln!("{line}"),
            Out::File(f) => {
                let mut f = f;
                let _ = writeln!(f, "{line}");
            }
        }
    }

    /// A frame starts.
    pub fn begin(&mut self) {
        let now = Instant::now();
        (self.began, self.at) = (now, now);
        self.parts.clear();
    }

    /// The frame's part since the last mark is done.
    pub fn mark(&mut self, what: &'static str) {
        let now = Instant::now();
        self.parts.push((what, now - self.at));
        self.at = now;
    }

    /// Parts timed elsewhere (the GPU's).
    pub fn add(&mut self, what: &'static str, d: Duration) {
        self.parts.push((what, d));
    }

    /// Something between frames (PipeWire's news, input) took `d`.
    pub fn between(&mut self, what: &'static str, d: Duration) {
        match self.between.iter_mut().find(|b| b.0 == what) {
            Some(b) => {
                b.1 += 1;
                b.2 += d;
            }
            None => self.between.push((what, 1, d)),
        }
    }

    /// The frame is done. `moving`: another frame follows at once; `state`: what was on screen.
    pub fn end(&mut self, moving: bool, state: impl FnOnce() -> String) {
        let now = Instant::now();
        let took = now - self.began;
        let gap = self.ended.map(|e| self.began.saturating_duration_since(e));
        let gap_slow = self.moving && gap.is_some_and(|g| g > GAP);
        if self.on() && (took > SLOW || gap_slow) {
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            let mut line = format!("{:9.3}s frame {:6.1} ms =", ms(self.began - self.started) / 1000.0, ms(took));
            let mut parts = self.parts.clone();
            parts.sort_by_key(|p| std::cmp::Reverse(p.1));
            for (what, d) in parts.iter().filter(|(_, d)| ms(*d) >= 0.5) {
                let _ = write!(line, " {what} {:.1}", ms(*d));
            }
            if let Some(g) = gap {
                let _ = write!(line, " | gap {:.1} ms", ms(g));
                for (what, n, d) in &self.between {
                    let _ = write!(line, " {what}×{n} {:.1}", ms(*d));
                }
            }
            let _ = write!(line, " | {}", state());
            self.write(&line);
        }
        self.between.clear();
        self.ended = Some(now);
        self.moving = moving;
    }
}
