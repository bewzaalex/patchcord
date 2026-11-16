//! The lettering printed on the top panel and the device faces: a few styles, defined here once
//! and used everywhere. Only the colour follows the panel it is printed on.

use super::text;

#[derive(Clone, Copy)]
pub struct Style {
    pub size: f64,
    pub spacing: f64,
}

/// Names of groups: EDIT, LAYOUT, CABLES, SAG, VIEW on the panel; PLAYBACK, MONITOR… on faces.
pub const SECTION: Style = Style { size: 9.0, spacing: 1.2 };
/// Small print: SHOW · FADE · HIDE, SIG, the IN/OUT tags, a device's type line.
pub const SMALL: Style = Style { size: 7.5, spacing: 0.8 };
/// Jack names: L, R, MONO, STEREO, PORT-0.
pub const JACK: Style = Style { size: 9.0, spacing: 0.0 };
/// Text on the little dark switch chips: SPLIT, JOIN.
pub const SWITCH: Style = Style { size: 6.8, spacing: 0.5 };

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Style {
    /// A line of text at (x, y). `anchor` is "start", "middle" or "end".
    pub fn text(self, x: f64, y: f64, fill: &str, anchor: &str, t: &str) -> String {
        // SVG puts the letter spacing after the last letter too; centred or right-aligned text
        // moves over by that much so it sits where it should
        let shift = match anchor {
            "middle" => self.spacing / 2.0,
            "end" => self.spacing,
            _ => 0.0,
        };
        format!(
            r#"<text x="{}" y="{y}" font-size="{}" letter-spacing="{}" fill="{fill}" text-anchor="{anchor}">{}</text>"#,
            x + shift,
            self.size,
            self.spacing,
            esc(t)
        )
    }

    /// How wide `t` is in this style.
    pub fn width(self, t: &str) -> f64 {
        text::width(text::BOLD, t, self.size, self.spacing)
    }
}
