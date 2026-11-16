//! The fonts Patchcord draws with, and text widths for fitting names on the panel.

use std::sync::{Arc, OnceLock};

use resvg::usvg;

pub const BLACK: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Black.ttf");
pub const BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Bold.ttf");
pub const MONO_BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono-Bold.ttf");
pub const MONO: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono-Regular.ttf");

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
pub fn width(font: &[u8], text: &str, size: f64, spacing: f64) -> f64 {
    let Ok(face) = ttf_parser::Face::parse(font, 0) else { return text.len() as f64 * size * 0.6 };
    let em = face.units_per_em() as f64;
    let adv: f64 = text
        .chars()
        .map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or((em * 0.6) as u16) as f64)
        .sum();
    adv * size / em + spacing * text.chars().count() as f64
}
