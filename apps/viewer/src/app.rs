//! The window: the viewer app, its panels and frame.

mod sim;

use crate::*;

#[derive(PartialEq, Eq)]
pub(crate) enum ListMode {
    Regions,
    Meshes,
    Textures,
    Text,
}

/// A texture shown in place of the 3D view.
pub(crate) struct TextureView {
    id: u32,
    name: String,
    handle: egui::TextureHandle,
    size: [usize; 2],
}

/// A scene being loaded on a background thread.
pub(crate) struct Loading {
    subject: Subject,
    rx: std::sync::mpsc::Receiver<Result<Loaded, String>>,
    started: std::time::Instant,
}

/// What a background load produces.
pub(crate) struct Loaded {
    scene: Scene,
    camera: OrbitCamera,
    status: String,
    hero: Option<walk::Hero>,
}

pub(crate) struct ViewerApp {
    /// The Tng sections (quest states) shown besides `NULL`.
    pub(crate) sections: Vec<String>,
    /// Sections chosen in the side panel; the region reloads with them.
    pub(crate) reload_sections: Option<Vec<String>>,
    /// A scene being loaded in the background, shown as a loading screen.
    pub(crate) loading: Option<Loading>,
    /// The quest being played, if this region has one, and what is on screen.
    pub(crate) story: Option<story::Story>,
    pub(crate) subs: talk::Subtitles,
    /// Sound output and the voices of lines, if they could be opened.
    pub(crate) audio: Option<audio::Audio>,
    pub(crate) voices: Option<audio::Voices>,
    pub(crate) music: soundscape::Music,
    pub(crate) ambience: soundscape::Ambience,
    /// Volumes set in the side panel: overall, voices, music, ambience.
    pub(crate) volumes: [f32; 4],
    /// Time of day in hours, and whether it moves on by itself.
    pub(crate) hour: f32,
    pub(crate) clock: bool,
    pub(crate) brightness: f32,
    pub(crate) highlights: f32,
    /// Animations, opened when first needed (gestures).
    pub(crate) anim_bank: Option<skin::AnimBank>,
    /// A mouth moving to a voice: the creature, the line's id, its lip sync
    /// track, and seconds since the voice began.
    pub(crate) lip: Option<(usize, u32, fable_data::lipsync::LipSync, f32)>,
    /// Someone gesturing as they speak: the creature, the line's id, the
    /// gestures to pick from, and seconds since the line began; and a counter
    /// for picking.
    pub(crate) talking: Option<(usize, u32, Vec<u32>, f32)>,
    /// The conversation camera: how far the view has eased into it (0..=1),
    /// and its shot (kept while easing back out).
    pub(crate) conversation: f32,
    pub(crate) conversation_camera: Option<OrbitCamera>,
    pub(crate) gesture_picks: u64,
    /// An answer clicked on a question, handled next frame.
    pub(crate) clicked_answer: Option<bool>,
    /// A point the Hero walks himself towards (`--walk-to`), until he is there.
    pub(crate) auto_walk: Option<walk::Follow>,
    /// Talk to whoever is near once `auto_walk` arrives (`--talk`).
    pub(crate) auto_talk: bool,
    /// The game's archives, opened once; background loads take clones.
    pub(crate) assets: AssetCache,
    pub(crate) wld: Wld,
    pub(crate) mesh_names: Vec<String>,
    pub(crate) textures: model::TextureBank,
    pub(crate) texture_names: Vec<(u32, String)>,
    pub(crate) texture_view: Option<TextureView>,
    /// The game's text (`lang/English/text.big`), if it loaded.
    pub(crate) text: Option<fable_data::strings::TextBank>,
    /// Entry names of the text, sorted, and the one shown.
    pub(crate) text_names: Vec<String>,
    pub(crate) text_view: Option<String>,
    /// Black over the view, fading out after a night passes (1 opaque).
    pub(crate) fade: f32,
    /// Turns frame times into fixed simulation ticks.
    pub(crate) time: engine_core::time::Clock,
    /// Preferred camera distance in walk mode (the scroll wheel changes it).
    pub(crate) follow_distance: f32,
    /// Region entrances, for region exits.
    pub(crate) gtg: Gtg,
    /// A region exit the Hero walked into, taken at the start of the next frame.
    pub(crate) pending_exit: Option<u64>,
    /// Exits trigger only after the Hero has been outside all of them, so arriving
    /// on an entrance next to an exit does not send him straight back.
    pub(crate) exits_armed: bool,
    pub(crate) want_walk: bool,
    pub(crate) list: ListMode,
    pub(crate) filter: String,
    pub(crate) subject: Subject,
    pub(crate) status: String,
    pub(crate) scene: Scene,
    pub(crate) camera: OrbitCamera,
    pub(crate) texture: Option<egui::TextureId>,
}

impl ViewerApp {
    pub(crate) fn new(
        rs: &egui_wgpu::RenderState,
        data_dir: PathBuf,
        wld: Wld,
        mesh_names: Vec<String>,
        subject: Subject,
        sections: Vec<String>,
    ) -> Result<Self, Error> {
        let sections = match &subject {
            Subject::Region(name) if sections.is_empty() => terrain::default_sections(name),
            _ => sections,
        };
        let assets = AssetCache::new(&data_dir);
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
        let list = match subject {
            Subject::Region(_) => ListMode::Regions,
            Subject::Mesh(_) => ListMode::Meshes,
        };
        let mut textures = model::TextureBank::open(&assets);
        let text = fable_data::strings::TextBank::open(&data_dir, "English")
            .map_err(|e| log::warn!("text: {e}"))
            .ok();
        let audio = audio::Audio::start()
            .map_err(|e| log::warn!("audio: {e}"))
            .ok();
        let voices = text.as_ref().and_then(|t| {
            audio::Voices::open(&data_dir, t)
                .map_err(|e| log::warn!("voices: {e}"))
                .ok()
        });
        if let Some(v) = &voices {
            log::info!("voices: {} lines voiced", v.len());
        }
        let text_names = text
            .as_ref()
            .map(|t| t.names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        let gtg = load_gtg(&data_dir)?;
        let texture_names = textures.names();
        Ok(Self {
            assets,
            wld,
            mesh_names,
            texture_names,
            texture_view: None,
            text,
            text_names,
            text_view: None,
            fade: 0.0,
            time: engine_core::time::Clock::new(),
            follow_distance: 6.0,
            gtg,
            pending_exit: None,
            loading: None,
            story,
            subs: talk::Subtitles::default(),
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
            reload_sections: None,
            auto_walk: None,
            auto_talk: false,
            exits_armed: false,
            want_walk: false,
            textures,
            list,
            filter: String::new(),
            subject,
            status,
            scene,
            camera,
            texture: None,
        })
    }

    /// Starts loading `subject` on a background thread; the window shows a
    /// loading screen until [`ViewerApp::finish_load`] installs it. `arrival`
    /// places the Hero at an entrance (position, facing) once a region loads.
    pub(crate) fn load(
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
    pub(crate) fn finish_load(&mut self, rs: &egui_wgpu::RenderState) -> bool {
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
    pub(crate) fn take_exit(&mut self, rs: &egui_wgpu::RenderState, connection: u64) {
        match resolve_exit(&self.wld, &self.gtg, connection) {
            Some((region, at, forward)) => {
                let sections = terrain::default_sections(&region);
                self.load(rs, Subject::Region(region), Some((at, forward)), sections)
            }
            None => self.status = format!("region exit {connection:#x} leads nowhere"),
        }
    }

    /// Returns a chosen region or mesh, and a chosen texture id.
    pub(crate) fn side_panel(&mut self, ui: &mut egui::Ui) -> (Option<Subject>, Option<u32>) {
        let mut chosen = None;
        let mut chosen_texture = None;
        let title = match &self.subject {
            Subject::Region(n) => self.region_title(n),
            Subject::Mesh(n) => n.clone(),
        };
        ui.heading(title);
        ui.label(&self.status);
        ui.label("Drag: orbit · Right-drag: pan · Scroll: zoom");
        if let Scene::Terrain(renderer, world) = &mut self.scene {
            ui.checkbox(&mut renderer.show_nav, "Show navigation");
            ui.checkbox(&mut self.want_walk, "Walk (WASD, Shift runs)");
            egui::CollapsingHeader::new("Time of day").show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut self.hour, 0.0..=23.99)
                        .text("Hour")
                        .custom_formatter(|h, _| {
                            format!("{:02}:{:02}", h as u32, (h.fract() * 60.0) as u32)
                        }),
                );
                ui.checkbox(&mut self.clock, "Clock runs (a day in 24 minutes)");
                ui.add(egui::Slider::new(&mut self.brightness, 0.5..=2.5).text("Brightness"));
                ui.add(egui::Slider::new(&mut self.highlights, 0.8..=3.0).text("Highlight limit"));
                if let Some(l) = &world.lighting {
                    ui.label(format!("Environment: {}", l.outside.0));
                }
            });
            if let Some(audio) = &self.audio {
                egui::CollapsingHeader::new("Sound").show(ui, |ui| {
                    let labels = ["Overall", "Voices", "Music", "Ambience"];
                    for (k, label) in labels.iter().enumerate() {
                        let slider = egui::Slider::new(&mut self.volumes[k], 0.0..=1.0)
                            .text(*label)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                        if ui.add(slider).changed() {
                            let v = self.volumes[k];
                            match k {
                                0 => audio.set_volume(v),
                                1 => audio.set_channel_volume(engine_audio::Channel::Voice, v),
                                2 => audio.set_channel_volume(engine_audio::Channel::Music, v),
                                _ => audio.set_channel_volume(engine_audio::Channel::Ambience, v),
                            }
                        }
                    }
                });
            }
            if !world.sections.is_empty() {
                egui::CollapsingHeader::new("Quest state (Tng sections)").show(ui, |ui| {
                    for section in &world.sections {
                        let mut on = self.sections.contains(section);
                        if ui.checkbox(&mut on, section).changed() {
                            let mut next = self.sections.clone();
                            next.retain(|s| s != section);
                            if on {
                                next.push(section.clone());
                            }
                            self.reload_sections = Some(next);
                        }
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.list, ListMode::Regions, "Regions");
            ui.selectable_value(&mut self.list, ListMode::Meshes, "Meshes");
            ui.selectable_value(&mut self.list, ListMode::Textures, "Textures");
            ui.selectable_value(&mut self.list, ListMode::Text, "Text");
        });
        ui.text_edit_singleline(&mut self.filter);
        let filter = self.filter.to_ascii_uppercase();
        egui::ScrollArea::vertical().show(ui, |ui| match self.list {
            ListMode::Regions => {
                for r in &self.wld.regions {
                    let label = self.region_title(&r.name);
                    if !label.to_ascii_uppercase().contains(&filter) {
                        continue;
                    }
                    let current = self.subject == Subject::Region(r.name.clone());
                    if ui.selectable_label(current, label).clicked() {
                        chosen = Some(Subject::Region(r.name.clone()));
                    }
                }
            }
            ListMode::Textures => {
                for (id, name) in self
                    .texture_names
                    .iter()
                    .filter(|(_, n)| n.contains(&filter))
                {
                    let current = self.texture_view.as_ref().is_some_and(|t| t.id == *id);
                    if ui.selectable_label(current, name).clicked() {
                        chosen_texture = Some(*id);
                    }
                }
            }
            ListMode::Text => {
                // Names, or text containing the search, up to a few hundred.
                let text = self.text.as_ref();
                let matches = self.text_names.iter().filter(|n| {
                    n.contains(&filter)
                        || (filter.len() > 2
                            && text
                                .and_then(|t| t.get(n))
                                .is_some_and(|s| s.to_ascii_uppercase().contains(&filter)))
                });
                for name in matches.take(500) {
                    let current = self.text_view.as_deref() == Some(name.as_str());
                    if ui.selectable_label(current, name).clicked() {
                        self.text_view = Some(name.clone());
                    }
                }
            }
            ListMode::Meshes => {
                for name in self.mesh_names.iter().filter(|n| n.contains(&filter)) {
                    let current = self.subject == Subject::Mesh(name.clone());
                    if ui.selectable_label(current, name).clicked() {
                        chosen = Some(Subject::Mesh(name.clone()));
                    }
                }
            }
        });
        (chosen, chosen_texture)
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state().cloned() else {
            return;
        };
        if self.finish_load(&rs) {
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
            return;
        }

        let (chosen, chosen_texture) = egui::Panel::left("list")
            .resizable(true)
            .show(ui, |ui| self.side_panel(ui))
            .inner;
        if let Some(subject) = chosen.filter(|s| *s != self.subject) {
            let sections = match &subject {
                Subject::Region(name) => terrain::default_sections(name),
                Subject::Mesh(_) => Vec::new(),
            };
            self.load(&rs, subject, None, sections);
        }
        if let Some(id) = chosen_texture {
            self.show_texture(ui.ctx(), id);
        }
        if let Some(sections) = self.reload_sections.take() {
            self.load(&rs, self.subject.clone(), None, sections);
        }
        if let Some(connection) = self.pending_exit.take() {
            self.take_exit(&rs, connection);
        }
        self.update_walk_mode(&rs);
        if self.list == ListMode::Text
            && let (Some(name), Some(text)) = (&self.text_view, &self.text)
        {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading(name);
                let id = text.id(name);
                let members: Vec<u32> = match id.and_then(|i| text.groups.get(&i)) {
                    Some(group) => {
                        ui.label(format!("A group of {} alternatives:", group.len()));
                        group.clone()
                    }
                    None => id.into_iter().collect(),
                };
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for m in members {
                        let Some(e) = text.strings.get(&m) else {
                            continue;
                        };
                        ui.separator();
                        let mut details = Vec::new();
                        if !e.speaker.is_empty() {
                            details.push(format!("speaker {}", e.speaker));
                        }
                        if !e.sound_bank.is_empty() {
                            details.push(format!("voice in {}", e.sound_bank));
                        }
                        if !e.tags.is_empty() {
                            details.push(e.tags.join(", "));
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  {}",
                                e.identifier,
                                details.join(" · ")
                            ))
                            .weak(),
                        );
                        ui.label(egui::RichText::new(&e.text).size(16.0));
                    }
                });
            });
            return;
        }
        if self.list == ListMode::Textures
            && let Some(view) = &self.texture_view
        {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(format!(
                    "{} (id {}): {}x{}",
                    view.name, view.id, view.size[0], view.size[1]
                ));
                let avail = ui.available_size();
                let scale = (avail.x / view.size[0] as f32)
                    .min(avail.y / view.size[1] as f32)
                    .min(4.0);
                let size = egui::vec2(view.size[0] as f32 * scale, view.size[1] as f32 * scale);
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((view.handle.id(), size)));
                });
            });
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
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
                    self.tick(&rs, &mut actions);
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
                    None => egui_renderer.register_native_texture(
                        &rs.device,
                        view,
                        wgpu::FilterMode::Linear,
                    ),
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
                let bar = rect.height() * 0.11 * conversation::ease(self.conversation);
                if bar > 0.5 {
                    let black = egui::Color32::BLACK;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar)),
                        0.0,
                        black,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(rect.min.x, rect.max.y - bar),
                            egui::vec2(rect.width(), bar),
                        ),
                        0.0,
                        black,
                    );
                }
                // A night passing: black, fading back in.
                if self.fade > 0.0 {
                    let alpha = (conversation::ease(self.fade.min(1.0)) * 255.0) as u8;
                    ui.painter()
                        .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(alpha));
                    self.fade -= ui.input(|i| i.stable_dt.min(0.1)) / NIGHT_FADE;
                    ui.ctx().request_repaint();
                }
                // What is being said, notices, and the Hero's gold and items.
                if let Some(answer) = self.subs.show(ui.ctx(), rect) {
                    self.clicked_answer = Some(answer);
                }
                if let (Some(story), Some(text), true) = (
                    &self.story,
                    &self.text,
                    self.hero().is_some() && self.conversation == 0.0,
                ) {
                    story.hud(ui.ctx(), rect, text);
                }
            });
        ui.ctx().request_repaint();
    }
}
