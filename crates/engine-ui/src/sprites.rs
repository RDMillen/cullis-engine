//! The game's interface sprites (`HUD_HEALTH_BAR_RING`, `HUD_TEXTBOX_TL`, …),
//! textures of the main texture bank, as egui textures.

use std::collections::HashMap;

use egui::{Color32, Context, Painter, Pos2, Rect, Shape, TextureHandle, Vec2};

/// Every sprite the HUD and text boxes draw.
pub const NAMES: &[&str] = &[
    "HUD_HEALTH_BAR_RING",
    "HUD_HEALTH_BAR_HEART",
    "HUD_HEALTH_BAR_MIDDLE",
    "HUD_HEALTH_BAR_END",
    "HUD_HEALTH_BAR_HEALTH_SEGMENT",
    "HUD_HEALTH_BAR_HEALTH_END",
    "HUD_STATBOX_LEFT_END_CURVE",
    "HUD_STATBOX_MIDDLE",
    "HUD_STATBOX_RIGHT_END_CURVE",
    "HUD_ICON_MONEY_BAG",
    "HUD_PC_MOUSE_RING",
    "HUD_ICON_BATTLE_UNARMED",
    "HUD_ICON_SPEAK",
    "HUD_ICON_INTERACT",
    "HUD_TEXTBOX_TL",
    "HUD_TEXTBOX_TM",
    "HUD_TEXTBOX_TR",
    "HUD_TEXTBOX_ML",
    "HUD_TEXTBOX_MR",
    "HUD_TEXTBOX_BL",
    "HUD_TEXTBOX_BM",
    "HUD_TEXTBOX_BR",
    "HUD_TEXTBOX_BACK",
    "GUI_DIGIT_0",
    "GUI_DIGIT_1",
    "GUI_DIGIT_2",
    "GUI_DIGIT_3",
    "GUI_DIGIT_4",
    "GUI_DIGIT_5",
    "GUI_DIGIT_6",
    "GUI_DIGIT_7",
    "GUI_DIGIT_8",
    "GUI_DIGIT_9",
];

/// The sprites loaded; a missing one draws nothing.
#[derive(Default)]
pub struct Sprites {
    textures: HashMap<&'static str, TextureHandle>,
}

impl Sprites {
    /// Loads every sprite in [`NAMES`] that `fetch` finds, as (width, height,
    /// RGBA pixels).
    pub fn load(
        ctx: &Context,
        mut fetch: impl FnMut(&str) -> Option<([usize; 2], Vec<u8>)>,
    ) -> Self {
        let textures = NAMES
            .iter()
            .filter_map(|&name| {
                let (size, rgba) = fetch(name)?;
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                Some((
                    name,
                    ctx.load_texture(name, image, egui::TextureOptions::LINEAR),
                ))
            })
            .collect();
        Self { textures }
    }

    /// How many sprites loaded.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Whether none loaded.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Sprite `name` stretched over `rect`, tinted by `tint`.
    pub fn shape(&self, name: &str, rect: Rect, tint: Color32) -> Option<Shape> {
        let t = self.textures.get(name)?;
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        Some(Shape::image(t.id(), rect, uv, tint))
    }

    /// Draws sprite `name` stretched over `rect`, tinted by `tint`.
    pub fn draw(&self, painter: &Painter, name: &str, rect: Rect, tint: Color32) {
        if let Some(shape) = self.shape(name, rect, tint) {
            painter.add(shape);
        }
    }

    /// The sprite's size in texels.
    pub fn size(&self, name: &str) -> Vec2 {
        self.textures
            .get(name)
            .map_or(Vec2::ZERO, |t| t.size_vec2())
    }

    /// A box of the nine `HUD_TEXTBOX_*` pieces over `rect`: corners `corner`
    /// units square, edges stretched between them, `HUD_TEXTBOX_BACK` filling
    /// the middle.
    pub fn text_box(&self, rect: Rect, corner: f32, tint: Color32) -> Shape {
        let c = corner.min(rect.width() / 2.0).min(rect.height() / 2.0);
        let xs = [rect.left(), rect.left() + c, rect.right() - c, rect.right()];
        let ys = [rect.top(), rect.top() + c, rect.bottom() - c, rect.bottom()];
        let pieces = [
            ["HUD_TEXTBOX_TL", "HUD_TEXTBOX_TM", "HUD_TEXTBOX_TR"],
            ["HUD_TEXTBOX_ML", "HUD_TEXTBOX_BACK", "HUD_TEXTBOX_MR"],
            ["HUD_TEXTBOX_BL", "HUD_TEXTBOX_BM", "HUD_TEXTBOX_BR"],
        ];
        let mut shapes = Vec::with_capacity(9);
        for (row, names) in pieces.iter().enumerate() {
            for (col, name) in names.iter().enumerate() {
                let r = Rect::from_min_max(
                    Pos2::new(xs[col], ys[row]),
                    Pos2::new(xs[col + 1], ys[row + 1]),
                );
                shapes.extend(self.shape(name, r, tint));
            }
        }
        Shape::Vec(shapes)
    }
}
