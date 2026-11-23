//! Everything on screen: device faces, cords and the window.

pub mod app;
pub mod cord;
pub mod engine;
mod face;
mod gpu;
mod mesh;
mod paint;
mod painter;
mod prof;
pub mod panel;
mod print;
pub mod scene;
mod text;

/// How far to go towards a target in `dt` seconds, for something that goes the fraction `k` of the
/// way each frame at 60 frames a second: the same pace whatever the screen's refresh rate.
pub fn ease(k: f32, dt: f32) -> f32 {
    1.0 - (1.0 - k).powf(dt * 60.0)
}

/// A device's jacks as (direction, label), for tests elsewhere.
#[cfg(test)]
pub fn face_jacks(dev: &crate::graph::Device) -> Vec<(crate::graph::Dir, String)> {
    face::jacks(dev, &[]).into_iter().map(|j| (j.dir, j.label)).collect()
}

/// A device's jacks' keys (what cords are matched to).
#[cfg(test)]
pub fn face_jack_keys(dev: &crate::graph::Device) -> Vec<String> {
    face::jacks(dev, &[]).into_iter().map(|j| j.key).collect()
}

/// A device's frames, by their GRAPH keys: the node each is (a frame is a block, ADR 0014).
#[cfg(test)]
pub fn face_frames(dev: &crate::graph::Device) -> Vec<u32> {
    let jd = face::jacks(dev, &[]);
    face::face_geometry(dev, &jd, &vec![0; jd.len()], None, true).graph.iter().map(|g| g.0).collect()
}

/// A device's face: its size, and where each group's frame starts (x, how far down its unit).
#[cfg(test)]
pub fn face_layout(dev: &crate::graph::Device) -> ((f64, f64), Vec<(f64, f64)>) {
    let jd = face::jacks(dev, &[]);
    let geo = face::face_geometry(dev, &jd, &vec![0; jd.len()], None, true);
    ((geo.w, geo.h), geo.groups.iter().map(|g| (g.0, g.3)).collect())
}

#[cfg(test)]
mod tests {
    use super::ease;

    /// Easing goes as far in a 60 Hz frame as before, and as far in two 120 Hz frames as in one
    /// 60 Hz frame.
    #[test]
    fn easing_keeps_its_pace_at_any_frame_rate() {
        assert!((ease(0.2, 1.0 / 60.0) - 0.2).abs() < 1e-5);
        let half = ease(0.2, 1.0 / 120.0);
        assert!((1.0 - (1.0 - half) * (1.0 - half) - 0.2).abs() < 1e-5);
    }
}
