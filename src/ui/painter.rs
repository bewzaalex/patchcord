//! Pictures drawn on threads of their own, so a frame never waits for one: device faces and
//! their shadows, ENGINE's graph, the top panel. Whoever asks shows the picture it had until
//! the new one comes (stretched, or of the old shape, for a few frames).
//!
//! A few threads take the jobs in turn. Each picture is known by a key; asking for a key again
//! makes an older job for it pointless, and a thread skips that one.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use resvg::tiny_skia::Pixmap;

use super::scene::rasterize;
use super::text;

/// A picture done: what it is (`key`), what it was drawn from (`sig`), its pixels.
pub struct Done {
    pub key: u64,
    pub sig: u64,
    pub pm: Option<Pixmap>,
}

struct Job {
    key: u64,
    sig: u64,
    svg: String,
    scale: f64,
    reply: Sender<(Done, bool)>,
}

struct Pool {
    jobs: Mutex<Sender<Job>>,
    /// the newest job for each key: an older one is skipped
    newest: Arc<Mutex<HashMap<u64, u64>>>,
}

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let newest: Arc<Mutex<HashMap<u64, u64>>> = Arc::default();
        let n = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
        for i in 0..n {
            let (rx, newest) = (rx.clone(), newest.clone());
            let _ = std::thread::Builder::new().name(format!("patchcord-paint-{i}")).spawn(move || {
                let opt = text::svg_options();
                loop {
                    let Ok(job) = rx.lock().map_err(|_| ()).and_then(|r| r.recv().map_err(|_| ())) else { return };
                    let wanted = newest.lock().map(|m| m.get(&job.key) == Some(&job.sig)).unwrap_or(true);
                    let pm = if wanted { rasterize(&opt, &job.svg, job.scale) } else { None };
                    let _ = job.reply.send((Done { key: job.key, sig: job.sig, pm }, wanted));
                }
            });
        }
        Pool { jobs: Mutex::new(tx), newest }
    })
}

/// A key for a picture: from what kind it is and whose.
pub fn key(kind: &str, of: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    (kind, of).hash(&mut h);
    h.finish()
}

/// A sig for a picture: what it is drawn from, and at what scale.
pub fn sig(of: impl Hash, scale: f64) -> u64 {
    let mut h = DefaultHasher::new();
    (of, scale.to_bits()).hash(&mut h);
    h.finish()
}

/// Where one asker's pictures come back.
pub struct Inbox {
    tx: Sender<(Done, bool)>,
    rx: Receiver<(Done, bool)>,
    /// jobs asked for and not back yet
    out: usize,
}

impl Default for Inbox {
    fn default() -> Self {
        let (tx, rx) = channel();
        Inbox { tx, rx, out: 0 }
    }
}

impl Inbox {
    /// Asks for `svg` drawn at `scale` as the picture `key` (drawn from `sig`).
    pub fn ask(&mut self, key: u64, sig: u64, svg: String, scale: f64) {
        let p = pool();
        if let Ok(mut m) = p.newest.lock() {
            m.insert(key, sig);
        }
        if p.jobs.lock().is_ok_and(|j| j.send(Job { key, sig, svg, scale, reply: self.tx.clone() }).is_ok()) {
            self.out += 1;
        }
    }

    /// The pictures done since the last look (not those skipped).
    pub fn take(&mut self) -> Vec<Done> {
        let got: Vec<(Done, bool)> = self.rx.try_iter().collect();
        self.out = self.out.saturating_sub(got.len());
        got.into_iter().filter(|(_, drawn)| *drawn).map(|(d, _)| d).collect()
    }

    /// Waits for the next picture (at most `max`): for the first pictures, which have nothing to
    /// show meanwhile.
    pub fn wait(&mut self, max: std::time::Duration) -> Option<Done> {
        loop {
            let (d, drawn) = self.rx.recv_timeout(max).ok()?;
            self.out = self.out.saturating_sub(1);
            if drawn {
                return Some(d);
            }
        }
    }

    /// Some pictures are still being drawn.
    pub fn busy(&self) -> bool {
        self.out > 0
    }
}
