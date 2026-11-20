//! The fonts Patchcord draws with, and text widths for fitting names on the panel.

use std::sync::{Arc, OnceLock};

use resvg::usvg;

pub static BLACK: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Black.ttf");
pub static BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Bold.ttf");
pub static MONO_BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono-Bold.ttf");
pub static MONO: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono-Regular.ttf");

/// Text made safe inside SVG (and its attributes): control characters (a NUL in a driver's
/// string) are not allowed in it at all.
pub fn esc(t: &str) -> String {
    t.chars().filter(|c| !c.is_control()).collect::<String>().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// SVG options with our fonts, read in place from the program's data.
pub fn svg_options() -> Arc<usvg::Options<'static>> {
    static OPT: OnceLock<Arc<usvg::Options<'static>>> = OnceLock::new();
    OPT.get_or_init(|| {
        let mut opt = usvg::Options { font_family: "Noto Sans".into(), ..Default::default() };
        let db = opt.fontdb_mut();
        for font in [BLACK, BOLD, MONO_BOLD, MONO] {
            db.load_font_source(usvg::fontdb::Source::Binary(Arc::new(font)));
        }
        Arc::new(opt)
    })
    .clone()
}

/// Width of `text` at `size` px, plus `spacing` px after every character (SVG letter-spacing).
pub fn width(font: &'static [u8], text: &str, size: f64, spacing: f64) -> f64 {
    let Some(face) = face(font) else { return text.chars().count() as f64 * size * 0.6 };
    let em = face.units_per_em() as f64;
    let adv: f64 = text
        .chars()
        .map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or((em * 0.6) as u16) as f64)
        .sum();
    adv * size / em + spacing * text.chars().count() as f64
}

/// One of our fonts, read once.
fn face(font: &'static [u8]) -> Option<&'static ttf_parser::Face<'static>> {
    static FACES: [OnceLock<Option<ttf_parser::Face<'static>>>; 4] = [const { OnceLock::new() }; 4];
    let i = [BLACK, BOLD, MONO_BOLD, MONO].iter().position(|f| std::ptr::eq(*f, font))?;
    FACES[i].get_or_init(|| ttf_parser::Face::parse(font, 0).ok()).as_ref()
}
