//! Text in the game's bitmap fonts (`fable_data::font`): measuring, wrapping
//! and drawing lines as textured quads, since egui draws only outline fonts.

use egui::{Color32, Context, Mesh, Painter, Pos2, Rect, TextureHandle, Vec2};

/// A font loaded for drawing.
pub struct BitmapFont {
    texture: TextureHandle,
    first: u32,
    glyphs: Vec<fable_data::font::Glyph>,
    /// Pixel height of a line.
    height: f32,
    texture_size: Vec2,
}

impl BitmapFont {
    pub fn load(ctx: &Context, name: &str, font: &fable_data::font::Font) -> Self {
        let size = [font.texture_size[0] as usize, font.texture_size[1] as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &font.rgba());
        Self {
            // Drawn a texel to a pixel, so no filtering.
            texture: ctx.load_texture(name, image, egui::TextureOptions::NEAREST),
            first: font.first,
            glyphs: font.glyphs().copied().collect(),
            height: font.height as f32,
            texture_size: Vec2::new(size[0] as f32, size[1] as f32),
        }
    }

    fn glyph(&self, ch: char) -> Option<&fable_data::font::Glyph> {
        let i = (ch as u32).checked_sub(self.first)? as usize;
        self.glyphs.get(i)
    }

    /// The font's line height in pixels.
    pub fn pixel_height(&self) -> f32 {
        self.height
    }

    /// The height of a line at `scale`.
    pub fn line_height(&self, scale: f32) -> f32 {
        self.height * scale
    }

    /// The width of `text` at `scale`. Characters the font lacks count as a
    /// space; curly quotes and dashes as their plain forms.
    pub fn width(&self, text: &str, scale: f32) -> f32 {
        text.chars()
            .filter_map(|c| self.glyph(plain(c)).or_else(|| self.glyph(' ')))
            .map(|g| g.advance as f32 * scale)
            .sum()
    }

    /// Splits `text` into lines no wider than `max_width` at `scale`, breaking
    /// at spaces, and calls `line` with each.
    pub fn wrap<'t>(
        &self,
        text: &'t str,
        max_width: f32,
        scale: f32,
        mut line: impl FnMut(&'t str),
    ) {
        for paragraph in text.split('\n') {
            let mut start = 0;
            let mut end = 0;
            for (i, _) in paragraph
                .match_indices(' ')
                .chain(std::iter::once((paragraph.len(), "")))
            {
                if end > start && self.width(&paragraph[start..i], scale) > max_width {
                    line(&paragraph[start..end]);
                    start = end + 1;
                }
                end = i;
            }
            line(&paragraph[start.min(paragraph.len())..]);
        }
    }

    /// Draws `text` on one line with its top left at `at` (moved to a whole
    /// pixel, so that at a scale of one pixel a texel each glyph stays sharp).
    pub fn draw(&self, painter: &Painter, at: Pos2, text: &str, scale: f32, colour: Color32) {
        let ppp = painter.ctx().pixels_per_point();
        let at = Pos2::new((at.x * ppp).round() / ppp, (at.y * ppp).round() / ppp);
        let mut mesh = Mesh::with_texture(self.texture.id());
        let mut x = at.x;
        for c in text.chars() {
            let Some(g) = self.glyph(plain(c)).or_else(|| self.glyph(' ')) else {
                continue;
            };
            // The cell (a texel wider than the glyph) in texels: the table
            // gives them as fractions of the size less one.
            let size_t = self.texture_size;
            let [l, t, r, b] = g.uv;
            let last = size_t - Vec2::splat(1.0);
            let texels = Rect::from_min_max(
                Pos2::new((l * last.x).round(), (t * last.y).round()),
                Pos2::new((r * last.x).round(), (b * last.y).round().min(size_t.y)),
            );
            let size = texels.size();
            let min = Pos2::new(x + g.offset as f32 * scale, at.y);
            let uv = Rect::from_min_max(
                (texels.min.to_vec2() / size_t).to_pos2(),
                (texels.max.to_vec2() / size_t).to_pos2(),
            );
            mesh.add_rect_with_uv(Rect::from_min_size(min, size * scale), uv, colour);
            x += g.advance as f32 * scale;
        }
        painter.add(mesh);
    }
}

/// The plain form of typographic punctuation the fonts lack.
fn plain(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' => '\'',
        '\u{201c}' | '\u{201d}' => '"',
        '\u{2013}' | '\u{2014}' => '-',
        '\u{2026}' => '.',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fable_data::font::{Font, Glyph, Range};

    /// Every character from ' ' to '~' advancing 2 pixels.
    fn font() -> BitmapFont {
        let glyphs = (0x20u32..0x7f)
            .map(|c| Glyph {
                ch: char::from_u32(c).unwrap(),
                uv: [0.0, 0.0, 1.0, 1.0],
                offset: 0,
                width: 1,
                advance: 2,
            })
            .collect();
        let font = Font {
            name: "Test".into(),
            height: 10,
            weight: 400,
            unknown_flag: 0,
            unknown_height: 10,
            texture_size: [1, 1],
            first: 0x20,
            last: 0x7e,
            ranges: vec![Range {
                unknown_header: [0, 0],
                glyphs,
            }],
            unknown_trailer: Vec::new(),
            texels: vec![255; 4],
        };
        BitmapFont::load(&Context::default(), "test", &font)
    }

    fn lines(text: &str, max_width: f32) -> Vec<String> {
        let mut out = Vec::new();
        font().wrap(text, max_width, 1.0, |l| out.push(l.to_owned()));
        out
    }

    #[test]
    fn measures_by_advance_and_scale() {
        let f = font();
        assert_eq!(f.width("abc", 1.0), 6.0);
        assert_eq!(
            f.width("a\u{2019}b", 2.0),
            12.0,
            "a curly quote counts as one"
        );
        assert_eq!(f.line_height(1.5), 15.0);
    }

    #[test]
    fn wraps_at_spaces() {
        // Each word is 4 wide, with a space 10.
        assert_eq!(lines("aa bb cc", 10.0), ["aa bb", "cc"]);
        assert_eq!(lines("aa bb cc", 4.0), ["aa", "bb", "cc"]);
        assert_eq!(lines("aa bb cc", 100.0), ["aa bb cc"]);
        assert_eq!(
            lines("aaaaaaaa bb", 4.0),
            ["aaaaaaaa", "bb"],
            "a long word alone"
        );
        assert_eq!(lines("aa\nbb", 100.0), ["aa", "bb"]);
    }
}
