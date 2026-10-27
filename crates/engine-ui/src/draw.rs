//! Draws the UI with egui over the 3D view: subtitles and questions, notices,
//! the HUD, the conversation letterbox and the black of a night passing.

use egui::{Color32, Context, Painter, Rect, RichText, Shape};

use crate::sprites::Sprites;
use crate::subtitles::{Item, Subtitles};
use crate::text::BitmapFont;

/// The game's fonts the UI sets its text in, each in the sizes the game has:
/// they are drawn at the size nearest the one wanted, a texel to a pixel,
/// since scaling a bitmap font distorts it.
pub struct Fonts {
    /// Lines and notices (`ENG_ARIAL_12` to `_28`).
    pub body: Vec<BitmapFont>,
    /// Hints (`ENG_TAHOMA_10`, `_12`).
    pub small: Vec<BitmapFont>,
}

impl Fonts {
    /// The font in `sizes` nearest `pixels` high.
    fn nearest(sizes: &[BitmapFont], pixels: f32) -> Option<&BitmapFont> {
        sizes.iter().min_by(|a, b| {
            (a.pixel_height() - pixels)
                .abs()
                .total_cmp(&(b.pixel_height() - pixels).abs())
        })
    }
}

/// The colour of a speaker's name.
const SPEAKER: Color32 = Color32::from_rgb(232, 200, 120);

/// Draws the subtitle at the bottom of `rect` and the notice at its top, in
/// the game's text boxes. Returns the answer if a question's button was
/// clicked.
pub fn subtitles(
    ctx: &Context,
    rect: Rect,
    subs: &Subtitles,
    sprites: &Sprites,
    fonts: Option<&Fonts>,
) -> Option<bool> {
    let u = rect.height() / 480.0;
    // Text as big as Arial 16 (21 pixels a line) on a 600-line screen, in the
    // nearest size the game has, one texel to a physical pixel.
    let ppp = ctx.pixels_per_point();
    let want = rect.height() * ppp / 600.0;
    let scale = 1.0 / ppp;
    let body = fonts.and_then(|f| Fonts::nearest(&f.body, 21.0 * want));
    let small = fonts.and_then(|f| Fonts::nearest(&f.small, 13.0 * want));
    if let Some((text, kind)) = subs.notice() {
        // Clear of the health bar in the top left and the map in the top right.
        let width = (rect.width() - 480.0).clamp(240.0, 620.0);
        egui::Area::new(egui::Id::new("notice"))
            .fixed_pos(egui::pos2(rect.center().x - width / 2.0, rect.top() + 16.0))
            .show(ctx, |ui| {
                in_text_box(ui, sprites, u, |ui| {
                    ui.set_width(width);
                    let colour = match kind {
                        Some(true) => Color32::from_rgb(120, 200, 120),
                        Some(false) => Color32::from_rgb(220, 110, 100),
                        None => ui.visuals().strong_text_color(),
                    };
                    label(ui, body, scale, text, colour, 16.0);
                });
            });
    }
    let width = (rect.width() * 0.7).min(720.0);
    let mut answer = None;
    let body_font = body;
    let (speaker, body, hint) = match subs.front() {
        Some(Item::Line { line, wait, .. }) => (
            line.speaker.as_str(),
            line.text.as_str(),
            wait.then_some("E to continue"),
        ),
        Some(Item::Ask { text, .. }) => ("", text.as_str(), None),
        _ => return None,
    };
    egui::Area::new(egui::Id::new("subtitle"))
        .fixed_pos(egui::pos2(
            rect.center().x - width / 2.0,
            rect.bottom() - 140.0,
        ))
        .show(ctx, |ui| {
            in_text_box(ui, sprites, u, |ui| {
                ui.set_width(width);
                if !speaker.is_empty() {
                    label(ui, body_font, scale, speaker, SPEAKER, 15.0);
                }
                label(ui, body_font, scale, body, Color32::from_gray(235), 17.0);
                if let Some(hint) = hint {
                    label(ui, small, scale, hint, Color32::from_gray(170), 11.0);
                }
                if subs.asking().is_some() {
                    ui.horizontal(|ui| {
                        if ui.button("Yes (Y)").clicked() {
                            answer = Some(true);
                        }
                        if ui.button("No (N)").clicked() {
                            answer = Some(false);
                        }
                    });
                }
            });
        });
    answer
}

/// Black bars over the top and bottom of `rect` while the conversation camera
/// is on; `amount` 0 (none) to 1 (full).
pub fn letterbox(painter: &Painter, rect: Rect, amount: f32) {
    let bar = rect.height() * 0.11 * amount;
    if bar <= 0.5 {
        return;
    }
    painter.rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar)),
        0.0,
        Color32::BLACK,
    );
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(rect.min.x, rect.max.y - bar),
            egui::vec2(rect.width(), bar),
        ),
        0.0,
        Color32::BLACK,
    );
}

/// Black over `rect`, `amount` 0 (clear) to 1 (opaque).
pub fn fade(painter: &Painter, rect: Rect, amount: f32) {
    let alpha = (amount.clamp(0.0, 1.0) * 255.0) as u8;
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(alpha));
}

/// Lays out `content` on the game's text box (a dark frame, so its text is
/// drawn light).
fn in_text_box(ui: &mut egui::Ui, sprites: &Sprites, u: f32, content: impl FnOnce(&mut egui::Ui)) {
    let behind = ui.painter().add(Shape::Noop);
    let margin = (10.0 * u).max(8.0);
    let inner = egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
        ui.visuals_mut().override_text_color = Some(Color32::from_gray(235));
        content(ui)
    });
    let box_shape = if sprites.is_empty() {
        Shape::rect_filled(inner.response.rect, 6.0, Color32::from_black_alpha(200))
    } else {
        sprites.text_box(inner.response.rect, margin * 1.5, Color32::WHITE)
    };
    ui.painter().set(behind, box_shape);
}

/// `text` in `font` at `scale`, wrapped to the width `ui` offers, else as an
/// egui label `size` points high.
fn label(
    ui: &mut egui::Ui,
    font: Option<&BitmapFont>,
    scale: f32,
    text: &str,
    colour: Color32,
    size: f32,
) {
    let Some(font) = font else {
        ui.label(RichText::new(text).size(size).color(colour));
        return;
    };
    let width = ui.available_width();
    let mut lines = 0;
    font.wrap(text, width, scale, |_| lines += 1);
    let line = font.line_height(scale);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, line * lines as f32), egui::Sense::hover());
    let painter = ui.painter();
    let mut y = rect.top();
    font.wrap(text, width, scale, |l| {
        font.draw(painter, egui::pos2(rect.left(), y), l, scale, colour);
        y += line;
    });
}
