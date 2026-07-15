//! The interactive Battlescape: the 3D voxel view plus its egui HUD.

use glam::{IVec3, Mat4, Vec3};
use ods_geo::MissionToken;
use ods_render::{OrbitCamera, OverlayVertex, Renderer};
use ods_sim::battle::{Action, Battle, Event};
use ods_sim::units::{FireMode, Side, UnitId};
use ods_sim::{TILE_VOXELS, ai, scenario, voxel_to_tile};
use ods_voxel::{mesh_chunk_capped};
use winit::keyboard::KeyCode;

use std::collections::{HashMap, VecDeque};

use crate::audio::{Audio, Sound};

const VS_F: f32 = ods_sim::VS as f32;
const HALF_TILE: f32 = TILE_VOXELS as f32 / 2.0;
const PAN_STEP: f32 = 12.0 * VS_F;
/// Seconds a figure takes to cross one tile at anim speed 1.
const STEP_SECS: f32 = 0.22;
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
    /// A projectile in flight: races from -> to over its life, lofted by
    /// `arc` voxels at the apex, `width` across.
    Bolt { arc: f32, width: f32 },
    Blast,
    Flash,
    /// A chip of the world knocked loose: flies, falls, fades.
    Debris { vel: Vec3 },
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
    /// Walk phases and recoil per unit — the figures' pulse.
    anim: HashMap<u32, figures::AnimState>,
    /// The story queue: the sim resolves instantly, but events play out
    /// here one at a time, at a speed the eye can follow.
    playback: VecDeque<Event>,
    /// Seconds until the next queued event fires.
    playback_wait: f32,
    /// Tile-by-tile walking routes the figures still owe the eye.
    waypoints: HashMap<u32, VecDeque<IVec3>>,
    /// Cached figure carves, keyed by what each figure IS; invalidated
    /// by the shell key, so standing figures cost transforms, not carves.
    shells: HashMap<u32, (u64, figures::FigureShell)>,
    /// Spent brass, dropped magazines, fallen quarrels: tiny persistent
    /// marks that make a long firefight look fought.
    litter: Vec<(Vec3, [f32; 4])>,
    /// The brightest transient light on the field (muzzle, blast):
    /// position and dying intensity, fed to the renderer every frame.
    pub muzzle: (Vec3, f32),
    /// The last shot's line of fire: deaths topple along it.
    last_shot: Option<(Vec3, Vec3)>,
    /// Tile under the cursor, plus a cached move preview to it.
    hover: Option<IVec3>,
    hover_path: Option<(Vec<IVec3>, i32)>,
    reachable: Vec<(IVec3, i32)>,
    /// Big announcement text, its remaining seconds, and its full life.
    banner: Option<(String, f32, f32)>,
    /// Cutaway: hide everything above z=16 to see ground-floor interiors.
    floor_cap: bool,
    /// Tint the ground known demons can see ([T]).
    show_threat: bool,
    /// Watch cones: the ground each soldier's reaction arc covers [N].
    show_cones: bool,
    /// Colorblind-safe overlays: orange/blue instead of red/green.
    pub colorblind: bool,
    /// Combat text floats over the field.
    pub combat_text: bool,
    /// Damp screen flashes.
    pub reduce_flash: bool,
    /// The big tactical map ([M]).
    show_map: bool,
    /// The demon turn waits behind the HIDDEN MOVEMENT curtain.
    demon_turn_pending: bool,
    hidden_timer: f32,
    /// Battle pacing: scales the walk glide (set from the options screen).
    pub anim_speed: f32,
    /// Pan the camera to visible demon action during their turn.
    pub event_cam: bool,
    /// The mission briefing card; input holds until DEPLOY is pressed.
    pub briefing: Option<Vec<String>>,
    /// Seconds left of the entry sweep down off the gondola.
    intro: f32,
    intro_from: Vec3,
    intro_to: Vec3,
    /// Where the camera is easing (zoom, yaw, and any event to look at).
    zoom_target: f32,
    yaw_target: f32,
    look_target: Option<Vec3>,
    /// (fx_clock stamp, unit) of the last click, for double-click centering.
    last_click: (f32, Option<UnitId>),
    /// White impact flash, seconds remaining.
    flash: f32,
    /// The end-turn guard is asking about unspent TU.
    confirm_end: bool,
    /// Time scale: the end of a battle lands in slow motion.
    time_scale: f32,
    /// The field's standing soundscape, chosen once from the ground.
    pub ambient: crate::audio::Ambient,
    /// How hot the field is right now (0 quiet .. 1 open contact).
    pub contact: f32,
    pub cursor: (f32, f32),
    pub right_drag: bool,
    pub last_cursor: (f32, f32),
}

impl BattleScreen {
    pub fn new(renderer: &mut Renderer, battle: Battle, token: Option<MissionToken>) -> Self {
        let (min, max) = battle.tiles.bounds();
        let center = ((min + max).as_vec3() / 2.0) * TILE_VOXELS as f32;
        // The sweep: open over the gondola on the west edge, glide out.
        let intro_from = Vec3::new(
            (min.x as f32 + 2.5) * TILE_VOXELS as f32,
            center.y,
            0.0,
        );
        let intro_to = Vec3::new(center.x, center.y, 0.0);
        let base_distance = 420.0 * VS_F;
        let mut screen = Self {
            battle,
            token,
            camera: {
                let mut cam = OrbitCamera::isometric(intro_from);
                cam.distance = base_distance * 1.6;
                cam
            },
            event_cam: true,
            briefing: None,
            intro: 1.5,
            intro_from,
            intro_to,
            zoom_target: base_distance,
            yaw_target: ods_render::ISO_YAW,
            look_target: None,
            last_click: (0.0, None),
            flash: 0.0,
            confirm_end: false,
            time_scale: 1.0,
            ambient: crate::audio::Ambient::Temperate,
            contact: 0.0,
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
            anim: HashMap::new(),
            playback: VecDeque::new(),
            playback_wait: 0.0,
            waypoints: HashMap::new(),
            shells: HashMap::new(),
            litter: Vec::new(),
            muzzle: (Vec3::ZERO, 0.0),
            last_shot: None,
            hover: None,
            hover_path: None,
            reachable: Vec::new(),
            banner: Some(("THE SQUAD DEPLOYS".to_string(), 1.6, 1.6)),
            floor_cap: false,
            show_threat: false,
            show_cones: false,
            colorblind: false,
            combat_text: true,
            reduce_flash: false,
            show_map: false,
            demon_turn_pending: false,
            hidden_timer: 0.0,
            anim_speed: 1.0,
            cursor: (0.0, 0.0),
            right_drag: false,
            last_cursor: (0.0, 0.0),
        };
        screen.ambient = choose_ambient(&screen.battle);
        renderer.clear_scene();
        // The bedrock the field sits on: built once, never dirty.
        let (bmin, bmax) = screen.battle.tiles.bounds();
        let (skirt_verts, skirt_idx) = figures::build_skirt(bmin, bmax, 0xBEDD0C);
        renderer.set_skirt(&skirt_verts, &skirt_idx);
        screen.refresh_chunks(renderer);
        screen.refresh_scene(renderer);
        screen
    }

    // ------------------------------------------------------------------
    // Input from the window (only reaches us when egui didn't consume it)

    pub fn click(&mut self, renderer: &mut Renderer, audio: Option<&Audio>, width: f32, height: f32) {
        if self.briefing.is_some() {
            return; // the card holds the field
        }
        if self.intro > 0.0 {
            self.intro = 0.0; // a click skips the sweep
            self.camera.target = self.intro_to;
            return;
        }
        if self.battle.winner.is_some() || self.demon_turn_pending || !self.playback.is_empty() {
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
                    // A double-click swings the camera to them.
                    if self.last_click.1 == Some(id)
                        && self.fx_clock - self.last_click.0 < 0.35
                    {
                        self.look_target = Some(self.unit_pos(id, 0.0) * Vec3::new(1.0, 1.0, 0.0));
                    }
                    self.last_click = (self.fx_clock, Some(id));
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
        if self.briefing.is_some() {
            return; // the card holds the field
        }
        if self.intro > 0.0 {
            self.intro = 0.0;
            self.camera.target = self.intro_to;
        }
        if !self.playback.is_empty() {
            return; // the field is telling you what happened; watch
        }
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
            KeyCode::KeyC => {
                if let Some(id) = self.selected {
                    let result = self.battle.perform(Action::Reload { unit: id });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::KeyZ => {
                if let Some(id) = self.selected {
                    let me = self.battle.unit(id).tile;
                    let victim = self
                        .battle
                        .units
                        .iter()
                        .find(|v| {
                            v.alive
                                && !v.conscious
                                && v.side == Side::Demons
                                && (v.tile - me).abs().max_element() <= 1
                        })
                        .map(|v| v.id);
                    if let Some(target) = victim {
                        let result = self.battle.perform(Action::Execute { unit: id, target });
                        self.apply(renderer, audio, result);
                    } else {
                        self.log.push("nothing helpless within reach".to_string());
                    }
                }
            }
            KeyCode::KeyN => {
                self.show_cones = !self.show_cones;
                self.refresh_scene(renderer);
            }
            KeyCode::KeyI => {
                if let Some(id) = self.selected {
                    let result = self.battle.perform(Action::SwapWeapon { unit: id });
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
            KeyCode::KeyL => {
                // Hurl a witchfire flare: at the hovered tile if it's in
                // reach, else out ahead of the soldier's facing.
                if let Some(id) = self.selected {
                    let me = self.battle.unit(id).tile;
                    let at = match self.hover {
                        Some(h) if (h - me).abs().max_element() <= 10 => h,
                        _ => me + self.battle.unit(id).facing * 5,
                    };
                    let result = self.battle.perform(Action::ThrowFlare { unit: id, at });
                    self.apply(renderer, audio, result);
                }
            }
            KeyCode::KeyQ => self.yaw_target += std::f32::consts::FRAC_PI_2,
            KeyCode::KeyE => self.yaw_target -= std::f32::consts::FRAC_PI_2,
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
        self.yaw_target = self.camera.yaw;
    }

    /// Smooth zoom: the wheel moves the target, the camera eases after it.
    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom_target = (self.zoom_target * factor).clamp(60.0, 3200.0);
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
                // Flags the console bars can't show: wind and blood.
                if let Some(id) = self.selected {
                    let u = self.battle.unit(id);
                    if u.stamina <= 0 && !u.flies {
                        ui.colored_label(egui::Color32::from_rgb(230, 200, 90), "WINDED");
                    }
                    if u.wounds > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 90, 70),
                            format!("BLEEDING x{}", u.wounds),
                        );
                    }
                }
            });
        });


        // The war-room table map.
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

        // The HIDDEN MOVEMENT curtain: the classic black beat while the
        // Otherside does what it does where you can't see it.
        if self.demon_turn_pending {
            let screen = ctx.viewport_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("hidden-movement"),
            ));
            painter.rect_filled(
                screen,
                0.0,
                egui::Color32::from_rgba_unmultiplied(4, 2, 5, 235),
            );
            let center = screen.center();
            let pulse = 26.0 + 5.0 * (self.fx_clock * 3.0).sin();
            painter.circle_stroke(
                center + egui::vec2(0.0, -60.0),
                pulse,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(160, 20, 18)),
            );
            painter.circle_stroke(
                center + egui::vec2(0.0, -60.0),
                pulse * 0.55,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(110, 14, 12)),
            );
            crate::pixfont::draw_centered(
                &painter,
                center,
                5.0,
                egui::Color32::from_rgb(200, 170, 130),
                "HIDDEN MOVEMENT",
            );
            painter.text(
                center + egui::vec2(0.0, 34.0),
                egui::Align2::CENTER_CENTER,
                "the Otherside stirs where no one is watching",
                egui::FontId::proportional(13.0),
                egui::Color32::from_rgb(120, 100, 90),
            );
        }

        // The impact flash: one white blink on the big detonations.
        if self.flash > 0.0 {
            let a = (self.flash / 0.16 * 70.0).min(70.0) as u8;
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("impact-flash"),
            ));
            painter.rect_filled(
                ctx.viewport_rect(),
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 245, 225, a),
            );
        }

        if let Some((text, ttl, total)) = &self.banner {
            // Slide in hard, settle, fade out — in the pixel banner face.
            let alpha = (ttl.min(0.6) / 0.6 * 255.0) as u8;
            let slide = (1.0 - ((total - ttl) / 0.18).min(1.0)) * 26.0;
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("banner"),
            ));
            crate::pixfont::draw_centered(
                &painter,
                egui::pos2(ctx.viewport_rect().center().x, 140.0 - slide),
                4.0,
                egui::Color32::from_rgba_unmultiplied(255, 225, 170, alpha),
                text,
            );
        }

        // The console: the whole squad at a glance along the very bottom,
        // the way the 1994 strip did it — vitals as bars, click to select.
        self.console(ctx, renderer, audio);

        // The briefing card: everything known, one DEPLOY button.
        if let Some(lines) = self.briefing.clone() {
            let mut deploy = false;
            egui::Window::new("MISSION BRIEFING")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, -20.0])
                .show(ctx, |ui| {
                    ui.set_min_width(340.0);
                    for line in &lines {
                        if let Some(rest) = line.strip_prefix("! ") {
                            ui.colored_label(egui::Color32::from_rgb(230, 170, 90), rest);
                        } else {
                            ui.label(line);
                        }
                    }
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        if ui
                            .button(egui::RichText::new("  D E P L O Y  ").size(18.0).strong())
                            .clicked()
                        {
                            deploy = true;
                        }
                    });
                });
            if deploy {
                self.briefing = None;
                if let Some(a) = audio {
                    a.play(Sound::Deploy);
                }
            }
        }

        // The end-turn guard's question.
        if self.confirm_end {
            // The checklist: what the turn would leave on the table.
            let mut items: Vec<(String, UnitId)> = Vec::new();
            for u in &self.battle.units {
                if !u.is_active() || u.side != Side::Order || u.civilian {
                    continue;
                }
                if u.tu * 2 > u.tu_max {
                    items.push((format!("{} holds {} TU", u.name, u.tu), u.id));
                }
                if u.wounds > 0 {
                    items.push((format!("{} is BLEEDING", u.name), u.id));
                }
                if u.reserve.is_none()
                    && u.fire_cost(FireMode::Snap).is_some_and(|c| u.tu >= c)
                {
                    items.push((format!("{} has no watch banked", u.name), u.id));
                }
            }
            let mut end = false;
            let mut stay = false;
            let mut jump: Option<UnitId> = None;
            egui::Window::new("End the turn?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 40.0])
                .show(ctx, |ui| {
                    if items.is_empty() {
                        ui.label("Nothing left on the table.");
                    } else {
                        ui.label("Left on the table (click to go to them):");
                        for (line, id) in items.iter().take(8) {
                            if ui
                                .add(
                                    egui::Label::new(egui::RichText::new(line).small())
                                        .sense(egui::Sense::click()),
                                )
                                .clicked()
                            {
                                jump = Some(*id);
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("End the turn").clicked() {
                            end = true;
                        }
                        if ui.button("Stand fast").clicked() {
                            stay = true;
                        }
                    });
                });
            if let Some(id) = jump {
                self.selected = Some(id);
                self.confirm_end = false;
                self.refresh_scene(renderer);
            } else if end {
                self.end_turn(renderer, audio);
            } else if stay {
                self.confirm_end = false;
            }
        }

        if let Some(winner) = self.battle.winner
            && self.playback.is_empty()
        {
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
        if self.battle.winner.is_some() || self.demon_turn_pending || !self.playback.is_empty() {
            return;
        }
        // The guard: half the squad's legs still under them? Ask once.
        if !self.confirm_end {
            let idle = self
                .battle
                .units
                .iter()
                .filter(|u| {
                    u.is_active()
                        && u.side == Side::Order
                        && !u.civilian
                        && u.tu * 2 > u.tu_max
                })
                .count();
            if idle >= 2 {
                self.confirm_end = true;
                return;
            }
        }
        self.confirm_end = false;
        let fled = ai::run_civilian_moves(&mut self.battle);
        self.consume(renderer, audio, &fled);
        match self.battle.perform(Action::EndTurn) {
            Ok(events) => self.consume(renderer, audio, &events),
            Err(e) => {
                self.log.push(format!("cannot end turn: {e:?}"));
                return;
            }
        }
        // The demon turn waits behind the curtain: the HIDDEN MOVEMENT
        // interstitial holds the screen for a beat before it resolves.
        if self.battle.winner.is_none() {
            self.demon_turn_pending = true;
            self.hidden_timer = 1.3;
        }
    }

    fn apply(
        &mut self,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        result: Result<Vec<Event>, ods_sim::battle::ActionError>,
    ) {
        match result {
            Ok(events) => self.consume(renderer, audio, &events),
            Err(e) => {
                if let Some(a) = audio {
                    a.play(Sound::Error);
                }
                self.log.push(format!("cannot: {e:?}"));
            }
        }
    }

    fn consume(&mut self, renderer: &mut Renderer, _audio: Option<&Audio>, events: &[Event]) {
        // Nothing plays here: events join the story queue and fire one at
        // a time from `pump_playback`, at a pace the eye can follow.
        self.playback.extend(events.iter().cloned());
        self.refresh_chunks(renderer);
        self.refresh_scene(renderer);
    }

    /// One event reaches the screen: log line, effects, camera glance.
    fn play_one(&mut self, audio: Option<&Audio>, e: &Event) {
        self.log.push(describe(e, &self.battle));
        self.spawn_fx(e, audio);
        // Deaths and knockouts topple the figure only once they're SEEN.
        match e {
            Event::Fired { unit, target, .. } => {
                self.last_shot =
                    Some((self.unit_pos(*unit, 9.0), self.unit_pos(*target, 9.0)));
            }
            Event::Died { unit } | Event::Gibbed { unit } => {
                self.anim.entry(unit.0).or_default().fall_goal = 1.0;
                // The body goes WITH the shot: chest to the shooter, a
                // shove along the line, and blood where it comes to rest.
                let vpos = self.unit_pos(*unit, 0.0);
                if let Some((from, _)) = self.last_shot {
                    let dir = (vpos - from) * Vec3::new(1.0, 1.0, 0.0);
                    if dir.length_squared() > 0.5 {
                        let d = dir.normalize();
                        let toward = -d;
                        self.anim.entry(unit.0).or_default().yaw =
                            toward.y.atan2(toward.x);
                        if let Some(v) = self.visual.get_mut(&unit.0) {
                            *v += d * 5.0;
                        }
                    }
                }
                for k in 0..3 {
                    let off = Vec3::new(
                        ((unit.0 * 31 + k * 17) % 13) as f32 - 6.0,
                        ((unit.0 * 47 + k * 29) % 13) as f32 - 6.0,
                        0.0,
                    );
                    self.push_litter(
                        vpos + off + Vec3::Z * 0.4,
                        [0.34, 0.04, 0.04, 0.9],
                    );
                }
            }
            Event::TurnStarted { .. } => {
                // Pools spread under the fallen while they lie there.
                let spots: Vec<Vec3> = self
                    .battle
                    .units
                    .iter()
                    .filter(|u| !u.alive && u.is_corpse())
                    .map(|u| Self::tile_feet(u.tile))
                    .collect();
                for (i, p) in spots.into_iter().enumerate() {
                    let off = Vec3::new(
                        ((self.battle.turn as usize * 7 + i * 13) % 15) as f32 - 7.0,
                        ((self.battle.turn as usize * 11 + i * 5) % 15) as f32 - 7.0,
                        0.4,
                    );
                    self.push_litter(p + off, [0.30, 0.03, 0.03, 0.85]);
                }
            }
            Event::PartSevered { unit, part } => {
                // The part comes OFF: a chunk in its own colors tumbles
                // and stays where it lands.
                let species = self.battle.unit(*unit).species;
                let color = figures::blueprint(species)
                    .iter()
                    .find(|b| b.part == *part)
                    .map(|b| b.color)
                    .unwrap_or([0.4, 0.1, 0.1, 1.0]);
                let p = self.unit_pos(*unit, 8.0);
                self.spawn_debris(p, color, 6);
                let ground = self.unit_pos(*unit, 0.4);
                self.push_litter(ground + Vec3::new(4.0, 2.0, 0.0), color);
                self.push_litter(ground + Vec3::new(-3.0, 5.0, 0.0), [0.34, 0.04, 0.04, 0.9]);
            }
            Event::Exploded { at, .. } | Event::WallSmashed { at, .. } => {
                // Scorch rings and debris that remembers what it was.
                let p = Self::tile_pos(*at, 4.0);
                let probe = *at * TILE_VOXELS
                    + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, scenario::GROUND_TOP + 4);
                let mat = mat_color(self.battle.world.voxel(probe));
                self.spawn_debris(p + Vec3::Z * 8.0, mat, 10);
                if matches!(e, Event::Exploded { .. }) {
                    for k in 0..8 {
                        let a = k as f32 * std::f32::consts::TAU / 8.0;
                        let r = 8.0 + (k % 3) as f32 * 3.0;
                        self.push_litter(
                            p * Vec3::new(1.0, 1.0, 0.0)
                                + Vec3::new(a.cos() * r, a.sin() * r, scenario::GROUND_TOP as f32 + 0.4),
                            [0.09, 0.07, 0.06, 0.9],
                        );
                    }
                }
            }
            Event::Subdued { unit } => {
                self.anim.entry(unit.0).or_default().fall_goal = 0.92;
            }
            Event::Awakened { unit }
            | Event::Taken { unit }
            | Event::Hatched { unit }
            | Event::InfectionTurned { unit } => {
                self.anim.entry(unit.0).or_default().fall_goal = 0.0;
            }
            _ => {}
        }
        // The camera directs: reaction fire snaps to the ambusher first;
        // demon fire and deliberate shots frame shooter and target both.
        if self.event_cam
            && let Event::Fired { unit, target, reaction, mode, .. } = e
        {
            let flat = Vec3::new(1.0, 1.0, 0.0);
            if *reaction {
                self.look_target = Some(self.unit_pos(*unit, 0.0) * flat);
            } else if self.battle.unit(*unit).side == Side::Demons
                || *mode != FireMode::Snap
            {
                let mid = (self.unit_pos(*unit, 0.0) + self.unit_pos(*target, 0.0)) / 2.0;
                self.look_target = Some(mid * flat);
            }
        }
        // A kill lands in a breath of slow motion.
        if matches!(e, Event::Died { .. } | Event::Gibbed { .. }) && self.time_scale >= 1.0 {
            self.time_scale = 0.45;
        }
        // Reloads and swaps read on the figure: the weapon dips to the belt.
        if let Event::Reloaded { unit } | Event::Swapped { unit } | Event::Scavenged { unit } = e
        {
            self.anim.entry(unit.0).or_default().reload = 0.55;
            let ground = self.unit_pos(*unit, 0.4);
            self.push_litter(ground, [0.22, 0.22, 0.25, 1.0]);
        }
    }

    /// How long the screen should dwell on an event before the next.
    /// Action nobody can see passes in a blink.
    fn dwell(&mut self, e: &Event, visible: &std::collections::HashSet<IVec3>) -> f32 {
        let seen = |id: &UnitId| {
            let u = self.battle.unit(*id);
            u.side == Side::Order || visible.contains(&u.tile)
        };
        match e {
            Event::Moved { unit, from, to, .. } => {
                let u = self.battle.unit(*unit);
                if u.side == Side::Order || visible.contains(to) || visible.contains(from) {
                    self.waypoints.entry(unit.0).or_default().push_back(*to);
                    STEP_SECS / self.anim_speed.max(0.1)
                } else {
                    // Unseen strides happen between blinks.
                    self.visual.insert(unit.0, Self::tile_feet(*to));
                    self.waypoints.remove(&unit.0);
                    0.0
                }
            }
            Event::Fired { unit, target, .. } => {
                if seen(unit) || seen(target) {
                    // Wait for the bolt: consequences land when it does.
                    let d = self.unit_pos(*unit, 9.0).distance(self.unit_pos(*target, 9.0));
                    let (speed, ..) = projectile(&self.battle.unit(*unit).weapon.key);
                    (d / speed + 0.3) / self.anim_speed.max(0.1)
                } else {
                    0.05
                }
            }
            Event::Threw { .. }
            | Event::Exploded { .. }
            | Event::WallSmashed { .. }
            | Event::Terrified { .. } => 0.5 / self.anim_speed.max(0.1),
            Event::Died { unit } | Event::Gibbed { unit } | Event::Subdued { unit } => {
                if seen(unit) { 0.55 / self.anim_speed.max(0.1) } else { 0.05 }
            }
            Event::Executed { .. }
            | Event::Riposte { .. }
            | Event::Panicked { .. }
            | Event::Berserked { .. } => 0.4 / self.anim_speed.max(0.1),
            Event::TurnStarted { .. } => 0.25,
            _ => 0.03,
        }
    }

    // ------------------------------------------------------------------
    // Effects

    /// Where a unit's feet stand on a tile.
    fn tile_feet(tile: IVec3) -> Vec3 {
        (tile * TILE_VOXELS).as_vec3()
            + Vec3::new(HALF_TILE, HALF_TILE, scenario::GROUND_TOP as f32)
    }

    /// Where the unit IS on screen right now — mid-stride included — so
    /// effects rise from the figure, not from where the sim already put it.
    fn unit_pos(&self, id: UnitId, z: f32) -> Vec3 {
        let feet = self
            .visual
            .get(&id.0)
            .copied()
            .unwrap_or_else(|| Self::tile_feet(self.battle.unit(id).tile));
        Vec3::new(feet.x, feet.y, z * VS_F)
    }

    fn tile_pos(at: IVec3, z: f32) -> Vec3 {
        (at * TILE_VOXELS).as_vec3() + Vec3::new(HALF_TILE, HALF_TILE, z * VS_F)
    }

    /// Fights on the night side of the world are lit by muzzle and flame.
    /// Drop a permanent speck on the field (brass, a spent magazine).
    fn push_litter(&mut self, at: Vec3, color: [f32; 4]) {
        if self.litter.len() >= 320 {
            self.litter.remove(0);
        }
        self.litter.push((at, color));
    }

    fn is_night(&self) -> bool {
        self.battle.vision_tiles < 14
    }

    /// Position a sound in the player's ears: louder near the camera's
    /// focus, panned by which side of the view the source sits on.
    fn emit(&self, audio: Option<&Audio>, sound: Sound, at: Vec3) {
        let Some(a) = audio else { return };
        let rel = at - self.camera.target;
        let right = Vec3::new(self.camera.yaw.sin(), -self.camera.yaw.cos(), 0.0);
        let pan = (rel.dot(right) / (40.0 * TILE_VOXELS as f32)).clamp(-1.0, 1.0);
        let dist_tiles = rel.length() / TILE_VOXELS as f32;
        let gain = (1.1 - dist_tiles / 26.0).clamp(0.2, 1.0);
        a.play_at(sound, gain, pan);
    }

    /// Knock a handful of material chips loose: they fly, arc, and die.
    fn spawn_debris(&mut self, at: Vec3, color: [f32; 4], count: usize) {
        for i in 0..count {
            let a = i as f32 * 2.399; // golden-angle scatter
            let speed = (12.0 + 9.0 * ((i * 7919) % 7) as f32 / 7.0) * VS_F;
            let up = (26.0 + 18.0 * ((i * 104729) % 13) as f32 / 13.0) * VS_F;
            self.fx.push(Fx {
                kind: FxKind::Debris {
                    vel: Vec3::new(a.cos() * speed, a.sin() * speed, up),
                },
                from: at,
                to: at,
                color,
                age: 0.0,
                life: 0.9,
            });
        }
    }

    fn float(&mut self, over: UnitId, text: impl Into<String>, color: egui::Color32) {
        if !self.combat_text {
            return;
        }
        let mut world = self.unit_pos(over, 20.0);
        // Stack, don't overlap: each floater sharing the spot rides higher.
        let crowd = self
            .floaters
            .iter()
            .filter(|f| (f.world - world).truncate().length() < TILE_VOXELS as f32 * 1.5)
            .count();
        world.z += crowd as f32 * 5.5 * VS_F;
        self.floaters.push(FloatText {
            text: text.into(),
            color,
            world,
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
            Event::Damaged { unit, amount: 0, .. } => {
                self.float(*unit, "CLINK", egui::Color32::from_gray(200));
            }
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
                // The turn has a voice: a ready click for ours, a low
                // drum for theirs.
                play(match side {
                    Side::Order => Sound::Click,
                    Side::Demons => Sound::PauseDrum,
                });
                self.banner = Some((text.to_string(), 1.1, 1.1));
            }
            Event::BattleOver { winner } => {
                // The camera steps back to take in the whole field.
                let (bmin, bmax) = self.battle.tiles.bounds();
                let center = ((bmin + bmax).as_vec3() / 2.0) * TILE_VOXELS as f32;
                self.look_target = Some(Vec3::new(center.x, center.y, 0.0));
                self.zoom_target = (self.zoom_target * 1.35).min(3200.0);
                self.time_scale = 0.3;
                let (text, sound) = match winner {
                    Side::Order => ("THE FIELD IS OURS", Sound::Victory),
                    Side::Demons => ("THE LINE BREAKS", Sound::Defeat),
                };
                self.banner = Some((text.to_string(), 3.0, 3.0));
                play(sound);
            }
            Event::Fired { unit, target, hit, .. } => {
                // The shooter takes the kick, weighted by what they fired.
                self.anim.entry(unit.0).or_default().recoil = 0.14;
                let power = self.battle.unit(*unit).weapon.power;
                self.shake += (power as f32 / 40.0).min(1.2);
                let key = self.battle.unit(*unit).weapon.key.clone();
                let (speed, arc_frac, mut color, width) = projectile(&key);
                if !hit {
                    color[3] *= 0.8; // a miss burns a little dimmer
                }
                let from = self.unit_pos(*unit, 13.0);
                let mut to = self.unit_pos(*target, 9.0);
                if !hit {
                    // A miss doesn't vanish: it flies past and strikes
                    // whatever the world puts in its way.
                    let dir = (to - from).normalize_or(Vec3::X);
                    let past = to + dir * 2.0;
                    to = match self.battle.world.raycast(past, dir, 220.0) {
                        Some(h) => h.position,
                        None => to + dir * 90.0,
                    };
                }
                let dist = from.distance(to).max(1.0);
                // The bolt takes real time to arrive; the playback queue
                // holds the consequences until it does.
                self.fx.push(Fx {
                    kind: FxKind::Bolt { arc: dist * arc_frac, width },
                    from,
                    to,
                    color,
                    age: 0.0,
                    life: dist / speed,
                });
                if !hit {
                    let life = dist / speed;
                    self.fx.push(Fx {
                        kind: FxKind::Flash,
                        from: to,
                        to,
                        color: [0.7, 0.65, 0.55, 0.5],
                        age: -life,
                        life: 0.25,
                    });
                    self.push_litter(to, [0.1, 0.09, 0.08, 0.8]);
                }
                // The muzzle answers with light and a curl of smoke.
                let p = self.unit_pos(*unit, 10.0);
                self.muzzle = (p, if self.is_night() { 1.0 } else { 0.45 });
                self.fx.push(Fx {
                    kind: FxKind::Flash,
                    from: p,
                    to: p,
                    color: [1.0, 0.75, 0.35, if self.is_night() { 0.5 } else { 0.28 }],
                    age: 0.0,
                    life: if self.is_night() { 0.16 } else { 0.10 },
                });
                for k in 0..3 {
                    let drift = Vec3::new(
                        (self.fx_clock * 17.3 + k as f32).sin() * 3.0,
                        (self.fx_clock * 11.7 + k as f32).cos() * 3.0,
                        8.0 + k as f32 * 3.0,
                    );
                    self.fx.push(Fx {
                        kind: FxKind::Debris { vel: drift },
                        from: p,
                        to: p,
                        color: [0.55, 0.55, 0.55, 0.35],
                        age: 0.0,
                        life: 0.9,
                    });
                }
                // Brass in the grass.
                let scatter = Vec3::new(
                    (self.fx_clock * 23.9).fract() * 8.0 - 4.0,
                    (self.fx_clock * 31.7).fract() * 8.0 - 4.0,
                    0.0,
                );
                let ground = self.unit_pos(*unit, 0.4) + scatter;
                self.push_litter(ground, [0.75, 0.62, 0.28, 1.0]);
                self.emit(audio, Sound::Shot, p);
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
                self.spawn_debris(p + Vec3::Z * 4.0 * VS_F, [0.55, 0.42, 0.28, 0.9], 8);
                self.shake += 5.0;
                if !self.reduce_flash {
                    self.flash = 0.16;
                }
                self.emit(audio, Sound::Blast, p);
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
                self.spawn_debris(*center, [0.45, 0.40, 0.34, 0.85], 5);
            }
            Event::WallSmashed { at, .. } => {
                let p = Self::tile_pos(*at, 12.0);
                self.spawn_debris(p, [0.40, 0.24, 0.18, 0.9], 9);
                self.shake += 3.0;
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
                self.emit(audio, Sound::Death, p);
                let fallen = self.battle.unit(*unit);
                if fallen.side == Side::Order && !fallen.civilian {
                    play(Sound::Mourning);
                }
            }
            // Boots tell the ground they walk on: earth, planking, snow.
            Event::Moved { to, .. } => {
                let probe = *to * TILE_VOXELS
                    + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, scenario::GROUND_TOP - 1);
                let sound = match self.battle.world.voxel(probe) {
                    v if v == scenario::MAT_SNOW || v == scenario::MAT_GLINT => Sound::Crunch,
                    v if v == scenario::MAT_TIMBER => Sound::Knock,
                    _ => Sound::Footstep,
                };
                self.emit(audio, sound, Self::tile_pos(*to, 4.0));
            }
            // The dark answers itself, panned to where it really is.
            Event::NoiseInDark { near } => {
                self.emit(audio, Sound::DemonCall, Self::tile_pos(*near, 10.0));
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
                // The pieces are recognizably THEIRS: debris in the body's
                // own part colors, and gore that stays.
                let bp = figures::blueprint(self.battle.unit(*unit).species);
                for (k, b) in bp.iter().step_by(3).take(4).enumerate() {
                    self.spawn_debris(p + Vec3::Z * (4 + k as i32) as f32, b.color, 4);
                }
                let ground = self.unit_pos(*unit, 0.4);
                for k in 0..5 {
                    let off = Vec3::new(
                        ((unit.0 * 13 + k * 37) % 17) as f32 - 8.0,
                        ((unit.0 * 19 + k * 23) % 17) as f32 - 8.0,
                        0.0,
                    );
                    self.push_litter(ground + off, [0.40, 0.05, 0.06, 0.9]);
                }
                self.spawn_debris(p + Vec3::Z * 6.0 * VS_F, [0.55, 0.05, 0.05, 0.95], 9);
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
                    color: if self.colorblind {
                        [1.0, 0.55, 0.05, 0.7]
                    } else {
                        [1.0, 0.15, 0.1, 0.7]
                    },
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
        // The end of a battle lands in slow motion, then time recovers.
        if self.time_scale < 1.0 {
            self.time_scale = (self.time_scale + dt * 0.4).min(1.0);
        }
        let dt = dt * self.time_scale;

        // The entry sweep: hold on the gondola, then glide out over the
        // field (the briefing card pauses it; a click skips it).
        if self.briefing.is_none() && self.intro > 0.0 {
            self.intro = (self.intro - dt).max(0.0);
            let t = 1.0 - self.intro / 1.5;
            let s = t * t * (3.0 - 2.0 * t);
            self.camera.target = self.intro_from.lerp(self.intro_to, s);
            self.camera.distance = self.zoom_target * (1.6 - 0.6 * s);
        }
        // Camera easing: zoom, quarter-turns, and event glances all lerp.
        if self.intro <= 0.0 {
            self.camera.distance +=
                (self.zoom_target - self.camera.distance) * (dt * 10.0).min(1.0);
        }
        let dyaw = self.yaw_target - self.camera.yaw;
        if dyaw.abs() > 0.0005 {
            self.camera.yaw += dyaw * (dt * 9.0).min(1.0);
        }
        if let Some(look) = self.look_target {
            let d = (look - self.camera.target) * Vec3::new(1.0, 1.0, 0.0);
            if d.length() < 2.0 {
                self.look_target = None;
            } else {
                self.camera.target += d * (dt * 5.0).min(1.0);
            }
        }
        // Edge scrolling: the cursor against the window rim pans the field.
        if self.intro <= 0.0 && self.briefing.is_none() {
            let m = 10.0;
            let step = 420.0 * VS_F * dt;
            if self.cursor.0 > 0.0 && self.cursor.0 < m {
                self.camera.pan(-step, 0.0);
            } else if self.cursor.0 > width - m && self.cursor.0 < width {
                self.camera.pan(step, 0.0);
            }
            if self.cursor.1 > 0.0 && self.cursor.1 < m {
                self.camera.pan(0.0, step);
            } else if self.cursor.1 > height - m && self.cursor.1 < height {
                self.camera.pan(0.0, -step);
            }
        }

        // The curtain lifts: the demons take their turn behind it.
        if self.demon_turn_pending {
            self.hidden_timer -= dt;
            if self.hidden_timer <= 0.0 {
                self.demon_turn_pending = false;
                let events = ai::run_demon_turn(&mut self.battle);
                self.consume(renderer, audio, &events);
            }
        }

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

        // The story queue: events fire one at a time, each waiting out
        // its dwell, so the turn reads as a sequence instead of a blink.
        let visible = self.battle.visible_tiles(Side::Order);
        if !self.playback.is_empty() {
            self.playback_wait -= dt;
            while self.playback_wait <= 0.0 {
                let Some(e) = self.playback.pop_front() else { break };
                self.playback_wait += self.dwell(&e, &visible);
                self.play_one(audio, &e);
            }
            if self.playback.is_empty() {
                self.playback_wait = 0.0;
            }
        }

        // The glide: figures walk their owed waypoints at boots-on-ground
        // speed, turn to face the way they're going, and topple when a
        // seen death says so.
        for u in &self.battle.units {
            let resting = Self::tile_feet(u.tile);
            let target = self
                .waypoints
                .get(&u.id.0)
                .and_then(|q| q.front().copied())
                .map(Self::tile_feet)
                .unwrap_or(resting);
            let entry = self.visual.entry(u.id.0).or_insert(resting);
            let state = self.anim.entry(u.id.0).or_insert_with(|| figures::AnimState {
                yaw: figures::facing_angle(u),
                fall: figures::fall_target(u),
                fall_goal: figures::fall_target(u),
                ..Default::default()
            });
            state.breath = self.fx_clock;
            // Pose eases toward what the state calls for: kneels sink
            // rather than pop.
            let want = figures::pose_target(u);
            if state.pose <= 0.0 {
                state.pose = want;
            } else {
                let rate = if want < state.pose { 6.0 } else { 9.0 };
                state.pose += (want - state.pose) * (dt * rate).min(1.0);
            }
            // The topple runs on its own clock: fast enough to hit, slow
            // enough to watch.
            state.fall += (state.fall_goal - state.fall) * (dt * 5.0).min(1.0);
            if state.recoil > 0.0 {
                state.recoil = (state.recoil - dt).max(0.0);
            }
            if state.reload > 0.0 {
                state.reload = (state.reload - dt).max(0.0);
            }
            let delta = target - *entry;
            let dist = delta.length();
            let mut heading = None;
            if dist > 0.01 {
                // Constant stride, not an exponential snap.
                let speed = TILE_VOXELS as f32 / STEP_SECS * self.anim_speed.max(0.1);
                let step = speed * dt;
                if step >= dist {
                    *entry = target;
                    if let Some(q) = self.waypoints.get_mut(&u.id.0) {
                        q.pop_front();
                        if q.is_empty() {
                            self.waypoints.remove(&u.id.0);
                        }
                    }
                } else {
                    *entry += delta / dist * step;
                }
                state.walk += dt * 11.0 * self.anim_speed;
                heading = Some(delta.truncate());
            } else if state.walk != 0.0 {
                state.walk = 0.0;
            }
            // Face the way you walk; at rest, the way the sim says.
            let desired = heading
                .filter(|h| h.length_squared() > 0.001)
                .map(|h| h.y.atan2(h.x))
                .unwrap_or_else(|| figures::facing_angle(u));
            let mut diff = desired - state.yaw;
            while diff > std::f32::consts::PI {
                diff -= std::f32::consts::TAU;
            }
            while diff < -std::f32::consts::PI {
                diff += std::f32::consts::TAU;
            }
            state.yaw += diff * (dt * 10.0).min(1.0);
        }
        {
            let (fig_verts, fig_indices) =
                figures::build_figures(&self.battle, &visible, &self.visual, &self.anim, &mut self.shells);
            renderer.set_figures(&fig_verts, &fig_indices);
        }

        // The muzzle light dies fast; blasts die slower by burning hotter.
        self.muzzle.1 = (self.muzzle.1 - dt * 5.0).max(0.0);

        if let Some((_, ttl, _)) = &mut self.banner {
            *ttl -= dt;
            if *ttl <= 0.0 {
                self.banner = None;
            }
        }

        for f in &mut self.floaters {
            f.age += dt;
        }
        self.floaters.retain(|f| f.age < f.life);

        // When the squad runs low on blood you hear your own — and the
        // first crossing gets a warning all its own.
        if self.battle.winner.is_none() && self.squad_vitality() < 0.4 {
            if self.heart_timer == 0.0
                && let Some(a) = audio
            {
                a.play(Sound::Dread);
            }
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
        self.flash = (self.flash - dt).max(0.0);
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
        // The permanent record first: brass, magazines, quarrels.
        for (p, c) in &self.litter {
            push_flat_square(&mut verts, &mut indices, *p, 0.6 * VS_F, *c);
        }
        for fx in &self.fx {
            if fx.age < 0.0 {
                continue; // scheduled for when the bolt lands
            }
            let t = (fx.age / fx.life).clamp(0.0, 1.0);
            let fade = 1.0 - t;
            let mut color = fx.color;
            color[3] *= fade;
            match fx.kind {
                FxKind::Bolt { arc, width } => {
                    // The projectile: a hot head with a fading tail, lofted
                    // on a lobbed arc where the weapon calls for one.
                    let lift = |q: f32| -> Vec3 {
                        Vec3::Z * (arc * (q * std::f32::consts::PI).sin())
                    };
                    let head = fx.from.lerp(fx.to, t) + lift(t);
                    let tail_t = (t - 0.12).max(0.0);
                    let tail = fx.from.lerp(fx.to, tail_t) + lift(tail_t);
                    let dir = fx.to - fx.from;
                    let perp = dir.cross(Vec3::Z).normalize_or(Vec3::X) * (width * VS_F);
                    let mut hot = fx.color;
                    hot[3] = fx.color[3]; // the bolt does not fade mid-flight
                    push_quad(
                        &mut verts,
                        &mut indices,
                        [tail - perp, tail + perp, head + perp, head - perp],
                        hot,
                    );
                }
                FxKind::Blast => {
                    let r = (3.0 + 26.0 * t) * VS_F;
                    push_flat_square(&mut verts, &mut indices, fx.from, r, color);
                }
                FxKind::Flash => {
                    push_flat_square(&mut verts, &mut indices, fx.from, 6.0 * VS_F, color);
                }
                FxKind::Debris { vel } => {
                    // Ballistics on a chip of the world.
                    let g = -140.0 * VS_F;
                    let p = fx.from + vel * fx.age + Vec3::Z * (0.5 * g * fx.age * fx.age);
                    if p.z > 0.5 {
                        push_flat_square(&mut verts, &mut indices, p, 1.1 * VS_F, color);
                    }
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

        // Blob shadows: every standing figure claims its patch of ground.
        {
            let visible = self.battle.visible_tiles(Side::Order);
            for u in &self.battle.units {
                if !u.is_active() {
                    continue;
                }
                if u.side == Side::Demons && !visible.contains(&u.tile) {
                    continue;
                }
                let feet = self.visual.get(&u.id.0).copied().unwrap_or_else(|| {
                    (u.tile * TILE_VOXELS).as_vec3()
                        + Vec3::new(HALF_TILE, HALF_TILE, scenario::GROUND_TOP as f32)
                });
                let big = matches!(
                    u.species,
                    ods_sim::units::Species::Behemoth | ods_sim::units::Species::Prince
                );
                let r = if big { 8.0 } else { 5.0 } * VS_F;
                push_flat_square(
                    &mut verts,
                    &mut indices,
                    Vec3::new(feet.x, feet.y, feet.z + 0.2),
                    r,
                    [0.0, 0.0, 0.0, 0.30],
                );
            }
        }

        // The selection ring: a slow-turning dashed circle at the chosen
        // soldier's feet, breathing on the clock.
        if let Some(id) = self.selected {
            let u = self.battle.unit(id);
            if u.is_active() {
                let c = (u.tile * TILE_VOXELS).as_vec3()
                    + Vec3::new(HALF_TILE, HALF_TILE, scenario::GROUND_TOP as f32 + 0.6);
                let r = (7.0 + 0.5 * (self.fx_clock * 3.0).sin()) * VS_F;
                for k in 0..8 {
                    let a0 = self.fx_clock * 0.9 + k as f32 * std::f32::consts::TAU / 8.0;
                    let a1 = a0 + 0.42;
                    let (p0, p1) = (
                        c + Vec3::new(a0.cos(), a0.sin(), 0.0) * r,
                        c + Vec3::new(a1.cos(), a1.sin(), 0.0) * r,
                    );
                    let perp = (p1 - p0).cross(Vec3::Z).normalize_or(Vec3::X) * (0.6 * VS_F);
                    push_quad(
                        &mut verts,
                        &mut indices,
                        [p0 - perp, p0 + perp, p1 + perp, p1 - perp],
                        [1.0, 0.85, 0.3, 0.8],
                    );
                }
            }
        }
        // The hover path breathes: a pulse runs shooter-to-destination.
        if let Some((path, _)) = &self.hover_path {
            let head = (self.fx_clock * 2.2) % 1.0;
            for (i, tile) in path.iter().enumerate() {
                let along = i as f32 / path.len().max(1) as f32;
                let d = (along - head).abs();
                let bright = (1.0 - d * 4.0).clamp(0.0, 1.0);
                if bright > 0.05 {
                    let c = (*tile * TILE_VOXELS).as_vec3()
                        + Vec3::new(HALF_TILE, HALF_TILE, scenario::GROUND_TOP as f32 + 0.7);
                    push_flat_square(
                        &mut verts,
                        &mut indices,
                        c,
                        2.2 * VS_F,
                        [0.4, 0.95, 1.0, 0.5 * bright],
                    );
                }
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

    // ------------------------------------------------------------------
    // The war console: the fixed command deck across the bottom quarter.
    // Every battle action is an icon cell here; the keys are shortcuts.

    fn console(&mut self, ctx: &egui::Context, renderer: &mut Renderer, audio: Option<&Audio>) {
        let h = (ctx.content_rect().height() * 0.25).clamp(190.0, 340.0);
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 14, 13))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(110, 88, 55)))
            .inner_margin(egui::Margin::same(8));
        egui::TopBottomPanel::bottom("war-console")
            .exact_height(h)
            .frame(frame)
            .show(ctx, |ui| {
                let inner = h - 18.0;
                ui.horizontal_top(|ui| {
                    self.console_squad(ui, renderer, inner);
                    ui.separator();
                    self.console_plate(ui, inner);
                    ui.separator();
                    self.console_actions(ui, renderer, audio, inner);
                    ui.separator();
                    self.console_intel(ui, renderer, audio, inner);
                    ui.separator();
                    self.console_map(ui, renderer, audio, inner);
                });
            });
    }

    /// The squad roster: one row a head, click to command.
    fn console_squad(&mut self, ui: &mut egui::Ui, renderer: &mut Renderer, h: f32) {
        use crate::icons::{self, Icon};
        struct Row {
            id: UnitId,
            name: String,
            hp: f32,
            tu: f32,
            kneeling: bool,
            bleeding: bool,
            seized: bool,
            down: bool,
            spent: bool,
        }
        let rows: Vec<Row> = self
            .battle
            .units
            .iter()
            .filter(|u| u.side == Side::Order && !u.civilian && u.alive)
            .map(|u| Row {
                id: u.id,
                name: u.name.split_whitespace().last().unwrap_or(&u.name).to_string(),
                hp: u.health as f32 / u.health_max.max(1) as f32,
                tu: u.tu as f32 / u.tu_max.max(1) as f32,
                kneeling: u.kneeling,
                bleeding: u.wounds > 0,
                seized: u.possessed > 0,
                down: !u.conscious,
                spent: u.is_active() && u.fire_cost(FireMode::Snap).is_none_or(|c| u.tu < c),
            })
            .collect();
        let mut select: Option<UnitId> = None;
        ui.vertical(|ui| {
            ui.set_width(150.0);
            egui::ScrollArea::vertical()
                .id_salt("console-squad")
                .max_height(h)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for r in &rows {
                        let is_sel = self.selected == Some(r.id);
                        let card = egui::Frame::new()
                            .fill(if is_sel {
                                egui::Color32::from_rgb(48, 40, 20)
                            } else {
                                egui::Color32::from_rgb(24, 20, 18)
                            })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if is_sel {
                                    egui::Color32::from_rgb(230, 190, 90)
                                } else {
                                    egui::Color32::from_gray(60)
                                },
                            ))
                            .inner_margin(egui::Margin::symmetric(4, 2));
                        let resp = card
                            .show(ui, |ui| {
                                ui.set_width(136.0);
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    crate::portraits::draw(
                                        ui,
                                        crate::portraits::seed_of(&r.name),
                                        14.0,
                                        0,
                                    );
                                    ui.label(egui::RichText::new(&r.name).small());
                                    if r.kneeling {
                                        icons::draw(ui, Icon::Kneel, 10.0);
                                    }
                                    if r.bleeding {
                                        icons::draw(ui, Icon::Blood, 10.0);
                                    }
                                    if r.seized {
                                        icons::draw(ui, Icon::Eye, 10.0);
                                    }
                                    if r.down {
                                        icons::draw(ui, Icon::Down, 10.0);
                                    }
                                    if r.spent && !r.down {
                                        ui.label(egui::RichText::new("✔").weak().small());
                                    }
                                });
                                mini_bar(ui, r.hp, egui::Color32::from_rgb(190, 60, 50));
                                mini_bar(ui, r.tu, egui::Color32::from_rgb(200, 170, 60));
                            })
                            .response;
                        if resp.interact(egui::Sense::click()).clicked() {
                            select = Some(r.id);
                        }
                    }
                });
        });
        if let Some(id) = select {
            self.selected = Some(id);
            self.refresh_scene(renderer);
        }
    }

    /// The soldier plate: who they are, what they hold, what's left in it.
    fn console_plate(&mut self, ui: &mut egui::Ui, _h: f32) {
        use crate::icons::{self, Icon};
        ui.vertical(|ui| {
            ui.set_width(216.0);
            let Some(id) = self.selected else {
                ui.weak("no soldier selected");
                ui.weak("click one, or press Tab");
                return;
            };
            let u = self.battle.unit(id);
            let name = u.name.clone();
            let scars = u.injuries.len() + u.severed.len();
            let (hp, hp_max) = (u.health, u.health_max);
            let (tu, tu_max) = (u.tu, u.tu_max);
            let (sta, sta_max) = (u.stamina, u.stamina_max);
            let morale = u.morale;
            let weapon = u.weapon.name;
            let (ammo, clip, mags) = (u.ammo, u.weapon.clip, u.mags);
            let belt = u.belt.min(3) as usize;
            let sidearm = u.sidearm.as_ref().map(|w| w.name);
            let (grenades, dressings, flares, blade) =
                (u.grenades, u.heal_charges, u.flares, u.blade);
            let modes: Vec<(FireMode, Icon, bool, String)> = [
                (FireMode::Snap, Icon::Snap, "snap"),
                (FireMode::Aimed, Icon::Aimed, "aimed"),
                (FireMode::Auto, Icon::Auto, "auto"),
            ]
            .into_iter()
            .map(|(m, ic, label)| {
                let detail = match (u.hit_chance(m), u.fire_cost(m)) {
                    (Some(c), Some(t)) => format!("{label}: {c}% · {t} TU"),
                    _ => format!("{label}: not with this weapon"),
                };
                (m, ic, u.fire_cost(m).is_some(), detail)
            })
            .collect();

            ui.horizontal(|ui| {
                crate::portraits::draw(ui, crate::portraits::seed_of(&name), 34.0, scars);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&name)
                            .strong()
                            .color(egui::Color32::from_rgb(230, 210, 170)),
                    );
                    ui.label(egui::RichText::new(weapon).small().weak());
                });
            });
            plate_bar(ui, "TU", tu, tu_max, egui::Color32::from_rgb(200, 170, 60));
            plate_bar(ui, "HP", hp, hp_max, egui::Color32::from_rgb(190, 60, 50));
            plate_bar(ui, "STA", sta, sta_max, egui::Color32::from_rgb(210, 205, 120));
            plate_bar(ui, "MRL", morale, 100, egui::Color32::from_rgb(150, 90, 200));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (m, ic, possible, detail) in &modes {
                    if icons::button(ui, *ic, 24.0, *possible, self.fire_mode == *m, detail) {
                        self.fire_mode = *m;
                    }
                }
                if clip > 0 {
                    let dry = ammo == 0;
                    ui.label(
                        egui::RichText::new(format!("{ammo}/{clip} ×{mags}"))
                            .small()
                            .color(if dry {
                                egui::Color32::from_rgb(230, 90, 70)
                            } else {
                                egui::Color32::from_gray(170)
                            }),
                    )
                    .on_hover_text(if dry {
                        "DRY — reload"
                    } else {
                        "rounds in the weapon / clip size × spare magazines"
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                icons::draw(ui, Icon::Charge, 12.0).on_hover_text("hellfire charges");
                ui.label(egui::RichText::new(grenades.to_string()).small());
                icons::draw(ui, Icon::Dressing, 12.0).on_hover_text("field dressings");
                ui.label(egui::RichText::new(dressings.to_string()).small());
                icons::draw(ui, Icon::Flare, 12.0).on_hover_text("witchfire flares");
                ui.label(egui::RichText::new(flares.to_string()).small());
                if blade {
                    icons::draw(ui, Icon::Blade, 12.0)
                        .on_hover_text("consecrated blade: ripostes melee");
                }
                let pips: String = "●".repeat(belt) + &"○".repeat(3usize.saturating_sub(belt));
                ui.label(egui::RichText::new(pips).small().weak()).on_hover_text(
                    "the belt: three at hand; anything past them costs +6 TU from the pack",
                );
            });
            if let Some(side) = sidearm {
                ui.label(egui::RichText::new(format!("at the hip: {side}")).small().weak());
            }
        });
    }

    /// The action deck: every order the field takes, one cell each.
    fn console_actions(
        &mut self,
        ui: &mut egui::Ui,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        h: f32,
    ) {
        use crate::icons::{self, Icon};
        use winit::keyboard::KeyCode as K;
        let sel = self.selected;
        let armed = self.grenade_armed;
        // While the field is telling you what happened, the deck sleeps —
        // same rule the keys already follow.
        let quiet = self.playback.is_empty() && !self.demon_turn_pending;
        let (ok, charges, dressings, flares, reloadable, has_sidearm, psi) = sel
            .map(|id| {
                let u = self.battle.unit(id);
                (
                    u.is_active(),
                    u.grenades,
                    u.heal_charges,
                    u.flares,
                    u.weapon.clip > 0 && u.mags > 0,
                    u.sidearm.is_some(),
                    u.psi,
                )
            })
            .unwrap_or((false, 0, 0, 0, false, false, false));
        let ok = ok && quiet;
        let officer = sel.is_some_and(|id| {
            let u = self.battle.unit(id);
            ok && u.can_rally && !u.rally_spent
        });
        let rot_near = sel.is_some_and(|id| {
            let me = self.battle.unit(id).tile;
            ok && self.battle.units.iter().any(|u| {
                u.alive
                    && u.side == Side::Order
                    && u.infected.is_some()
                    && (u.tile - me).abs().max_element() <= 1
            })
        });
        let victim = sel.and_then(|id| {
            let me = self.battle.unit(id).tile;
            self.battle
                .units
                .iter()
                .find(|v| {
                    v.alive
                        && !v.conscious
                        && v.side == Side::Demons
                        && (v.tile - me).abs().max_element() <= 1
                })
                .map(|v| v.id)
        });
        let range = ods_sim::battle::TERRIFY_RANGE_TILES;
        let steady_target = sel.filter(|_| psi).and_then(|id| {
            let me = self.battle.unit(id).tile;
            self.battle
                .units
                .iter()
                .filter(|a| {
                    a.is_active()
                        && a.side == Side::Order
                        && !a.civilian
                        && a.id != id
                        && a.morale < 70
                        && (a.tile - me).abs().max_element() <= range
                })
                .min_by_key(|a| a.morale)
                .map(|a| a.id)
        });
        let dread_target = sel.filter(|_| psi).and_then(|id| {
            let me = self.battle.unit(id).tile;
            self.battle
                .units
                .iter()
                .filter(|f| {
                    f.is_active()
                        && f.side == Side::Demons
                        && (f.tile - me).abs().max_element() <= range
                })
                .min_by_key(|f| f.morale + f.bravery / 2)
                .map(|f| f.id)
        });
        let (threat_on, cones_on, map_on, cut_on) =
            (self.show_threat, self.show_cones, self.show_map, self.floor_cap);

        let size = ((h - 20.0) / 4.0 - 4.0).clamp(26.0, 44.0);
        let mut go: Option<K> = None;
        let mut act: Option<Action> = None;
        egui::Grid::new("console-actions").spacing([4.0, 4.0]).show(ui, |ui| {
            let mut cell =
                |ui: &mut egui::Ui, icon: Icon, on: bool, active: bool, hover: &str, key: K| {
                    if icons::button(ui, icon, size, on, active, hover) {
                        go = Some(key);
                    }
                };
            // Row 1: what the hands do.
            cell(ui, Icon::Charge, ok && charges > 0, armed, "arm a hellfire charge, then click a tile [G]", K::KeyG);
            cell(ui, Icon::Dressing, ok && dressings > 0, false, "dress wounds — theirs, or a neighbor's [H]", K::KeyH);
            cell(ui, Icon::Flare, ok && flares > 0, false, "throw a witchfire flare: light in the dark [L]", K::KeyL);
            cell(ui, Icon::Smoke, ok, false, "pop smoke: cover to move behind [V]", K::KeyV);
            cell(ui, Icon::Reload, ok && reloadable, false, "a fresh magazine — 12 TU [C]", K::KeyC);
            cell(ui, Icon::Swap, ok && has_sidearm, false, "trade hands with the hip — 6 TU [I]", K::KeyI);
            ui.end_row();
            // Row 2: the ground and the fallen.
            cell(ui, Icon::Kneel, ok, false, "kneel: steadier aim, smaller shape [K]", K::KeyK);
            cell(ui, Icon::Door, ok, false, "open the door ahead [O]", K::KeyO);
            cell(ui, Icon::Bind, ok, false, "bind an adjacent demon: stun, then take it alive [B]", K::KeyB);
            cell(ui, Icon::Ward, ok, false, "chalk a burning ward on this ground [R]", K::KeyR);
            cell(ui, Icon::Scavenge, ok, false, "take up a fallen weapon — 8 TU [J]", K::KeyJ);
            cell(ui, Icon::Carry, ok, false, "shoulder a downed comrade [U]", K::KeyU);
            ui.end_row();
            // Row 3: officers, saws, whispers, and the unkind mercies.
            cell(ui, Icon::Rally, officer, false, "once a battle: +30 morale in earshot [Y]", K::KeyY);
            cell(ui, Icon::Amputate, rot_near, false, "saw off a rotting limb before it turns them [X]", K::KeyX);
            if icons::button(
                ui,
                Icon::Execute,
                size,
                ok && victim.is_some(),
                false,
                "end a helpless enemy — 10 TU, the capture is forfeit [Z]",
            ) && let (Some(unit), Some(target)) = (sel, victim)
            {
                act = Some(Action::Execute { unit, target });
            }
            if icons::button(
                ui,
                Icon::Steady,
                size,
                quiet && steady_target.is_some(),
                false,
                "steady the most shaken ally in reach — burns the channel's keeper",
            ) && let (Some(unit), Some(target)) = (sel, steady_target)
            {
                act = Some(Action::Steady { unit, target });
            }
            if icons::button(
                ui,
                Icon::Dread,
                size,
                quiet && dread_target.is_some(),
                false,
                "batter the shakiest demon's mind in reach — burns the channel's keeper",
            ) && let (Some(unit), Some(target)) = (sel, dread_target)
            {
                act = Some(Action::Terrify { unit, target });
            }
            cell(ui, Icon::Next, true, false, "next soldier with something left [Tab]", K::Tab);
            ui.end_row();
            // Row 4: how the field is seen.
            cell(ui, Icon::Threat, true, threat_on, "tint the ground known demons can see [T]", K::KeyT);
            cell(ui, Icon::Cones, true, cones_on, "show the squad's watch arcs [N]", K::KeyN);
            cell(ui, Icon::Map, true, map_on, "the tactical map [M]", K::KeyM);
            cell(ui, Icon::Cutaway, true, cut_on, "cut away the upper floors [F]", K::KeyF);
            ui.end_row();
        });
        if let Some(k) = go {
            self.key(renderer, audio, k);
        }
        if let Some(a) = act {
            let result = self.battle.perform(a);
            self.apply(renderer, audio, result);
        }
    }

    /// Watch orders, the shot forecast, and the field log.
    fn console_intel(
        &mut self,
        ui: &mut egui::Ui,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        h: f32,
    ) {
        use crate::icons::{self, Icon};
        ui.vertical(|ui| {
            ui.set_width(250.0);
            // The watch: what shot is banked for the demons' turn.
            if let Some(id) = self.selected
                && self.playback.is_empty()
            {
                let current = self.battle.unit(id).reserve;
                let mut set: Option<Option<FireMode>> = None;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.label(egui::RichText::new("Watch").small().weak())
                        .on_hover_text("bank TUs for a reaction shot on the demons' turn");
                    for (icon, mode, hint) in [
                        (Icon::NoWatch, None, "no watch: spend freely, react with a snap"),
                        (Icon::Snap, Some(FireMode::Snap), "bank a snap: the cheap trip-wire"),
                        (Icon::Aimed, Some(FireMode::Aimed), "bank an aimed shot: one good answer"),
                        (Icon::Auto, Some(FireMode::Auto), "bank the whole burst: the storm"),
                    ] {
                        let possible = mode
                            .is_none_or(|m| self.battle.unit(id).fire_cost(m).is_some());
                        if icons::button(ui, icon, 22.0, possible, current == mode, hint) {
                            set = Some(mode);
                        }
                    }
                });
                if let Some(mode) = set {
                    let result = self.battle.perform(Action::SetReserve { unit: id, mode });
                    self.apply(renderer, audio, result);
                }
            }
            if self.grenade_armed {
                ui.colored_label(egui::Color32::ORANGE, "CHARGE ARMED — click a tile");
            }
            // The forecast: what the cursor is worth.
            match (self.selected, self.hover) {
                (Some(id), Some(tile)) => {
                    if let Some(enemy) = self
                        .battle
                        .unit_at(tile)
                        .filter(|&e| self.battle.unit(e).side == Side::Demons)
                    {
                        let mut line = format!("Target: {}", self.battle.unit(enemy).name);
                        let mut seen = true;
                        let mut breakdown: Option<String> = None;
                        for (label, mode) in [
                            ("snap", FireMode::Snap),
                            ("aimed", FireMode::Aimed),
                            ("auto", FireMode::Auto),
                        ] {
                            if let Some(f) = self.battle.forecast_shot(id, enemy, mode) {
                                if f.rounds > 1 {
                                    line.push_str(&format!(
                                        "  {label} {}%×{} ({}TU)",
                                        f.chance, f.rounds, f.cost
                                    ));
                                } else {
                                    line.push_str(&format!("  {label} {}% ({}TU)", f.chance, f.cost));
                                }
                                seen = f.seen;
                                if mode == FireMode::Snap {
                                    line.push_str(&if f.stun {
                                        format!("  [SALT: stuns ≤{}]", f.power)
                                    } else {
                                        format!("  [dmg 0–{}]", f.power * 2)
                                    });
                                    breakdown = Some(format!(
                                        "skill {} × mode {}%{}{} = {}%",
                                        f.skill,
                                        f.mode_pct,
                                        if f.kneeling { " × kneel 115%" } else { "" },
                                        if f.high_ground > 0 { " + high ground 10" } else { "" },
                                        f.chance
                                    ));
                                }
                            }
                        }
                        if !seen {
                            line.push_str("  [NO LINE OF SIGHT]");
                        }
                        let resp = ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            egui::RichText::new(line).small(),
                        );
                        if let Some(b) = breakdown {
                            resp.on_hover_text(b);
                        }
                    } else if let Some((_, cost)) = &self.hover_path {
                        let u = self.battle.unit(id);
                        let okm = *cost <= u.tu;
                        let breaks_watch = u
                            .reserve
                            .is_some_and(|m| u.fire_cost(m).is_some_and(|c| *cost > u.tu - c));
                        ui.colored_label(
                            if !okm {
                                egui::Color32::GRAY
                            } else if breaks_watch {
                                egui::Color32::from_rgb(230, 180, 70)
                            } else {
                                egui::Color32::LIGHT_GREEN
                            },
                            egui::RichText::new(format!(
                                "Move: {cost} TU of {}{}",
                                u.tu,
                                if okm && breaks_watch { " — BREAKS YOUR WATCH" } else { "" }
                            ))
                            .small(),
                        );
                    }
                }
                _ => {
                    ui.label(
                        egui::RichText::new("hover a tile for costs; a demon for odds")
                            .small()
                            .weak(),
                    );
                }
            }
            ui.separator();
            // The field log rides in the console now.
            egui::ScrollArea::vertical()
                .id_salt("console-log")
                .stick_to_bottom(true)
                .max_height(h - 84.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for line in &self.log {
                        ui.colored_label(
                            crate::geoscape::log_color(line),
                            egui::RichText::new(line).small(),
                        );
                    }
                });
        });
    }

    /// The table map and the big red END TURN.
    fn console_map(
        &mut self,
        ui: &mut egui::Ui,
        renderer: &mut Renderer,
        audio: Option<&Audio>,
        h: f32,
    ) {
        let (min, max) = self.battle.tiles.bounds();
        let size = max - min;
        let px = ((h - 46.0) / size.y.max(1) as f32)
            .min(210.0 / size.x.max(1) as f32)
            .clamp(1.5, 6.0);
        let mut jump: Option<IVec3> = None;
        let turn = self.battle.turn;
        let mut end_turn = false;
        ui.vertical(|ui| {
            jump = self.minimap(ui, px, true);
            let w = size.x as f32 * px;
            ui.horizontal(|ui| {
                crate::icons::draw(ui, crate::icons::Icon::EndTurn, 26.0);
                let end = egui::Button::new(
                    egui::RichText::new("END TURN")
                        .strong()
                        .color(egui::Color32::from_rgb(255, 225, 180)),
                )
                .fill(egui::Color32::from_rgb(96, 32, 24));
                if ui
                    .add_sized([(w - 30.0).max(110.0), 26.0], end)
                    .on_hover_text("hand the field to the Otherside [Space]")
                    .clicked()
                {
                    end_turn = true;
                }
            });
            ui.label(egui::RichText::new(format!("turn {turn}")).small().weak());
        });
        if end_turn {
            self.end_turn(renderer, audio);
        }
        if let Some(tile) = jump {
            let p = (tile * TILE_VOXELS).as_vec3() + Vec3::new(HALF_TILE, HALF_TILE, 0.0);
            self.camera.target.x = p.x;
            self.camera.target.y = p.y;
        }
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
        // Where the camera stands: a gold brace on the table map.
        {
            let t = self.camera.target / TILE_VOXELS as f32;
            let p = egui::pos2(
                rect.min.x + (t.x - min.x as f32) * px,
                rect.max.y - (t.y - min.y as f32) * px,
            );
            let half = (self.camera.distance * 0.42 / TILE_VOXELS as f32) * px * 0.9;
            paint.rect_stroke(
                egui::Rect::from_center_size(p, egui::vec2(half * 2.0, half * 1.4)),
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 180, 90)),
                egui::StrokeKind::Middle,
            );
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
        // Cycle those who can still do something; fall back to everyone.
        let mut soldiers: Vec<UnitId> = self
            .battle
            .living(Side::Order)
            .filter(|u| {
                !u.civilian && u.fire_cost(FireMode::Snap).is_some_and(|c| u.tu >= c)
            })
            .map(|u| u.id)
            .collect();
        if soldiers.is_empty() {
            soldiers = self.battle.living(Side::Order).map(|u| u.id).collect();
        }
        if soldiers.is_empty() {
            self.selected = None;
            return;
        }
        // The current pick may not be in the list at all (spent, dead,
        // possessed) — look up its position rather than cycling after it.
        let at = self
            .selected
            .and_then(|cur| soldiers.iter().position(|&id| id == cur));
        self.selected = match at {
            Some(i) => Some(soldiers[(i + 1) % soldiers.len()]),
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
        // The soundscape reads the field: open contact heats the score.
        self.contact =
            (self.battle.visible_enemies(Side::Order).len() as f32 / 3.0).min(1.0);

        self.reachable = match self.selected {
            Some(id) if self.battle.unit(id).is_active() => self.battle.reachable(id),
            _ => Vec::new(),
        };

        // Body-part voxel figures for every visible unit.
        let (fig_verts, fig_indices) =
            figures::build_figures(&self.battle, &visible, &self.visual, &self.anim, &mut self.shells);
        renderer.set_figures(&fig_verts, &fig_indices);

        let mut verts: Vec<OverlayVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let (min, max) = self.battle.tiles.bounds();
        // At night, open flame throws a pool of warm light; everything else
        // sinks into cold blue. Distance to the nearest fire decides which.
        let night = self.is_night();
        let mut lights: Vec<(IVec3, i32)> = self
            .battle
            .clouds
            .iter()
            .filter(|(_, kind, _)| *kind == ods_sim::battle::CloudKind::Fire)
            .map(|(t, _, _)| (*t, 0))
            .collect();
        // Flares throw a wider pool than open flame.
        lights.extend(self.battle.flares.iter().map(|f| (*f, -1)));
        let fire_dist = |tile: IVec3| -> i32 {
            lights
                .iter()
                .map(|(f, bias)| {
                    (f.x - tile.x).abs().max((f.y - tile.y).abs()) + (f.z - tile.z).abs() + bias
                })
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
        // Witchfire flares: a hard teal-white core and a warm pool.
        for tile in &self.battle.flares {
            push_tile_quad(&mut verts, &mut indices, *tile, [0.75, 1.0, 0.9, 0.5]);
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
                let friendly = if self.colorblind {
                    [0.25, 0.6, 1.0, 0.35]
                } else {
                    [0.2, 1.0, 0.35, 0.35]
                };
                push_tile_quad(&mut verts, &mut indices, u.tile, friendly);
                // The soldier's own cursor box, in the Order's gold.
                push_wire_box(&mut verts, &mut indices, u.tile, [0.95, 0.85, 0.3, 0.9]);
                // The facing wedge: where their reaction arc looks.
                let c = (u.tile * TILE_VOXELS).as_vec3()
                    + Vec3::new(HALF_TILE, HALF_TILE, scenario::GROUND_TOP as f32 + 0.5);
                let f = Vec3::new(u.facing.x as f32, u.facing.y as f32, 0.0)
                    .normalize_or(Vec3::X);
                let perp = f.cross(Vec3::Z) * (4.5 * VS_F);
                let tip = c + f * (11.0 * VS_F);
                let base = c + f * (6.0 * VS_F);
                push_quad(
                    &mut verts,
                    &mut indices,
                    [base - perp, base + perp, tip, tip],
                    [1.0, 0.9, 0.4, 0.35],
                );
            }
        }
        // Overhead markers, the original's colored language: a gold arrow
        // over the selected soldier, pale chevrons over the squad, red pips
        // over every demon in sight.
        for u in &self.battle.units {
            if !u.is_active() || u.civilian {
                continue;
            }
            let base = (u.tile * TILE_VOXELS).as_vec3()
                + Vec3::new(HALF_TILE, HALF_TILE, TILE_VOXELS as f32 + 4.0 * VS_F);
            match u.side {
                Side::Order if self.selected == Some(u.id) => {
                    push_marker(&mut verts, &mut indices, base, 3.2 * VS_F, [1.0, 0.85, 0.2, 0.95]);
                }
                Side::Order => {
                    push_marker(&mut verts, &mut indices, base, 1.7 * VS_F, [0.7, 0.82, 1.0, 0.65]);
                }
                Side::Demons if visible.contains(&u.tile) => {
                    let danger = if self.colorblind {
                        [1.0, 0.55, 0.05, 0.9]
                    } else {
                        [1.0, 0.15, 0.1, 0.9]
                    };
                    push_marker(&mut verts, &mut indices, base, 2.2 * VS_F, danger);
                }
                _ => {}
            }
        }
        // The ground within this turn's legs, faintly.
        if let Some(id) = self.selected
            && self.battle.unit(id).is_active()
        {
            for (tile, _) in self.battle.reachable(id) {
                push_tile_quad(&mut verts, &mut indices, tile, [0.9, 0.9, 0.7, 0.035]);
            }
        }

        // Watch cones: the ground each soldier's reaction arc actually
        // covers — sightline and facing both. The selected soldier's cone
        // always shows; the squad's ghost in when toggled [N].
        {
            let in_arc = |u: &ods_sim::units::Unit, tile: IVec3| -> bool {
                let d = tile - u.tile;
                if d.x == 0 && d.y == 0 {
                    return true;
                }
                let dir = glam::Vec2::new(d.x as f32, d.y as f32).normalize_or_zero();
                let face =
                    glam::Vec2::new(u.facing.x as f32, u.facing.y as f32).normalize_or_zero();
                dir.dot(face) >= 0.38
            };
            for u in &self.battle.units {
                if !u.is_active() || u.side != Side::Order || u.civilian {
                    continue;
                }
                let is_sel = self.selected == Some(u.id);
                if !self.show_cones && !is_sel {
                    continue;
                }
                let color = if is_sel {
                    [0.95, 0.8, 0.3, 0.10]
                } else {
                    [0.5, 0.65, 0.9, 0.05]
                };
                for tile in self.battle.tiles_seen_from(&[u.tile]) {
                    if in_arc(u, tile) {
                        push_tile_quad(&mut verts, &mut indices, tile, color);
                    }
                }
            }
        }
        // Fallen weapons glint where they lie: a low steel cross wherever
        // the squad can see the floor.
        for (tile, _, _) in &self.battle.ground {
            if visible.contains(tile) {
                let base = (*tile * TILE_VOXELS).as_vec3()
                    + Vec3::new(HALF_TILE, HALF_TILE, scenario::GROUND_TOP as f32 + 2.0);
                push_marker(&mut verts, &mut indices, base, 1.2 * VS_F, [0.8, 0.85, 0.92, 0.8]);
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

/// An overhead marker: a downward arrow built as two crossed vertical
/// triangles, readable from any camera yaw.
fn push_marker(
    verts: &mut Vec<OverlayVertex>,
    indices: &mut Vec<u32>,
    tip: Vec3,
    r: f32,
    color: [f32; 4],
) {
    for axis in [Vec3::X, Vec3::Y] {
        let a = tip + axis * r + Vec3::Z * (2.2 * r);
        let b = tip - axis * r + Vec3::Z * (2.2 * r);
        push_quad(verts, indices, [tip, a, b, tip], color);
    }
}

/// A slim console gauge: background groove plus a colored fill fraction.
/// Roughly what a voxel material looks like, for debris that remembers
/// what it was chipped from.
fn mat_color(v: ods_voxel::Voxel) -> [f32; 4] {
    match v.0 {
        2 => [0.40, 0.24, 0.18, 0.95],  // brick
        5 | 13 => [0.28, 0.19, 0.10, 0.95], // timber
        10 => [0.62, 0.53, 0.33, 0.95], // sand
        11 => [0.72, 0.76, 0.82, 0.95], // snow
        25 => [0.38, 0.38, 0.35, 0.95], // fieldstone
        8 => [0.42, 0.13, 0.24, 0.95],  // flesh
        _ => [0.27, 0.25, 0.22, 0.95],  // rubble-grey
    }
}

/// What flies when a weapon speaks: (speed voxels/s, arc height as a
/// fraction of range, color, half-width in voxels).
fn projectile(key: &str) -> (f32, f32, [f32; 4], f32) {
    if key.contains("mortar") {
        (200.0, 0.35, [0.85, 0.9, 0.95, 0.95], 1.2)
    } else if key.contains("censer") {
        (240.0, 0.22, [1.0, 0.55, 0.15, 0.95], 1.0)
    } else if key.contains("lance") {
        (430.0, 0.0, [1.0, 0.4, 0.1, 0.95], 1.4)
    } else if key.contains("arbalest") || key.contains("crossbow") {
        (330.0, 0.05, [0.85, 0.75, 0.5, 0.95], 0.7)
    } else if key.contains("bile") || key.contains("spit") || key.contains("gout") {
        (190.0, 0.18, [0.5, 0.9, 0.2, 0.95], 1.1)
    } else {
        (360.0, 0.0, [1.0, 0.85, 0.45, 0.95], 0.6)
    }
}

/// A labeled console bar: name at the left, value over the fill.
fn plate_bar(ui: &mut egui::Ui, label: &str, val: i32, max: i32, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_sized(
            [26.0, 12.0],
            egui::Label::new(egui::RichText::new(label).small().weak()),
        );
        let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 11.0), egui::Sense::hover());
        let paint = ui.painter_at(rect);
        paint.rect_filled(rect, 2.0, egui::Color32::from_gray(34));
        let frac = (val.max(0) as f32 / max.max(1) as f32).clamp(0.0, 1.0);
        let mut fill = rect;
        fill.set_width(rect.width() * frac);
        paint.rect_filled(fill, 2.0, color);
        paint.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{val}/{max}"),
            egui::FontId::proportional(9.0),
            egui::Color32::from_gray(235),
        );
    });
}

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

/// Somewhere a person would say, not a coordinate.
fn place_name(battle: &Battle, at: IVec3) -> String {
    let (min, max) = battle.tiles.bounds();
    let base = if at.x <= min.x + 5 && (10..=13).contains(&at.y) {
        "the gondola".to_string()
    } else if (16..=20).contains(&at.x) && (2..=6).contains(&at.y) {
        "the watchtower".to_string()
    } else if at.x >= max.x - 3 && (10..=14).contains(&at.y) {
        "the obelisk ground".to_string()
    } else if (9..=14).contains(&at.x) && (8..=15).contains(&at.y) {
        "the shelter yard".to_string()
    } else {
        let third_x = (max.x - min.x) / 3;
        let third_y = (max.y - min.y) / 3;
        let ew = if at.x < min.x + third_x {
            "west"
        } else if at.x >= max.x - third_x {
            "east"
        } else {
            ""
        };
        let ns = if at.y < min.y + third_y {
            "south"
        } else if at.y >= max.y - third_y {
            "north"
        } else {
            ""
        };
        match (ns, ew) {
            ("", "") => "the open middle".to_string(),
            (n, "") => format!("the {n} field"),
            ("", e) => format!("the {e} field"),
            (n, e) => format!("the {n}-{e} field"),
        }
    };
    if at.z > 0 { format!("upstairs over {base}") } else { base }
}

fn describe(event: &Event, battle: &Battle) -> String {
    let name = |id: &UnitId| battle.unit(*id).name.clone();
    let place = |at: &IVec3| place_name(battle, *at);
    match event {
        Event::TurnStarted { side, turn } => format!("— turn {turn}: {side:?} to move —"),
        Event::Moved { unit, to, tu_left, .. } => {
            format!("{} moves through {} ({tu_left} TU left)", name(unit), place(to))
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
            format!("!!! a summoning circle scribes itself in {} — foul it or face what comes", place(at))
        }
        Event::Summoned { unit } => {
            format!("!!! the circle delivers: {} steps through", name(unit))
        }
        Event::SummoningDisrupted { at } => {
            format!("the summoning in {} is fouled — nothing comes through", place(at))
        }
        Event::WardInscribed { at } => {
            format!("a ward burns witchfire-bright in {}", place(at))
        }
        Event::WardBurned { unit, at } => {
            format!("{} crosses the ward in {} — and the ward answers", name(unit), place(at))
        }
        Event::CorruptionSpread { at } => {
            format!("the obelisk's veins reach {}", place(at))
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
            format!("*** {} reaches the gondola and is AWAY ***", name(unit))
        }
        Event::TimeExpired => "!!! too late. The clock has taken the field".to_string(),
        Event::FloorCollapsed { at } => {
            format!("!!! the floor gives way over {} and comes down", place(at))
        }
        Event::AtrocityFound { unit, at } => {
            format!("!!! {} finds what the demons left in {}. Nobody should see this.", name(unit), place(at))
        }
        Event::TerrainDestroyed { voxels, .. } => {
            format!("terrain shattered ({voxels} voxels)")
        }
        Event::Threw { unit, at } => format!("{} lobs a hellfire charge into {}", name(unit), place(at)),
        Event::Exploded { at, voxels } => {
            format!("detonation in {} ({voxels} voxels destroyed)", place(at))
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
            format!("a primed charge drops in {} — {timer} half-turns on the fuse", place(at))
        }
        Event::SmokePopped { at } => format!("smoke blooms over {}", place(at)),
        Event::FireStarted { at } => format!("fire takes hold in {}", place(at)),
        Event::FlareThrown { at } => format!("a witchfire flare burns in {} — light holds there", place(at)),
        Event::Burned { unit, amount } => format!("{} burns ({amount})", name(unit)),
        Event::DoorOpened { at } => format!("a door swings open in {}", place(at)),
        Event::Possessed { unit, by } => {
            format!("!!! {} SEIZES {}'s MIND !!!", name(by), name(unit))
        }
        Event::PossessionEnds { unit } => format!("{} is their own again", name(unit)),
        Event::WallSmashed { at, voxels } => {
            format!("!!! masonry EXPLODES inward in {} ({voxels} voxels) !!!", place(at))
        }
        Event::Fell { unit, to } => format!("{} falls — down into {}", name(unit), place(to)),
        Event::CarriedUp { unit, carried } => {
            format!("{} shoulders {}", name(unit), name(carried))
        }
        Event::SetDown { unit, carried } => {
            format!("{} lays {} down", name(unit), name(carried))
        }
        Event::Scavenged { unit } => format!("{} takes up a fallen weapon", name(unit)),
        Event::Reloaded { unit } => format!("{} slaps a fresh magazine home", name(unit)),
        Event::Swapped { unit } => {
            format!("{} draws what was at the hip", name(unit))
        }
        Event::WeaponDropped { unit, .. } => {
            format!("{}'s weapon falls to the ground", name(unit))
        }
        Event::Executed { unit, target } => {
            format!("{} puts {} down where it lies", name(unit), name(target))
        }
        Event::RestGranted { unit } => {
            format!("{} is at rest — the wall remembers", name(unit))
        }
        Event::PackShaken => "the last driver is dead: the pack feels the leash go slack".into(),
        Event::PackBroken => "THE PACK BREAKS — everything that knows fear turns and runs".into(),
        Event::Escaped { unit } => {
            format!("{} reaches the way out and is gone — to tell of you", name(unit))
        }
        Event::Lashed { unit } => {
            format!("a Prince's will falls across {} — it turns back", name(unit))
        }
        Event::Steadied { unit, target } => {
            format!("{} whispers {} steady again", name(unit), name(target))
        }
        Event::NoiseInDark { near } => {
            format!("something shrieks in the dark, out in {}...", place(near))
        }
        Event::BattleOver { winner } => format!("=== BATTLE OVER: {winner:?} wins ==="),
    }
}


/// Read the field once and pick its standing soundscape: weather first,
/// then the ground itself.
fn choose_ambient(battle: &Battle) -> crate::audio::Ambient {
    use crate::audio::Ambient;
    use ods_sim::battle::Weather;
    match battle.weather {
        Weather::Rain => return Ambient::Rain,
        Weather::Sandstorm => return Ambient::Sandstorm,
        _ => {}
    }
    let (min, max) = battle.tiles.bounds();
    let mid = (min + max) / 2;
    let ground = battle.world.voxel(
        mid * TILE_VOXELS
            + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, scenario::GROUND_TOP - 1),
    );
    if ground == scenario::MAT_SAND {
        return crate::audio::Ambient::Desert;
    }
    if ground == scenario::MAT_SNOW || ground == scenario::MAT_GLINT {
        return crate::audio::Ambient::Tundra;
    }
    // Canopy check: enough foliage overhead reads as jungle.
    let mut canopy = 0;
    for (dx, dy) in [(3, 3), (-4, 5), (6, -3), (-5, -5), (0, 7)] {
        let t = mid + IVec3::new(dx, dy, 0);
        let p = t * TILE_VOXELS + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, TILE_VOXELS + 2);
        if battle.world.voxel(p) == scenario::MAT_FOLIAGE {
            canopy += 1;
        }
    }
    if canopy >= 2 { Ambient::Jungle } else { Ambient::Temperate }
}
