//! The interactive Battlescape: the 3D voxel view plus its egui HUD.

use glam::{IVec3, Mat4, Vec3};
use ods_geo::MissionToken;
use ods_render::{OrbitCamera, OverlayVertex, Renderer};
use ods_sim::battle::{Action, Battle, Event};
use ods_sim::units::{FireMode, Side, UnitId};
use ods_sim::{TILE_VOXELS, ai, scenario, voxel_to_tile};
use ods_voxel::{mesh_chunk_capped};
use winit::keyboard::KeyCode;

use std::collections::HashMap;

use crate::audio::{Audio, Sound};

const VS_F: f32 = ods_sim::VS as f32;
const HALF_TILE: f32 = TILE_VOXELS as f32 / 2.0;
const PAN_STEP: f32 = 12.0 * VS_F;
use crate::figures;

/// A transient battlefield effect.
struct Fx {
    kind: FxKind,
    from: Vec3,
    to: Vec3,
    color: [f32; 4],
    age: f32,
    life: f32,
}

enum FxKind {
    Tracer,
    Blast,
    Flash,
}

/// A scrap of combat text drifting up from a point on the field.
struct FloatText {
    text: String,
    color: egui::Color32,
    world: Vec3,
    age: f32,
    life: f32,
}

pub struct BattleScreen {
    pub battle: Battle,
    /// Present when this battle belongs to the campaign.
    pub token: Option<MissionToken>,
    pub camera: OrbitCamera,
    pub log: Vec<String>,
    selected: Option<UnitId>,
    fire_mode: FireMode,
    grenade_armed: bool,
    fx: Vec<Fx>,
    floaters: Vec<FloatText>,
    heart_timer: f32,
    shake: f32,
    fx_clock: f32,
    /// Visual (lerped) feet positions per unit index — the glide.
    visual: HashMap<u32, Vec3>,
    /// Tile under the cursor, plus a cached move preview to it.
    hover: Option<IVec3>,
    hover_path: Option<(Vec<IVec3>, i32)>,
    reachable: Vec<(IVec3, i32)>,
    /// Big announcement text and its remaining seconds.
    banner: Option<(String, f32)>,
    /// Cutaway: hide everything above z=16 to see ground-floor interiors.
    floor_cap: bool,
    /// Tint the ground known demons can see ([T]).
    show_threat: bool,
    /// The big tactical map ([M]).
    show_map: bool,
    /// Battle pacing: scales the walk glide (set from the options screen).
    pub anim_speed: f32,
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
            camera: {
                let mut cam = OrbitCamera::isometric(Vec3::new(center.x, center.y, 0.0));
                cam.distance *= VS_F; // the field doubled; stand back to match
                cam
            },
            log: vec!["The squad deploys.".to_string()],
            selected: None,
            fire_mode: FireMode::Snap,
            grenade_armed: false,
            fx: Vec::new(),
            floaters: Vec::new(),
            heart_timer: 0.0,
            shake: 0.0,
            fx_clock: 0.0,
            visual: HashMap::new(),
            hover: None,
            hover_path: None,
            reachable: Vec::new(),
            banner: Some(("THE SQUAD DEPLOYS".to_string(), 1.6)),
            floor_cap: false,
            show_threat: false,
            show_map: false,
            anim_speed: 1.0,
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

    pub fn click(&mut self, renderer: &mut Renderer, audio: Option<&Audio>, width: f32, height: f32) {
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
            self.apply(renderer, audio, result);
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
                    self.apply(renderer, audio, result);
                }
            }
        } else {
            let Some(mover) = self.selected else { return };
            let result = self.battle.perform(Action::Move { unit: mover, to: tile });
            self.apply(renderer, audio, result);
        }
    }

    pub fn key(&mut self, renderer: &mut Renderer, audio: Option<&Audio>, code: KeyCode) {
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
            KeyCode::KeyV => {
                // Pop smoke at the selected soldier's feet-ish forward tile.
                if let Some(id) = self.selected {
                    let at = self.battle.unit(id).tile + self.battle.unit(id).facing * 2;
                    let result = self.battle.perform(Action::ThrowSmoke { unit: id, at });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::KeyO => {
                // Open the nearest adjacent closed door.
                if let Some(id) = self.selected {
                    let me = self.battle.unit(id).tile;
                    let door = self
                        .battle
                        .doors
                        .iter()
                        .find(|(tile, open)| !open && (*tile - me).abs().max_element() <= 1)
                        .map(|(tile, _)| *tile);
                    if let Some(at) = door {
                        let result = self.battle.perform(Action::OpenDoor { unit: id, at });
                        self.apply(renderer, audio, result);
                    } else {
                        self.log.push("no closed door within reach".to_string());
                    }
                }
            }
            KeyCode::KeyK => {
                if let Some(id) = self.selected {
                    let result = self.battle.perform(Action::Kneel { unit: id });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::KeyB => {
                // Bind: strike the adjacent demon with the rod.
                if let Some(id) = self.selected {
                    let me = self.battle.unit(id).tile;
                    let target = self
                        .battle
                        .units
                        .iter()
                        .find(|u| {
                            u.is_active()
                                && u.side == Side::Demons
                                && (u.tile - me).abs().max_element() <= 1
                        })
                        .map(|u| u.id);
                    if let Some(target) = target {
                        let result = self.battle.perform(Action::Bind { unit: id, target });
                        self.apply(renderer, audio, result);
                    } else {
                        self.log.push("no demon within reach of the rod".to_string());
                    }
                }
            }
            KeyCode::KeyU => {
                // Pick up / put down a fallen comrade.
                if let Some(id) = self.selected {
                    if self.battle.unit(id).carrying.is_some() {
                        let at = self.battle.unit(id).tile + self.battle.unit(id).facing;
                        let result = self.battle.perform(Action::PutDown { unit: id, at });
                        self.apply(renderer, audio, result);
                    } else {
                        let me = self.battle.unit(id).tile;
                        let body = self
                            .battle
                            .units
                            .iter()
                            .find(|u| {
                                ((u.alive && !u.conscious) || u.is_corpse())
                                    && u.side == Side::Order
                                    && (u.tile - me).abs().max_element() <= 1
                            })
                            .map(|u| u.id);
                        if let Some(target) = body {
                            let result = self.battle.perform(Action::PickUp { unit: id, target });
                            self.apply(renderer, audio, result);
                        } else {
                            self.log.push("nobody down within reach".to_string());
                        }
                    }
                }
            }
            KeyCode::KeyJ => {
                if let Some(id) = self.selected {
                    let result = self.battle.perform(Action::Scavenge { unit: id });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::KeyH => self.heal_selected(renderer, audio),
            KeyCode::KeyX => self.amputate_selected(renderer, audio),
            KeyCode::KeyR => {
                if let Some(id) = self.selected {
                    let result = self.battle.perform(Action::InscribeWard { unit: id });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::KeyY => {
                if let Some(id) = self.selected {
                    let result = self.battle.perform(Action::Rally { unit: id });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::Tab => {
                self.select_next_soldier();
                self.refresh_scene(renderer);
            }
            KeyCode::Space | KeyCode::Enter => self.end_turn(renderer, audio),
            KeyCode::KeyF => {
                self.floor_cap = !self.floor_cap;
                self.remesh_all(renderer);
            }
            KeyCode::KeyT => {
                self.show_threat = !self.show_threat;
                self.refresh_scene(renderer);
            }
            KeyCode::KeyM => self.show_map = !self.show_map,
            KeyCode::KeyQ => self.camera.snap_turn(1),
            KeyCode::KeyE => self.camera.snap_turn(-1),
            KeyCode::KeyW => self.camera.pan(0.0, PAN_STEP),
            KeyCode::KeyS => self.camera.pan(0.0, -PAN_STEP),
            KeyCode::KeyA => self.camera.pan(-PAN_STEP, 0.0),
            KeyCode::KeyD => self.camera.pan(PAN_STEP, 0.0),
            _ => {}
        }
    }

    pub fn drag(&mut self, dx: f32, dy: f32) {
        // Horizontal drag walks around the field; vertical is damped so the
        // classic tabletop angle survives casual mouse movement (Q/E snap
        // back to the true diagonals).
        self.camera.orbit(dx * -0.008, dy * 0.003);
    }

    // ------------------------------------------------------------------
    // HUD

    /// Returns true when the player asked to leave the battle.
    pub fn hud(
        &mut self,
        ctx: &egui::Context,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        codex: Option<&ods_geo::Campaign>,
    ) -> bool {
        let mut leave = false;

        egui::TopBottomPanel::top("battle-top").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "Turn {} — {:?} to move",
                    self.battle.turn, self.battle.side_to_move
                ));
                use ods_sim::battle::{MissionRule, Weather};
                match self.battle.rule {
                    MissionRule::Standard => {}
                    MissionRule::Evacuate { needed, turns } => {
                        ui.colored_label(
                            egui::Color32::from_rgb(120, 230, 140),
                            format!(
                                "EVACUATE {}/{needed} — {} turns left",
                                self.battle.evacuated,
                                turns.saturating_sub(self.battle.turn)
                            ),
                        );
                    }
                    MissionRule::Interrupt { turns } => {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 160, 80),
                            format!(
                                "THE RITUAL COMPLETES IN {} — demolish the obelisk",
                                turns.saturating_sub(self.battle.turn)
                            ),
                        );
                    }
                    MissionRule::Snatch { target } => {
                        ui.colored_label(
                            egui::Color32::from_rgb(120, 200, 255),
                            format!(
                                "TAKE {} ALIVE — its death fails the mission",
                                self.battle.unit(target).name
                            ),
                        );
                    }
                }
                match self.battle.weather {
                    Weather::Clear => {}
                    Weather::Sandstorm => {
                        ui.colored_label(egui::Color32::from_rgb(220, 190, 120), "SANDSTORM");
                    }
                    Weather::Snowfall => {
                        ui.colored_label(egui::Color32::from_rgb(220, 230, 255), "SNOWFALL");
                    }
                    Weather::Rain => {
                        ui.colored_label(egui::Color32::from_rgb(140, 170, 220), "RAIN");
                    }
                }
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
                    self.heal_selected(renderer, audio);
                }
                let officer = self.selected.is_some_and(|id| {
                    let u = self.battle.unit(id);
                    u.can_rally && !u.rally_spent
                });
                if officer
                    && ui
                        .button(egui::RichText::new("📣 Rally [Y]").color(egui::Color32::from_rgb(255, 220, 120)))
                        .on_hover_text("once a battle: +30 morale to everyone within 8 tiles")
                        .clicked()
                    && let Some(id) = self.selected
                {
                    let result = self.battle.perform(Action::Rally { unit: id });
                    self.apply(renderer, audio, result);
                }
                let rot_near = self.selected.is_some_and(|id| {
                    let me = self.battle.unit(id).tile;
                    self.battle.units.iter().any(|u| {
                        u.alive
                            && u.side == Side::Order
                            && u.infected.is_some()
                            && (u.tile - me).abs().max_element() <= 1
                    })
                });
                if rot_near
                    && ui
                        .button(egui::RichText::new("🪚 Amputate [X]").color(egui::Color32::from_rgb(150, 220, 90)))
                        .on_hover_text("demonic rot festers in a crippled limb: saw it off before it turns them")
                        .clicked()
                {
                    self.amputate_selected(renderer, audio);
                }
                if ui.button("🧎 Kneel [K]").clicked()
                    && let Some(id) = self.selected
                {
                    let result = self.battle.perform(Action::Kneel { unit: id });
                    self.apply(renderer, audio, result);
                }
                if ui.button("Next [Tab]").clicked() {
                    self.select_next_soldier();
                    self.refresh_scene(renderer);
                }
                ui.separator();
                if ui.button("⏭ End turn [Space]").clicked() {
                    self.end_turn(renderer, audio);
                }
                if self.grenade_armed {
                    ui.colored_label(egui::Color32::ORANGE, "CHARGE ARMED — click a tile");
                }
            });
            // The intelligence line: what the cursor is worth.
            ui.horizontal_wrapped(|ui| {
                match (self.selected, self.hover) {
                    (Some(id), Some(tile)) => {
                        if let Some(enemy) = self.battle.unit_at(tile).filter(|&e| {
                            self.battle.unit(e).side == Side::Demons
                        }) {
                            // The shot forecast: the resolver's true odds.
                            let mut line = format!("Target: {}", self.battle.unit(enemy).name);
                            let mut seen = true;
                            for (label, mode) in
                                [("snap", FireMode::Snap), ("aimed", FireMode::Aimed), ("auto", FireMode::Auto)]
                            {
                                if let Some(f) = self.battle.forecast_shot(id, enemy, mode) {
                                    if f.rounds > 1 {
                                        line.push_str(&format!(
                                            "  {label} {}%×{} ({}TU)",
                                            f.chance, f.rounds, f.cost
                                        ));
                                    } else {
                                        line.push_str(&format!(
                                            "  {label} {}% ({}TU)",
                                            f.chance, f.cost
                                        ));
                                    }
                                    seen = f.seen;
                                    if mode == FireMode::Snap {
                                        line.push_str(&if f.stun {
                                            format!("  [SALT: stuns ≤{}]", f.power)
                                        } else {
                                            format!("  [dmg 0–{}]", f.power * 2)
                                        });
                                    }
                                }
                            }
                            if !seen {
                                line.push_str("  [NO LINE OF SIGHT]");
                            }
                            ui.colored_label(egui::Color32::LIGHT_RED, line);
                        } else if let Some((_, cost)) = &self.hover_path {
                            let u = self.battle.unit(id);
                            let ok = *cost <= u.tu;
                            ui.colored_label(
                                if ok { egui::Color32::LIGHT_GREEN } else { egui::Color32::GRAY },
                                format!("Move: {cost} TU of {}", u.tu),
                            );
                        }
                    }
                    _ => {
                        ui.weak("hover a tile for move costs; hover a demon for hit odds");
                    }
                }
                ui.separator();
                ui.weak("[Q]/[E] turn the field  [F] floor cutaway  [T] threat  [M] map  [O] door  [V] smoke  [B] bind  [K] kneel  [X] amputate  [R] ward  [Y] rally");
            });
        });

        // The war-room table map.
        egui::Window::new("Field map")
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, 64.0])
            .collapsible(true)
            .resizable(false)
            .show(ctx, |ui| {
                self.minimap(ui, 6.0, false);
            });

        // The tactical map [M]: the whole field at reading size; click a
        // tile and the camera walks there.
        if self.show_map {
            let mut open = true;
            let mut jump: Option<IVec3> = None;
            egui::Window::new("Tactical map [M]")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .show(ctx, |ui| {
                    ui.weak("click anywhere to swing the camera there");
                    jump = self.minimap(ui, 13.0, true);
                });
            if let Some(tile) = jump {
                let p = (tile * TILE_VOXELS).as_vec3() + Vec3::new(HALF_TILE, HALF_TILE, 0.0);
                self.camera.target.x = p.x;
                self.camera.target.y = p.y;
            }
            self.show_map = open;
        }

        // The field codex: hover a demon and the bestiary answers with
        // what the Order actually knows about the breed.
        if let Some(tile) = self.hover
            && let Some(enemy) = self
                .battle
                .unit_at(tile)
                .filter(|&e| self.battle.unit(e).side == Side::Demons)
        {
            let species = self.battle.unit(enemy).species;
            egui::Window::new("Field codex")
                .anchor(egui::Align2::LEFT_TOP, [8.0, 120.0])
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .show(ctx, |ui| {
                    ui.strong(species.name());
                    ui.set_max_width(240.0);
                    match codex {
                        // In a campaign the codex only says what's earned.
                        Some(c) => {
                            ui.label(
                                egui::RichText::new(crate::geoscape::bestiary_lore(species))
                                    .small(),
                            );
                            if c.codex_slain.contains(&species) {
                                let key = species.name().to_lowercase().replace(' ', "_");
                                if let Some(d) = ods_sim::data::species().get(&key) {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Necropsy: {} TU · {} HP · armor {}/{}/{}",
                                            d.tu, d.health, d.armor.0, d.armor.1, d.armor.2
                                        ))
                                        .small()
                                        .color(egui::Color32::from_rgb(200, 150, 120)),
                                    );
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new("no necropsy on record")
                                        .weak()
                                        .small(),
                                );
                            }
                        }
                        // A skirmish teaches freely.
                        None => {
                            ui.label(
                                egui::RichText::new(crate::geoscape::bestiary_lore(species))
                                    .small(),
                            );
                        }
                    }
                });
        }

        self.draw_floaters(ctx, renderer.aspect());

        // The HUD wears the squad's blood: dark red creeping in from the
        // corners as the muster bleeds out.
        let vitality = self.squad_vitality();
        if vitality < 0.6 {
            let screen = ctx.viewport_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("blood-hud"),
            ));
            let soak = ((0.6 - vitality) / 0.6).clamp(0.0, 1.0);
            let a = (soak * 110.0) as u8;
            let color = egui::Color32::from_rgba_unmultiplied(110, 8, 8, a);
            let r = 90.0 + 130.0 * soak;
            for corner in [
                screen.min,
                egui::pos2(screen.max.x, screen.min.y),
                egui::pos2(screen.min.x, screen.max.y),
                screen.max,
            ] {
                painter.circle_filled(corner, r, color);
            }
        }

        // While a Prince holds one of yours, the world's edges bleed violet.
        let mind_held = self
            .battle
            .units
            .iter()
            .any(|u| u.is_active() && u.side == Side::Order && u.possessed > 0);
        if mind_held {
            let screen = ctx.viewport_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("possession-vignette"),
            ));
            let pulse = 0.10 + 0.07 * (self.fx_clock * 2.2).sin();
            let a = (pulse * 255.0) as u8;
            let color = egui::Color32::from_rgba_unmultiplied(120, 25, 160, a);
            let t = 30.0;
            painter.rect_filled(
                egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, screen.min.y + t)),
                0.0,
                color,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(screen.min.x, screen.max.y - t), screen.max),
                0.0,
                color,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(screen.min, egui::pos2(screen.min.x + t, screen.max.y)),
                0.0,
                color,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(screen.max.x - t, screen.min.y), screen.max),
                0.0,
                color,
            );
        }

        if let Some((text, ttl)) = &self.banner {
            let alpha = (ttl.min(0.6) / 0.6 * 255.0) as u8;
            egui::Area::new(egui::Id::new("banner"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 120.0])
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .size(30.0)
                            .strong()
                            .color(egui::Color32::from_rgba_unmultiplied(255, 235, 200, alpha)),
                    );
                });
        }

        // The console: the whole squad at a glance along the very bottom,
        // the way the 1994 strip did it — vitals as bars, click to select.
        egui::TopBottomPanel::bottom("squad-console").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let ids: Vec<UnitId> = self
                    .battle
                    .units
                    .iter()
                    .filter(|u| u.side == Side::Order && !u.civilian && u.alive)
                    .map(|u| u.id)
                    .collect();
                let mut select: Option<UnitId> = None;
                for id in ids {
                    let u = self.battle.unit(id);
                    let is_sel = self.selected == Some(id);
                    let card = egui::Frame::new()
                        .fill(if is_sel {
                            egui::Color32::from_rgb(48, 40, 20)
                        } else {
                            egui::Color32::from_rgb(22, 18, 16)
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if is_sel {
                                egui::Color32::from_rgb(230, 190, 90)
                            } else {
                                egui::Color32::from_gray(70)
                            },
                        ))
                        .inner_margin(egui::Margin::symmetric(6, 3));
                    let resp = card
                        .show(ui, |ui| {
                            ui.set_width(96.0);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let short =
                                u.name.split_whitespace().last().unwrap_or(&u.name);
                            let mut tags = String::new();
                            if u.kneeling {
                                tags.push('🧎');
                            }
                            if u.wounds > 0 {
                                tags.push('🩸');
                            }
                            if u.possessed > 0 {
                                tags.push('👁');
                            }
                            if !u.conscious {
                                tags.push('✖');
                            }
                            ui.label(egui::RichText::new(format!("{short} {tags}")).small());
                            mini_bar(
                                ui,
                                u.health as f32 / u.health_max.max(1) as f32,
                                egui::Color32::from_rgb(190, 60, 50),
                            );
                            mini_bar(
                                ui,
                                u.tu as f32 / u.tu_max.max(1) as f32,
                                egui::Color32::from_rgb(200, 170, 60),
                            );
                            mini_bar(
                                ui,
                                u.morale as f32 / 100.0,
                                egui::Color32::from_rgb(150, 90, 200),
                            );
                        })
                        .response;
                    if resp.interact(egui::Sense::click()).clicked() {
                        select = Some(id);
                    }
                }
                if let Some(id) = select {
                    self.selected = Some(id);
                    self.refresh_scene(renderer);
                }
                ui.separator();
                // The selected soldier's weapon plate.
                if let Some(id) = self.selected {
                    let u = self.battle.unit(id);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(u.weapon.name)
                                .strong()
                                .color(egui::Color32::from_rgb(220, 200, 150)),
                        );
                        let mut plate = String::new();
                        for (label, mode) in [
                            ("snap", FireMode::Snap),
                            ("aim", FireMode::Aimed),
                            ("auto", FireMode::Auto),
                        ] {
                            if let (Some(c), Some(t)) = (u.hit_chance(mode), u.fire_cost(mode)) {
                                plate.push_str(&format!("{label} {c}%/{t}TU  "));
                            }
                        }
                        ui.label(egui::RichText::new(plate).small().weak());
                        ui.label(
                            egui::RichText::new(format!(
                                "🧨 {}   ✚ {}{}",
                                u.grenades,
                                u.heal_charges,
                                if u.blade { "   🗡" } else { "" }
                            ))
                            .small(),
                        );
                    });
                }
                ui.separator();
                let threat = ui
                    .selectable_label(self.show_threat, "☠ Threat [T]")
                    .on_hover_text("tint every tile a known demon can see");
                if threat.clicked() {
                    self.show_threat = !self.show_threat;
                    self.refresh_scene(renderer);
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

    fn heal_selected(&mut self, renderer: &mut Renderer, audio: Option<&Audio>) {
        let Some(id) = self.selected else { return };
        let result = self.battle.perform(Action::Heal { medic: id, target: id });
        self.apply(renderer, audio, result);
    }

    /// The saw: take the rot off yourself or an adjacent squadmate.
    fn amputate_selected(&mut self, renderer: &mut Renderer, audio: Option<&Audio>) {
        let Some(id) = self.selected else { return };
        let me = self.battle.unit(id).tile;
        let target = self
            .battle
            .units
            .iter()
            .filter(|u| {
                u.alive
                    && u.side == Side::Order
                    && u.infected.is_some()
                    && (u.tile - me).abs().max_element() <= 1
            })
            .min_by_key(|u| if u.id == id { 0 } else { 1 })
            .map(|u| u.id);
        match target {
            Some(target) => {
                let result = self.battle.perform(Action::Amputate { medic: id, target });
                self.apply(renderer, audio, result);
            }
            None => self.log.push("nobody in reach has the rot".to_string()),
        }
    }

    fn end_turn(&mut self, renderer: &mut Renderer, audio: Option<&Audio>) {
        if self.battle.winner.is_some() {
            return;
        }
        let fled = ai::run_civilian_moves(&mut self.battle);
        self.consume(renderer, audio, &fled);
        match self.battle.perform(Action::EndTurn) {
            Ok(events) => self.consume(renderer, audio, &events),
            Err(e) => {
                self.log.push(format!("cannot end turn: {e:?}"));
                return;
            }
        }
        let events = ai::run_demon_turn(&mut self.battle);
        self.consume(renderer, audio, &events);
    }

    fn apply(
        &mut self,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        result: Result<Vec<Event>, ods_sim::battle::ActionError>,
    ) {
        match result {
            Ok(events) => self.consume(renderer, audio, &events),
            Err(e) => self.log.push(format!("cannot: {e:?}")),
        }
    }

    fn consume(&mut self, renderer: &mut Renderer, audio: Option<&Audio>, events: &[Event]) {
        for e in events {
            self.log.push(describe(e, &self.battle));
            self.spawn_fx(e, audio);
        }
        self.refresh_chunks(renderer);
        self.refresh_scene(renderer);
    }

    // ------------------------------------------------------------------
    // Effects

    fn unit_pos(&self, id: UnitId, z: f32) -> Vec3 {
        (self.battle.unit(id).tile * TILE_VOXELS).as_vec3()
            + Vec3::new(HALF_TILE, HALF_TILE, z * VS_F)
    }

    fn tile_pos(at: IVec3, z: f32) -> Vec3 {
        (at * TILE_VOXELS).as_vec3() + Vec3::new(HALF_TILE, HALF_TILE, z * VS_F)
    }

    /// Fights on the night side of the world are lit by muzzle and flame.
    fn is_night(&self) -> bool {
        self.battle.vision_tiles < 14
    }

    fn float(&mut self, over: UnitId, text: impl Into<String>, color: egui::Color32) {
        self.floaters.push(FloatText {
            text: text.into(),
            color,
            world: self.unit_pos(over, 20.0),
            age: 0.0,
            life: 1.4,
        });
    }

    fn spawn_fx(&mut self, event: &Event, audio: Option<&Audio>) {
        let play = |s: Sound| {
            if let Some(a) = audio {
                a.play(s);
            }
        };
        // Floating combat text: the numbers rise from where they happened.
        match event {
            Event::Damaged { unit, amount, .. } => {
                self.float(*unit, format!("-{amount}"), egui::Color32::from_rgb(240, 80, 60));
            }
            Event::Burned { unit, amount } => {
                self.float(*unit, format!("-{amount}"), egui::Color32::from_rgb(255, 150, 40));
            }
            Event::Fired { target, hit: false, .. } => {
                self.float(*target, "MISS", egui::Color32::from_gray(170));
            }
            Event::Healed { target, .. } => {
                self.float(*target, "STAUNCHED", egui::Color32::from_rgb(110, 220, 120));
            }
            Event::Stunned { unit, .. } => {
                self.float(*unit, "STUN", egui::Color32::from_rgb(120, 200, 255));
            }
            Event::Terrified { target, morale_lost, .. } => {
                if *morale_lost > 0 {
                    self.float(
                        *target,
                        format!("TERROR -{morale_lost}"),
                        egui::Color32::from_rgb(200, 120, 255),
                    );
                } else {
                    self.float(*target, "RESISTED", egui::Color32::from_gray(200));
                }
            }
            Event::PartCrippled { unit, part } => {
                self.float(
                    *unit,
                    format!("{} CRIPPLED", part.name().to_uppercase()),
                    egui::Color32::from_rgb(255, 120, 120),
                );
            }
            Event::PartSevered { unit, part } => {
                self.float(
                    *unit,
                    format!("{} SEVERED", part.name().to_uppercase()),
                    egui::Color32::from_rgb(200, 30, 30),
                );
            }
            Event::Gibbed { unit } => {
                self.float(*unit, "OBLITERATED", egui::Color32::from_rgb(180, 20, 20));
            }
            Event::Infected { unit, .. } => {
                self.float(*unit, "INFECTED", egui::Color32::from_rgb(150, 220, 90));
            }
            Event::Amputated { target, .. } => {
                self.float(*target, "AMPUTATED", egui::Color32::from_rgb(230, 200, 120));
            }
            Event::InfectionTurned { unit } => {
                self.float(*unit, "TURNED", egui::Color32::from_rgb(190, 90, 240));
            }
            Event::Defiled { corpse } => {
                self.float(*corpse, "THE DEAD RISE", egui::Color32::from_rgb(190, 90, 240));
            }
            Event::CorpseEaten { corpse, .. } => {
                self.float(*corpse, "DEVOURED", egui::Color32::from_rgb(200, 120, 60));
            }
            Event::Summoned { unit } => {
                self.float(*unit, "IT COMES THROUGH", egui::Color32::from_rgb(255, 60, 50));
            }
            Event::WardBurned { unit, .. } => {
                self.float(*unit, "WARDED", egui::Color32::from_rgb(60, 230, 200));
            }
            Event::Whispered { unit } => {
                self.float(*unit, "whispers...", egui::Color32::from_rgb(190, 120, 230));
            }
            Event::AtrocityFound { unit, .. } => {
                self.float(*unit, "ATROCITY", egui::Color32::from_rgb(220, 60, 40));
            }
            Event::Riposte { target, hit: true, .. } => {
                self.float(*target, "RIPOSTE", egui::Color32::from_rgb(230, 230, 160));
            }
            Event::CircletShattered { unit } => {
                self.float(*unit, "CIRCLET SHATTERS", egui::Color32::from_rgb(120, 200, 255));
            }
            Event::Rallied { by } => {
                self.float(*by, "RALLY", egui::Color32::from_rgb(255, 220, 120));
            }
            Event::Evacuated { unit } => {
                self.float(*unit, "AWAY", egui::Color32::from_rgb(120, 230, 140));
            }
            Event::FloorCollapsed { at } => {
                let p = Self::tile_pos(*at, 18.0);
                self.fx.push(Fx {
                    kind: FxKind::Blast,
                    from: p,
                    to: p,
                    color: [0.7, 0.6, 0.5, 0.7],
                    age: 0.0,
                    life: 0.6,
                });
                self.shake += 4.0;
            }
            Event::Panicked { unit } => {
                self.float(*unit, "PANIC", egui::Color32::from_rgb(255, 210, 90));
            }
            _ => {}
        }
        match event {
            Event::TurnStarted { side, .. } => {
                let text = match side {
                    Side::Order => "THE ORDER MOVES",
                    Side::Demons => "THE OTHERSIDE STIRS",
                };
                self.banner = Some((text.to_string(), 1.1));
            }
            Event::BattleOver { winner } => {
                let (text, sound) = match winner {
                    Side::Order => ("THE FIELD IS OURS", Sound::Victory),
                    Side::Demons => ("THE LINE BREAKS", Sound::Defeat),
                };
                self.banner = Some((text.to_string(), 3.0));
                play(sound);
            }
            Event::Fired { unit, target, .. } => {
                let side = self.battle.unit(*unit).side;
                let color = if side == Side::Order {
                    [1.0, 0.9, 0.4, 0.9]
                } else {
                    [0.9, 0.4, 0.2, 0.9]
                };
                self.fx.push(Fx {
                    kind: FxKind::Tracer,
                    from: self.unit_pos(*unit, 13.0),
                    to: self.unit_pos(*target, 9.0),
                    color,
                    age: 0.0,
                    life: 0.22,
                });
                // After dark the muzzle is a lantern: a brief warm glow.
                if self.is_night() {
                    let p = self.unit_pos(*unit, 10.0);
                    self.fx.push(Fx {
                        kind: FxKind::Flash,
                        from: p,
                        to: p,
                        color: [1.0, 0.75, 0.35, 0.5],
                        age: 0.0,
                        life: 0.16,
                    });
                }
                play(Sound::Shot);
            }
            Event::Exploded { at, .. } => {
                let p = Self::tile_pos(*at, 5.0);
                self.fx.push(Fx {
                    kind: FxKind::Blast,
                    from: p,
                    to: p,
                    color: [1.0, 0.55, 0.15, 0.8],
                    age: 0.0,
                    life: 0.55,
                });
                self.shake += 5.0;
                play(Sound::Blast);
            }
            Event::TerrainDestroyed { center, .. } => {
                self.fx.push(Fx {
                    kind: FxKind::Blast,
                    from: *center,
                    to: *center,
                    color: [0.9, 0.8, 0.5, 0.5],
                    age: 0.0,
                    life: 0.3,
                });
            }
            Event::Died { unit } => {
                let p = self.unit_pos(*unit, 6.0);
                self.fx.push(Fx {
                    kind: FxKind::Flash,
                    from: p,
                    to: p,
                    color: [0.9, 0.1, 0.1, 0.7],
                    age: 0.0,
                    life: 0.5,
                });
                play(Sound::Death);
            }
            Event::Gibbed { unit } => {
                let p = self.unit_pos(*unit, 6.0);
                self.fx.push(Fx {
                    kind: FxKind::Blast,
                    from: p,
                    to: p,
                    color: [0.8, 0.08, 0.08, 0.9],
                    age: 0.0,
                    life: 0.7,
                });
                self.shake += 4.0;
                play(Sound::Death);
            }
            Event::Defiled { corpse } => {
                let p = self.unit_pos(*corpse, 8.0);
                self.fx.push(Fx {
                    kind: FxKind::Flash,
                    from: p,
                    to: p,
                    color: [0.6, 0.15, 0.7, 0.8],
                    age: 0.0,
                    life: 0.8,
                });
                play(Sound::Dread);
            }
            Event::Taken { unit } | Event::Hatched { unit } => {
                let p = self.unit_pos(*unit, 8.0);
                self.fx.push(Fx {
                    kind: FxKind::Flash,
                    from: p,
                    to: p,
                    color: [0.6, 0.15, 0.7, 0.8],
                    age: 0.0,
                    life: 0.8,
                });
                play(Sound::Dread);
            }
            Event::Panicked { .. } | Event::Berserked { .. } => play(Sound::Dread),
            Event::AtrocityFound { .. } => play(Sound::Dread),
            Event::Whispered { .. } | Event::Possessed { .. } => play(Sound::Whisper),
            Event::SummoningScribed { at } | Event::SummoningDisrupted { at } => {
                let p = Self::tile_pos(*at, 5.0);
                self.fx.push(Fx {
                    kind: FxKind::Flash,
                    from: p,
                    to: p,
                    color: [1.0, 0.15, 0.1, 0.7],
                    age: 0.0,
                    life: 0.6,
                });
                play(Sound::Dread);
            }
            Event::Terrified { morale_lost, .. } if *morale_lost > 0 => play(Sound::Dread),
            Event::Subdued { .. } => play(Sound::Click),
            _ => {}
        }
    }

    /// The squad's remaining blood, 0..=1 (dead men hold none).
    fn squad_vitality(&self) -> f32 {
        let (mut have, mut max) = (0i32, 0i32);
        for u in &self.battle.units {
            if u.side == Side::Order && !u.civilian {
                max += u.health_max;
                if u.alive {
                    have += u.health.max(0);
                }
            }
        }
        if max == 0 { 1.0 } else { have as f32 / max as f32 }
    }

    /// Per-frame upkeep: hover intelligence, gliding figures, banner, fx.
    pub fn update_frame(
        &mut self,
        dt: f32,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        width: f32,
        height: f32,
    ) {
        // Hover: what tile is under the cursor, and what would a move cost?
        let (origin, dir) = self.camera.screen_ray(self.cursor.0, self.cursor.1, width, height);
        let hover = self.battle.world.raycast(origin, dir, 4000.0).map(|hit| {
            let open = hit.position + hit.normal.as_vec3() * 0.01;
            voxel_to_tile(open.floor().as_ivec3())
        });
        if hover != self.hover {
            self.hover = hover;
            self.hover_path = match (self.selected, hover) {
                (Some(id), Some(tile))
                    if self.battle.unit(id).is_active()
                        && self.battle.unit_at(tile).is_none() =>
                {
                    self.battle.preview_path(id, tile)
                }
                _ => None,
            };
            self.refresh_scene(renderer);
        }

        // The glide: visual positions chase the sim tiles.
        let mut moved = false;
        for u in &self.battle.units {
            let target = (u.tile * TILE_VOXELS).as_vec3()
                + Vec3::new(HALF_TILE, HALF_TILE, ods_sim::scenario::GROUND_TOP as f32);
            let entry = self.visual.entry(u.id.0).or_insert(target);
            let delta = target - *entry;
            if delta.length_squared() > 0.05 {
                *entry += delta * (dt * 9.0 * self.anim_speed).min(1.0);
                moved = true;
            } else if *entry != target {
                *entry = target;
                moved = true;
            }
        }
        if moved {
            let visible = self.battle.visible_tiles(Side::Order);
            let (fig_verts, fig_indices) =
                figures::build_figures(&self.battle, &visible, &self.visual);
            renderer.set_figures(&fig_verts, &fig_indices);
        }

        if let Some((_, ttl)) = &mut self.banner {
            *ttl -= dt;
            if *ttl <= 0.0 {
                self.banner = None;
            }
        }

        for f in &mut self.floaters {
            f.age += dt;
        }
        self.floaters.retain(|f| f.age < f.life);

        // When the squad runs low on blood you hear your own.
        if self.battle.winner.is_none() && self.squad_vitality() < 0.4 {
            self.heart_timer -= dt;
            if self.heart_timer <= 0.0 {
                self.heart_timer = 1.15;
                if let Some(a) = audio {
                    a.play(Sound::Heartbeat);
                }
            }
        } else {
            self.heart_timer = 0.0;
        }

        self.update_fx(dt, renderer);
    }

    /// Paint the floating combat text: project each scrap of text from the
    /// field into screen space and let it rise and fade.
    fn draw_floaters(&self, ctx: &egui::Context, aspect: f32) {
        if self.floaters.is_empty() {
            return;
        }
        let vp = self.camera_vp(aspect);
        // The 3D view fills the whole window, so project against the full
        // viewport, not the panel-clipped content area.
        let screen = ctx.viewport_rect();
        let painter =
            ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("floaters")));
        for f in &self.floaters {
            let t = (f.age / f.life).clamp(0.0, 1.0);
            let world = f.world + Vec3::Z * (8.0 * VS_F * t);
            let clip = vp * world.extend(1.0);
            if clip.w <= 0.1 {
                continue; // behind the camera
            }
            let ndc = clip.truncate() / clip.w;
            let pos = egui::pos2(
                screen.center().x + ndc.x * screen.width() / 2.0,
                screen.center().y - ndc.y * screen.height() / 2.0,
            );
            let alpha = ((1.0 - t) * 255.0) as u8;
            let color = egui::Color32::from_rgba_unmultiplied(
                f.color.r(),
                f.color.g(),
                f.color.b(),
                alpha,
            );
            painter.text(
                pos,
                egui::Align2::CENTER_BOTTOM,
                &f.text,
                egui::FontId::proportional(15.0),
                color,
            );
        }
    }

    /// Advance effect ages and camera shake; rebuild the fx mesh.
    fn update_fx(&mut self, dt: f32, renderer: &mut Renderer) {
        self.fx_clock += dt;
        self.shake *= (-6.0 * dt).exp();
        if self.shake < 0.05 {
            self.shake = 0.0;
        }
        for fx in &mut self.fx {
            fx.age += dt;
        }
        self.fx.retain(|f| f.age < f.life);

        let mut verts: Vec<OverlayVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for fx in &self.fx {
            let t = (fx.age / fx.life).clamp(0.0, 1.0);
            let fade = 1.0 - t;
            let mut color = fx.color;
            color[3] *= fade;
            match fx.kind {
                FxKind::Tracer => {
                    // A bolt in flight: a short bright segment racing along
                    // the line of fire.
                    let head = fx.from.lerp(fx.to, t);
                    let tail = fx.from.lerp(fx.to, (t - 0.3).max(0.0));
                    let dir = fx.to - fx.from;
                    let perp = dir.cross(Vec3::Z).normalize_or(Vec3::X) * (0.5 * VS_F);
                    push_quad(
                        &mut verts,
                        &mut indices,
                        [tail - perp, tail + perp, head + perp, head - perp],
                        color,
                    );
                }
                FxKind::Blast => {
                    let r = (3.0 + 26.0 * t) * VS_F;
                    push_flat_square(&mut verts, &mut indices, fx.from, r, color);
                }
                FxKind::Flash => {
                    push_flat_square(&mut verts, &mut indices, fx.from, 6.0 * VS_F, color);
                }
            }
        }
        // Weather: streaks falling around the camera's patch of the field.
        {
            use ods_sim::battle::Weather;
            let (count, color, len, drift) = match self.battle.weather {
                Weather::Clear => (0, [0.0; 4], 0.0, 0.0),
                Weather::Sandstorm => (70, [0.82, 0.7, 0.4, 0.35], 2.0, 26.0),
                Weather::Snowfall => (50, [0.95, 0.96, 1.0, 0.5], 1.2, 4.0),
                Weather::Rain => (60, [0.6, 0.7, 0.95, 0.4], 5.0, 6.0),
            };
            let anchor = self.camera.target;
            for i in 0..count {
                // Deterministic scatter, cycling on the clock.
                let h = (i * 2654435761u32) as f32 / u32::MAX as f32;
                let h2 = (i * 40503u32 + 977) as f32 / u32::MAX as f32 * 1000.0 % 1.0;
                let cycle = 40.0;
                let fall = (self.fx_clock * (18.0 + h * 8.0) + h2 * cycle) % cycle;
                let p = anchor
                    + Vec3::new(
                        ((h - 0.5) * 160.0 + self.fx_clock.sin() * drift * h) * VS_F,
                        (h2 - 0.5) * 160.0 * VS_F,
                        (38.0 - fall) * VS_F,
                    );
                push_quad(
                    &mut verts,
                    &mut indices,
                    [
                        p,
                        p + Vec3::new(0.4, 0.0, 0.0),
                        p + Vec3::new(0.4, 0.0, -len),
                        p + Vec3::new(0.0, 0.0, -len),
                    ],
                    color,
                );
            }
        }

        // Possession halos: a slow-turning sigil diamond over seized minds.
        for u in &self.battle.units {
            if u.is_active() && u.possessed > 0 {
                let c = (u.tile * TILE_VOXELS).as_vec3()
                    + Vec3::new(HALF_TILE, HALF_TILE, 21.0 * VS_F);
                let a = self.fx_clock * 1.7;
                let r = 4.5 * VS_F;
                let e1 = Vec3::new(a.cos(), a.sin(), 0.0) * r;
                let e2 = Vec3::new(-a.sin(), a.cos(), 0.0) * r;
                let pulse = 0.45 + 0.2 * (self.fx_clock * 3.0).sin();
                push_quad(
                    &mut verts,
                    &mut indices,
                    [c + e1, c + e2, c - e1, c - e2],
                    [0.65, 0.2, 0.9, pulse],
                );
            }
        }
        renderer.set_fx(&verts, &indices);
    }

    /// The battle camera's matrix, with explosion shake applied.
    pub fn camera_vp(&self, aspect: f32) -> Mat4 {
        let mut cam = self.camera.clone();
        if self.shake > 0.0 {
            let s = self.shake;
            cam.target += Vec3::new(
                (self.fx_clock * 71.0).sin() * s,
                (self.fx_clock * 63.0).cos() * s,
                0.0,
            );
        }
        cam.view_proj(aspect)
    }

    /// The table map at `px` per tile. When clickable, returns the tile
    /// the player tapped (for camera jumps).
    fn minimap(&mut self, ui: &mut egui::Ui, px: f32, clickable: bool) -> Option<IVec3> {
        let (min, max) = self.battle.tiles.bounds();
        let size = max - min;
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(size.x as f32 * px, size.y as f32 * px),
            if clickable { egui::Sense::click() } else { egui::Sense::hover() },
        );
        let clicked = if resp.clicked() {
            resp.interact_pointer_pos().map(|p| {
                IVec3::new(
                    min.x + ((p.x - rect.min.x) / px) as i32,
                    min.y + ((rect.max.y - p.y) / px) as i32,
                    0,
                )
            })
        } else {
            None
        };
        let paint = ui.painter_at(rect);
        let visible = self.battle.visible_tiles(Side::Order);
        for y in min.y..max.y {
            for x in min.x..max.x {
                let tile = IVec3::new(x, y, 0);
                let mut color = if self.battle.tiles.is_walkable(tile) {
                    egui::Color32::from_gray(60)
                } else {
                    egui::Color32::from_gray(25)
                };
                if self.battle.tiles.is_walkable(IVec3::new(x, y, 1)) {
                    color = egui::Color32::from_gray(90); // upper floor
                }
                if !visible.contains(&tile) {
                    color = color.linear_multiply(0.4);
                }
                for (ct, kind, _) in &self.battle.clouds {
                    if ct.x == x && ct.y == y {
                        color = match kind {
                            ods_sim::battle::CloudKind::Fire => egui::Color32::from_rgb(220, 110, 30),
                            ods_sim::battle::CloudKind::Smoke => egui::Color32::from_gray(140),
                        };
                    }
                }
                let p = egui::pos2(
                    rect.min.x + (x - min.x) as f32 * px,
                    // North up: higher y draws higher.
                    rect.max.y - (y - min.y + 1) as f32 * px,
                );
                paint.rect_filled(egui::Rect::from_min_size(p, egui::vec2(px, px)), 0.0, color);
            }
        }
        if let Some(obj) = &self.battle.objective {
            let t0 = ods_sim::voxel_to_tile(obj.min);
            let p = egui::pos2(
                rect.min.x + (t0.x - min.x) as f32 * px,
                rect.max.y - (t0.y - min.y + 1) as f32 * px,
            );
            paint.rect_filled(
                egui::Rect::from_min_size(p, egui::vec2(px * 1.5, px * 2.0)),
                0.0,
                egui::Color32::GOLD,
            );
        }
        for u in &self.battle.units {
            if !u.alive {
                continue;
            }
            if u.side == Side::Demons && !visible.contains(&u.tile) {
                continue;
            }
            let color = if u.civilian {
                egui::Color32::YELLOW
            } else if u.side == Side::Order {
                if u.possessed > 0 { egui::Color32::from_rgb(180, 60, 220) } else { egui::Color32::from_rgb(80, 140, 255) }
            } else {
                egui::Color32::from_rgb(230, 60, 40)
            };
            let p = egui::pos2(
                rect.min.x + (u.tile.x - min.x) as f32 * px + px / 2.0,
                rect.max.y - (u.tile.y - min.y) as f32 * px - px / 2.0,
            );
            paint.circle_filled(p, px * 0.45, color);
        }
        // Ghost intel: hollow rings where lost demons were last seen.
        for (&id, &tile) in &self.battle.last_known {
            let u = self.battle.unit(id);
            if u.is_active() && !visible.contains(&u.tile) {
                let p = egui::pos2(
                    rect.min.x + (tile.x - min.x) as f32 * px + px / 2.0,
                    rect.max.y - (tile.y - min.y) as f32 * px - px / 2.0,
                );
                paint.circle_stroke(
                    p,
                    px * 0.45,
                    egui::Stroke::new(1.2, egui::Color32::from_rgb(190, 70, 50)),
                );
            }
        }
        clicked
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

    fn cap(&self) -> Option<i32> {
        self.floor_cap.then_some(TILE_VOXELS)
    }

    fn refresh_chunks(&mut self, renderer: &mut Renderer) {
        let cap = self.cap();
        for coord in self.battle.world.take_dirty_chunks() {
            let mesh = mesh_chunk_capped(&self.battle.world, coord, cap);
            renderer.upsert_chunk(coord, &mesh);
        }
    }

    /// Rebuild every chunk (floor-slice toggles change the whole view).
    fn remesh_all(&mut self, renderer: &mut Renderer) {
        let cap = self.cap();
        for coord in self.battle.world.chunk_coords() {
            let mesh = mesh_chunk_capped(&self.battle.world, coord, cap);
            renderer.upsert_chunk(coord, &mesh);
        }
        self.battle.world.take_dirty_chunks();
    }

    fn refresh_scene(&mut self, renderer: &mut Renderer) {
        let visible = self.battle.visible_tiles(Side::Order);

        self.reachable = match self.selected {
            Some(id) if self.battle.unit(id).is_active() => self.battle.reachable(id),
            _ => Vec::new(),
        };

        // Body-part voxel figures for every visible unit.
        let (fig_verts, fig_indices) =
            figures::build_figures(&self.battle, &visible, &self.visual);
        renderer.set_figures(&fig_verts, &fig_indices);

        let mut verts: Vec<OverlayVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let (min, max) = self.battle.tiles.bounds();
        // At night, open flame throws a pool of warm light; everything else
        // sinks into cold blue. Distance to the nearest fire decides which.
        let night = self.is_night();
        let fires: Vec<IVec3> = self
            .battle
            .clouds
            .iter()
            .filter(|(_, kind, _)| *kind == ods_sim::battle::CloudKind::Fire)
            .map(|(t, _, _)| *t)
            .collect();
        let fire_dist = |tile: IVec3| -> i32 {
            fires
                .iter()
                .map(|f| (f.x - tile.x).abs().max((f.y - tile.y).abs()) + (f.z - tile.z).abs())
                .min()
                .unwrap_or(i32::MAX)
        };
        for z in min.z..max.z {
            for y in min.y..max.y {
                for x in min.x..max.x {
                    let tile = IVec3::new(x, y, z);
                    if !visible.contains(&tile) {
                        push_tile_quad(&mut verts, &mut indices, tile, [0.0, 0.0, 0.02, 0.55]);
                    } else if night {
                        match fire_dist(tile) {
                            0 => {} // the burning tile draws its own color below
                            1 => push_tile_quad(
                                &mut verts,
                                &mut indices,
                                tile,
                                [1.0, 0.6, 0.25, 0.16],
                            ),
                            2 => push_tile_quad(
                                &mut verts,
                                &mut indices,
                                tile,
                                [0.9, 0.5, 0.2, 0.07],
                            ),
                            _ => push_tile_quad(
                                &mut verts,
                                &mut indices,
                                tile,
                                [0.02, 0.04, 0.12, 0.28],
                            ),
                        }
                    }
                }
            }
        }
        // Ghost intel: a demon that slipped out of sight leaves a dim red
        // box where it was LAST seen — memory, not truth.
        for (&id, &tile) in &self.battle.last_known {
            let u = self.battle.unit(id);
            if u.is_active() && !visible.contains(&u.tile) {
                push_wire_box(&mut verts, &mut indices, tile, [0.75, 0.22, 0.18, 0.55]);
                push_tile_quad(&mut verts, &mut indices, tile, [0.6, 0.1, 0.08, 0.10]);
            }
        }
        // The threat overlay [T]: ground watched by every demon the squad
        // knows about — the seen at their true posts, the lost at their
        // ghosts. Only painted where the squad itself can see.
        if self.show_threat {
            let mut watchers: Vec<IVec3> = self
                .battle
                .visible_enemies(Side::Order)
                .iter()
                .map(|&id| self.battle.unit(id).tile)
                .collect();
            for (&id, &tile) in &self.battle.last_known {
                let u = self.battle.unit(id);
                if u.is_active() && !visible.contains(&u.tile) {
                    watchers.push(tile);
                }
            }
            if !watchers.is_empty() {
                let watched = self.battle.tiles_seen_from(&watchers);
                for tile in watched.intersection(&visible) {
                    push_tile_quad(&mut verts, &mut indices, *tile, [0.9, 0.15, 0.1, 0.12]);
                }
            }
        }
        // Where the selected soldier could stand this turn.
        for (tile, _) in &self.reachable {
            push_tile_quad(&mut verts, &mut indices, *tile, [0.25, 0.8, 0.4, 0.10]);
        }
        if let Some((path, _)) = &self.hover_path {
            for tile in path {
                push_tile_quad(&mut verts, &mut indices, *tile, [0.3, 0.9, 1.0, 0.30]);
            }
        }
        for (tile, kind, _) in &self.battle.clouds {
            let color = match kind {
                ods_sim::battle::CloudKind::Smoke => [0.7, 0.7, 0.75, 0.45],
                ods_sim::battle::CloudKind::Fire => [1.0, 0.45, 0.1, 0.5],
            };
            push_tile_quad(&mut verts, &mut indices, *tile, color);
        }
        // Occult ground: summoning circles bleed red light, wards burn teal,
        // corruption veins glow violet (the voxel runes carry the detail).
        for (tile, _, _) in &self.battle.summons {
            push_tile_quad(&mut verts, &mut indices, *tile, [1.0, 0.1, 0.08, 0.22]);
        }
        for tile in &self.battle.wards {
            push_tile_quad(&mut verts, &mut indices, *tile, [0.1, 0.9, 0.8, 0.16]);
        }
        for tile in &self.battle.corruption {
            push_tile_quad(&mut verts, &mut indices, *tile, [0.6, 0.15, 0.8, 0.14]);
        }
        if let Some(id) = self.selected {
            let u = self.battle.unit(id);
            if u.alive {
                push_tile_quad(&mut verts, &mut indices, u.tile, [0.2, 1.0, 0.35, 0.35]);
                // The soldier's own cursor box, in the Order's gold.
                push_wire_box(&mut verts, &mut indices, u.tile, [0.95, 0.85, 0.3, 0.9]);
            }
        }
        // The hovered tile wears the classic wireframe cursor: red over
        // enemies, arming-orange with a charge out, white over open ground.
        if let Some(tile) = self.hover {
            let color = if self.grenade_armed {
                [1.0, 0.55, 0.1, 0.95]
            } else if self
                .battle
                .unit_at(tile)
                .is_some_and(|id| self.battle.unit(id).side == Side::Demons)
            {
                [1.0, 0.2, 0.15, 0.95]
            } else {
                [0.9, 0.9, 0.95, 0.8]
            };
            push_wire_box(&mut verts, &mut indices, tile, color);
        }
        renderer.set_overlay(&verts, &indices);
    }
}

/// The X-COM tile cursor: a wireframe box drawn as twelve thin ribbons
/// around the tile's standing volume.
fn push_wire_box(
    verts: &mut Vec<OverlayVertex>,
    indices: &mut Vec<u32>,
    tile: IVec3,
    color: [f32; 4],
) {
    let t = TILE_VOXELS as f32;
    let w = 0.5 * VS_F; // ribbon half-width
    let base = (tile * TILE_VOXELS).as_vec3()
        + Vec3::new(0.0, 0.0, ods_sim::scenario::GROUND_TOP as f32 + 0.15);
    let top = t * 0.75; // the standing volume, not the whole column
    let corners = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(t, 0.0, 0.0),
        Vec3::new(t, t, 0.0),
        Vec3::new(0.0, t, 0.0),
    ];
    let edge = |verts: &mut Vec<OverlayVertex>, indices: &mut Vec<u32>, a: Vec3, b: Vec3| {
        let dir = (b - a).normalize_or(Vec3::X);
        // A ribbon facing up for floor edges, sideways for verticals.
        let side = if dir.z.abs() > 0.5 { Vec3::X } else { Vec3::Z };
        let perp = dir.cross(side).normalize_or(Vec3::Y) * w;
        push_quad(verts, indices, [a - perp, a + perp, b + perp, b - perp], color);
    };
    for i in 0..4 {
        let (a, b) = (base + corners[i], base + corners[(i + 1) % 4]);
        edge(verts, indices, a, b); // floor square
        edge(
            verts,
            indices,
            base + corners[i] + Vec3::new(0.0, 0.0, top),
            base + corners[(i + 1) % 4] + Vec3::new(0.0, 0.0, top),
        ); // ceiling square
        edge(verts, indices, a, a + Vec3::new(0.0, 0.0, top)); // verticals
    }
}

/// A slim console gauge: background groove plus a colored fill fraction.
fn mini_bar(ui: &mut egui::Ui, frac: f32, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width().min(88.0), 4.0), egui::Sense::hover());
    let paint = ui.painter_at(rect);
    paint.rect_filled(rect, 1.0, egui::Color32::from_gray(38));
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    paint.rect_filled(fill, 1.0, color);
}

fn push_quad(
    verts: &mut Vec<OverlayVertex>,
    indices: &mut Vec<u32>,
    corners: [Vec3; 4],
    color: [f32; 4],
) {
    let first = verts.len() as u32;
    for c in corners {
        verts.push(OverlayVertex { position: c.to_array(), color });
    }
    indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
}

fn push_flat_square(
    verts: &mut Vec<OverlayVertex>,
    indices: &mut Vec<u32>,
    center: Vec3,
    r: f32,
    color: [f32; 4],
) {
    push_quad(
        verts,
        indices,
        [
            center + Vec3::new(-r, -r, 0.4),
            center + Vec3::new(r, -r, 0.4),
            center + Vec3::new(r, r, 0.4),
            center + Vec3::new(-r, r, 0.4),
        ],
        color,
    );
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
        Event::PartSevered { unit, part } => {
            format!("!!! {}'s {} is SEVERED", name(unit), part.name())
        }
        Event::Gibbed { unit } => {
            format!("!!! {} comes apart — nothing left to bury", name(unit))
        }
        Event::CorpseEaten { unit, corpse } => {
            format!("{} feeds on the body of {}", name(unit), name(corpse))
        }
        Event::Defiled { corpse } => {
            format!("!!! {} rises — the Taker's work", name(corpse))
        }
        Event::Infected { unit, part } => {
            format!("{}'s {} festers with demonic rot — amputate before it turns", name(unit), part.name())
        }
        Event::Amputated { medic, target, part } => {
            format!("{} saws {}'s {} off — the rot dies with it", name(medic), name(target), part.name())
        }
        Event::InfectionTurned { unit } => {
            format!("!!! the rot finishes its work: {} is one of THEM now", name(unit))
        }
        Event::SummoningScribed { at } => {
            format!("!!! a summoning circle scribes itself at {at} — foul it or face what comes")
        }
        Event::Summoned { unit } => {
            format!("!!! the circle delivers: {} steps through", name(unit))
        }
        Event::SummoningDisrupted { at } => {
            format!("the summoning at {at} is fouled — nothing comes through")
        }
        Event::WardInscribed { at } => {
            format!("a ward burns witchfire-bright at {at}")
        }
        Event::WardBurned { unit, at } => {
            format!("{} crosses the ward at {at} — and the ward answers", name(unit))
        }
        Event::CorruptionSpread { at } => {
            format!("the obelisk's veins reach {at}")
        }
        Event::Whispered { unit } => {
            format!("{} stands on corrupted ground... the ground knows their name", name(unit))
        }
        Event::Riposte { unit, target, hit } => format!(
            "{}'s blade answers {} — {}",
            name(unit),
            name(target),
            if *hit { "and BITES" } else { "and misses" }
        ),
        Event::CircletShattered { unit } => {
            format!("{}'s circlet takes the psi blow and SHATTERS", name(unit))
        }
        Event::Rallied { by } => {
            format!("{} rallies the line — every heart steadies", name(by))
        }
        Event::Evacuated { unit } => {
            format!("*** {} reaches the wagons and is AWAY ***", name(unit))
        }
        Event::TimeExpired => "!!! too late. The clock has taken the field".to_string(),
        Event::FloorCollapsed { at } => {
            format!("!!! the floor at {at} gives way and comes down")
        }
        Event::AtrocityFound { unit, at } => {
            format!("!!! {} finds what the demons left at {at}. Nobody should see this.", name(unit))
        }
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
        Event::Kneeled { unit, kneeling } => {
            if *kneeling {
                format!("{} kneels", name(unit))
            } else {
                format!("{} rises", name(unit))
            }
        }
        Event::Stunned { unit, stun } => {
            format!("{} reels from the binding rod (stun {stun})", name(unit))
        }
        Event::Subdued { unit } => format!("*** {} is subdued — bound where it lies ***", name(unit)),
        Event::Awakened { unit } => format!("{} shakes off the binding!", name(unit)),
        Event::Terrified { unit, target, morale_lost } => {
            if *morale_lost > 0 {
                format!("{} whispers into {}'s mind (-{morale_lost} morale)", name(unit), name(target))
            } else {
                format!("{} resists the whispering of {}", name(target), name(unit))
            }
        }
        Event::Taken { unit } => format!("!!! {} IS TAKEN — the body rises !!!", name(unit)),
        Event::Hatched { unit } => format!("!!! {} tears free of the husk !!!", name(unit)),
        Event::ObjectiveDestroyed => "THE OBELISK FALLS — the rift collapses!".to_string(),
        Event::PartCrippled { unit, part } => {
            format!("*** {}'s {} is crippled ***", name(unit), part.name())
        }
        Event::Turned { unit, .. } => format!("{} takes a new watch arc", name(unit)),
        Event::ChargeDropped { at, timer } => {
            format!("a primed charge drops at {at} — {timer} half-turns on the fuse")
        }
        Event::SmokePopped { at } => format!("smoke blooms at {at}"),
        Event::FireStarted { at } => format!("fire takes hold at {at}"),
        Event::Burned { unit, amount } => format!("{} burns ({amount})", name(unit)),
        Event::DoorOpened { at } => format!("a door swings open at {at}"),
        Event::Possessed { unit, by } => {
            format!("!!! {} SEIZES {}'s MIND !!!", name(by), name(unit))
        }
        Event::PossessionEnds { unit } => format!("{} is their own again", name(unit)),
        Event::WallSmashed { at, voxels } => {
            format!("!!! masonry EXPLODES inward at {at} ({voxels} voxels) !!!")
        }
        Event::Fell { unit, to } => format!("{} falls to {to}", name(unit)),
        Event::CarriedUp { unit, carried } => {
            format!("{} shoulders {}", name(unit), name(carried))
        }
        Event::SetDown { unit, carried } => {
            format!("{} lays {} down", name(unit), name(carried))
        }
        Event::Scavenged { unit } => format!("{} takes up a fallen weapon", name(unit)),
        Event::NoiseInDark { near } => {
            format!("something shrieks in the dark, near {near}...")
        }
        Event::BattleOver { winner } => format!("=== BATTLE OVER: {winner:?} wins ==="),
    }
}
