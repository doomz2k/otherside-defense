//! Otherside Defense — application shell.
//!
//! Screens: main menu → Geoscape (campaign management) → Battlescape
//! (3D voxel battle with an egui HUD). Campaign battles can be led
//! interactively ("Lead") or auto-resolved ("Auto"); either way the same
//! rules run underneath.
//!
//! Headless modes for CI / displayless sessions:
//!   --headless       tactical smoke test
//!   --campaign [N]   N-month narrated campaign

mod audio;
mod basescape;
mod battle_screen;
mod chronicle;
mod config;
mod figures;
mod geoscape;
mod globe;
mod theme;

use std::sync::Arc;
use std::time::Instant;

use battle_screen::BattleScreen;
use geoscape::GeoAction;
use glam::Vec3;
use ods_geo::{Campaign, Facility, Region};
use ods_render::{OrbitCamera, Renderer, UiFrame};
use ods_sim::scenario;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

pub const SAVE_PATH: &str = "otherside-save.json";
pub const AUTOSAVE_PATH: &str = "otherside-autosave.json";

pub fn slot_path(slot: usize) -> String {
    format!("otherside-save-{slot}.json")
}

pub fn autosave_history_path(n: usize) -> String {
    format!("otherside-autosave-{n}.json")
}

/// Rolling autosaves: the newest is AUTOSAVE_PATH, and the last three
/// generations survive behind it — one bad day never eats the record.
pub fn write_autosave(c: &ods_geo::Campaign) {
    let _ = std::fs::rename(autosave_history_path(2), autosave_history_path(3));
    let _ = std::fs::rename(autosave_history_path(1), autosave_history_path(2));
    let _ = std::fs::rename(AUTOSAVE_PATH, autosave_history_path(1));
    let _ = std::fs::write(AUTOSAVE_PATH, c.save_to_string());
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--headless") {
        return chronicle::headless_smoke_test();
    }
    if let Some(pos) = args.iter().position(|a| a == "--campaign") {
        let months: u32 = args.get(pos + 1).and_then(|m| m.parse().ok()).unwrap_or(6);
        return chronicle::campaign_chronicle(months);
    }
    let event_loop = EventLoop::new()?;
    let mut app = App { core: None };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// The after-action report: what the field cost and who distinguished
/// themselves on it.
pub struct Debrief {
    pub victory: bool,
    pub label: String,
    pub turns: u32,
    pub demons_slain: u32,
    pub captures: u32,
    pub civilians: (u32, u32),
    pub fallen: Vec<String>,
    pub commendations: Vec<String>,
}

impl Debrief {
    fn from_report(label: &str, report: &ods_geo::BattleReport, names: &[String]) -> Self {
        let name = |i: usize| -> String {
            names.get(i).cloned().unwrap_or_else(|| format!("soldier #{i}"))
        };
        let mut commendations = Vec::new();
        if let Some((i, _, xp)) = report
            .survivors
            .iter()
            .filter(|(_, _, xp)| xp.kills >= 2)
            .max_by_key(|(_, _, xp)| xp.kills)
        {
            commendations.push(format!("⚔ The Reaper's Due — {} ({} kills)", name(*i), xp.kills));
        }
        if let Some((i, _, xp)) = report
            .survivors
            .iter()
            .filter(|(_, _, xp)| xp.shots_fired >= 4 && xp.shots_hit * 100 >= xp.shots_fired * 60)
            .max_by_key(|(_, _, xp)| xp.shots_hit * 100 / xp.shots_fired)
        {
            commendations.push(format!(
                "🎯 Sharpshooter — {} ({}/{} shots told)",
                name(*i),
                xp.shots_hit,
                xp.shots_fired
            ));
        }
        if let Some((i, _, xp)) = report
            .survivors
            .iter()
            .filter(|(_, _, xp)| xp.reaction_shots >= 2)
            .max_by_key(|(_, _, xp)| xp.reaction_shots)
        {
            commendations.push(format!(
                "⚡ The Watchful — {} ({} reaction shots)",
                name(*i),
                xp.reaction_shots
            ));
        }
        for (i, _, xp) in &report.survivors {
            if xp.dread_survived > 0 {
                commendations.push(format!("🕯 Unbroken — {} stared into it and held", name(*i)));
            }
        }
        Self {
            victory: report.victory,
            label: label.to_string(),
            turns: report.turns,
            demons_slain: report.demons_slain,
            captures: report.captured_grunts + report.captured_overseers,
            civilians: (report.civilians_saved, report.civilians_dead),
            fallen: report.dead.iter().map(|&i| name(i)).collect(),
            commendations,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Geoscape,
    /// The chapterhouse diorama: build, hire, research, forge.
    Base,
    Battle,
}

struct App {
    core: Option<Core>,
}

pub struct Core {
    window: Arc<Window>,
    renderer: Renderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    pub screen: Screen,
    pub campaign: Option<Campaign>,
    pub battle: Option<BattleScreen>,
    pub log: Vec<String>,
    pub status: Option<String>,
    pub build_choice: Facility,
    pub selected_base: usize,
    pub difficulty_choice: ods_geo::Difficulty,
    pub ironman_choice: bool,
    pub show_codex: bool,
    pub show_stats: bool,
    pub show_options: bool,
    /// Geoscape time compression: 0 holds, 1..=3 run the calendar.
    pub geo_speed: u8,
    /// How far through the current day the clock stands (0..1).
    pub day_progress: f32,
    /// Master volume (0..=1) and orbit-drag sensitivity (0.3..=2.5).
    pub volume: f32,
    pub cam_sense: f32,
    /// Battle pacing multiplier: how fast figures glide (0.5..=3).
    pub anim_speed: f32,
    /// The rebindable battle keys and, while listening, which is rebinding.
    pub binds: Vec<config::Bind>,
    pub rebinding: Option<usize>,
    /// The after-action report awaiting review on the Geoscape.
    pub debrief: Option<Debrief>,
    audio: Option<audio::Audio>,
    /// The big spinning world.
    geo_camera: OrbitCamera,
    geo_drag: bool,
    /// The chapterhouse diorama's slow orbit.
    base_camera: OrbitCamera,
    /// The diorama needs rebuilding (construction started, base switched).
    pub base_dirty: bool,
    /// Soldier index whose paper-doll equip window is open.
    pub equip_for: Option<usize>,
    pub selected_region: Option<Region>,
    globe_built_for: Option<Option<Region>>,
    /// The title screen's frozen skirmish, slowly orbited.
    menu_built: bool,
    menu_camera: OrbitCamera,
    cursor: (f32, f32),
    last_cursor: (f32, f32),
    last_frame: Instant,
    /// Seconds since launch; feeds the emissive-material pulse.
    clock: f32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.core.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(Window::default_attributes().with_title("Otherside Defense"))
                    .expect("create window"),
            );
            match Core::new(window) {
                Ok(core) => self.core = Some(core),
                Err(e) => {
                    eprintln!("failed to initialise renderer: {e:#}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(core) = self.core.as_mut() else { return };
        if core.handle_event(event) {
            event_loop.exit();
        }
    }
}

impl Core {
    fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let cfg = config::Config::load();
        let mut binds = config::default_binds();
        config::apply_saved(&mut binds, &cfg.binds);
        let mut renderer = Renderer::new(window.clone())?;
        renderer.set_pixel_scale(cfg.pixel_scale);
        renderer.set_crt(cfg.crt);
        let mut audio = audio::Audio::new();
        if let Some(a) = audio.as_mut() {
            a.set_volume(cfg.volume);
        }
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            None,
            None,
            None,
        );
        theme::apply(&egui_ctx);
        let mut geo_camera = OrbitCamera::new(Vec3::ZERO);
        geo_camera.distance = 640.0;
        geo_camera.pitch = 0.35;
        let mut menu_camera = OrbitCamera::isometric(Vec3::new(96.0, 96.0, 10.0));
        menu_camera.distance = 220.0 * ods_sim::VS as f32;
        let mut base_camera = OrbitCamera::isometric(basescape::scene_center());
        base_camera.distance = 420.0;

        Ok(Self {
            window,
            renderer,
            egui_ctx,
            egui_state,
            screen: Screen::Menu,
            campaign: None,
            battle: None,
            log: Vec::new(),
            status: None,
            build_choice: Facility::Quarters,
            selected_base: 0,
            difficulty_choice: ods_geo::Difficulty::Veteran,
            ironman_choice: false,
            show_codex: false,
            show_stats: false,
            show_options: false,
            geo_speed: 0,
            day_progress: 0.0,
            volume: cfg.volume,
            cam_sense: cfg.cam_sense,
            anim_speed: cfg.anim_speed,
            binds,
            rebinding: None,
            debrief: None,
            audio,
            geo_camera,
            geo_drag: false,
            base_camera,
            base_dirty: false,
            equip_for: None,
            selected_region: None,
            globe_built_for: None,
            menu_built: false,
            menu_camera,
            cursor: (0.0, 0.0),
            last_cursor: (0.0, 0.0),
            last_frame: Instant::now(),
            clock: 0.0,
        })
    }

    pub(crate) fn audio_mut(&mut self) -> Option<&mut audio::Audio> {
        self.audio.as_mut()
    }

    /// Persist the player's preferences (called whenever one changes).
    pub fn save_config(&self) {
        config::Config {
            volume: self.volume,
            cam_sense: self.cam_sense,
            anim_speed: self.anim_speed,
            pixel_scale: self.renderer.pixel_scale(),
            crt: self.renderer.crt(),
            binds: self
                .binds
                .iter()
                .filter(|b| b.current != b.default)
                .map(|b| (b.label.to_string(), config::code_name(b.current)))
                .collect(),
        }
        .save();
    }

    /// Switch to the Geoscape and (re)install the globe scene.
    pub fn enter_geoscape(&mut self) {
        self.renderer.clear_scene();
        self.menu_built = false;
        let (vertices, indices) = globe::build_globe(self.selected_region);
        self.renderer.set_globe(&vertices, &indices);
        self.globe_built_for = Some(self.selected_region);
        self.screen = Screen::Geoscape;
    }

    /// Switch to the Basescape: the selected chapterhouse as a diorama.
    pub fn enter_base(&mut self) {
        let Some(c) = &self.campaign else { return };
        self.renderer.clear_scene();
        self.menu_built = false;
        self.globe_built_for = None;
        let bi = self.selected_base.min(c.bases.len() - 1);
        let (verts, indices) = basescape::build_base_scene(&c.bases[bi]);
        self.renderer.set_figures(&verts, &indices);
        self.base_dirty = false;
        self.screen = Screen::Base;
    }

    /// Build the title screen's voxel diorama: a small skirmish scene,
    /// mid-fight forever, slowly orbited by the camera.
    fn build_menu_diorama(&mut self) {
        self.renderer.clear_scene();
        let mut battle = scenario::skirmish(1349);
        for coord in battle.world.take_dirty_chunks() {
            let mesh = ods_voxel::mesh_chunk(&battle.world, coord);
            self.renderer.upsert_chunk(coord, &mesh);
        }
        // Every figure on parade — the diorama has no fog of war.
        let visible: std::collections::HashSet<glam::IVec3> =
            battle.units.iter().map(|u| u.tile).collect();
        let (verts, indices) = figures::build_figures(
            &battle,
            &visible,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        self.renderer.set_figures(&verts, &indices);
        let (min, max) = battle.tiles.bounds();
        let center = ((min + max).as_vec3() / 2.0) * ods_sim::TILE_VOXELS as f32;
        self.menu_camera.target = Vec3::new(center.x, center.y, 8.0);
        self.menu_built = true;
    }

    pub fn start_skirmish(&mut self) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let battle = scenario::skirmish(seed);
        self.battle = Some(BattleScreen::new(&mut self.renderer, battle, None));
        self.menu_built = false;
        self.screen = Screen::Battle;
    }

    /// Returns true when the app should exit.
    fn handle_event(&mut self, event: WindowEvent) -> bool {
        let response = self.egui_state.on_window_event(&self.window, &event);

        match event {
            WindowEvent::CloseRequested => return true,
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                if self.geo_drag && !response.consumed {
                    let dx = self.cursor.0 - self.last_cursor.0;
                    let dy = self.cursor.1 - self.last_cursor.1;
                    match self.screen {
                        Screen::Geoscape => self
                            .geo_camera
                            .orbit(dx * -0.006 * self.cam_sense, dy * 0.006 * self.cam_sense),
                        Screen::Base => self
                            .base_camera
                            .orbit(dx * -0.006 * self.cam_sense, dy * 0.003 * self.cam_sense),
                        _ => {}
                    }
                }
                self.last_cursor = self.cursor;
                let sense = self.cam_sense;
                if let Some(b) = self.battle.as_mut() {
                    b.cursor = (position.x as f32, position.y as f32);
                    if b.right_drag && !response.consumed {
                        let dx = b.cursor.0 - b.last_cursor.0;
                        let dy = b.cursor.1 - b.last_cursor.1;
                        b.drag(dx * sense, dy * sense);
                    }
                    b.last_cursor = b.cursor;
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if matches!(self.screen, Screen::Geoscape | Screen::Base)
                    && !response.consumed =>
            {
                match button {
                    MouseButton::Right | MouseButton::Middle => {
                        self.geo_drag = state == ElementState::Pressed;
                    }
                    MouseButton::Left
                        if state == ElementState::Pressed && self.screen == Screen::Geoscape =>
                    {
                        let (w, h) = self.renderer.size();
                        let (origin, dir) =
                            self.geo_camera.screen_ray(self.cursor.0, self.cursor.1, w, h);
                        self.selected_region = globe::pick_region(origin, dir);
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if matches!(self.screen, Screen::Geoscape | Screen::Base)
                    && !response.consumed =>
            {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if self.screen == Screen::Geoscape {
                    self.geo_camera.zoom(1.0 - scroll * 0.1);
                    self.geo_camera.distance = self.geo_camera.distance.max(320.0);
                } else {
                    self.base_camera.zoom(1.0 - scroll * 0.1);
                    self.base_camera.distance = self.base_camera.distance.clamp(180.0, 900.0);
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if self.screen == Screen::Battle && !response.consumed =>
            {
                let (w, h) = self.renderer.size();
                if let Some(b) = self.battle.as_mut() {
                    match button {
                        MouseButton::Right => b.right_drag = state == ElementState::Pressed,
                        MouseButton::Left if state == ElementState::Pressed => {
                            b.click(&mut self.renderer, self.audio.as_ref(), w, h);
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if self.screen == Screen::Battle && !response.consumed =>
            {
                if let Some(b) = self.battle.as_mut() {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                    };
                    b.camera.zoom(1.0 - scroll * 0.1);
                }
            }
            // A binding is listening: the next key press becomes its home.
            WindowEvent::KeyboardInput { event, .. }
                if self.rebinding.is_some() && event.state == ElementState::Pressed =>
            {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == winit::keyboard::KeyCode::Escape {
                        self.rebinding = None;
                    } else if config::REBINDABLE.contains(&code)
                        && let Some(i) = self.rebinding.take()
                    {
                        self.binds[i].current = code;
                        self.save_config();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if self.screen == Screen::Battle && !response.consumed =>
            {
                if event.state == ElementState::Pressed
                    && let PhysicalKey::Code(code) = event.physical_key
                    && let Some(code) = config::translate(&self.binds, code)
                    && let Some(b) = self.battle.as_mut()
                {
                    b.key(&mut self.renderer, self.audio.as_ref(), code);
                }
            }
            _ => {}
        }
        false
    }

    fn redraw(&mut self) {
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();
        self.clock = (self.clock + dt) % 3600.0;

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_ctx.clone();
        let full_output = ctx.run(raw_input, |ctx| self.ui(ctx));
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);
        let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        match self.screen {
            Screen::Battle => {
                if let Some(a) = self.audio.as_mut() {
                    a.music(Some(audio::MusicTrack::Warfront));
                }
                if let Some(b) = self.battle.as_mut() {
                    let (w, h) = self.renderer.size();
                    b.anim_speed = self.anim_speed;
                    b.update_frame(dt, &mut self.renderer, self.audio.as_ref(), w, h);
                    let vp = b.camera_vp(self.renderer.aspect());
                    // Night fights are lit low and flat.
                    let sun = if b.battle.vision_tiles < 14 {
                        Vec3::new(-0.2, -0.3, 0.35)
                    } else {
                        Vec3::new(0.35, 0.5, 0.8)
                    };
                    self.renderer.set_camera(vp, sun, self.clock);
                }
            }
            Screen::Geoscape => {
                if let Some(a) = self.audio.as_mut() {
                    a.music(Some(audio::MusicTrack::Vigil));
                }
                // The world turns on its own until the player grabs it.
                if !self.geo_drag {
                    self.geo_camera.yaw += dt * 0.08;
                }
                if self.globe_built_for != Some(self.selected_region) {
                    let (vertices, indices) = globe::build_globe(self.selected_region);
                    self.renderer.set_globe(&vertices, &indices);
                    self.globe_built_for = Some(self.selected_region);
                }
                // Real time flows through the calendar at the chosen
                // compression — and stops dead the moment the world needs
                // an answer (an event fires, or gargoyles find a sortie).
                if let Some(c) = &mut self.campaign {
                    if c.over.is_some() || c.interception.is_some() {
                        self.geo_speed = 0;
                    }
                    let rate = match self.geo_speed {
                        0 => 0.0,
                        1 => 1.0 / 12.0, // a day each twelve seconds
                        2 => 1.0 / 3.0,  // a day every three
                        _ => 2.0,        // days streak past
                    };
                    if rate > 0.0 {
                        self.day_progress += dt * rate;
                        let mut crossed = 0;
                        while self.day_progress >= 1.0 && crossed < 8 {
                            self.day_progress -= 1.0;
                            crossed += 1;
                            let events = c.advance_day();
                            if !events.is_empty() {
                                // Something happened: the clock waits.
                                self.geo_speed = 0;
                                self.day_progress = 0.0;
                                for e in &events {
                                    self.log.push(chronicle::narrate(c, e));
                                }
                                break;
                            }
                        }
                        if crossed > 0 {
                            write_autosave(c);
                        }
                    }
                }
                // The terminator: the sun tracks the campaign calendar,
                // gliding smoothly through the day in flight.
                let sun_lon = self
                    .campaign
                    .as_ref()
                    .map_or(self.clock * 4.0, |c| c.sun_lon() + self.day_progress * 137.0);
                let sun = globe::latlon_to_pos(12.0, sun_lon, 1.0);
                if let Some(c) = &self.campaign {
                    let (vertices, indices) = globe::build_markers(c, self.clock);
                    self.renderer.set_markers(&vertices, &indices);
                }
                // Civilization glitters on the night side of the line.
                let (lights, light_idx) = globe::build_city_lights(sun_lon, self.clock);
                self.renderer.set_fx(&lights, &light_idx);
                let vp = self.geo_camera.view_proj(self.renderer.aspect());
                self.renderer.set_camera(vp, sun, self.clock);
            }
            Screen::Base => {
                if let Some(a) = self.audio.as_mut() {
                    a.music(Some(audio::MusicTrack::Vigil));
                }
                if self.base_dirty
                    && let Some(c) = &self.campaign
                {
                    let bi = self.selected_base.min(c.bases.len() - 1);
                    let (verts, indices) = basescape::build_base_scene(&c.bases[bi]);
                    self.renderer.set_figures(&verts, &indices);
                    self.base_dirty = false;
                }
                self.base_camera.yaw += dt * 0.04;
                let vp = self.base_camera.view_proj(self.renderer.aspect());
                self.renderer.set_camera(vp, Vec3::new(0.4, 0.5, 0.75), self.clock);
            }
            Screen::Menu => {
                if let Some(a) = self.audio.as_mut() {
                    a.music(Some(audio::MusicTrack::Vigil));
                }
                // A frozen skirmish smoulders behind the title.
                if !self.menu_built {
                    self.build_menu_diorama();
                }
                self.menu_camera.yaw += dt * 0.07;
                let vp = self.menu_camera.view_proj(self.renderer.aspect());
                self.renderer.set_camera(vp, Vec3::new(-0.3, -0.4, 0.45), self.clock);
            }
        }

        if let Err(e) = self.renderer.render(Some(UiFrame {
            textures_delta: full_output.textures_delta,
            primitives,
            pixels_per_point: full_output.pixels_per_point,
        })) {
            eprintln!("render error: {e:#}");
        }
        self.window.request_redraw();
    }

    fn ui(&mut self, ctx: &egui::Context) {
        match self.screen {
            Screen::Menu => self.menu_ui(ctx),
            Screen::Geoscape => match self.geoscape_ui(ctx) {
                GeoAction::LeadMission(kind) => self.launch_mission(kind),
                GeoAction::EnterBase => self.enter_base(),
                GeoAction::None => {}
            },
            Screen::Base => {
                if self.base_ui(ctx) {
                    self.enter_geoscape();
                }
            }
            Screen::Battle => self.battle_ui(ctx),
        }
        // The options window follows the commander onto any screen.
        if self.show_options && self.screen != Screen::Battle {
            self.options_window(ctx);
        }
    }

    fn options_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Options")
            .open(&mut open)
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 16.0])
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Volume");
                if ui
                    .add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    let volume = self.volume;
                    if let Some(a) = self.audio_mut() {
                        a.set_volume(volume);
                    }
                    self.save_config();
                }
                ui.label("Camera sensitivity");
                if ui
                    .add(egui::Slider::new(&mut self.cam_sense, 0.3..=2.5).show_value(false))
                    .changed()
                {
                    self.save_config();
                }
                ui.label(
                    egui::RichText::new(
                        "Applies to right-drag orbiting on both the globe and the field.",
                    )
                    .weak()
                    .small(),
                );
                ui.label("Battle pace");
                if ui
                    .add(egui::Slider::new(&mut self.anim_speed, 0.5..=3.0).show_value(false))
                    .changed()
                {
                    self.save_config();
                }
                ui.label(
                    egui::RichText::new("How fast figures cross the field; right for instant.")
                        .weak()
                        .small(),
                );

                ui.separator();
                ui.label("Pixel scale");
                let mut scale = self.renderer.pixel_scale();
                ui.horizontal(|ui| {
                    for s in 1..=4u32 {
                        if ui
                            .add(egui::Button::selectable(scale == s, format!("{s}×")))
                            .clicked()
                        {
                            scale = s;
                        }
                    }
                });
                if scale != self.renderer.pixel_scale() {
                    self.renderer.set_pixel_scale(scale);
                    self.save_config();
                }
                let mut crt = self.renderer.crt();
                if ui.checkbox(&mut crt, "CRT dressing").on_hover_text(
                    "scanlines, a whisper of phosphor mask, corners that fall away",
                ).changed()
                {
                    self.renderer.set_crt(crt);
                    self.save_config();
                }

                ui.separator();
                egui::CollapsingHeader::new("Battle keys").show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Click a key to rebind it; Escape cancels. Camera keys stay put.",
                        )
                        .weak()
                        .small(),
                    );
                    let mut start: Option<usize> = None;
                    let mut reset = false;
                    egui::Grid::new("keybinds").striped(true).show(ui, |ui| {
                        for (i, b) in self.binds.iter().enumerate() {
                            ui.label(b.label);
                            let text = if self.rebinding == Some(i) {
                                "press a key…".to_string()
                            } else {
                                config::code_name(b.current)
                            };
                            if ui.small_button(text).clicked() {
                                start = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    if ui.small_button("Reset all").clicked() {
                        reset = true;
                    }
                    if let Some(i) = start {
                        self.rebinding = Some(i);
                    }
                    if reset {
                        for b in &mut self.binds {
                            b.current = b.default;
                        }
                        self.rebinding = None;
                        self.save_config();
                    }
                });
            });
        self.show_options = open;
    }

    fn launch_mission(&mut self, kind: ods_geo::MissionKind) {
        let Some(c) = &mut self.campaign else { return };
        match c.begin_mission(kind) {
            Ok((battle, token)) => {
                self.battle = Some(BattleScreen::new(&mut self.renderer, battle, Some(token)));
                self.menu_built = false;
                self.screen = Screen::Battle;
            }
            Err(e) => self.log.push(format!("cannot stage the mission: {e:?}")),
        }
    }

    fn battle_ui(&mut self, ctx: &egui::Context) {
        let leave = match self.battle.as_mut() {
            Some(b) => b.hud(ctx, &mut self.renderer, self.audio.as_ref(), self.campaign.as_ref()),
            None => {
                self.screen = Screen::Menu;
                return;
            }
        };
        if leave {
            let mut screen = self.battle.take().expect("battle present");
            self.renderer.clear_scene();
            self.menu_built = false;
            match (screen.token.take(), &mut self.campaign) {
                (Some(token), Some(c)) => {
                    // Names now: the roster shrinks when the report lands.
                    let names: Vec<String> = token
                        .squad()
                        .iter()
                        .map(|&i| c.soldiers[i].name.clone())
                        .collect();
                    let label = token.kind().label().to_string();
                    let report = c.conclude_mission(token, &screen.battle);
                    self.debrief = Some(Debrief::from_report(&label, &report, &names));
                    self.log.push(if report.victory {
                        format!(
                            "Mission complete: {} demons slain, {} soldiers lost.",
                            report.demons_slain,
                            report.dead.len()
                        )
                    } else {
                        format!(
                            "The squad withdraws: {} soldiers lost. The enemy holds.",
                            report.dead.len()
                        )
                    });
                    self.enter_geoscape();
                }
                _ => {
                    self.screen = Screen::Menu;
                }
            }
        }
    }
}
