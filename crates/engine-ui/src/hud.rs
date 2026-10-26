//! The HUD, from the game's sprites: the health bar (top left), the gold box
//! that pops up when the Hero's gold changes (under it), and the rings for
//! what the mouse buttons do (bottom right).
//!
//! Sizes and places follow the original's HUD in a 4:3 screenshot, measured
//! in units of a 480-line screen and scaled to the view's height.

use egui::{Color32, Painter, Pos2, Rect, Vec2};

use crate::sprites::Sprites;

/// What the HUD shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hud {
    /// Health, 0 to 1.
    pub health: f32,
    /// The action at hand for the right ring: someone to talk to, or something
    /// to use.
    pub action: Option<Action>,
    /// The Hero on the region's map: where (0 to 1 across from the west and
    /// down from the north) and his heading (radians anticlockwise from north).
    pub on_map: Option<(Vec2, f32)>,
    /// The time of day, in hours.
    pub hour: f32,
}

/// What using (E) would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Speak,
    Interact,
}

/// The gold box: up for a while after the Hero's gold changes.
#[derive(Debug, Default)]
pub struct GoldPopup {
    last: Option<u32>,
    /// Seconds since it changed, while up.
    shown: Option<f32>,
}

impl GoldPopup {
    /// Seconds the box stays up, and how long it then takes to fade.
    const UP: f32 = 4.0;
    const FADE: f32 = 0.5;

    /// Follows the Hero's gold, `dt` seconds on: a change puts the box up. The
    /// first amount seen does not.
    pub fn update(&mut self, gold: Option<u32>, dt: f32) {
        if let Some(gold) = gold {
            if self.last.is_some_and(|l| l != gold) {
                self.shown = Some(0.0);
            }
            self.last = Some(gold);
        }
        if let Some(t) = &mut self.shown {
            *t += dt;
            if *t > Self::UP + Self::FADE {
                self.shown = None;
            }
        }
    }

    /// The amount shown and the box's opacity, while it is up.
    pub fn visible(&self) -> Option<(u32, f32)> {
        let t = self.shown?;
        let alpha = ((Self::UP + Self::FADE - t) / Self::FADE).clamp(0.0, 1.0);
        Some((self.last?, alpha))
    }
}

/// The visible part of a ring sprite: its middle 38 of 64 texels (the health
/// ring; the mouse ring's is 36).
const RING_VISIBLE: f32 = 38.0 / 64.0;

/// Draws the HUD over `rect`.
pub fn draw(painter: &Painter, rect: Rect, sprites: &Sprites, hud: &Hud, gold: &GoldPopup) {
    let u = rect.height() / 480.0;
    health_bar(painter, rect.min, u, sprites, hud.health);
    if let Some((amount, alpha)) = gold.visible() {
        gold_box(
            painter,
            rect.min + Vec2::new(8.0 * u, 30.0 * u),
            u,
            sprites,
            amount,
            alpha,
        );
    }
    action_rings(
        painter,
        Pos2::new(rect.right(), rect.bottom()),
        u,
        sprites,
        hud.action,
    );
    minimap(
        painter,
        Pos2::new(rect.right(), rect.top()),
        u,
        sprites,
        hud.on_map,
        hud.hour,
    );
}

/// The region's map in its ring at the top right corner `corner`, with the
/// Hero's arrow where he is, turned the way he faces.
fn minimap(
    painter: &Painter,
    corner: Pos2,
    u: f32,
    sprites: &Sprites,
    hero: Option<(Vec2, f32)>,
    hour: f32,
) {
    if sprites.size(crate::sprites::MINIMAP) == Vec2::ZERO {
        return;
    }
    let white = Color32::WHITE;
    // The ring reaches 48 units right of the map and 44 above it.
    let centre = corner + Vec2::new(-52.0 * u, 47.0 * u);
    let map = Rect::from_center_size(centre, Vec2::splat(80.0 * u));
    sprites.draw(painter, crate::sprites::MINIMAP, map, white);
    if let Some((uv, heading)) = hero {
        let at = map.min + uv.clamp(Vec2::ZERO, Vec2::ONE) * map.size();
        // The arrow is drawn in texels 24..38 by 14..35 of its 64, about
        // (31, 25). North is up on screen; anticlockwise in the world is
        // anticlockwise on screen, which egui (y down) counts negative.
        let pivot = Vec2::new(31.0, 25.0) / 64.0;
        sprites.draw_turned(
            painter,
            "MINIMAP_ICON_HERO",
            at,
            Vec2::splat(30.0 * u),
            pivot,
            -heading,
        );
    }
    // The ring is drawn in texels 0..135 by 0..113 of its 256 by 128, around
    // (67, 62), its inside about 116 across: fitted around the map.
    let scale = 82.0 * u / 116.0;
    let ring = Rect::from_min_size(
        centre - Vec2::new(67.0, 62.0) * scale,
        Vec2::new(135.0, 113.0) * scale,
    );
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(135.0 / 256.0, 113.0 / 128.0));
    sprites.draw_part(painter, "MINIMAP_BORDER", ring, uv, white);
    // The ring's right circle (about texel (116, 17), 26 across inside) holds
    // the clock: night on its left half, day on its right, the hand pointing
    // at the sun at noon. Which way the hand goes round is a guess (clockwise).
    let clock = ring.min + Vec2::new(116.0, 17.0) * scale;
    let size = Vec2::splat(26.0 * scale);
    sprites.draw(
        painter,
        "HUD_CLOCK_FACE",
        Rect::from_center_size(clock, size),
        white,
    );
    let angle = (hour - 12.0) / 24.0 * std::f32::consts::TAU;
    sprites.draw_turned(
        painter,
        "HUD_CLOCK_HAND",
        clock,
        size,
        Vec2::new(0.48, 0.5),
        angle,
    );
}

/// The heart in its ring in the top left corner `corner` and the red bar
/// running right from it, filled to `health`.
fn health_bar(painter: &Painter, corner: Pos2, u: f32, sprites: &Sprites, health: f32) {
    let white = Color32::WHITE;
    // The bar's frame is 16 texels high, visible in 15; the fill is visible in
    // its top 11, set 2 texels down into the frame.
    let height = 9.0 * u;
    let texel = height / 16.0;
    let (left, right) = (corner.x + 20.0 * u, corner.x + 178.0 * u);
    let top = corner.y + 3.0 * u;
    let end = 8.0 * texel;
    let frame = Rect::from_min_max(Pos2::new(left, top), Pos2::new(right - end, top + height));
    sprites.draw(painter, "HUD_HEALTH_BAR_MIDDLE", frame, white);
    let cap = Rect::from_min_size(Pos2::new(frame.right(), top), Vec2::new(end, height));
    sprites.draw(painter, "HUD_HEALTH_BAR_END", cap, white);
    let filled = (frame.width() - end) * health.clamp(0.0, 1.0);
    if filled > 0.0 {
        let fill_top = top + 2.0 * texel;
        let fill = Rect::from_min_size(Pos2::new(left, fill_top), Vec2::new(filled, height));
        sprites.draw(painter, "HUD_HEALTH_BAR_HEALTH_SEGMENT", fill, white);
        let tip = Rect::from_min_size(Pos2::new(fill.right(), fill_top), Vec2::new(end, height));
        sprites.draw(painter, "HUD_HEALTH_BAR_HEALTH_END", tip, white);
    }
    let centre = corner + Vec2::splat(14.0 * u);
    let ring = Rect::from_center_size(centre, Vec2::splat(26.0 * u / RING_VISIBLE));
    sprites.draw(painter, "HUD_HEALTH_BAR_RING", ring, white);
    let heart = Rect::from_center_size(centre, Vec2::splat(17.0 * u));
    sprites.draw(painter, "HUD_HEALTH_BAR_HEART", heart, white);
}

/// The stat box at `at` with the money bag and `amount` in the game's digits.
fn gold_box(painter: &Painter, at: Pos2, u: f32, sprites: &Sprites, amount: u32, alpha: f32) {
    let tint = Color32::from_white_alpha((alpha * 255.0) as u8);
    let height = 20.0 * u;
    let digits = amount.to_string();
    let digit = height * 0.75;
    let curve = height * 0.5;
    let inside = height * 0.95 + digit * 0.62 * digits.len() as f32 + 3.0 * u;
    let left = Rect::from_min_size(at, Vec2::new(curve, height));
    let middle = Rect::from_min_size(Pos2::new(left.right(), at.y), Vec2::new(inside, height));
    let right = Rect::from_min_size(Pos2::new(middle.right(), at.y), Vec2::new(curve, height));
    sprites.draw(painter, "HUD_STATBOX_LEFT_END_CURVE", left, tint);
    sprites.draw(painter, "HUD_STATBOX_MIDDLE", middle, tint);
    sprites.draw(painter, "HUD_STATBOX_RIGHT_END_CURVE", right, tint);
    let bag = Rect::from_min_size(
        Pos2::new(middle.left() - curve * 0.3, at.y + height * 0.05),
        Vec2::splat(height * 0.9),
    );
    sprites.draw(painter, "HUD_ICON_MONEY_BAG", bag, tint);
    let mut x = bag.right() + 2.0 * u;
    for c in digits.chars() {
        let r = Rect::from_min_size(
            Pos2::new(x - digit * 0.19, at.y + (height - digit) / 2.0),
            Vec2::splat(digit),
        );
        sprites.draw(painter, &format!("GUI_DIGIT_{c}"), r, tint);
        x += digit * 0.62;
    }
}

/// Two rings at the bottom right corner `corner`: the left mouse button's (an
/// unarmed attack) and the right's (what using would do).
fn action_rings(
    painter: &Painter,
    corner: Pos2,
    u: f32,
    sprites: &Sprites,
    action: Option<Action>,
) {
    let white = Color32::WHITE;
    let ring = 22.0 * u / (36.0 / 64.0);
    let right = action.map(|a| match a {
        Action::Speak => "HUD_ICON_SPEAK",
        Action::Interact => "HUD_ICON_INTERACT",
    });
    for (from_right, icon) in [(38.0, Some("HUD_ICON_BATTLE_UNARMED")), (14.0, right)] {
        let centre = corner - Vec2::new(from_right * u, 24.0 * u);
        sprites.draw(
            painter,
            "HUD_PC_MOUSE_RING",
            Rect::from_center_size(centre, Vec2::splat(ring)),
            white,
        );
        if let Some(icon) = icon {
            // Icons are visible in about 20 of their 32 texels.
            sprites.draw(
                painter,
                icon,
                Rect::from_center_size(centre, Vec2::splat(21.0 * u)),
                white,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_pops_up_when_it_changes_then_goes() {
        let mut popup = GoldPopup::default();
        popup.update(Some(0), 0.1);
        assert_eq!(popup.visible(), None, "the first amount does not pop up");
        popup.update(Some(1), 0.1);
        assert_eq!(popup.visible().map(|v| v.0), Some(1));
        for _ in 0..40 {
            popup.update(Some(1), 0.1);
        }
        let (_, alpha) = popup.visible().unwrap();
        assert!(alpha < 1.0, "fading");
        popup.update(Some(1), 1.0);
        assert_eq!(popup.visible(), None);
    }
}
