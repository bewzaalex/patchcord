//! Everything on screen: device faces, cords and the window.

pub mod app;
pub mod cord;
mod face;
mod gpu;
mod paint;
pub mod panel;
mod print;
pub mod scene;
mod text;

/// How far to go towards a target in `dt` seconds, for something that goes the fraction `k` of the
/// way each frame at 60 frames a second: the same pace whatever the screen's refresh rate.
pub fn ease(k: f32, dt: f32) -> f32 {
    1.0 - (1.0 - k).powf(dt * 60.0)
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
