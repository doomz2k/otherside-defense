//! The interactive Battlescape: the 3D voxel view plus its egui HUD.

use glam::{IVec3, Vec3};
use ods_geo::MissionToken;
use ods_render::{OrbitCamera, OverlayVertex, Renderer};
use ods_sim::battle::{Action, Battle, Event};
use ods_sim::units::{FireMode, Side, UnitId};
use ods_sim::{TILE_VOXELS, ai, scenario, voxel_to_tile};
use ods_voxel::{MeshData, mesh_chunk};
use winit::keyboard::KeyCode;

const MAT_SOLDIER: u8 = 6;
const MAT_IMP: u8 = 7;

pub struct BattleScreen {
    pub battle: Battle,
    /// Present when this battle belongs to the campaign.
    pub token: Option<MissionToken>,
    pub camera: OrbitCamera,
    pub log: Vec<String>,
    selected: Option<UnitId>,
    fire_mode: FireMode,
    grenade_armed: bool,
    pub cursor: (f32, f32),
    pub right_drag: bool,
    pub last_cursor: (f32, f32),
}

impl BattleScreen {
    pub fn new(renderer: &mut Renderer, battle: Battle, token: Option<MissionToken>) -> Self {
        let (min, max) = battle.tiles.bounds();
        let center = ((min + max).as_vec3() / 2.0) * TILE_VOXELS as f32;
        let mut screen = Self {
            battle,
            token,
            camera: OrbitCamera::new(Vec3::new(center.x, center.y, 0.0)),
            log: vec!["The squad deploys.".to_string()],
            selected: None,
            fire_mode: FireMode::Snap,
            grenade_armed: false,
            cursor: (0.0, 0.0),
            right_drag: false,
            last_cursor: (0.0, 0.0),
        };
        renderer.clear_scene();
        screen.refresh_chunks(renderer);
        screen.refresh_scene(renderer);
        screen
    }

    // ------------------------------------------------------------------
    // Input from the window (only reaches us when egui didn't consume it)

    pub fn click(&mut self, renderer: &mut Renderer, width: f32, height: f32) {
        if self.battle.winner.is_some() {
            return;
        }
        let (origin, dir) = self.camera.screen_ray(self.cursor.0, self.cursor.1, width, height);
        let Some(hit) = self.battle.world.raycast(origin, dir, 4000.0) else {
            return;
        };
        let open = hit.position + hit.normal.as_vec3() * 0.01;
        let tile = voxel_to_tile(open.floor().as_ivec3());

        if self.grenade_armed {
            self.grenade_armed = false;
            let Some(thrower) = self.selected else { return };
            let result = self.battle.perform(Action::Throw { unit: thrower, at: tile });
            self.apply(renderer, result);
            return;
        }

        if let Some(id) = self.battle.unit_at(tile) {
            match self.battle.unit(id).side {
                Side::Order => {
                    self.selected = Some(id);
                    self.refresh_scene(renderer);
                }
                Side::Demons => {
                    let Some(shooter) = self.selected else { return };
                    let result = self.battle.perform(Action::Fire {
                        unit: shooter,
                        target: id,
                        mode: self.fire_mode,
                    });
                    self.apply(renderer, result);
                }
            }
        } else {
            let Some(mover) = self.selected else { return };
            let result = self.battle.perform(Action::Move { unit: mover, to: tile });
            self.apply(renderer, result);
        }
    }

    pub fn key(&mut self, renderer: &mut Renderer, code: KeyCode) {
        match code {
            KeyCode::Escape => {
                if self.grenade_armed {
                    self.grenade_armed = false;
                } else {
                    self.selected = None;
                    self.refresh_scene(renderer);
                }
            }
            KeyCode::Digit1 => self.fire_mode = FireMode::Snap,
            KeyCode::Digit2 => self.fire_mode = FireMode::Aimed,
            KeyCode::Digit3 => self.fire_mode = FireMode::Auto,
            KeyCode::KeyG => self.grenade_armed = !self.grenade_armed,
            KeyCode::KeyH => self.heal_selected(renderer),
            KeyCode::Tab => {
                self.select_next_soldier();
                self.refresh_scene(renderer);
            }
            KeyCode::Space | KeyCode::Enter => self.end_turn(renderer),
            KeyCode::KeyW => self.camera.pan(0.0, 12.0),
            KeyCode::KeyS => self.camera.pan(0.0, -12.0),
            KeyCode::KeyA => self.camera.pan(-12.0, 0.0),
            KeyCode::KeyD => self.camera.pan(12.0, 0.0),
            _ => {}
        }
    }

    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.camera.orbit(dx * -0.008, dy * 0.008);
    }

    // ------------------------------------------------------------------
    // HUD

    /// Returns true when the player asked to leave the battle.
    pub fn hud(&mut self, ctx: &egui::Context, renderer: &mut Renderer) -> bool {
        let mut leave = false;

        egui::TopBottomPanel::top("battle-top").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "Turn {} — {:?} to move",
                    self.battle.turn, self.battle.side_to_move
                ));
                ui.separator();
                match self.selected {
                    Some(id) => {
                        let u = self.battle.unit(id);
                        ui.label(format!(
                            "{} | TU {}/{} | HP {}/{} | morale {}{}",
                            u.name,
                            u.tu,
                            u.tu_max,
                            u.health,
                            u.health_max,
                            u.morale,
                            if u.wounds > 0 {
                                format!(" | BLEEDING x{}", u.wounds)
                            } else {
                                String::new()
                            }
                        ));
                        ui.label(format!("charges {} | dressings {}", u.grenades, u.heal_charges));
                    }
                    None => {
                        ui.label("no soldier selected — click one or press Tab");
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Fire:");
                ui.selectable_value(&mut self.fire_mode, FireMode::Snap, "Snap [1]");
                ui.selectable_value(&mut self.fire_mode, FireMode::Aimed, "Aimed [2]");
                ui.selectable_value(&mut self.fire_mode, FireMode::Auto, "Auto [3]");
                ui.separator();
                let charge = ui.selectable_label(self.grenade_armed, "🧨 Charge [G]");
                if charge.clicked() {
                    self.grenade_armed = !self.grenade_armed;
                }
                if ui.button("✚ Dress wounds [H]").clicked() {
                    self.heal_selected(renderer);
                }
                if ui.button("Next [Tab]").clicked() {
                    self.select_next_soldier();
                    self.refresh_scene(renderer);
                }
                ui.separator();
                if ui.button("⏭ End turn [Space]").clicked() {
                    self.end_turn(renderer);
                }
                if self.grenade_armed {
                    ui.colored_label(egui::Color32::ORANGE, "CHARGE ARMED — click a tile");
                }
            });
        });

        egui::TopBottomPanel::bottom("battle-log")
            .default_height(110.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
            });

        if let Some(winner) = self.battle.winner {
            egui::Window::new("Battle over")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let text = match winner {
                        Side::Order => "VICTORY — the incursion is banished.",
                        Side::Demons => "DEFEAT — the squad is lost.",
                    };
                    ui.label(egui::RichText::new(text).size(20.0).strong());
                    if ui.button("Return").clicked() {
                        leave = true;
                    }
                });
        }
        leave
    }

    // ------------------------------------------------------------------
    // Actions & scene upkeep

    fn heal_selected(&mut self, renderer: &mut Renderer) {
        let Some(id) = self.selected else { return };
        let result = self.battle.perform(Action::Heal { medic: id, target: id });
        self.apply(renderer, result);
    }

    fn end_turn(&mut self, renderer: &mut Renderer) {
        if self.battle.winner.is_some() {
            return;
        }
        match self.battle.perform(Action::EndTurn) {
            Ok(events) => self.consume(renderer, &events),
            Err(e) => {
                self.log.push(format!("cannot end turn: {e:?}"));
                return;
            }
        }
        let events = ai::run_demon_turn(&mut self.battle);
        self.consume(renderer, &events);
    }

    fn apply(
        &mut self,
        renderer: &mut Renderer,
        result: Result<Vec<Event>, ods_sim::battle::ActionError>,
    ) {
        match result {
            Ok(events) => self.consume(renderer, &events),
            Err(e) => self.log.push(format!("cannot: {e:?}")),
        }
    }

    fn consume(&mut self, renderer: &mut Renderer, events: &[Event]) {
        for e in events {
            self.log.push(describe(e, &self.battle));
        }
        self.refresh_chunks(renderer);
        self.refresh_scene(renderer);
    }

    fn select_next_soldier(&mut self) {
        let soldiers: Vec<UnitId> = self.battle.living(Side::Order).map(|u| u.id).collect();
        if soldiers.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = match self.selected {
            Some(current) => soldiers
                .iter()
                .cycle()
                .skip_while(|&&id| id != current)
                .nth(1)
                .copied(),
            None => soldiers.first().copied(),
        };
    }

    fn refresh_chunks(&mut self, renderer: &mut Renderer) {
        for coord in self.battle.world.take_dirty_chunks() {
            let mesh = mesh_chunk(&self.battle.world, coord);
            renderer.upsert_chunk(coord, &mesh);
        }
    }

    fn refresh_scene(&mut self, renderer: &mut Renderer) {
        let visible = self.battle.visible_tiles(Side::Order);

        let mut units = MeshData::default();
        for u in &self.battle.units {
            if !u.alive {
                continue;
            }
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
        renderer.set_units(&units);

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
        renderer.set_overlay(&verts, &indices);
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
