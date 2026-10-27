//! The pause menu (Esc): the options backdrop over the whole screen, the PC
//! options frame on it, and the game's own wording. Its main page returns to
//! the game, opens the audio options or quits; the audio page sets the
//! volumes and returns to the game.

use egui::{Color32, Painter, Pos2, Rect, Sense, Vec2};

use crate::draw::Fonts;
use crate::sprites::Sprites;
use crate::text::BitmapFont;

/// The menu's text, from the game's text bank.
#[derive(Debug, Clone)]
pub struct Labels {
    pub paused: String,
    pub return_to_game: String,
    pub audio_options: String,
    pub quit: String,
    pub sound_volume: String,
    pub music_volume: String,
    pub dialogue_volume: String,
    /// What each main page item does.
    pub return_help: String,
    pub audio_help: String,
    pub quit_help: String,
}

impl Labels {
    /// The labels from `text` (an entry's text by name), each falling back to
    /// its English when missing.
    pub fn new(text: impl Fn(&str) -> Option<String>) -> Self {
        let t = |entry: &str, english: &str| text(entry).unwrap_or_else(|| english.to_owned());
        Self {
            paused: t("TEXT_GUI_MENU_PAUSED", "Paused"),
            return_to_game: t("TEXT_GUI_MENU_RETURN", "Return To Game"),
            audio_options: t("TEXT_GUI_MENU_AUDIO_OPTIONS", "Audio Options"),
            quit: t("TEXT_GUI_MENU_QUIT", "Quit"),
            sound_volume: t("TEXT_GUI_MENU_SOUND_VOLUME", "Sound Volume"),
            music_volume: t("TEXT_GUI_MENU_MUSIC_VOLUME", "Music Volume"),
            dialogue_volume: t("TEXT_GUI_MENU_DIALOGUE_VOLUME", "Dialogue Volume"),
            return_help: t(
                "TEXT_GUI_MENUGUIDE_PAUSEMENU_RETURNTOGAME",
                "Back to the game...",
            ),
            audio_help: t(
                "TEXT_GUI_MENUGUIDE_PAUSEMENU_AUDIOOPTIONS",
                "Adjust your audio preferences.",
            ),
            quit_help: t("TEXT_GUI_MENUGUIDE_PAUSEMENU_QUIT", "Quit this game."),
        }
    }
}

/// The volumes the audio page sets, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volumes {
    pub sound: f32,
    pub music: f32,
    pub dialogue: f32,
}

/// What the player chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Return,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    Main,
    Audio,
}

/// The pause menu's state.
#[derive(Debug, Default)]
pub struct PauseMenu {
    pub open: bool,
    page: Page,
}

/// The backdrop's tiles: a 640 by 512 picture in two rows of 256, 256 and 128.
const BACKDROP: [(&str, [f32; 4]); 6] = [
    ("UI_OPTIONS_BACKDROP_SPRITE_1", [0.0, 0.0, 256.0, 256.0]),
    ("UI_OPTIONS_BACKDROP_SPRITE_2", [256.0, 0.0, 256.0, 256.0]),
    ("UI_OPTIONS_BACKDROP_SPRITE_3", [512.0, 0.0, 128.0, 256.0]),
    ("UI_OPTIONS_BACKDROP_SPRITE_4", [0.0, 256.0, 256.0, 256.0]),
    ("UI_OPTIONS_BACKDROP_SPRITE_5", [256.0, 256.0, 256.0, 256.0]),
    ("UI_OPTIONS_BACKDROP_SPRITE_6", [512.0, 256.0, 128.0, 256.0]),
];

/// The frame's tiles: three rows of 256 (the last 128 high); the frame itself
/// fills 672 by 598 of them, its title bar the top 36.
const FRAME: [&str; 9] = [
    "PC_UI_OPTIONS_FRAME_1_SPRITE",
    "PC_UI_OPTIONS_FRAME_2_SPRITE",
    "PC_UI_OPTIONS_FRAME_3_SPRITE",
    "PC_UI_OPTIONS_FRAME_4_SPRITE",
    "PC_UI_OPTIONS_FRAME_5_SPRITE",
    "PC_UI_OPTIONS_FRAME_6_SPRITE",
    "PC_UI_OPTIONS_FRAME_7_SPRITE",
    "PC_UI_OPTIONS_FRAME_8_SPRITE",
    "PC_UI_OPTIONS_FRAME_9_SPRITE",
];
const FRAME_SIZE: Vec2 = Vec2::new(672.0, 598.0);
const TITLE_HEIGHT: f32 = 36.0;

const GOLD: Color32 = Color32::from_rgb(232, 200, 120);
const BRIGHT: Color32 = Color32::from_rgb(255, 236, 170);

impl PauseMenu {
    /// Opens the menu at its main page, or closes it.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.page = Page::Main;
    }

    /// Draws the open menu over `rect` and handles the pointer. Returns what
    /// was chosen.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        sprites: &Sprites,
        fonts: &Fonts,
        labels: &Labels,
        volumes: &mut Volumes,
    ) -> Option<Choice> {
        if !self.open {
            return None;
        }
        let painter = ui.painter().clone();
        for (name, [x, y, w, h]) in BACKDROP {
            let sx = rect.width() / 640.0;
            let sy = rect.height() / 512.0;
            let tile = Rect::from_min_size(
                rect.min + Vec2::new(x * sx, y * sy),
                Vec2::new(w * sx, h * sy),
            );
            sprites.draw(&painter, name, tile, Color32::WHITE);
        }
        // The frame, as big as fits in nine tenths of the screen.
        let scale = (rect.width() * 0.9 / FRAME_SIZE.x).min(rect.height() * 0.9 / FRAME_SIZE.y);
        let frame = Rect::from_center_size(rect.center(), FRAME_SIZE * scale);
        for (k, name) in FRAME.iter().enumerate() {
            let (col, row) = ((k % 3) as f32, (k / 3) as f32);
            let height = if row == 2.0 { 128.0 } else { 256.0 };
            let tile = Rect::from_min_size(
                frame.min + Vec2::new(col * 256.0, row * 256.0) * scale,
                Vec2::new(256.0, height) * scale,
            );
            sprites.draw(&painter, name, tile, Color32::WHITE);
        }
        let ppp = ui.ctx().pixels_per_point();
        let pick = Fonts::nearest;
        let big = pick(&fonts.body, 30.0 * scale * ppp)?;
        let body = pick(&fonts.body, 24.0 * scale * ppp)?;
        let small = pick(&fonts.small, 16.0 * scale * ppp)?;
        let px = 1.0 / ppp;
        // The title, centred in the title bar.
        let title_bar =
            Rect::from_min_size(frame.min, Vec2::new(frame.width(), TITLE_HEIGHT * scale));
        let title = match self.page {
            Page::Main => &labels.paused,
            Page::Audio => &labels.audio_options,
        };
        centred(
            &painter,
            big,
            px,
            title,
            title_bar.center(),
            Color32::from_gray(235),
        );

        let line = body.line_height(px) * 1.8;
        let mut y = frame.top() + 110.0 * scale;
        let mut chosen = None;
        let mut help = None;
        match self.page {
            Page::Main => {
                for (k, (label, what)) in [
                    (&labels.return_to_game, &labels.return_help),
                    (&labels.audio_options, &labels.audio_help),
                    (&labels.quit, &labels.quit_help),
                ]
                .into_iter()
                .enumerate()
                {
                    let at = Pos2::new(frame.center().x, y);
                    let (clicked, hovered) = item(ui, &painter, body, px, label, at);
                    if hovered {
                        help = Some(what.as_str());
                    }
                    if clicked {
                        match k {
                            0 => chosen = Some(Choice::Return),
                            1 => self.page = Page::Audio,
                            _ => chosen = Some(Choice::Quit),
                        }
                    }
                    y += line;
                }
            }
            Page::Audio => {
                let bar_width = 260.0 * scale;
                for (label, value) in [
                    (&labels.sound_volume, &mut volumes.sound),
                    (&labels.music_volume, &mut volumes.music),
                    (&labels.dialogue_volume, &mut volumes.dialogue),
                ] {
                    let label_at = Pos2::new(frame.left() + 70.0 * scale, y);
                    body.draw(&painter, label_at, label, px, GOLD);
                    let bar = Rect::from_min_size(
                        Pos2::new(frame.right() - 70.0 * scale - bar_width, y + 4.0 * scale),
                        Vec2::new(bar_width, 14.0 * scale),
                    );
                    slider(ui, &painter, sprites, bar, value);
                    y += line;
                }
                y += line * 0.5;
                let at = Pos2::new(frame.center().x, y);
                let (clicked, hovered) = item(ui, &painter, body, px, &labels.return_to_game, at);
                if hovered {
                    help = Some(labels.return_help.as_str());
                }
                if clicked {
                    chosen = Some(Choice::Return);
                }
            }
        }
        if let Some(help) = help {
            let foot = Pos2::new(frame.center().x, frame.bottom() - 60.0 * scale);
            centred(&painter, small, px, help, foot, Color32::from_gray(200));
        }
        if chosen.is_some() {
            self.open = false;
        }
        chosen
    }
}

/// `text` centred on `centre`.
fn centred(
    painter: &Painter,
    font: &BitmapFont,
    px: f32,
    text: &str,
    centre: Pos2,
    colour: Color32,
) {
    let size = Vec2::new(font.width(text, px), font.line_height(px));
    font.draw(painter, centre - size / 2.0, text, px, colour);
}

/// A menu item centred on `at`'s x, its top at `at`'s y: gold, brighter
/// under the pointer. Whether it was clicked, and whether it is hovered.
fn item(
    ui: &mut egui::Ui,
    painter: &Painter,
    font: &BitmapFont,
    px: f32,
    text: &str,
    at: Pos2,
) -> (bool, bool) {
    let size = Vec2::new(font.width(text, px), font.line_height(px));
    let rect = Rect::from_min_size(Pos2::new(at.x - size.x / 2.0, at.y), size);
    let response = ui.interact(rect, ui.id().with(text), Sense::click());
    let colour = if response.hovered() { BRIGHT } else { GOLD };
    font.draw(painter, rect.min, text, px, colour);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (response.clicked(), response.hovered())
}

/// A volume bar over `bar`, filled to `value`, which clicking or dragging
/// sets.
fn slider(ui: &mut egui::Ui, painter: &Painter, sprites: &Sprites, bar: Rect, value: &mut f32) {
    let response = ui.interact(bar, ui.id().with(bar.min.y as i32), Sense::click_and_drag());
    if let Some(p) = response.interact_pointer_pos()
        && (response.clicked() || response.dragged())
    {
        *value = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
    }
    sprites.draw(painter, "UI_BAR_WHITE_SEGMENT", bar, Color32::from_gray(50));
    let filled = Rect::from_min_size(bar.min, Vec2::new(bar.width() * *value, bar.height()));
    sprites.draw(painter, "UI_BAR_WHITE_SEGMENT", filled, GOLD);
    let end = Rect::from_min_size(
        Pos2::new(filled.right() - bar.height() * 0.25, bar.top()),
        Vec2::new(bar.height() * 0.5, bar.height()),
    );
    sprites.draw(painter, "UI_BAR_WHITE_END", end, BRIGHT);
}
