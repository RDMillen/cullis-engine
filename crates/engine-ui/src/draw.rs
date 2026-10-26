//! Draws the UI with egui over the 3D view: subtitles and questions, notices,
//! the HUD, the conversation letterbox and the black of a night passing.

use egui::{Color32, Context, Painter, Rect, RichText};

use crate::Hud;
use crate::subtitles::{Item, Subtitles};

/// Draws the subtitle at the bottom of `rect` and the notice at its top.
/// Returns the answer if a question's button was clicked.
pub fn subtitles(ctx: &Context, rect: Rect, subs: &Subtitles) -> Option<bool> {
    if let Some((text, kind)) = subs.notice() {
        // Clear of the HUD in the top right.
        let width = (rect.width() - 480.0).clamp(240.0, 620.0);
        egui::Area::new(egui::Id::new("notice"))
            .fixed_pos(egui::pos2(rect.center().x - width / 2.0, rect.top() + 16.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    let colour = match kind {
                        Some(true) => Color32::from_rgb(120, 200, 120),
                        Some(false) => Color32::from_rgb(220, 110, 100),
                        None => ui.visuals().strong_text_color(),
                    };
                    ui.label(RichText::new(text).size(16.0).color(colour));
                });
            });
    }
    let width = (rect.width() * 0.7).min(720.0);
    let mut answer = None;
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
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(width);
                if !speaker.is_empty() {
                    ui.label(RichText::new(speaker).strong());
                }
                ui.label(RichText::new(body).size(17.0));
                if let Some(hint) = hint {
                    ui.label(RichText::new(hint).weak().small());
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

/// The Hero's gold, items and deeds, in the top right of `rect`.
pub fn hud(ctx: &Context, rect: Rect, hud: &Hud) {
    egui::Area::new(egui::Id::new("hud"))
        .fixed_pos(egui::pos2(rect.right() - 200.0, rect.top() + 16.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(170.0);
                ui.label(
                    RichText::new(format!("Gold: {}", hud.gold))
                        .strong()
                        .size(16.0),
                );
                for item in &hud.items {
                    ui.label(item);
                }
                ui.label(
                    RichText::new(format!(
                        "Good deeds {} · Bad deeds {}",
                        hud.good_deeds, hud.bad_deeds
                    ))
                    .small(),
                );
                ui.label(RichText::new("E talk · F kick · Y/N answer").weak().small());
            });
        });
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
