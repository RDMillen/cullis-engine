//! A game session: one window's game, from loading a region to drawing it.

mod sim;

use crate::*;

/// A scene being loaded on a background thread.
pub struct Loading {
    subject: Subject,
    rx: std::sync::mpsc::Receiver<Result<Loaded, String>>,
    started: std::time::Instant,
}

/// What a background load produces.
pub struct Loaded {
    scene: Scene,
    camera: OrbitCamera,
    status: String,
    hero: Option<walk::Hero>,
}

/// A game being played in a window: the scene and its world, the quest,
/// what is said, sound, the camera and the clock.
pub struct Session {
    /// The Tng sections (quest states) shown besides `NULL`.
    pub sections: Vec<String>,
    /// A scene being loaded in the background, shown as a loading screen.
    pub loading: Option<Loading>,
    /// The quest being played, if this region has one, and what is on screen.
    pub story: Option<story::Story>,
    pub subs: subtitles::Subtitles,
    /// Sound output and the voices of lines, if they could be opened.
    pub audio: Option<audio::Audio>,
    pub voices: Option<audio::Voices>,
    pub music: soundscape::Music,
    pub ambience: soundscape::Ambience,
    /// Volumes set in the side panel: overall, voices, music, ambience.
    pub volumes: [f32; 4],
    /// Time of day in hours, and whether it moves on by itself.
    pub hour: f32,
    pub clock: bool,
    pub brightness: f32,
    pub highlights: f32,
    /// Animations, opened when first needed (gestures).
    pub anim_bank: Option<skin::AnimBank>,
    /// A mouth moving to a voice: the creature, the line's id, its lip sync
    /// track, and seconds since the voice began.
    pub lip: Option<(usize, u32, fable_data::lipsync::LipSync, f32)>,
    /// Someone gesturing as they speak: the creature, the line's id, the
    /// gestures to pick from, and seconds since the line began; and a counter
    /// for picking.
    pub talking: Option<(usize, u32, Vec<u32>, f32)>,
    /// The conversation camera: how far the view has eased into it (0..=1),
    /// and its shot (kept while easing back out).
    pub conversation: f32,
    pub conversation_camera: Option<OrbitCamera>,
    pub gesture_picks: u64,
    /// An answer clicked on a question, handled next frame.
    pub clicked_answer: Option<bool>,
    /// A point the Hero walks himself towards (`--walk-to`), until he is there.
    pub auto_walk: Option<walk::Follow>,
    /// Talk to whoever is near once `auto_walk` arrives (`--talk`).
    pub auto_talk: bool,
    /// The game's archives, opened once; background loads take clones.
    pub assets: AssetCache,
    pub wld: Wld,
    /// The game's text (`lang/English/text.big`), if it loaded.
    pub text: Option<fable_data::strings::TextBank>,
    /// Black over the view, fading out after a night passes (1 opaque).
    pub fade: f32,
    /// Turns frame times into fixed simulation ticks.
    pub time: engine_core::time::Clock,
    /// Preferred camera distance in walk mode (the scroll wheel changes it).
    pub follow_distance: f32,
    /// Region entrances, for region exits.
    pub gtg: Gtg,
    /// A region exit the Hero walked into, taken at the start of the next frame.
    pub pending_exit: Option<u64>,
    /// Exits trigger only after the Hero has been outside all of them, so arriving
    /// on an entrance next to an exit does not send him straight back.
    pub exits_armed: bool,
    pub want_walk: bool,
    pub subject: Subject,
    pub status: String,
    pub scene: Scene,
    pub camera: OrbitCamera,
    pub texture: Option<egui::TextureId>,
    /// The HUD's sprites, loaded on the first frame drawn; the gold box; what
    /// using would do now.
    pub sprites: Option<engine_ui::Sprites>,
    /// The region whose map the sprites hold.
    pub minimap_region: Option<String>,
    /// The game's fonts, loaded on the first frame drawn (`None` inside if
    /// they could not be read).
    pub fonts: Option<Option<engine_ui::draw::Fonts>>,
    pub gold_popup: engine_ui::GoldPopup,
    pub action: Option<engine_ui::Action>,
}

impl Session {
    /// Opens the game's data and loads `subject` (`sections` empty for the
    /// region's starting quest state).
    pub fn new(
        rs: &egui_wgpu::RenderState,
        data_dir: &Path,
        wld: Wld,
        subject: Subject,
        sections: Vec<String>,
    ) -> Result<Self, Error> {
        let sections = match &subject {
            Subject::Region(name) if sections.is_empty() => terrain::default_sections(name),
            _ => sections,
        };
        let assets = AssetCache::new(data_dir);
        let (mut scene, camera, status) =
            Scene::load(&assets, &wld, &subject, &sections, &rs.device, &rs.queue)?;
        // The region's quest, if it has one, with its people placed.
        let story = match &subject {
            Subject::Region(name) => story::Story::for_region(name, &sections),
            Subject::Mesh(_) => None,
        };
        let mut story = story;
        if let (Some(story), Scene::Terrain(_renderer, world)) = (&mut story, &mut scene) {
            story.place(&mut world.npcs, &assets);
        }
        let text = fable_data::strings::TextBank::open(data_dir, "English")
            .map_err(|e| log::warn!("text: {e}"))
            .ok();
        let audio = audio::Audio::start()
            .map_err(|e| log::warn!("audio: {e}"))
            .ok();
        let voices = text.as_ref().and_then(|t| {
            audio::Voices::open(data_dir, t)
                .map_err(|e| log::warn!("voices: {e}"))
                .ok()
        });
        if let Some(v) = &voices {
            log::info!("voices: {} lines voiced", v.len());
        }
        let gtg = load_gtg(data_dir)?;
        Ok(Self {
            assets,
            wld,
            text,
            fade: 0.0,
            time: engine_core::time::Clock::new(),
            follow_distance: 6.0,
            gtg,
            pending_exit: None,
            loading: None,
            story,
            subs: subtitles::Subtitles::default(),
            audio,
            voices,
            music: Default::default(),
            ambience: Default::default(),
            anim_bank: None,
            lip: None,
            talking: None,
            conversation: 0.0,
            conversation_camera: None,
            gesture_picks: 0,
            hour: DEFAULT_HOUR,
            clock: false,
            brightness: lighting::BRIGHTNESS,
            highlights: lighting::HIGHLIGHT_LIMIT,
            volumes: [
                1.0,
                1.0,
                soundscape::MUSIC_VOLUME,
                soundscape::AMBIENCE_VOLUME,
            ],
            clicked_answer: None,
            sections,
            auto_walk: None,
            auto_talk: false,
            exits_armed: false,
            want_walk: false,
            subject,
            status,
            scene,
            camera,
            texture: None,
            sprites: None,
            minimap_region: None,
            fonts: None,
            gold_popup: Default::default(),
            action: None,
        })
    }

    /// Starts loading `subject` on a background thread; the window shows a
    /// loading screen until [`Session::finish_load`] installs it. `arrival`
    /// places the Hero at an entrance (position, facing) once a region loads.
    pub fn load(
        &mut self,
        rs: &egui_wgpu::RenderState,
        subject: Subject,
        arrival: Option<(glam::Vec3, glam::Vec3)>,
        sections: Vec<String>,
    ) {
        self.sections = sections.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (assets, wld, device, queue) = (
            self.assets.clone(),
            self.wld.clone(),
            rs.device.clone(),
            rs.queue.clone(),
        );
        let job = subject.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Loaded, Error> {
                let (mut scene, mut camera, status) =
                    Scene::load(&assets, &wld, &job, &sections, &device, &queue)?;
                let mut hero = None;
                if let (Some((at, forward)), Scene::Terrain(renderer, world)) =
                    (arrival, &mut scene)
                {
                    let mut h = spawn_hero(
                        renderer,
                        world,
                        &assets,
                        &device,
                        &queue,
                        at.truncate(),
                        Some(at.z),
                    )?
                    .ok_or("nowhere walkable at the entrance")?;
                    h.heading = (-forward.x).atan2(forward.y);
                    if let Some((_, transform)) = &mut renderer.hero {
                        *transform = hero_transform(&h);
                    }
                    camera.yaw = h.heading - std::f32::consts::FRAC_PI_2;
                    hero = Some(h);
                }
                Ok(Loaded {
                    scene,
                    camera,
                    status,
                    hero,
                })
            })();
            // The window may have gone; nothing to do then.
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
        // Nothing from the old scene carries over: no queued exit, no scripted walk.
        self.pending_exit = None;
        self.exits_armed = false;
        self.auto_walk = None;
        self.loading = Some(Loading {
            subject,
            rx,
            started: std::time::Instant::now(),
        });
    }

    /// Installs a finished background load. Returns whether one is still running.
    pub fn finish_load(&mut self, rs: &egui_wgpu::RenderState) -> bool {
        let Some(loading) = &self.loading else {
            return false;
        };
        let result = match loading.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("loader stopped".into()),
        };
        let loading = self.loading.take().unwrap();
        let subject = loading.subject;
        match result {
            Ok(loaded) => {
                self.scene = loaded.scene;
                self.camera = loaded.camera;
                self.status = format!(
                    "{} (loaded in {:.1} s)",
                    loaded.status,
                    loading.started.elapsed().as_secs_f32()
                );
                if let Some(id) = self.texture.take() {
                    rs.renderer.write().free_texture(&id);
                }
                self.subject = subject;
                self.want_walk = false;
                self.subs.clear();
                if let Subject::Region(region) = &self.subject
                    && self.story.is_none()
                {
                    self.story = story::Story::for_region(region, &self.sections);
                }
                if let (Some(story), Scene::Terrain(_renderer, world)) =
                    (&mut self.story, &mut self.scene)
                {
                    story.place(&mut world.npcs, &self.assets);
                }
                if let (Some(hero), Scene::Terrain(renderer, world)) =
                    (loaded.hero, &mut self.scene)
                {
                    follow(
                        &mut self.camera,
                        &hero,
                        renderer,
                        world,
                        self.follow_distance,
                    );
                    world.hero = Some(hero);
                    self.want_walk = true;
                    self.exits_armed = false;
                }
            }
            Err(e) => self.status = format!("{subject:?}: {e}"),
        }
        false
    }

    /// Starts loading the region a region exit leads to, with the Hero arriving at
    /// its entrance.
    pub fn take_exit(&mut self, rs: &egui_wgpu::RenderState, connection: u64) {
        match resolve_exit(&self.wld, &self.gtg, connection) {
            Some((region, at, forward)) => {
                let sections = terrain::default_sections(&region);
                self.load(rs, Subject::Region(region), Some((at, forward)), sections)
            }
            None => self.status = format!("region exit {connection:#x} leads nowhere"),
        }
    }

    /// While a load runs, the loading screen (the region's name); installs a
    /// finished load. Returns whether the loading screen is up.
    pub fn loading_screen(&mut self, ui: &mut egui::Ui, rs: &egui_wgpu::RenderState) -> bool {
        if self.finish_load(rs) {
            // The loading screen: the region's name while it loads.
            let name = match &self.loading.as_ref().unwrap().subject {
                Subject::Region(n) => self.region_display_name(n),
                Subject::Mesh(n) => n.clone(),
            };
            egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 2.0 - 40.0);
                        ui.heading(format!("Loading {name}…"));
                        ui.add(egui::Spinner::new().size(32.0));
                    });
                });
            });
            ui.ctx().request_repaint();
            return true;
        }
        false
    }

    /// Per frame, before drawing: takes a region exit the Hero walked into and
    /// spawns or removes the Hero as `want_walk` asks.
    pub fn update(&mut self, rs: &egui_wgpu::RenderState) {
        if let Some(connection) = self.pending_exit.take() {
            self.take_exit(rs, connection);
        }
        self.update_walk_mode(rs);
    }

    /// The 3D view filling `ui`: camera controls, the fixed-tick simulation,
    /// the scene, letterbox and night fades, subtitles and the HUD.
    pub fn view(&mut self, ui: &mut egui::Ui, rs: &egui_wgpu::RenderState) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::drag());
        let delta = response.drag_delta();
        if response.dragged_by(egui::PointerButton::Primary) {
            self.camera.orbit(delta.x, delta.y);
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.camera.pan(delta.x, delta.y);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if self.hero().is_some() {
                self.follow_distance =
                    (self.follow_distance * (-scroll * 0.002).exp()).clamp(2.0, 15.0);
            } else {
                self.camera.zoom(scroll);
            }
        }
        // The simulation runs in fixed ticks; what is only seen
        // (camera framing, highlight, fades) runs per frame.
        let frame_dt = ui.input(|i| i.stable_dt.min(0.1));
        let ticks = self.time.advance(frame_dt);
        let mut actions = ui.input(Actions::read);
        for _ in 0..ticks {
            self.tick(rs, &mut actions);
        }
        self.speak();
        self.frame_conversation(frame_dt);
        self.choose_highlight(ui.input(|i| i.time) as f32);
        self.play_soundscape(frame_dt);
        if self.hero().is_some() || ticks > 0 {
            ui.ctx().request_repaint();
        }
        if let Scene::Terrain(_renderer, world) = &mut self.scene
            && !world.npcs.is_empty()
        {
            ui.ctx().request_repaint();
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        let view_proj = self
            .shown_camera()
            .view_proj(rect.width() / rect.height().max(1.0));
        if self.clock {
            ui.ctx().request_repaint();
        }
        if let Scene::Terrain(renderer, world) = &mut self.scene {
            world.set_hour(self.hour, self.brightness, self.highlights);
            if renderer.particles.is_some() {
                ui.ctx().request_repaint();
            }
        }
        let recreated = self.scene.render(&rs.device, &rs.queue, size, view_proj);
        let view = &self.scene.target().unwrap().color_view;
        let mut egui_renderer = rs.renderer.write();
        let id = match self.texture {
            Some(id) if recreated => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    &rs.device,
                    view,
                    wgpu::FilterMode::Linear,
                    id,
                );
                id
            }
            Some(id) => id,
            None => {
                egui_renderer.register_native_texture(&rs.device, view, wgpu::FilterMode::Linear)
            }
        };
        self.texture = Some(id);
        drop(egui_renderer);
        ui.painter().image(
            id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        // Letterbox bars while the conversation camera is on.
        draw::letterbox(ui.painter(), rect, conversation::ease(self.conversation));
        // A night passing: black, fading back in.
        if self.fade > 0.0 {
            draw::fade(ui.painter(), rect, conversation::ease(self.fade.min(1.0)));
            self.fade -= ui.input(|i| i.stable_dt.min(0.1)) / NIGHT_FADE;
            ui.ctx().request_repaint();
        }
        // What is being said and notices, and the HUD while walking.
        let walking = self.hero().is_some() && self.conversation == 0.0;
        let sprites = self.sprites.get_or_insert_with(|| {
            let mut assets = self.assets.clone();
            let mut bank = model::TextureBank::open(&assets);
            let sprites = engine_ui::Sprites::load(ui.ctx(), |name| {
                let id = assets.id_of(engine_core::assets::Archive::Textures, name)?;
                let image = bank.load(id).ok()?;
                Some(([image.width as usize, image.height as usize], image.rgba))
            });
            log::info!(
                "HUD: {} of {} sprites",
                sprites.len(),
                engine_ui::sprites::NAMES.len()
            );
            sprites
        });
        // The region's map, when the region changes.
        let region = match &self.subject {
            Subject::Region(name) => Some(name.clone()),
            Subject::Mesh(_) => None,
        };
        if self.minimap_region != region {
            let mut assets = self.assets.clone();
            let mut bank = model::TextureBank::open(&assets);
            let image = region.as_ref().and_then(|name| {
                let texture = format!("MINIMAP_{}", name.to_ascii_uppercase());
                let id = assets.id_of(engine_core::assets::Archive::Textures, &texture)?;
                let image = bank.load(id).ok()?;
                Some(([image.width as usize, image.height as usize], image.rgba))
            });
            log::info!(
                "minimap for {region:?}: {}",
                if image.is_some() { "found" } else { "none" }
            );
            sprites.set(ui.ctx(), engine_ui::sprites::MINIMAP, image);
            self.minimap_region = region;
        }
        // The quest's people on the map: its goal with the orb, the others it
        // has something for with green dots.
        let mut markers = Vec::new();
        if let (Scene::Terrain(_, world), Some(story)) = (&self.scene, &self.story)
            && let Some(area) = world.map_area
        {
            let goal = story.goal();
            for npc in world.npcs.iter().filter(|n| !n.hidden) {
                let Some(role) = npc.script_name.as_deref() else {
                    continue;
                };
                let marker = if Some(role) == goal {
                    engine_ui::Marker::QuestGoal
                } else if story.of_interest(role) {
                    engine_ui::Marker::Person
                } else {
                    continue;
                };
                let uv = area.uv(npc.position());
                markers.push((egui::vec2(uv.x, uv.y), marker));
            }
        }
        let on_map = match &self.scene {
            Scene::Terrain(_, world) => {
                world.map_area.zip(world.hero.as_ref()).map(|(area, hero)| {
                    let uv = area.uv(hero.position.truncate());
                    (egui::vec2(uv.x, uv.y), hero.heading)
                })
            }
            Scene::Model(_) => None,
        };
        let fonts = self
            .fonts
            .get_or_insert_with(|| {
                let mut assets = self.assets.clone();
                let mut load = |name: &str| {
                    let id = assets.id_of(engine_core::assets::Archive::Fonts, name)?;
                    let (_, data) = assets
                        .read(engine_core::assets::Archive::Fonts, id)
                        .map_err(|e| log::warn!("font {name}: {e}"))
                        .ok()?;
                    let font = fable_data::font::Font::parse(&data)
                        .map_err(|e| log::warn!("font {name}: {e}"))
                        .ok()?;
                    Some(engine_ui::text::BitmapFont::load(ui.ctx(), name, &font))
                };
                let fonts = Some(engine_ui::draw::Fonts {
                    body: load("ENG_ARIAL_16")?,
                    small: load("ENG_TAHOMA_10")?,
                });
                log::info!("fonts: {}", if fonts.is_some() { "loaded" } else { "none" });
                fonts
            })
            .as_ref();
        if let Some(answer) = draw::subtitles(ui.ctx(), rect, &self.subs, sprites, fonts) {
            self.clicked_answer = Some(answer);
        }
        let dt = ui.input(|i| i.stable_dt.min(0.1));
        self.gold_popup
            .update(self.story.as_ref().and_then(story::Story::gold), dt);
        if walking {
            let hud = engine_ui::Hud {
                health: 1.0,
                action: self.action,
                on_map,
                hour: self.hour,
                markers,
            };
            engine_ui::hud::draw(ui.painter(), rect, sprites, &hud, &self.gold_popup);
            if self.gold_popup.visible().is_some() {
                ui.ctx().request_repaint();
            }
        }
    }
}
