//! The app's simulation steps and quest effects.

use crate::*;

impl ViewerApp {
    /// A region's name in the game's text (`TXT_REGION_OAKVALE` -> "Oakvale"), or
    /// its own name.
    pub(crate) fn region_display_name(&self, region: &str) -> String {
        self.wld
            .region(region)
            .and_then(|r| self.text.as_ref()?.get(&r.display_name))
            .filter(|t| !t.is_empty())
            .map_or_else(|| region.to_owned(), str::to_owned)
    }

    /// "Oakvale (OakBay)": the game's name and the wld's.
    pub(crate) fn region_title(&self, region: &str) -> String {
        let shown = self.region_display_name(region);
        if shown == region {
            shown
        } else {
            format!("{shown} ({region})")
        }
    }

    /// Spawns or removes the Hero when the walk checkbox changes.
    pub(crate) fn update_walk_mode(&mut self, rs: &egui_wgpu::RenderState) {
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        match (self.want_walk, world.hero.is_some()) {
            (true, false) => {
                match spawn_hero(
                    renderer,
                    world,
                    &self.assets,
                    &rs.device,
                    &rs.queue,
                    self.camera.target.truncate(),
                    None,
                ) {
                    Ok(Some(hero)) => {
                        follow(
                            &mut self.camera,
                            &hero,
                            renderer,
                            world,
                            self.follow_distance,
                        );
                        world.hero = Some(hero);
                    }
                    Ok(None) => {
                        self.status = "nowhere walkable in this region".into();
                        self.want_walk = false;
                    }
                    Err(e) => {
                        self.status = format!("hero: {e}");
                        self.want_walk = false;
                    }
                }
            }
            (false, true) => {
                renderer.hero = None;
                world.hero = None;
            }
            _ => {}
        }
    }

    /// Moves the Hero from the keyboard, relative to the camera, and follows it.
    /// The Hero, when he is in the world.
    pub(crate) fn hero(&self) -> Option<&walk::Hero> {
        match &self.scene {
            Scene::Terrain(_, world) => world.hero.as_ref(),
            Scene::Model(_) => None,
        }
    }

    /// One simulation tick ([`TICK`] seconds): the Hero, what is being said
    /// and its effects, gestures and mouths, creatures, doors, the clock and
    /// particles.
    pub(crate) fn tick(&mut self, rs: &egui_wgpu::RenderState, actions: &mut Actions) {
        let dt = TICK;
        self.walk(actions, dt);
        let mut effects = Vec::new();
        self.subs.step(dt, &mut effects);
        self.gesture(dt);
        self.move_mouth(dt);
        for effect in effects {
            self.apply(rs, effect);
        }
        if self.clock {
            self.hour = (self.hour + dt * CLOCK_RATE).rem_euclid(24.0);
        }
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        let (_layers, _heights) = (&world.walk, world.heights.as_ref());
        let hero = world.hero.as_ref().map(|h| h.position.truncate());
        world.step_creatures(dt, hero, PERSON_RADIUS);
        world.tick(dt);
        let focus = world.focus;
        if let Some(p) = &mut renderer.particles {
            p.system.update(dt, focus);
        }
    }

    /// One simulation tick of the walking Hero: `input` this frame's actions
    /// (presses are taken, so a frame running two ticks acts once), `dt` the
    /// tick's length.
    pub(crate) fn walk(&mut self, input: &mut Actions, dt: f32) {
        if self.loading.is_some() {
            return;
        }
        // The Hero comes out of the world for his step, which reads the world
        // around him, and goes back after.
        let Scene::Terrain(_, world) = &mut self.scene else {
            return;
        };
        let Some(mut hero) = world.hero.take() else {
            return;
        };
        self.walk_with(&mut hero, input, dt);
        if let Scene::Terrain(_, world) = &mut self.scene {
            world.hero = Some(hero);
        }
    }

    pub(crate) fn walk_with(&mut self, hero: &mut walk::Hero, input: &mut Actions, dt: f32) {
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        let (walk, Some(heights)) = (&world.walk, &world.heights) else {
            return;
        };
        let (forward, right, run) = (input.forward, input.right, input.run);
        let view = (self.camera.target - self.camera.eye())
            .truncate()
            .normalize_or_zero();
        let side = glam::Vec2::new(view.y, -view.x);
        let mut direction = view * forward + side * right;
        // A walk to a point (`--walk-to`) along a route, while no key steers.
        // In a conversation or a quest's closing scene the Hero stays put.
        let held = self.subs.in_scene();
        if held {
            direction = glam::Vec2::ZERO;
        }
        if direction == glam::Vec2::ZERO
            && !held
            && let Some(follow) = &mut self.auto_walk
        {
            let (doors, npcs) = (&**world, &world.npcs);
            match follow.direction(
                hero,
                walk,
                heights,
                dt,
                |p| blocked_for_route(doors, npcs, p),
                |p| blocked_for_hero(doors, npcs, p),
            ) {
                Some(to) => direction = to,
                None => {
                    let goal = follow.goal;
                    self.auto_walk = None;
                    let at = hero.position.truncate();
                    log::info!(
                        "walk to {goal}: ended at {} ({:.1} away)",
                        hero.position,
                        at.distance(goal)
                    );
                    if std::mem::take(&mut self.auto_talk)
                        && let (Some(text), Some(story)) = (&self.text, &mut self.story)
                        && let Some(i) =
                            story::nearest_person(&world.npcs, at, story::TALK_REACH, None)
                    {
                        story.talk(&world.npcs, i, at, text, &mut self.subs);
                    }
                }
            }
        }
        let at = hero.position.truncate();
        let (talk_key, kick_key, answer_key) = (
            std::mem::take(&mut input.use_),
            std::mem::take(&mut input.kick),
            input.answer.take(),
        );
        let mut use_door = None;
        // E: go on with what is being said, else talk to someone near, else use a door.
        if talk_key && !self.subs.advance() && self.subs.asking().is_none() {
            let near = story::nearest_person(&world.npcs, at, story::TALK_REACH, None);
            if let (Some(story), Some(text), Some(i)) = (&mut self.story, &self.text, near) {
                story.talk(&world.npcs, i, at, text, &mut self.subs);
            } else {
                // Used at the end of the step, once the walk grids are free.
                use_door = world.nearest_door(hero.position);
            }
        }
        // Y/N (or a click): answer the question on screen.
        if let Some(yes) = answer_key.or(self.clicked_answer.take())
            && self.subs.asking().is_some()
            && let (Some(story), Some(text)) = (&mut self.story, &self.text)
        {
            story.answer(yes, &world.npcs, at, text, &mut self.subs);
        }
        // F: kick whoever is in front.
        if kick_key && !held && world.start_kick() {
            let facing = glam::Vec2::new(-hero.heading.sin(), hero.heading.cos());
            if let Some(i) = story::nearest_person(&world.npcs, at, story::KICK_REACH, Some(facing))
            {
                world.npcs[i].flinch();
                if let (Some(story), Some(text)) = (&mut self.story, &self.text) {
                    story.hit(&world.npcs, i, at, text, &mut self.subs);
                }
            }
        }
        // Walking away ends a conversation.
        if let Some(i) = self.subs.waiting_on()
            && world
                .npcs
                .get(i)
                .is_none_or(|n| n.position().distance(at) > 4.0)
        {
            self.subs.walked_away();
        }
        if let (Some(story), Some(text)) = (&mut self.story, &self.text) {
            let height = hero.position.z;
            story.update(dt, &world.npcs, (at, height), text, &mut self.subs);
        }
        world.move_hero(hero, direction, run, dt);
        if let Some((_, transform)) = &mut renderer.hero {
            *transform = hero_transform(hero);
        }
        if world.update_interior(hero.position) {
            renderer.set_inside(world.inside);
        }
        follow(
            &mut self.camera,
            hero,
            renderer,
            world,
            self.follow_distance,
        );
        match exit_at(world, hero) {
            Some(connection) if self.exits_armed => self.pending_exit = Some(connection),
            Some(_) => {}
            None => self.exits_armed = true,
        }
        if let Some(d) = use_door {
            world.toggle_door(d);
        }
    }

    /// Plays the region's music and the ambience where the Hero (or, when not
    /// walking, the camera) is.
    pub(crate) fn play_soundscape(&mut self, dt: f32) {
        let Some(audio) = &self.audio else {
            return;
        };
        // While a region loads, what plays carries on.
        if self.loading.is_some() {
            return;
        }
        let (music, bank, night, indoors) = match &self.scene {
            Scene::Terrain(_, world) => {
                let at = world
                    .hero
                    .as_ref()
                    .map_or(self.camera.target.truncate(), |h| h.position.truncate());
                // Night when the sun's light is low (as the lighting reckons it).
                let sun = world.light.sun.iter().sum::<f32>() / 3.0;
                (
                    world.soundscape.music.clone(),
                    world.soundscape.bank_at(at).cloned(),
                    world.lighting.is_some() && sun < 0.3,
                    world.inside.is_some(),
                )
            }
            Scene::Model(_) => (None, None, false, false),
        };
        self.music.update(music.as_deref(), audio);
        self.ambience
            .update(dt, bank.as_deref(), night, indoors, audio);
    }

    /// Picks what glows as usable: whoever E would talk to (someone the quest
    /// has something for, in reach), else the door E would use. Green, gently
    /// pulsing; nothing during a scene or when not walking. Also tells the
    /// renderer where the camera is.
    pub(crate) fn choose_highlight(&mut self, time: f32) {
        let shown = self.shown_camera();
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        renderer.eye = shown.eye();
        world.focus = shown.target;
        renderer.highlight = None;
        let Some(hero) = &world.hero else {
            return;
        };
        if self.subs.in_scene() || self.subs.asking().is_some() {
            return;
        }
        let at = hero.position.truncate();
        let person = self.story.as_ref().and_then(|story| {
            story::nearest_person(&world.npcs, at, story::TALK_REACH, None).filter(|&i| {
                world.npcs[i]
                    .script_name
                    .as_deref()
                    .is_some_and(|r| story.knows(r))
            })
        });
        let what = match person {
            Some(i) => Some(render::Highlight::Npc(i)),
            None => world
                .nearest_door(hero.position)
                .map(render::Highlight::Door),
        };
        let strength = 0.5 + 0.15 * (time * 4.0).sin();
        renderer.highlight = what.map(|w| (w, [0.3, 1.0, 0.4, strength]));
    }

    /// The camera the view uses: the orbit or follow camera, eased towards the
    /// conversation camera while someone talks to the Hero.
    pub(crate) fn shown_camera(&self) -> OrbitCamera {
        match &self.conversation_camera {
            Some(shot) => {
                conversation::blend(&self.camera, shot, conversation::ease(self.conversation))
            }
            None => self.camera,
        }
    }

    /// While a conversation line or question is on screen: the shot its line
    /// names (or over the Hero's shoulder onto the speaker), easing in over
    /// half a second, with the speaker and the Hero turned to each other.
    /// Afterwards it eases back to the follow camera.
    pub(crate) fn frame_conversation(&mut self, dt: f32) {
        const EASE: f32 = 0.5;
        let active = match (self.subs.waiting_on(), &mut self.scene) {
            (Some(talked), Scene::Terrain(renderer, world)) if world.hero.is_some() => {
                // The Hero and the creatures are separate fields of the world.
                let world: &mut World = world;
                let hero = world.hero.as_mut().expect("checked");
                let speaker = self.subs.speaker().unwrap_or(talked);
                let tags = self
                    .subs
                    .line()
                    .map(|(l, _)| l.tags.clone())
                    .unwrap_or_default();
                match world.npcs.get_mut(speaker) {
                    Some(npc) => {
                        npc.turn_towards(hero.position.truncate(), dt);
                        // The Hero turns to the speaker too.
                        let d = npc.position() - hero.position.truncate();
                        if d.length() > 0.1 {
                            let want = (-d.x).atan2(d.y);
                            let diff = (want - hero.heading + std::f32::consts::PI)
                                .rem_euclid(std::f32::consts::TAU)
                                - std::f32::consts::PI;
                            hero.heading += diff.clamp(-6.0 * dt, 6.0 * dt);
                            if let Some((_, transform)) = &mut renderer.hero {
                                *transform = hero_transform(hero);
                            }
                        }
                        let speaker = conversation::Actor {
                            feet: npc.feet(),
                            face_height: npc.face_height(),
                        };
                        let speakee = conversation::Actor {
                            feet: hero.position,
                            face_height: 1.05,
                        };
                        let shot = conversation::Shot::from_tags(&tags)
                            .unwrap_or(conversation::Shot::DEFAULT);
                        self.conversation_camera = Some(shot.camera(speaker, speakee));
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        };
        if active {
            self.conversation = (self.conversation + dt / EASE).min(1.0);
        } else {
            self.conversation = (self.conversation - dt / EASE).max(0.0);
            if self.conversation == 0.0 {
                self.conversation_camera = None;
            }
        }
    }

    /// Has whoever says the line on screen gesture while they say it: a gesture
    /// the line asks for, or one for its mood, then another as each ends, until
    /// the line is done.
    pub(crate) fn gesture(&mut self, dt: f32) {
        let Scene::Terrain(_renderer, world) = &mut self.scene else {
            return;
        };
        let line = self.subs.line().map(|(l, _)| (l.id, l.tags.clone()));
        let speaker = self.subs.speaker();
        // A new line: who says it, and the gestures to pick from.
        if let (Some((id, tags)), Some(i)) = (&line, speaker)
            && self.talking.as_ref().is_none_or(|t| t.1 != *id)
            && let Some(npc) = world.npcs.get(i)
        {
            let (slot, mood) = talk::gesture_of(tags);
            self.talking = Some((i, *id, npc.gesture_ids(slot, mood), 0.0));
            // A fresh line starts a fresh gesture.
            self.gesture_picks += 1;
        }
        let saying = self.subs.saying_time();
        let Some((i, id, choices, since)) = &mut self.talking else {
            return;
        };
        if line.as_ref().is_none_or(|(l, _)| l != id) {
            self.talking = None;
            return;
        }
        // No new gesture once the line has been said (a line waiting for E stays
        // on screen after).
        *since += dt;
        if *since >= saying {
            return;
        }
        let Some(npc) = world.npcs.get_mut(*i) else {
            return;
        };
        if choices.is_empty() || npc.gesturing() {
            return;
        }
        if self.anim_bank.is_none() {
            self.anim_bank = skin::AnimBank::open(&self.assets)
                .map_err(|e| log::warn!("animations: {e}"))
                .ok();
        }
        let Some(bank) = &mut self.anim_bank else {
            return;
        };
        self.gesture_picks = self
            .gesture_picks
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let pick = choices[(self.gesture_picks >> 33) as usize % choices.len()];
        npc.gesture(bank, pick);
    }

    /// Speaks a line newly on screen (holding it there while it is said), and
    /// stops speaking when no line is.
    pub(crate) fn speak(&mut self) {
        let Some(audio) = &self.audio else {
            return;
        };
        if let Some(id) = self.subs.next_to_voice() {
            match self.voices.as_mut().and_then(|v| v.sound(id)) {
                Some(sound) => {
                    self.subs.hold_for(sound.seconds() + 0.3);
                    audio.say(sound);
                    // The speaker's mouth follows the voice's lip sync track.
                    let track = self.voices.as_mut().and_then(|v| v.lipsync(id));
                    self.stop_mouth();
                    self.lip = self
                        .subs
                        .speaker()
                        .zip(track)
                        .map(|(who, track)| (who, id, track, 0.0));
                }
                None => audio.hush(),
            }
        } else if self.subs.line().is_none() {
            audio.hush();
        }
    }

    /// Releases the mouth of whoever was speaking.
    pub(crate) fn stop_mouth(&mut self) {
        if let (Some((who, ..)), Scene::Terrain(_renderer, world), Some(bank)) =
            (self.lip.take(), &mut self.scene, &mut self.anim_bank)
            && let Some(npc) = world.npcs.get_mut(who)
        {
            npc.mouth(bank, &[]);
        }
    }

    /// Moves the speaker's mouth along their line's lip sync track, `dt`
    /// seconds on; stops at its end or when the line goes.
    pub(crate) fn move_mouth(&mut self, dt: f32) {
        let Some((_, id, track, since)) = &mut self.lip else {
            return;
        };
        let on_screen = self.subs.line().is_some_and(|(l, _)| l.id == *id);
        *since += dt;
        if !on_screen || *since > track.seconds() {
            self.stop_mouth();
            return;
        }
        let shapes: Vec<(String, f32)> = track
            .weights_at(*since)
            .into_iter()
            .map(|(n, w)| (n.to_owned(), w))
            .collect();
        let who = self.lip.as_ref().map(|l| l.0);
        if self.anim_bank.is_none() {
            self.anim_bank = skin::AnimBank::open(&self.assets)
                .map_err(|e| log::warn!("animations: {e}"))
                .ok();
        }
        if let (Some(who), Scene::Terrain(_renderer, world), Some(bank)) =
            (who, &mut self.scene, &mut self.anim_bank)
            && let Some(npc) = world.npcs.get_mut(who)
        {
            let shapes: Vec<(&str, f32)> = shapes.iter().map(|(n, w)| (n.as_str(), *w)).collect();
            npc.mouth(bank, &shapes);
        }
    }

    /// Applies a quest beat the subtitles reached: someone leaving or coming back,
    /// or the quest ending (the region reloads in the next quest state, the Hero
    /// where he stands).
    pub(crate) fn apply(&mut self, rs: &egui_wgpu::RenderState, beat: engine_script::Beat) {
        use engine_script::Beat;
        match beat {
            Beat::Leave(role) | Beat::Return(role) => {
                if let Scene::Terrain(_renderer, world) = &mut self.scene {
                    for npc in &mut world.npcs {
                        if npc.script_name.as_deref() == Some(role) {
                            npc.hidden = matches!(beat, Beat::Leave(_));
                        }
                    }
                }
            }
            Beat::Walk { who, to } => {
                log::info!("quest: {who} walks to {to:?}");
                if let Scene::Terrain(_renderer, world) = &mut self.scene {
                    for npc in world
                        .npcs
                        .iter_mut()
                        .filter(|n| n.script_name.as_deref() == Some(who))
                    {
                        if !npc.walk_to(glam::Vec3::from_array(to)) {
                            log::warn!("{who} cannot walk");
                        }
                    }
                }
            }
            Beat::Place {
                who,
                at,
                facing: [fx, fy],
            } => {
                if let Scene::Terrain(_renderer, world) = &mut self.scene {
                    let (layers, heights) = (&world.walk, world.heights.as_ref());
                    for npc in world
                        .npcs
                        .iter_mut()
                        .filter(|n| n.script_name.as_deref() == Some(who))
                    {
                        npc.hidden = false;
                        let at = glam::Vec3::from_array(at);
                        npc.place(at, glam::Vec2::new(fx, fy), layers, heights);
                    }
                }
            }
            Beat::Sleep {
                hour,
                at,
                facing: [fx, fy],
            } => {
                log::info!("quest: night passes, waking at {at:?} at {hour}:00");
                // Black at once, fading back in on the new day.
                self.hour = hour;
                self.fade = 1.0 + NIGHT_HOLD;
                if let Scene::Terrain(renderer, world) = &mut self.scene
                    && let (Some(heights), Some(hero)) = (&world.heights, &mut world.hero)
                {
                    let at = glam::Vec3::from_array(at);
                    if let Some((layer, spot, z)) =
                        walk::stand_at(&world.walk, heights, at.truncate(), Some(at.z))
                    {
                        hero.position = spot.extend(z);
                        hero.layer = layer;
                    }
                    let to = glam::Vec2::new(fx, fy) - hero.position.truncate();
                    hero.heading = (-to.x).atan2(to.y);
                    let at = hero.position;
                    if world.update_interior(at) {
                        renderer.set_inside(world.inside);
                    }
                }
            }
            Beat::Finish(sections) => {
                self.story = None;
                let arrival = self.hero().map(|h| {
                    (
                        h.position,
                        glam::Vec3::new(-h.heading.sin(), h.heading.cos(), 0.0),
                    )
                });
                let sections = sections.iter().map(|s| (*s).to_owned()).collect();
                self.load(rs, self.subject.clone(), arrival, sections);
            }
            Beat::Travel {
                region,
                sections,
                at: [x, y, z],
                facing: [fx, fy],
            } => {
                self.story = None;
                let forward = (glam::Vec2::new(fx, fy) - glam::Vec2::new(x, y))
                    .normalize_or(glam::Vec2::Y)
                    .extend(0.0);
                let arrival = (glam::Vec3::new(x, y, z), forward);
                let sections = sections.iter().map(|s| (*s).to_owned()).collect();
                self.load(
                    rs,
                    Subject::Region(region.to_owned()),
                    Some(arrival),
                    sections,
                );
            }
            _ => {}
        }
    }

    /// Decodes texture `id` and shows it in place of the 3D view.
    pub(crate) fn show_texture(&mut self, ctx: &egui::Context, id: u32) {
        let name = self
            .texture_names
            .iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| id.to_string(), |(_, n)| n.clone());
        match self.textures.load(id) {
            Ok(img) => {
                let size = [img.width as usize, img.height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                let handle = ctx.load_texture(&name, image, egui::TextureOptions::LINEAR);
                self.status = format!("texture {id}: {name}, {}x{}", size[0], size[1]);
                self.texture_view = Some(TextureView {
                    id,
                    name,
                    handle,
                    size,
                });
            }
            Err(e) => self.status = format!("texture {id}: {e}"),
        }
    }
}
