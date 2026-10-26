//! The viewer's window: the game session with the browser and debug panel
//! beside it.

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

pub(crate) struct ViewerApp {
    /// The game, played or watched.
    pub(crate) session: Session,
    /// Sections chosen in the side panel; the region reloads with them.
    pub(crate) reload_sections: Option<Vec<String>>,
    pub(crate) mesh_names: Vec<String>,
    pub(crate) textures: model::TextureBank,
    pub(crate) texture_names: Vec<(u32, String)>,
    pub(crate) texture_view: Option<TextureView>,
    /// Entry names of the text, sorted, and the one shown.
    pub(crate) text_names: Vec<String>,
    pub(crate) text_view: Option<String>,
    pub(crate) list: ListMode,
    pub(crate) filter: String,
}

impl ViewerApp {
    pub(crate) fn new(
        rs: &egui_wgpu::RenderState,
        data_dir: &Path,
        wld: Wld,
        mesh_names: Vec<String>,
        subject: Subject,
        sections: Vec<String>,
    ) -> Result<Self, Error> {
        let list = match subject {
            Subject::Region(_) => ListMode::Regions,
            Subject::Mesh(_) => ListMode::Meshes,
        };
        let session = Session::new(rs, data_dir, wld, subject, sections)?;
        let mut textures = model::TextureBank::open(&session.assets);
        let texture_names = textures.names();
        let text_names = session
            .text
            .as_ref()
            .map(|t| t.names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        Ok(Self {
            session,
            reload_sections: None,
            mesh_names,
            textures,
            texture_names,
            texture_view: None,
            text_names,
            text_view: None,
            list,
            filter: String::new(),
        })
    }

    /// Returns a chosen region or mesh, and a chosen texture id.
    pub(crate) fn side_panel(&mut self, ui: &mut egui::Ui) -> (Option<Subject>, Option<u32>) {
        let mut chosen = None;
        let mut chosen_texture = None;
        let title = match &self.session.subject {
            Subject::Region(n) => self.session.region_title(n),
            Subject::Mesh(n) => n.clone(),
        };
        ui.heading(title);
        ui.label(&self.session.status);
        ui.label("Drag: orbit · Right-drag: pan · Scroll: zoom");
        if let Scene::Terrain(renderer, world) = &mut self.session.scene {
            ui.checkbox(&mut renderer.show_nav, "Show navigation");
            ui.checkbox(&mut self.session.want_walk, "Walk (WASD, Shift runs)");
            egui::CollapsingHeader::new("Time of day").show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut self.session.hour, 0.0..=23.99)
                        .text("Hour")
                        .custom_formatter(|h, _| {
                            format!("{:02}:{:02}", h as u32, (h.fract() * 60.0) as u32)
                        }),
                );
                ui.checkbox(&mut self.session.clock, "Clock runs (a day in 24 minutes)");
                ui.add(
                    egui::Slider::new(&mut self.session.brightness, 0.5..=2.5).text("Brightness"),
                );
                ui.add(
                    egui::Slider::new(&mut self.session.highlights, 0.8..=3.0)
                        .text("Highlight limit"),
                );
                if let Some(l) = &world.lighting {
                    ui.label(format!("Environment: {}", l.outside.0));
                }
            });
            if let Some(audio) = &self.session.audio {
                egui::CollapsingHeader::new("Sound").show(ui, |ui| {
                    let labels = ["Overall", "Voices", "Music", "Ambience"];
                    for (k, label) in labels.iter().enumerate() {
                        let slider = egui::Slider::new(&mut self.session.volumes[k], 0.0..=1.0)
                            .text(*label)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                        if ui.add(slider).changed() {
                            let v = self.session.volumes[k];
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
                        let mut on = self.session.sections.contains(section);
                        if ui.checkbox(&mut on, section).changed() {
                            let mut next = self.session.sections.clone();
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
                for r in &self.session.wld.regions {
                    let label = self.session.region_title(&r.name);
                    if !label.to_ascii_uppercase().contains(&filter) {
                        continue;
                    }
                    let current = self.session.subject == Subject::Region(r.name.clone());
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
                let text = self.session.text.as_ref();
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
                    let current = self.session.subject == Subject::Mesh(name.clone());
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
        if self.session.loading_screen(ui, &rs) {
            return;
        }

        let (chosen, chosen_texture) = egui::Panel::left("list")
            .resizable(true)
            .show(ui, |ui| self.side_panel(ui))
            .inner;
        if let Some(subject) = chosen.filter(|s| *s != self.session.subject) {
            let sections = match &subject {
                Subject::Region(name) => terrain::default_sections(name),
                Subject::Mesh(_) => Vec::new(),
            };
            self.session.load(&rs, subject, None, sections);
        }
        if let Some(id) = chosen_texture {
            self.show_texture(ui.ctx(), id);
        }
        if let Some(sections) = self.reload_sections.take() {
            self.session
                .load(&rs, self.session.subject.clone(), None, sections);
        }
        self.session.update(&rs);
        if self.list == ListMode::Text
            && let (Some(name), Some(text)) = (&self.text_view, &self.session.text)
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
                self.session.view(ui, &rs);
            });
        ui.ctx().request_repaint();
    }
}
