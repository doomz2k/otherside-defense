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
mod battle_screen;
mod chronicle;
mod figures;
mod geoscape;
mod globe;

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Geoscape,
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
    audio: Option<audio::Audio>,
    /// The big spinning world.
    geo_camera: OrbitCamera,
    geo_drag: bool,
    pub selected_region: Option<Region>,
    globe_built_for: Option<Option<Region>>,
    cursor: (f32, f32),
    last_cursor: (f32, f32),
    last_frame: Instant,
    sun_drift: f32,
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
        let renderer = Renderer::new(window.clone())?;
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            None,
            None,
            None,
        );
        let mut geo_camera = OrbitCamera::new(Vec3::ZERO);
        geo_camera.distance = 640.0;
        geo_camera.pitch = 0.35;

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
            audio: audio::Audio::new(),
            geo_camera,
            geo_drag: false,
            selected_region: None,
            globe_built_for: None,
            cursor: (0.0, 0.0),
            last_cursor: (0.0, 0.0),
            last_frame: Instant::now(),
            sun_drift: 0.0,
        })
    }

    /// Switch to the Geoscape and (re)install the globe scene.
    pub fn enter_geoscape(&mut self) {
        self.renderer.clear_scene();
        let (vertices, indices) = globe::build_globe(self.selected_region);
        self.renderer.set_globe(&vertices, &indices);
        self.globe_built_for = Some(self.selected_region);
        self.screen = Screen::Geoscape;
    }

    pub fn start_skirmish(&mut self) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let battle = scenario::skirmish(seed);
        self.battle = Some(BattleScreen::new(&mut self.renderer, battle, None));
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
                if self.screen == Screen::Geoscape && self.geo_drag && !response.consumed {
                    let dx = self.cursor.0 - self.last_cursor.0;
                    let dy = self.cursor.1 - self.last_cursor.1;
                    self.geo_camera.orbit(dx * -0.006, dy * 0.006);
                }
                self.last_cursor = self.cursor;
                if let Some(b) = self.battle.as_mut() {
                    b.cursor = (position.x as f32, position.y as f32);
                    if b.right_drag && !response.consumed {
                        let dx = b.cursor.0 - b.last_cursor.0;
                        let dy = b.cursor.1 - b.last_cursor.1;
                        b.drag(dx, dy);
                    }
                    b.last_cursor = b.cursor;
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if self.screen == Screen::Geoscape && !response.consumed =>
            {
                match button {
                    MouseButton::Right | MouseButton::Middle => {
                        self.geo_drag = state == ElementState::Pressed;
                    }
                    MouseButton::Left if state == ElementState::Pressed => {
                        let (w, h) = self.renderer.size();
                        let (origin, dir) =
                            self.geo_camera.screen_ray(self.cursor.0, self.cursor.1, w, h);
                        self.selected_region = globe::pick_region(origin, dir);
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if self.screen == Screen::Geoscape && !response.consumed =>
            {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.geo_camera.zoom(1.0 - scroll * 0.1);
                self.geo_camera.distance = self.geo_camera.distance.max(320.0);
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
            WindowEvent::KeyboardInput { event, .. }
                if self.screen == Screen::Battle && !response.consumed =>
            {
                if event.state == ElementState::Pressed
                    && let PhysicalKey::Code(code) = event.physical_key
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

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_ctx.clone();
        let full_output = ctx.run(raw_input, |ctx| self.ui(ctx));
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);
        let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        match self.screen {
            Screen::Battle => {
                if let Some(b) = self.battle.as_mut() {
                    b.update_fx(dt, &mut self.renderer);
                    let vp = b.camera_vp(self.renderer.aspect());
                    // Night fights are lit low and flat.
                    let sun = if b.battle.vision_tiles < 14 {
                        Vec3::new(-0.2, -0.3, 0.35)
                    } else {
                        Vec3::new(0.35, 0.5, 0.8)
                    };
                    self.renderer.set_camera(vp, sun);
                }
            }
            Screen::Geoscape => {
                // The world turns on its own until the player grabs it.
                if !self.geo_drag {
                    self.geo_camera.yaw += dt * 0.08;
                }
                if self.globe_built_for != Some(self.selected_region) {
                    let (vertices, indices) = globe::build_globe(self.selected_region);
                    self.renderer.set_globe(&vertices, &indices);
                    self.globe_built_for = Some(self.selected_region);
                }
                // The terminator: the sun tracks the campaign calendar and
                // drifts in real time, sweeping day across the globe.
                self.sun_drift += dt * 1.5;
                let sun_lon = self
                    .campaign
                    .as_ref()
                    .map_or(0.0, |c| c.sun_lon())
                    + self.sun_drift;
                let sun = globe::latlon_to_pos(12.0, sun_lon, 1.0);
                if let Some(c) = &self.campaign {
                    let (vertices, indices) = globe::build_markers(c);
                    self.renderer.set_markers(&vertices, &indices);
                }
                let vp = self.geo_camera.view_proj(self.renderer.aspect());
                self.renderer.set_camera(vp, sun);
            }
            Screen::Menu => {}
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
            Screen::Geoscape => {
                let action = self.geoscape_ui(ctx);
                if let GeoAction::LeadMission(kind) = action {
                    self.launch_mission(kind);
                }
            }
            Screen::Battle => self.battle_ui(ctx),
        }
    }

    fn launch_mission(&mut self, kind: ods_geo::MissionKind) {
        let Some(c) = &mut self.campaign else { return };
        match c.begin_mission(kind) {
            Ok((battle, token)) => {
                self.battle = Some(BattleScreen::new(&mut self.renderer, battle, Some(token)));
                self.screen = Screen::Battle;
            }
            Err(e) => self.log.push(format!("cannot stage the mission: {e:?}")),
        }
    }

    fn battle_ui(&mut self, ctx: &egui::Context) {
        let leave = match self.battle.as_mut() {
            Some(b) => b.hud(ctx, &mut self.renderer, self.audio.as_ref()),
            None => {
                self.screen = Screen::Menu;
                return;
            }
        };
        if leave {
            let mut screen = self.battle.take().expect("battle present");
            self.renderer.clear_scene();
            match (screen.token.take(), &mut self.campaign) {
                (Some(token), Some(c)) => {
                    let report = c.conclude_mission(token, &screen.battle);
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
