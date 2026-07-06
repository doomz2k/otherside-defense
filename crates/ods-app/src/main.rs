//! Otherside Defense — first playable Battlescape slice.
//!
//! Controls:
//! - Left click a soldier: select. Left click ground: move there.
//! - Left click a visible imp: fire at it with the current mode.
//! - `1` / `2` / `3`: snap / aimed / auto fire mode. Tab: next soldier.
//! - `G`: arm a hellfire charge — the next click throws it at that tile
//!   (arcs over walls). `H`: field-dress the selected soldier.
//! - Space or Enter: end turn (the demons then play).
//! - Right-drag: orbit camera. Scroll: zoom. WASD: pan. Esc: deselect/disarm.
//!
//! Run with `--headless` to run the simulation smoke test without a window.

use std::sync::Arc;

use glam::{IVec3, Vec3};
use ods_render::{OrbitCamera, OverlayVertex, Renderer};
use ods_sim::battle::{Action, Battle, Event};
use ods_sim::units::{FireMode, Side, UnitId};
use ods_sim::{TILE_VOXELS, ai, scenario, voxel_to_tile};
use ods_voxel::{MeshData, mesh_chunk};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const MAT_SOLDIER: u8 = 6;
const MAT_IMP: u8 = 7;

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--headless") {
        return headless_smoke_test();
    }
    let event_loop = EventLoop::new()?;
    let mut app = App { game: None };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// The old smoke test, kept for CI and cloud sessions with no display.
fn headless_smoke_test() -> anyhow::Result<()> {
    let mut battle = scenario::skirmish(42);
    println!("skirmish begins: turn {}, {:?} to move", battle.turn, battle.side_to_move);
    for _ in 0..30 {
        if battle.winner.is_some() {
            break;
        }
        battle.perform(Action::EndTurn).ok();
        let events = ai::run_demon_turn(&mut battle);
        println!("demon turn: {} events", events.len());
    }
    println!(
        "after 30 turns: soldiers {}, imps {}",
        battle.living(Side::Order).count(),
        battle.living(Side::Demons).count()
    );
    Ok(())
}

struct App {
    game: Option<Game>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes().with_title("Otherside Defense"),
                    )
                    .expect("create window"),
            );
            match Game::new(window) {
                Ok(game) => self.game = Some(game),
                Err(e) => {
                    eprintln!("failed to initialise renderer: {e:#}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(game) = self.game.as_mut() else { return };
        if game.handle_event(event) {
            event_loop.exit();
        }
    }
}

struct Game {
    window: Arc<Window>,
    renderer: Renderer,
    camera: OrbitCamera,
    battle: Battle,
    selected: Option<UnitId>,
    fire_mode: FireMode,
    /// When true, the next ground click lobs a hellfire charge.
    grenade_armed: bool,
    cursor: (f32, f32),
    right_drag: bool,
    last_cursor: (f32, f32),
}

impl Game {
    fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let renderer = Renderer::new(window.clone())?;
        let battle = scenario::skirmish(42);
        let center = (scenario::MAP_TILES.as_vec3() * TILE_VOXELS as f32) / 2.0;
        let mut game = Self {
            window,
            renderer,
            camera: OrbitCamera::new(Vec3::new(center.x, center.y, 0.0)),
            battle,
            selected: None,
            fire_mode: FireMode::Snap,
            grenade_armed: false,
            cursor: (0.0, 0.0),
            right_drag: false,
            last_cursor: (0.0, 0.0),
        };
        game.refresh_chunks();
        game.refresh_scene();
        game.update_title();
        Ok(game)
    }

    /// Returns true when the app should exit.
    fn handle_event(&mut self, event: WindowEvent) -> bool {
        match event {
            WindowEvent::CloseRequested => return true,
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                if self.right_drag {
                    let dx = self.cursor.0 - self.last_cursor.0;
                    let dy = self.cursor.1 - self.last_cursor.1;
                    self.camera.orbit(dx * -0.008, dy * 0.008);
                }
                self.last_cursor = self.cursor;
            }
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Right => self.right_drag = state == ElementState::Pressed,
                MouseButton::Left if state == ElementState::Pressed => self.click(),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.zoom(1.0 - scroll * 0.1);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let PhysicalKey::Code(code) = event.physical_key
                {
                    self.key(code);
                }
            }
            WindowEvent::RedrawRequested => {
                let vp = self.camera.view_proj(self.renderer.aspect());
                self.renderer.set_camera(vp);
                if let Err(e) = self.renderer.render() {
                    eprintln!("render error: {e:#}");
                }
                self.window.request_redraw();
            }
            _ => {}
        }
        false
    }

    fn key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Escape => {
                if self.grenade_armed {
                    self.grenade_armed = false;
                } else {
                    self.selected = None;
                }
            }
            KeyCode::Digit1 => self.fire_mode = FireMode::Snap,
            KeyCode::Digit2 => self.fire_mode = FireMode::Aimed,
            KeyCode::Digit3 => self.fire_mode = FireMode::Auto,
            KeyCode::KeyG => self.grenade_armed = !self.grenade_armed,
            KeyCode::KeyH => self.heal_selected(),
            KeyCode::Tab => self.select_next_soldier(),
            KeyCode::Space | KeyCode::Enter => self.end_turn(),
            KeyCode::KeyW => self.camera.pan(0.0, 12.0),
            KeyCode::KeyS => self.camera.pan(0.0, -12.0),
            KeyCode::KeyA => self.camera.pan(-12.0, 0.0),
            KeyCode::KeyD => self.camera.pan(12.0, 0.0),
            _ => return,
        }
        self.refresh_scene();
        self.update_title();
    }

    fn select_next_soldier(&mut self) {
        let soldiers: Vec<UnitId> = self.battle.living(Side::Order).map(|u| u.id).collect();
        if soldiers.is_empty() {
            self.selected = None;
            return;
        }
        let next = match self.selected {
            Some(current) => soldiers
                .iter()
                .cycle()
                .skip_while(|&&id| id != current)
                .nth(1)
                .copied(),
            None => soldiers.first().copied(),
        };
        self.selected = next;
    }

    fn click(&mut self) {
        if self.battle.winner.is_some() {
            return;
        }
        let (w, h) = self.renderer.size();
        let (origin, dir) = self.camera.screen_ray(self.cursor.0, self.cursor.1, w, h);
        let Some(hit) = self.battle.world.raycast(origin, dir, 4000.0) else {
            return;
        };
        // Nudge out of the surface so a floor hit resolves to the tile above.
        let open = hit.position + hit.normal.as_vec3() * 0.01;
        let tile = voxel_to_tile(open.floor().as_ivec3());

        if self.grenade_armed {
            self.grenade_armed = false;
            let Some(thrower) = self.selected else { return };
            match self.battle.perform(Action::Throw { unit: thrower, at: tile }) {
                Ok(events) => self.consume_events(&events),
                Err(e) => println!("cannot throw: {e:?}"),
            }
            return;
        }

        let events = if let Some(id) = self.battle.unit_at(tile) {
            let unit = self.battle.unit(id);
            match unit.side {
                Side::Order => {
                    self.selected = Some(id);
                    self.refresh_scene();
                    self.update_title();
                    return;
                }
                Side::Demons => {
                    let Some(shooter) = self.selected else { return };
                    self.battle.perform(Action::Fire {
                        unit: shooter,
                        target: id,
                        mode: self.fire_mode,
                    })
                }
            }
        } else {
            let Some(mover) = self.selected else { return };
            self.battle.perform(Action::Move { unit: mover, to: tile })
        };

        match events {
            Ok(events) => self.consume_events(&events),
            Err(e) => println!("cannot: {e:?}"),
        }
    }

    fn heal_selected(&mut self) {
        let Some(id) = self.selected else { return };
        match self.battle.perform(Action::Heal { medic: id, target: id }) {
            Ok(events) => self.consume_events(&events),
            Err(e) => println!("cannot heal: {e:?}"),
        }
    }

    fn end_turn(&mut self) {
        if self.battle.winner.is_some() {
            return;
        }
        match self.battle.perform(Action::EndTurn) {
            Ok(events) => self.consume_events(&events),
            Err(e) => {
                println!("cannot end turn: {e:?}");
                return;
            }
        }
        // The demons play immediately, then hand back.
        let events = ai::run_demon_turn(&mut self.battle);
        self.consume_events(&events);
    }

    fn consume_events(&mut self, events: &[Event]) {
        for e in events {
            println!("{}", describe(e, &self.battle));
        }
        self.refresh_chunks();
        self.refresh_scene();
        self.update_title();
    }

    /// Re-mesh chunks whose voxels changed (destruction, initial build).
    fn refresh_chunks(&mut self) {
        for coord in self.battle.world.take_dirty_chunks() {
            let mesh = mesh_chunk(&self.battle.world, coord);
            self.renderer.upsert_chunk(coord, &mesh);
        }
    }

    /// Rebuild unit markers and the fog/selection overlay.
    fn refresh_scene(&mut self) {
        let visible = self.battle.visible_tiles(Side::Order);

        let mut units = MeshData::default();
        for u in &self.battle.units {
            if !u.alive {
                continue;
            }
            // Imps hide in the fog.
            if u.side == Side::Demons && !visible.contains(&u.tile) {
                continue;
            }
            let base = (u.tile * TILE_VOXELS).as_vec3();
            let material = if u.side == Side::Order { MAT_SOLDIER } else { MAT_IMP };
            units.push_box(
                base + Vec3::new(5.0, 5.0, 4.0),
                base + Vec3::new(11.0, 11.0, 15.0),
                material,
            );
        }
        self.renderer.set_units(&units);

        let mut verts: Vec<OverlayVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let (min, max) = self.battle.tiles.bounds();
        for z in min.z..max.z {
            for y in min.y..max.y {
                for x in min.x..max.x {
                    let tile = IVec3::new(x, y, z);
                    if !visible.contains(&tile) {
                        push_tile_quad(&mut verts, &mut indices, tile, [0.0, 0.0, 0.02, 0.55]);
                    }
                }
            }
        }
        if let Some(id) = self.selected {
            let u = self.battle.unit(id);
            if u.alive {
                push_tile_quad(&mut verts, &mut indices, u.tile, [0.2, 1.0, 0.35, 0.35]);
            }
        }
        self.renderer.set_overlay(&verts, &indices);
    }

    fn update_title(&mut self) {
        let title = if let Some(winner) = self.battle.winner {
            match winner {
                Side::Order => "Otherside Defense — VICTORY: the incursion is banished".to_string(),
                Side::Demons => "Otherside Defense — DEFEAT: the squad is lost".to_string(),
            }
        } else {
            let sel = self
                .selected
                .map(|id| {
                    let u = self.battle.unit(id);
                    let wounds = if u.wounds > 0 {
                        format!(" bleeding x{}", u.wounds)
                    } else {
                        String::new()
                    };
                    format!(
                        "{} TU {}/{} HP {}{} | chg {} med {}",
                        u.name, u.tu, u.tu_max, u.health, wounds, u.grenades, u.heal_charges
                    )
                })
                .unwrap_or_else(|| "no selection (Tab)".to_string());
            format!(
                "Otherside Defense — turn {} | {} | mode: {:?} [1/2/3]{} | Space: end turn",
                self.battle.turn,
                sel,
                self.fire_mode,
                if self.grenade_armed { " | CHARGE ARMED — click target" } else { "" }
            )
        };
        self.window.set_title(&title);
    }
}

fn push_tile_quad(
    verts: &mut Vec<OverlayVertex>,
    indices: &mut Vec<u32>,
    tile: IVec3,
    color: [f32; 4],
) {
    let o = (tile * TILE_VOXELS).as_vec3();
    let z = o.z + scenario::GROUND_TOP as f32 + 0.15;
    let first = verts.len() as u32;
    let s = TILE_VOXELS as f32;
    for (dx, dy) in [(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)] {
        verts.push(OverlayVertex {
            position: [o.x + dx, o.y + dy, z],
            color,
        });
    }
    indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
}

fn describe(event: &Event, battle: &Battle) -> String {
    let name = |id: &UnitId| battle.unit(*id).name.clone();
    match event {
        Event::TurnStarted { side, turn } => format!("— turn {turn}: {side:?} to move —"),
        Event::Moved { unit, to, tu_left, .. } => {
            format!("{} moves to {to} ({tu_left} TU left)", name(unit))
        }
        Event::Fired { unit, target, mode, reaction, hit } => format!(
            "{}{} fires ({mode:?}) at {} — {}",
            name(unit),
            if *reaction { " [reaction]" } else { "" },
            name(target),
            if *hit { "HIT" } else { "miss" }
        ),
        Event::Damaged { unit, amount, health_left } => {
            format!("{} takes {amount} ({health_left} HP left)", name(unit))
        }
        Event::Died { unit } => format!("*** {} is down ***", name(unit)),
        Event::TerrainDestroyed { voxels, .. } => {
            format!("terrain shattered ({voxels} voxels)")
        }
        Event::Threw { unit, at } => format!("{} lobs a hellfire charge at {at}", name(unit)),
        Event::Exploded { at, voxels } => {
            format!("detonation at {at} ({voxels} voxels destroyed)")
        }
        Event::Wounded { unit, total } => {
            format!("{} is bleeding ({total} open wounds)", name(unit))
        }
        Event::Bled { unit, health_left } => {
            format!("{} bleeds ({health_left} HP left)", name(unit))
        }
        Event::Healed { medic, target, health_left } => {
            format!("{} dresses {}'s wounds ({health_left} HP)", name(medic), name(target))
        }
        Event::Panicked { unit } => format!("{} freezes in dread!", name(unit)),
        Event::Berserked { unit } => format!("{} SNAPS — firing wildly!", name(unit)),
        Event::BattleOver { winner } => format!("=== BATTLE OVER: {winner:?} wins ==="),
    }
}
